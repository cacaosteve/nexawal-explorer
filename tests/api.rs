mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::*;
use http_body_util::BodyExt;
use nexawal_explorer::{rpc::Rpc, web};
use serde_json::Value;
use tower::ServiceExt;

async fn lookup(fake: Fake, path: &str) -> (StatusCode, Value) {
    let node = Node::start(fake).await;
    let response = web::router(Rpc::new(&node.url).unwrap())
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert!(!response
        .headers()
        .contains_key("access-control-allow-origin"));
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn summary_is_exact_verified_and_contains_no_wallet_data() {
    let (status, body) = lookup(Fake::default(), &format!("/api/v1/transactions/{TXID}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["api_version"], 1);
    assert_eq!(body["txid"], TXID);
    assert_eq!(body["network"], "mainnet");
    assert_eq!(body["status"], "confirmed");
    assert_eq!(body["fee_piconero"], "44540000");
    assert_eq!(body["confirmations"], 2);
    assert_eq!(body["block_height"], 8);
    assert_eq!(body["node_ready"], true);
    assert!(body.get("amount").is_none());
    assert!(body.get("address").is_none());
    assert!(body.get("inputs").is_none());
}

#[tokio::test]
async fn pending_and_stale_observations_are_explicit() {
    let (_, body) = lookup(
        Fake {
            pool: true,
            ..Fake::default()
        },
        &format!("/api/v1/transactions/{TXID}"),
    )
    .await;
    assert_eq!(body["status"], "pending");
    assert!(body["block_height"].is_null());
    assert!(body["block_hash"].is_null());
    assert_eq!(body["confirmations"], 0);
    let (_, body) = lookup(
        Fake {
            tip_timestamp: Some(1),
            ..Fake::default()
        },
        &format!("/api/v1/transactions/{TXID}"),
    )
    .await;
    assert_eq!(body["node_ready"], false);
}

#[tokio::test]
async fn failures_never_become_missing_or_zero_observations() {
    for (fake, expected) in [
        (
            Fake {
                missing: true,
                ..Fake::default()
            },
            StatusCode::NOT_FOUND,
        ),
        (
            Fake {
                missing: true,
                syncing: true,
                ..Fake::default()
            },
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            Fake {
                down: true,
                ..Fake::default()
            },
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            Fake {
                no_membership: true,
                ..Fake::default()
            },
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            Fake {
                wrong_blob: true,
                ..Fake::default()
            },
            StatusCode::SERVICE_UNAVAILABLE,
        ),
    ] {
        let (status, body) = lookup(fake, &format!("/api/v1/transactions/{TXID}")).await;
        assert_eq!(status, expected, "{body}");
        assert!(body["error"]["code"].is_string());
        assert!(body.get("fee_piconero").is_none());
    }
    for path in [
        "/api/v1/transactions/bad",
        "/api/unknown",
        &format!("/api/v1/transactions/{TXID}?url=http://other"),
    ] {
        let (status, _) = lookup(Fake::default(), path).await;
        assert!(status.is_client_error());
    }
}
