use crate::model::{unix_time, Header, Snapshot, TipFreshness};
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use std::fmt::Write;

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
pub fn xmr(amount: u64) -> String {
    format!(
        "{}.{:012}",
        amount / 1_000_000_000_000,
        amount % 1_000_000_000_000
    )
}
pub fn time(timestamp: u64) -> String {
    i64::try_from(timestamp)
        .ok()
        .and_then(|t| DateTime::<Utc>::from_timestamp(t, 0))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Unavailable".into())
}
pub fn number(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (idx, c) in s.chars().enumerate() {
        if idx > 0 && (s.len() - idx).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub fn hashrate(n: u128) -> String {
    for (scale, unit) in [
        (1_000_000_000_000u128, "TH/s"),
        (1_000_000_000, "GH/s"),
        (1_000_000, "MH/s"),
        (1_000, "kH/s"),
    ] {
        if n >= scale {
            return format!("{}.{:02} {unit}", n / scale, (n % scale) * 100 / scale);
        }
    }
    format!("{n} H/s")
}
pub fn tx_link(txid: &str) -> String {
    // IDs are validated at the RPC boundary; escaping remains defense in depth.
    let id = escape(txid);
    format!("<a class=hash href=\"/tx/{id}\">{id}</a>")
}
pub fn row(label: &str, value: &str) -> String {
    format!(
        "<div class=fact><dt>{}</dt><dd>{}</dd></div>",
        escape(label),
        escape(value)
    )
}
pub fn row_html(label: &str, value: &str) -> String {
    format!(
        "<div class=fact><dt>{}</dt><dd>{value}</dd></div>",
        escape(label)
    )
}
pub fn node_banner(s: &Snapshot) -> String {
    let now = unix_time();
    let freshness = s.freshness_at(now);
    let status = if s.info.offline {
        "Node is offline"
    } else if freshness == TipFreshness::Unknown {
        "Tip freshness unknown — results may be incomplete"
    } else if freshness == TipFreshness::Future {
        "Tip timestamp is in the future — check node and server clocks"
    } else if freshness == TipFreshness::Stale {
        "Node tip is stale — results may be incomplete"
    } else if s.info.ready() {
        "Node reports synchronized"
    } else {
        "Node is still synchronizing — results may be incomplete"
    };
    let class = if s.ready_at(now) {
        "node"
    } else {
        "node warning"
    };
    let tip_time = if s.tip.timestamp == 0 {
        "Unavailable".into()
    } else {
        time(s.tip.timestamp)
    };
    format!("<aside class=\"{class}\"><strong>{}</strong><span>{} · tip block {} · tip time {} · observed {}</span></aside>", escape(status), escape(&s.info.nettype), number(s.tip.height), tip_time, time(s.observed_at))
}

pub fn page(
    title: &str,
    body: String,
    snapshot: Option<&Snapshot>,
    status: StatusCode,
) -> Response {
    let banner = snapshot.map(node_banner).unwrap_or_default();
    let title = escape(title);
    let html = format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="dark light"><meta name="robots" content="noindex,nofollow"><title>{title} · NexaWal Explorer</title><link rel="stylesheet" href="/style.css"></head>
<body><a class="skip" href="#content">Skip to content</a><header><a class="brand" href="/">NEXAWAL <span>EXPLORER</span></a><nav aria-label="Main"><a href="/blocks">Blocks</a><a href="/mempool">Mempool</a><a href="/about">About &amp; privacy</a></nav></header>
<main id="content"><form class="search" action="/search" method="get" role="search"><label for="q">Find a transaction or block</label><div><input id="q" name="q" type="search" maxlength="80" placeholder="Transaction ID, block hash, or height" required autocomplete="off" autocapitalize="none" spellcheck="false"><button type="submit">Search</button></div></form>
{banner}<h1>{title}</h1>{body}</main><footer>No JavaScript. No cookies. No analytics. No external assets.<br>Public blockchain data only. This explorer cannot show private balances, recipients, or RingCT payment amounts.</footer></body></html>"##
    );
    (status, Html(html)).into_response()
}

pub fn headers_table(headers: &[Header]) -> String {
    let mut html = String::from("<div class=table-wrap tabindex=\"0\" role=\"region\" aria-label=\"Blocks table\"><table><thead><tr><th scope=col>Height</th><th scope=col>Time (UTC)</th><th scope=col>Transactions</th><th scope=col>Weight</th><th scope=col>Block hash</th></tr></thead><tbody>");
    for h in headers.iter().rev() {
        let _ = write!(html, "<tr><td><a href=\"/block/{}\">{}</a></td><td class=nowrap>{}</td><td>{}</td><td>{} B</td><td><a class=hash href=\"/block/{}\">{}</a></td></tr>", h.height, number(h.height), time(h.timestamp), number(h.num_txes), number(h.block_weight), escape(&h.hash), escape(&h.hash));
    }
    html.push_str("</tbody></table></div>");
    html
}

pub fn pager(path: &str, current: usize, total: usize) -> String {
    let pages = total.max(1).div_ceil(25);
    let mut html = String::from("<nav class=pager aria-label=\"Pagination\">");
    if current > 0 {
        let _ = write!(
            html,
            "<a href=\"{path}?page={}\">← Previous</a>",
            current - 1
        );
    }
    let _ = write!(html, "<span>Page {} of {pages}</span>", current + 1);
    if current + 1 < pages {
        let _ = write!(html, "<a href=\"{path}?page={}\">Next →</a>", current + 1);
    }
    html.push_str("</nav>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaping_covers_attributes_and_text() {
        assert_eq!(
            escape("<script a='x'>&\""),
            "&lt;script a=&#39;x&#39;&gt;&amp;&quot;"
        );
    }
    #[test]
    fn money_is_integer_exact() {
        assert_eq!(xmr(232640000), "0.000232640000");
        assert_eq!(xmr(u64::MAX), "18446744.073709551615");
    }
    #[test]
    fn invalid_times_are_not_epoch_zero() {
        assert_eq!(time(u64::MAX), "Unavailable");
    }
    #[test]
    fn grouping_is_readable() {
        assert_eq!(number(3786560), "3,786,560");
    }
}
