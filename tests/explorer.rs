mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use common::*;
use http_body_util::BodyExt;
use monero_oxide::transaction::{Input, Pruned, Transaction};
use nexawal_explorer::{
    model::{self, decode_transaction, Error, Info, TxEntry},
    rpc::Rpc,
    view, web,
};
use serde_json::json;
use tower::ServiceExt;

async fn request(app: Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

#[test]
fn identifiers_are_strict_and_normalized() {
    assert_eq!(model::hash(&"A".repeat(64)).unwrap(), "a".repeat(64));
    for bad in [
        "<script>",
        "https://x/tx/foo",
        "1e6",
        "-1",
        "+1",
        "18446744073709551616",
    ] {
        assert!(model::height(bad).is_err());
    }
    assert!(model::hash(&"g".repeat(64)).is_err());
    assert_eq!(model::height("00042").unwrap(), 42);
}

#[tokio::test]
async fn about_omits_unnecessary_affiliation_disclaimer() {
    let app = web::router(Rpc::new("http://127.0.0.1:1").unwrap());
    let (status, _, body) = request(app, "/about").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("MIT-licensed source"));
    assert!(!body.contains("No affiliation with xmrchain.net."));
}

#[test]
fn pruned_ringct_hash_and_fee_are_verified() {
    let entry: TxEntry = serde_json::from_value(tx()).unwrap();
    let d = decode_transaction(&entry, TXID).unwrap();
    assert_eq!(d.fee, Some(44_540_000));
    assert_eq!(d.version, 2);
    assert!(!d.coinbase);
    assert_eq!(d.inputs.len(), 2);
    assert_eq!(d.inputs[0].ring_indices.len(), 16);
    assert_eq!(d.outputs.len(), 2);
    assert!(d.outputs.iter().all(|o| o.amount.is_none()));
    assert!(d.weight.is_none());
    assert!(d.full_size.is_none());
}

#[test]
fn incorrect_ids_trailing_and_malformed_data_fail_closed() {
    let mut entry: TxEntry = serde_json::from_value(tx()).unwrap();
    assert!(decode_transaction(&entry, &"0".repeat(64)).is_err());
    entry.pruned_as_hex.push_str("00");
    assert!(decode_transaction(&entry, TXID).is_err());
    entry.pruned_as_hex = "00".into();
    assert!(decode_transaction(&entry, TXID).is_err());
}

#[test]
fn full_legacy_coinbase_hash_and_zero_fee() {
    let entry: TxEntry = serde_json::from_value(tx()).unwrap();
    let bytes = hex::decode(&entry.pruned_as_hex).unwrap();
    let parsed = Transaction::<Pruned>::read(&mut bytes.as_slice()).unwrap();
    let mut prefix = parsed.prefix().clone();
    prefix.inputs = vec![Input::Gen(0)];
    prefix.outputs[0].amount = Some(600_000_000_000);
    prefix.outputs.truncate(1);
    let tx = Transaction::V1 {
        prefix,
        signatures: vec![],
    };
    let id = hex::encode(tx.hash());
    let mut e = entry.clone();
    e.tx_hash = id.clone();
    e.as_hex = hex::encode(tx.serialize());
    let details = decode_transaction(&e, &id).unwrap();
    assert_eq!(details.fee, Some(0));
    assert!(details.coinbase);
    assert_eq!(details.outputs[0].amount, Some(600_000_000_000));
    assert!(details.full_size.is_some());
}

#[test]
fn zero_target_height_is_not_a_sync_failure() {
    let info: Info = serde_json::from_value(json!({"height":3786500,"target_height":0,"synchronized":true,"nettype":"mainnet","difficulty":0,"wide_difficulty":"0x10000000000000000","target":120,"tx_pool_size":0})).unwrap();
    assert!(info.ready());
    assert_eq!(info.difficulty(), Some(1u128 << 64));
}

#[test]
fn legacy_fee_is_public_inputs_minus_outputs() {
    let mut entry: TxEntry = serde_json::from_value(tx()).unwrap();
    let bytes = hex::decode(&entry.pruned_as_hex).unwrap();
    let parsed = Transaction::<Pruned>::read(&mut bytes.as_slice()).unwrap();
    let mut prefix = parsed.prefix().clone();
    prefix.inputs.truncate(1);
    if let Input::ToKey {
        amount,
        key_offsets,
        ..
    } = &mut prefix.inputs[0]
    {
        *amount = Some(100);
        *key_offsets = vec![0];
    }
    prefix.outputs.truncate(1);
    prefix.outputs[0].amount = Some(70);
    let prefix_only = Transaction::<Pruned>::V1 {
        prefix,
        signatures: (),
    };
    let mut bytes = prefix_only.serialize();
    // Syntactic ring-size-one fixture; this explorer parses, not consensus-verifies signatures.
    bytes.extend_from_slice(&[0u8; 64]);
    let full = Transaction::read(&mut bytes.as_slice()).unwrap();
    let id = hex::encode(full.hash());
    entry.tx_hash = id.clone();
    entry.as_hex = hex::encode(bytes);
    assert_eq!(decode_transaction(&entry, &id).unwrap().fee, Some(30));
}

#[tokio::test]
async fn every_page_is_scriptless_and_has_privacy_headers() {
    let node = Node::start(Fake::default()).await;
    let app = web::router(Rpc::new(&node.url).unwrap());
    for path in [
        "/",
        "/blocks",
        "/block/8",
        &format!("/tx/{TXID}"),
        "/mempool",
        "/about",
    ] {
        let (status, headers, body) = request(app.clone(), path).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert!(!body.contains("<script"));
        assert!(!body.contains("onclick"));
        assert!(!body.contains("<img"));
        assert!(!body.contains("https://"));
        assert_eq!(headers["referrer-policy"], "no-referrer");
        assert_eq!(headers["cache-control"], "no-store");
        assert!(headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("script-src 'none'"));
        assert!(!headers.contains_key("set-cookie"));
    }
}

#[tokio::test]
async fn confirmed_tx_checks_membership_and_inclusive_confirmations() {
    let node = Node::start(Fake::default()).await;
    let app = web::router(Rpc::new(&node.url).unwrap());
    let (status, _, body) = request(app, &format!("/tx/{TXID}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("2 confirmations"));
    assert!(body.contains("0.000044540000 XMR"));
    assert!(body.contains("Hidden (RingCT)"));
}

#[tokio::test]
async fn confirmed_time_comes_from_containing_block_not_duplicate_metadata() {
    let node = Node::start(Fake {
        transaction_timestamp: Some(0),
        block_timestamp: Some(1_792_305_650),
        ..Fake::default()
    })
    .await;
    let (status, _, body) = request(
        web::router(Rpc::new(&node.url).unwrap()),
        &format!("/tx/{TXID}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(&view::time(1_792_305_650)));
    assert!(!body.contains("1970-01-01"));
}

#[tokio::test]
async fn freshness_boundaries_and_configuration_are_explicit() {
    let node = Node::start(Fake::default()).await;
    for seconds in [0, 119, 86_401] {
        assert!(Rpc::with_max_tip_age(&node.url, seconds).is_err());
    }
    for seconds in [120, 3600, 86_400] {
        assert!(Rpc::with_max_tip_age(&node.url, seconds).is_ok());
    }
    let mut s = Rpc::new(&node.url).unwrap().snapshot().await.unwrap();
    assert_eq!(s.max_tip_age_secs, 3600);
    s.tip.timestamp = 10_000;
    assert!(s.ready_at(13_600));
    assert!(!s.ready_at(13_601));
    assert!(s.ready_at(9_400));
    assert!(!s.ready_at(9_399));
    assert!(!s.ready_at(0));
    s.tip.timestamp = 0;
    assert!(!s.ready_at(10_000));
}

#[tokio::test]
async fn stale_unknown_and_future_tips_fail_readiness_without_false_not_found() {
    let now = model::unix_time();
    for (timestamp, warning) in [
        (0, "Tip freshness unknown"),
        (now - 7200, "Node tip is stale"),
        (now + 7200, "Tip timestamp is in the future"),
    ] {
        let node = Node::start(Fake {
            tip_timestamp: Some(timestamp),
            missing: true,
            ..Fake::default()
        })
        .await;
        let app = web::router(Rpc::new(&node.url).unwrap());
        assert_eq!(
            request(app.clone(), "/readyz").await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        let (status, _, body) = request(app.clone(), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(warning));
        assert!(!body.contains("Node reports synchronized"));
        for path in [format!("/tx/{TXID}"), "/block/10".into()] {
            assert_eq!(
                request(app.clone(), &path).await.0,
                StatusCode::SERVICE_UNAVAILABLE
            );
        }
        assert_eq!(request(app, "/healthz").await.0, StatusCode::OK);
    }
}

#[tokio::test]
async fn pending_is_not_reported_as_confirmed() {
    let node = Node::start(Fake {
        pool: true,
        ..Fake::default()
    })
    .await;
    let (status, _, body) = request(
        web::router(Rpc::new(&node.url).unwrap()),
        &format!("/tx/{TXID}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Pending — observed"));
    assert!(!body.contains("Containing block"));
}

#[tokio::test]
async fn missing_during_sync_is_unavailable_not_absent() {
    for syncing in [false, true] {
        let node = Node::start(Fake {
            missing: true,
            syncing,
            ..Fake::default()
        })
        .await;
        let (status, _, body) = request(
            web::router(Rpc::new(&node.url).unwrap()),
            &format!("/tx/{TXID}"),
        )
        .await;
        assert_eq!(
            status,
            if syncing {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::NOT_FOUND
            }
        );
        if syncing {
            assert!(body.contains("still synchronizing"));
        }
    }
}

#[tokio::test]
async fn inconsistency_never_becomes_success_or_an_empty_chain() {
    for fake in [
        Fake {
            no_membership: true,
            ..Fake::default()
        },
        Fake {
            wrong_blob: true,
            ..Fake::default()
        },
        Fake {
            incomplete: true,
            ..Fake::default()
        },
    ] {
        let node = Node::start(fake).await;
        let (status, headers, body) = request(
            web::router(Rpc::new(&node.url).unwrap()),
            &format!("/tx/{TXID}"),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(headers["retry-after"], "5");
        assert!(body.contains("could not be verified"));
    }
}

#[tokio::test]
async fn down_malformed_and_oversized_upstreams_fail_visibly() {
    for fake in [
        Fake {
            down: true,
            ..Fake::default()
        },
        Fake {
            malformed: true,
            ..Fake::default()
        },
        Fake {
            oversized: true,
            ..Fake::default()
        },
    ] {
        let node = Node::start(fake).await;
        let (status, _, body) = request(web::router(Rpc::new(&node.url).unwrap()), "/").await;
        assert!(status.is_server_error());
        assert!(!body.contains("No transactions currently visible"));
    }
}

#[tokio::test]
async fn search_and_validation_do_not_become_a_public_rpc_proxy() {
    let node = Node::start(Fake::default()).await;
    let app = web::router(Rpc::new(&node.url).unwrap());
    let (status, headers, _) = request(app.clone(), "/search?q=8").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers["location"], "/block/8");
    let (status, headers, _) = request(app.clone(), &format!("/search?q={TXID}")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers["location"], format!("/tx/{TXID}"));
    for path in [
        "/search?q=%3Cscript%3E",
        "/block/-1",
        "/blocks?page=18446744073709551615",
        "/mempool?page=x",
        "/tx/nope",
        "/search?q=https%3A%2F%2Fexample.com",
    ] {
        let (status, _, body) = request(app.clone(), path).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
        assert!(!body.contains("<script>"));
    }
    for path in [
        "/json_rpc",
        "/send_raw_transaction",
        "/wallet",
        "/api?url=http://localhost",
    ] {
        assert_eq!(request(app.clone(), path).await.0, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn pagination_and_health_are_bounded() {
    let node = Node::start(Fake::default()).await;
    let app = web::router(Rpc::new(&node.url).unwrap());
    assert_eq!(
        request(app.clone(), "/blocks?page=2").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(app.clone(), "/block/10").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(app.clone(), "/mempool?page=1").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(request(app.clone(), "/healthz").await.0, StatusCode::OK);
    assert_eq!(request(app.clone(), "/readyz").await.0, StatusCode::OK);
    let down = Node::start(Fake {
        down: true,
        ..Fake::default()
    })
    .await;
    let app = web::router(Rpc::new(&down.url).unwrap());
    assert_eq!(request(app.clone(), "/healthz").await.0, StatusCode::OK);
    assert_eq!(
        request(app, "/readyz").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn empty_and_unsupported_mempools_are_distinct() {
    let node = Node::start(Fake {
        empty_pool: true,
        ..Fake::default()
    })
    .await;
    let (status, _, body) = request(web::router(Rpc::new(&node.url).unwrap()), "/mempool").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("No transactions currently visible"));
    let node = Node::start(Fake {
        no_pool: true,
        ..Fake::default()
    })
    .await;
    assert_eq!(
        request(web::router(Rpc::new(&node.url).unwrap()), "/mempool")
            .await
            .0,
        StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn identical_requests_coalesce() {
    let node = Node::start(Fake::default()).await;
    let rpc = Rpc::new(&node.url).unwrap();
    let s = rpc.snapshot().await.unwrap();
    let mut jobs = Vec::new();
    for _ in 0..20 {
        let rpc = rpc.clone();
        let s = s.clone();
        jobs.push(tokio::spawn(async move {
            rpc.transaction(&s, TXID).await.unwrap()
        }));
    }
    for job in jobs {
        assert!(job.await.unwrap().is_some());
    }
    assert_eq!(node.state.lock().unwrap().hits["/get_transactions"], 1);
}

#[tokio::test]
async fn same_height_reorg_uses_new_cache_namespace() {
    let node = Node::start(Fake::default()).await;
    let rpc = Rpc::new(&node.url).unwrap();
    let s = rpc.snapshot().await.unwrap();
    let old = rpc.block(&s, "8").await.unwrap();
    let mut next = s.clone();
    // Model the next observed snapshot at the same height, with a replacement tip.
    next.tip.hash = block_hash(9, true);
    node.state.lock().unwrap().reorg = true;
    let new = rpc.block(&next, "8").await.unwrap();
    assert_ne!(old.block_header.hash, new.block_header.hash);
    assert_eq!(node.state.lock().unwrap().hits["get_block"], 2);
    assert_eq!(next.confirmations(8).unwrap(), 2);
    assert_eq!(next.confirmations(10), Err(Error::Inconsistent));
}

#[test]
fn html_escaping_is_not_optional() {
    let raw = "<script>alert('x')</script>\"&";
    let html = view::row("Label", raw);
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
}

#[test]
fn cuprate_miner_only_blocks_can_omit_empty_hashes() {
    let mut h = header(0, false);
    h["num_txes"] = json!(0);
    let block: model::Block =
        serde_json::from_value(json!({"block_header":h,"miner_tx_hash":"d".repeat(64)})).unwrap();
    block.validate().unwrap();
    assert!(block.tx_hashes.is_empty());
    let mut invalid = block;
    invalid.block_header.num_txes = 1;
    assert!(invalid.validate().is_err());
}

#[tokio::test]
async fn busy_blocks_render_only_a_bounded_page() {
    let node = Node::start(Fake {
        block_tx_count: 1000,
        ..Fake::default()
    })
    .await;
    let app = web::router(Rpc::new(&node.url).unwrap());
    let (status, _, body) = request(app.clone(), "/block/8?page=0").await;
    assert_eq!(status, StatusCode::OK);
    // One miner-transaction link and at most 25 normal transactions.
    assert_eq!(body.matches("href=\"/tx/").count(), 26);
    assert!(body.contains("Page 1 of 40"));
    let (status, _, body) = request(app, "/block/8?page=39").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Page 40 of 40"));
    assert_eq!(body.matches("href=\"/tx/").count(), 26);
}
