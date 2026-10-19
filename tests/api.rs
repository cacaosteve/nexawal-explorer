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

#[tokio::test]
async fn advanced_endpoint_keeps_summary_verification_and_explicit_conflict() {
    let (status, body) = lookup(
        Fake {
            conflict: true,
            ..Fake::default()
        },
        &format!("/api/v1/transactions/{TXID}/details/0"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["double_spend_seen"], true);
    assert_eq!(body["fee_piconero"], "44540000");
    let d = &body["details"];
    assert_eq!(d["version"], 2);
    assert_eq!(d["page"], 0);
    assert_eq!(d["page_size"], 10);
    assert_eq!(d["input_count"], 2);
    assert_eq!(d["output_count"], 2);
    assert!(d["serialized_size_bytes"].is_null() && d["weight"].is_null());
    assert_eq!(d["inputs"][0]["ring_size"], 16);
    assert_eq!(
        d["inputs"][0]["absolute_indices"].as_array().unwrap().len(),
        16
    );
    assert!(d["outputs"][0]["public_amount_piconero"].is_null());
    assert!(d["outputs"][0]["global_index"].is_string());
    assert!(serde_json::to_vec(&body).unwrap().len() < 65_536);
    // Shared with all native parsers; detect serializer/consumer field drift.
    let contract: Value =
        serde_json::from_str(include_str!("fixtures/client_advanced_page0.json")).unwrap();
    for (actual, expected) in [
        (&body, &contract),
        (&body["details"], &contract["details"]),
        (
            &body["details"]["inputs"][0],
            &contract["details"]["inputs"][0],
        ),
        (
            &body["details"]["outputs"][0],
            &contract["details"]["outputs"][0],
        ),
    ] {
        assert_eq!(
            actual.as_object().unwrap().keys().collect::<Vec<_>>(),
            expected.as_object().unwrap().keys().collect::<Vec<_>>()
        );
    }
    let (_, summary) = lookup(
        Fake {
            conflict: true,
            ..Fake::default()
        },
        &format!("/api/v1/transactions/{TXID}"),
    )
    .await;
    assert_eq!(summary["double_spend_seen"], true);
    assert!(summary.get("details").is_none());
    for suffix in ["1", "410", "-1", "999999999999999999999999", "0?anything=1"] {
        let (status, _) = lookup(
            Fake::default(),
            &format!("/api/v1/transactions/{TXID}/details/{suffix}"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    for fake in [
        Fake {
            wrong_blob: true,
            ..Fake::default()
        },
        Fake {
            no_membership: true,
            ..Fake::default()
        },
    ] {
        let (status, body) = lookup(fake, &format!("/api/v1/transactions/{TXID}/details/0")).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.get("details").is_none());
    }
}
