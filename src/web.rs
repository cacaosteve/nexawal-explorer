use crate::{
    model::{self, decode_transaction, Error, Result, Snapshot},
    rpc::Rpc,
    view::{self, escape, number, row, row_html, tx_link, xmr},
};
use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::{fmt::Write, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone)]
struct App {
    rpc: Rpc,
    requests: Arc<Semaphore>,
}

pub fn router(rpc: Rpc) -> Router {
    let app = App {
        rpc,
        requests: Arc::new(Semaphore::new(32)),
    };
    routes(app)
}

fn routes(app: App) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/blocks", get(blocks))
        .route("/block/{id}", get(block))
        .route("/tx/{id}", get(transaction))
        .route("/mempool", get(mempool))
        .route("/search", get(search))
        .route("/about", get(about))
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .route("/style.css", get(styles))
        .route(
            "/robots.txt",
            get(|| async { "User-agent: *\nDisallow: /\n" }),
        )
        .fallback(|| async { fail(Error::NotFound, None) })
        .layer(middleware::from_fn_with_state(app.clone(), protect))
        .with_state(app)
}

async fn protect(State(app): State<App>, request: Request, next: Next) -> Response {
    let mut response = if request.uri().to_string().len() > 1024 {
        fail(Error::InvalidInput, None)
    } else if request.uri().path() == "/healthz" {
        // Liveness must not share the admission budget for node-dependent work.
        // This fixed route performs no upstream calls, even under saturation.
        next.run(request).await
    } else if let Ok(_permit) = app.requests.clone().try_acquire_owned() {
        match tokio::time::timeout(Duration::from_secs(20), next.run(request)).await {
            Ok(response) => response,
            Err(_) => fail(Error::Unavailable, None),
        }
    } else {
        fail(Error::Busy, None)
    };
    let headers = response.headers_mut();
    headers.insert("content-security-policy", HeaderValue::from_static("default-src 'none'; script-src 'none'; style-src 'self'; img-src 'none'; font-src 'none'; connect-src 'none'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    headers.insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    if !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

pub fn fail(error: Error, s: Option<&Snapshot>) -> Response {
    let (status, title, text) = match error {
        Error::InvalidInput => (StatusCode::BAD_REQUEST, "Check your search", "Enter a 64-character hexadecimal transaction/block hash or a non-negative block height. Pages contain at most 25 rows."),
        Error::NotFound => (StatusCode::NOT_FOUND, "Not found on this node", "This node does not currently have that transaction or block. A missing transaction is not proof that a payment never existed. Check the network, ID, and wallet; pending transactions may leave the mempool."),
        Error::Unsupported => (StatusCode::BAD_GATEWAY, "Data unavailable", "The node does not support this operation, or this result exceeds the explorer's safety limits. No empty or zero result has been substituted."),
        Error::Inconsistent => (StatusCode::SERVICE_UNAVAILABLE, "Chain data changed or could not be verified", "The node returned incomplete, inconsistent, or unsupported transaction data. A reorganization may be in progress. Refresh to try again; no confirmation count has been invented."),
        Error::Busy => (StatusCode::SERVICE_UNAVAILABLE, "Please try again shortly", "The explorer is at its request limit. Refresh in a few seconds."),
        Error::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "Node temporarily unavailable or still syncing", "The explorer could not obtain the requested node data. This does not mean the blockchain is empty or your transaction was lost. Refresh to try again."),
        Error::BlockUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "Block lookup unavailable", "This node could not return that block hash. The hash may be unknown, on a different network, orphaned, or temporarily unavailable. Check the hash or try again. The daemon does not provide enough information to distinguish these cases safely."),
    };
    let body = format!(
        "<section class=panel><p>{}</p><p><a href=\"/\">Return to explorer</a></p></section>",
        escape(text)
    );
    let mut response = view::page(title, body, s, status);
    if status == StatusCode::SERVICE_UNAVAILABLE {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
    }
    response
}

#[derive(Default, Deserialize)]
struct Page {
    #[serde(default)]
    page: usize,
}
fn offset(page: usize) -> Result<usize> {
    page.checked_mul(25)
        .filter(|n| *n <= 100_000_000)
        .ok_or(Error::InvalidInput)
}
fn subset<T>(values: &[T], page: usize) -> Result<&[T]> {
    let start = offset(page)?;
    if start >= values.len() && page != 0 {
        return Err(Error::NotFound);
    }
    Ok(&values[start.min(values.len())..(start + 25).min(values.len())])
}

async fn home(State(app): State<App>) -> Response {
    let s = match app.rpc.snapshot().await {
        Ok(s) => s,
        Err(e) => return fail(e, None),
    };
    let start = s.tip.height.saturating_sub(9);
    let headers = match app.rpc.headers(&s, start, s.tip.height).await {
        Ok(h) => h,
        Err(e) => return fail(e, Some(&s)),
    };
    let hashrate = s
        .info
        .difficulty()
        .and_then(|d| d.checked_div(s.info.target.into()))
        .map(view::hashrate)
        .unwrap_or_else(|| "Unavailable".into());
    let mut body = format!("<section class=panel><h2>Network snapshot</h2><dl>{}{}{}{}</dl><p class=muted>Height is the number of blocks; the latest block index is one less. Hashrate is estimated from difficulty and target block time.</p></section>", row("Network", &s.info.nettype), row("Chain height", &number(s.info.height)), row("Estimated network hashrate", &hashrate), row_html("Mempool", &format!("<a href=\"/mempool\">{} transactions reported by this node</a>", number(s.info.tx_pool_size))));
    body.push_str("<section class=panel><div class=section-heading><h2>Recent blocks</h2><a href=\"/blocks\">Browse blocks →</a></div>");
    body.push_str(&view::headers_table(&headers));
    body.push_str("</section><section class=panel><div class=section-heading><h2>Mempool preview</h2><a href=\"/mempool\">Browse mempool →</a></div>");
    match app.rpc.pool_hashes(&s).await {
        Ok(ids) if ids.is_empty() => {
            body.push_str("<p>No transactions currently visible in this node's mempool.</p>")
        }
        Ok(ids) => {
            body.push_str("<p class=muted>Up to five IDs, sorted alphabetically rather than by arrival time.</p><ul class=hash-list>");
            for id in ids.iter().take(5) {
                let _ = write!(body, "<li>{}</li>", tx_link(id));
            }
            body.push_str("</ul>");
        }
        Err(_) => body.push_str(
            "<p class=warning>Mempool preview unavailable. This is not an empty-pool result.</p>",
        ),
    }
    body.push_str(
        "</section><p class=muted>Refresh this page to update. No automatic browser requests.</p>",
    );
    view::page("Monero, without the noise.", body, Some(&s), StatusCode::OK)
}

async fn blocks(
    State(app): State<App>,
    query: std::result::Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let page = match query {
        Ok(Query(p)) => p.page,
        Err(_) => return fail(Error::InvalidInput, None),
    };
    let skip = match offset(page) {
        Ok(n) => n as u64,
        Err(e) => return fail(e, None),
    };
    let s = match app.rpc.snapshot().await {
        Ok(s) => s,
        Err(e) => return fail(e, None),
    };
    let end = match s.tip.height.checked_sub(skip) {
        Some(h) => h,
        None => return fail(Error::NotFound, Some(&s)),
    };
    let start = end.saturating_sub(24);
    let headers = match app.rpc.headers(&s, start, end).await {
        Ok(h) => h,
        Err(e) => return fail(e, Some(&s)),
    };
    let body = format!("<section class=panel>{}{}</section><p class=muted>Newest first. Block pages may shift as new blocks arrive.</p>", view::headers_table(&headers), view::pager("/blocks", page, s.info.height as usize));
    view::page("Blocks", body, Some(&s), StatusCode::OK)
}

async fn block(
    State(app): State<App>,
    Path(raw): Path<String>,
    query: std::result::Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let id = match if raw.len() == 64 {
        model::hash(&raw)
    } else {
        model::height(&raw).map(|h| h.to_string())
    } {
        Ok(id) => id,
        Err(e) => return fail(e, None),
    };
    let page = match query {
        Ok(Query(p)) => p.page,
        Err(_) => return fail(Error::InvalidInput, None),
    };
    let s = match app.rpc.snapshot().await {
        Ok(s) => s,
        Err(e) => return fail(e, None),
    };
    let b = match app.rpc.block(&s, &id).await {
        Ok(b) => b,
        Err(e) => return fail(e, Some(&s)),
    };
    let h = &b.block_header;
    let txs = match subset(&b.tx_hashes, page) {
        Ok(t) => t,
        Err(e) => return fail(e, Some(&s)),
    };
    let confirmations = if h.orphan_status {
        "Orphaned — not in the canonical chain".into()
    } else {
        match s.confirmations(h.height) {
            Ok(n) => format!("{n} (includes its containing block)"),
            Err(e) => return fail(e, Some(&s)),
        }
    };
    let mut body = format!(
        "<section class=panel><dl>{}{}{}{}{}{}{}{}{}{}{}</dl></section>",
        row("Block hash", &h.hash),
        row("Height", &number(h.height)),
        row("Confirmations", &confirmations),
        row("Time", &view::time(h.timestamp)),
        row("Weight", &format!("{} B", number(h.block_weight))),
        row(
            "Serialized block size",
            &format!("{} B", number(h.block_size))
        ),
        row(
            "Miner reward (including fees)",
            &format!("{} XMR", xmr(h.reward))
        ),
        row("Difficulty (low 64 bits)", &h.difficulty.to_string()),
        row("Protocol version", &h.major_version.to_string()),
        row("Transactions (excluding miner)", &number(h.num_txes)),
        row_html("Miner transaction", &tx_link(&b.miner_tx_hash))
    );
    body.push_str("<nav class=pager aria-label=\"Adjacent blocks\">");
    if h.height > 0 {
        let _ = write!(
            body,
            "<a href=\"/block/{}\">← Previous height</a>",
            h.height - 1
        );
    }
    if h.height < s.tip.height {
        let _ = write!(
            body,
            "<a href=\"/block/{}\">Next height →</a>",
            h.height + 1
        );
    }
    body.push_str("</nav><section class=panel><h2>Transactions</h2><ol class=hash-list>");
    for tx in txs {
        let _ = write!(body, "<li>{}</li>", tx_link(tx));
    }
    if b.tx_hashes.is_empty() {
        body.push_str("<li>No non-miner transactions in this block.</li>");
    }
    body.push_str("</ol>");
    body.push_str(&view::pager(
        &format!("/block/{}", h.hash),
        page,
        b.tx_hashes.len(),
    ));
    body.push_str("</section>");
    view::page(
        &format!("Block {}", number(h.height)),
        body,
        Some(&s),
        StatusCode::OK,
    )
}

async fn transaction(
    State(app): State<App>,
    Path(raw): Path<String>,
    query: std::result::Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let id = match model::hash(&raw) {
        Ok(id) => id,
        Err(e) => return fail(e, None),
    };
    let page = match query {
        Ok(Query(p)) => p.page,
        Err(_) => return fail(Error::InvalidInput, None),
    };
    let s = match app.rpc.snapshot().await {
        Ok(s) => s,
        Err(e) => return fail(e, None),
    };
    let entry = match app.rpc.transaction(&s, &id).await {
        Ok(Some(tx)) => tx,
        Ok(None) => {
            return fail(
                if s.ready() {
                    Error::NotFound
                } else {
                    Error::Unavailable
                },
                Some(&s),
            )
        }
        Err(e) => return fail(e, Some(&s)),
    };
    let details = match decode_transaction(&entry, &id) {
        Ok(d) => d,
        Err(e) => return fail(e, Some(&s)),
    };
    let (confirmations, confirmed_block_time) = if entry.in_pool {
        (
            "Pending — observed in this node's mempool".to_string(),
            None,
        )
    } else {
        let height = match entry.block_height {
            Some(h) => h,
            None => return fail(Error::Inconsistent, Some(&s)),
        };
        let b = match app.rpc.block(&s, &height.to_string()).await {
            Ok(b) => b,
            Err(e) => return fail(e, Some(&s)),
        };
        if b.block_header.orphan_status || !b.contains(&id) {
            return fail(Error::Inconsistent, Some(&s));
        }
        match s.confirmations(height) {
            Ok(n) => (
                format!("Confirmed · {n} confirmations (includes containing block)"),
                Some(b.block_header.timestamp),
            ),
            Err(e) => return fail(e, Some(&s)),
        }
    };
    if offset(page).is_err() {
        return fail(Error::InvalidInput, Some(&s));
    }
    let max_rows = details.inputs.len().max(details.outputs.len());
    if page > 0 && page * 25 >= max_rows {
        return fail(Error::NotFound, Some(&s));
    }
    let fee = details
        .fee
        .map(|f| format!("{} XMR", xmr(f)))
        .unwrap_or_else(|| "Unavailable — not assumed to be zero".into());
    let mut body = format!(
        "<section class=panel><p class=hash>{}</p><dl>{}{}{}{}{}{}{}{}</dl>",
        escape(&id),
        row("Status", &confirmations),
        row(
            "Type",
            if details.coinbase {
                "Miner transaction"
            } else {
                "Transfer"
            }
        ),
        row("Network fee", &fee),
        row("Version", &details.version.to_string()),
        row(
            "Serialized size",
            &details
                .full_size
                .map(|n| format!("{n} B"))
                .unwrap_or_else(|| format!(
                    "Pruned data only ({} B); full size unavailable",
                    details.downloaded_size
                ))
        ),
        row(
            "Weight",
            &details
                .weight
                .map(|n| format!("{n} B"))
                .unwrap_or_else(|| "Unavailable from pruned data".into())
        ),
        row("Additional unlock condition", &details.unlock),
        row(
            "Inputs / outputs",
            &format!(
                "{} / {}",
                if details.coinbase {
                    1
                } else {
                    details.inputs.len()
                },
                details.outputs.len()
            )
        )
    );
    if !entry.in_pool {
        if let Some(h) = entry.block_height {
            body.push_str(&format!(
                "<dl>{}</dl>",
                row_html(
                    "Containing block",
                    &format!("<a href=\"/block/{h}\">{}</a>", number(h))
                )
            ));
        }
        if let Some(t) = confirmed_block_time {
            body.push_str(&format!("<dl>{}</dl>", row("Block time", &view::time(t))));
        }
    } else if let Some(t) = entry.received_timestamp {
        body.push_str(&format!(
            "<dl>{}</dl>",
            row("Observed by node", &view::time(t))
        ));
    }
    if entry.double_spend_seen {
        body.push_str("<p class=warning>The node reports a conflicting spend. Do not treat this as a confirmed payment.</p>");
    }
    body.push_str("<p class=muted>The network fee is public. For RingCT transfers, payment amounts and recipient addresses are not public. A confirmation count alone does not prove this transaction paid you; verify in your wallet.</p></section>");
    body.push_str("<section class=panel><h2>Public transaction details</h2><details><summary>Inputs and ring-member indices</summary><p class=muted>Ring members are possible inputs, not an identification of the actual spent output.</p>");
    for (idx, input) in details.inputs.iter().enumerate().skip(page * 25).take(25) {
        let indices = input
            .ring_indices
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            body,
            "<article><h3>Input {idx}</h3><dl>{}{}{}</dl></article>",
            row("Key image", &input.key_image),
            row("Ring size", &input.ring_indices.len().to_string()),
            row("Absolute output indices", &indices)
        );
    }
    if details.coinbase {
        body.push_str("<p>Coinbase input: newly generated coins and collected fees.</p>");
    }
    body.push_str("</details><details><summary>Output keys and public amounts</summary>");
    for (idx, output) in details.outputs.iter().enumerate().skip(page * 25).take(25) {
        let amount = output
            .amount
            .map(|n| format!("{} XMR", xmr(n)))
            .unwrap_or_else(|| "Hidden (RingCT)".into());
        let _ = write!(
            body,
            "<article><h3>Output {idx}</h3><dl>{}{}{}</dl></article>",
            row("One-time output key", &output.key),
            row("Amount", &amount),
            row(
                "View tag",
                &output
                    .view_tag
                    .map(|v| format!("{v:02x}"))
                    .unwrap_or_else(|| "None".into())
            )
        );
        if let Some(index) = entry.output_indices.get(idx) {
            body.push_str(&format!(
                "<dl>{}</dl>",
                row("Global output index", &index.to_string())
            ));
        }
    }
    body.push_str("</details>");
    body.push_str(&view::pager(&format!("/tx/{id}"), page, max_rows));
    let extra_preview = &details.extra[..details.extra.len().min(4096)];
    let _ = write!(
        body,
        "<details><summary>Transaction extra (hex)</summary><pre>{}</pre>",
        escape(extra_preview)
    );
    if details.extra.len() > 4096 {
        body.push_str("<p>Preview limited to the first 2,048 bytes.</p>");
    }
    body.push_str("</details></section>");
    view::page("Transaction", body, Some(&s), StatusCode::OK)
}

async fn mempool(
    State(app): State<App>,
    query: std::result::Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let page = match query {
        Ok(Query(p)) => p.page,
        Err(_) => return fail(Error::InvalidInput, None),
    };
    let s = match app.rpc.snapshot().await {
        Ok(s) => s,
        Err(e) => return fail(e, None),
    };
    let hashes = match app.rpc.pool_hashes(&s).await {
        Ok(h) => h,
        Err(e) => return fail(e, Some(&s)),
    };
    let ids = match subset(&hashes, page) {
        Ok(ids) => ids,
        Err(e) => return fail(e, Some(&s)),
    };
    let entries = match app.rpc.pool_entries(&s, ids).await {
        Ok(entries) => entries,
        Err(e) => return fail(e, Some(&s)),
    };
    let mut body = format!("<section class=panel><p>{} transactions visible through this node's pool-hash endpoint. Sorted by ID, not by arrival time. The pool can change between requests.</p>", number(hashes.len() as u64));
    if hashes.is_empty() {
        body.push_str("<p>No transactions currently visible in this node's mempool.</p>");
    }
    body.push_str("<div class=table-wrap tabindex=\"0\" role=\"region\" aria-label=\"Mempool table\"><table><thead><tr><th scope=col>Transaction ID</th><th scope=col>Network fee</th><th scope=col>Weight</th><th scope=col>Observed state</th></tr></thead><tbody>");
    for id in ids {
        let mut fee = "Unavailable".to_string();
        let mut weight = "Unavailable".to_string();
        let state;
        if let Some(entry) = entries.iter().find(|e| &e.tx_hash == id) {
            let details = match decode_transaction(entry, id) {
                Ok(d) => d,
                Err(e) => return fail(e, Some(&s)),
            };
            if let Some(n) = details.fee {
                fee = format!("{} XMR", xmr(n));
            }
            if let Some(n) = details.weight {
                weight = format!("{n} WU");
            }
            state = if entry.double_spend_seen {
                "Conflicting spend reported"
            } else if entry.in_pool {
                "Pending"
            } else {
                "Now mined — open for details"
            };
        } else {
            state = "No longer returned by node";
        }
        let _ = write!(
            body,
            "<tr><td>{}</td><td class=nowrap>{}</td><td class=nowrap>{}</td><td>{}</td></tr>",
            tx_link(id),
            escape(&fee),
            escape(&weight),
            escape(state)
        );
    }
    body.push_str("</tbody></table></div>");
    body.push_str(&view::pager("/mempool", page, hashes.len()));
    body.push_str("</section><p class=muted>A node's mempool is a local observation, not a guarantee of network-wide propagation or future confirmation. Refresh manually to update.</p>");
    view::page("Mempool", body, Some(&s), StatusCode::OK)
}

#[derive(Deserialize)]
struct Search {
    q: String,
}
async fn search(
    State(app): State<App>,
    query: std::result::Result<Query<Search>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let q = match query {
        Ok(Query(q)) => q.q.trim().to_string(),
        Err(_) => return fail(Error::InvalidInput, None),
    };
    if q.len() == 64 {
        let id = match model::hash(&q) {
            Ok(id) => id,
            Err(e) => return fail(e, None),
        };
        let s = match app.rpc.snapshot().await {
            Ok(s) => s,
            Err(e) => return fail(e, None),
        };
        match app.rpc.transaction(&s, &id).await {
            Ok(Some(_)) => Redirect::to(&format!("/tx/{id}")).into_response(),
            Ok(None) => Redirect::to(&format!("/block/{id}")).into_response(),
            Err(e) => fail(e, Some(&s)),
        }
    } else {
        match model::height(&q) {
            Ok(height) => Redirect::to(&format!("/block/{height}")).into_response(),
            Err(e) => fail(e, None),
        }
    }
}

async fn about() -> Response {
    let body = concat!(
        "<section class=panel><h2>A simpler Monero explorer</h2><p>Server-rendered HTML and local CSS. No JavaScript, cookies, browser storage, analytics, external fonts, images, or automatic refresh. No wallet or login is required.</p>",
        "<p>The service queries one operator-configured Cuprate or monerod RPC endpoint. It does not mount a blockchain database, scan a wallet, or accept private view keys, spend keys, seed phrases, signed transactions, or transaction submission requests.</p>",
        "<h2>What an explorer cannot tell you</h2><p>Monero hides RingCT transfer amounts and recipients. Public output keys are not recipient addresses. Ring members are not a list of real senders. Check receipt of a payment in your own wallet.</p>",
        "<h2>Your searches are not anonymous by default</h2><p>The operator and hosting/network providers may observe your IP address and requested transaction IDs. Application access logging is disabled, but reverse proxies, hosting providers, and upstream nodes have their own policies. No-JavaScript does not mean no metadata. Use an operator you trust, or self-host. Use a separately configured Tor or I2P service when available; this deployment does not automatically provide either.</p>",
        "<h2>Freshness and failures</h2><p>Node observations and successful RPC responses are cached in memory for at most five seconds. Chain-dependent cache keys include the observed tip hash and height. Browser pages are not cached. Confirmed transaction pages verify that the containing block returned by the node includes the transaction and use that block's timestamp.</p>",
        "<p>Tip timestamps are checked against the server clock and an operator-configured age limit. Stale, unknown, or implausibly future tips trigger warnings and fail readiness, even if the node reports synchronized. This is an operational heuristic, not independent consensus verification or proof of the latest chain. Genuine long block intervals or clock errors can trigger warnings.</p>",
        "<p>Node failures produce an explicit unavailable message, never a fabricated zero balance, zero fee, or empty chain. Node synchronization is reported separately. All dates are UTC.</p>",
        "<h2>Software</h2><p>NexaWal Explorer 0.1.0. MIT-licensed source. Prototype: test against your deployed node before public exposure. No affiliation with xmrchain.net.</p></section>",
    );
    view::page("About & privacy", body.into(), None, StatusCode::OK)
}
async fn health() -> &'static str {
    "ok\n"
}
async fn ready(State(app): State<App>) -> Response {
    match app.rpc.snapshot().await {
        Ok(s) if s.ready() => (StatusCode::OK, "ready\n").into_response(),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            "node unavailable, syncing, or tip freshness unverified\n",
        )
            .into_response(),
    }
}
async fn styles() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        include_str!("../assets/style.css"),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn liveness_bypasses_exhausted_admission_budget() {
        let node = crate::fixture::Node::start(crate::fixture::Fake::default()).await;
        let app = App {
            rpc: Rpc::new(&node.url).unwrap(),
            requests: Arc::new(Semaphore::new(32)),
        };
        let _all_permits = app.requests.clone().acquire_many_owned(32).await.unwrap();
        let router = routes(app);
        for path in ["/healthz", "/healthz?probe=1"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
        }
        for path in ["/", "/readyz", "/healthz/extra"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
        assert!(
            node.state.lock().unwrap().hits.is_empty(),
            "liveness must not hit RPC"
        );
    }
}
