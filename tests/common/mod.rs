#![allow(dead_code)]
use axum::{
    extract::State,
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub const TXID: &str = "8c2e05ba45eb3abae801ffa7c00af30d0e2776678e42f0660fd11e56ba40c71a";
pub fn tx() -> Value {
    serde_json::from_str(include_str!("../fixtures/transaction.json")).unwrap()
}
pub fn block_hash(h: u64, reorg: bool) -> String {
    format!("{:064x}", h + if reorg { 500 } else { 100 })
}
pub fn header(h: u64, reorg: bool) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"hash":block_hash(h,reorg),"height":h,"timestamp":now,"prev_hash":block_hash(h.saturating_sub(1),reorg),"num_txes":1,"reward":600044540000u64,"block_size":1500,"block_weight":1600,"difficulty":100000,"major_version":16,"orphan_status":false})
}
#[derive(Default)]
pub struct Fake {
    pub down: bool,
    pub syncing: bool,
    pub reorg: bool,
    pub missing: bool,
    pub pool: bool,
    pub wrong_blob: bool,
    pub no_membership: bool,
    pub incomplete: bool,
    pub empty_pool: bool,
    pub no_pool: bool,
    pub malformed: bool,
    pub oversized: bool,
    pub block_tx_count: usize,
    pub tip_timestamp: Option<u64>,
    pub transaction_timestamp: Option<u64>,
    pub block_timestamp: Option<u64>,
    pub hits: HashMap<String, usize>,
}
pub struct Node {
    pub url: String,
    pub state: Arc<Mutex<Fake>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Node {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Node {
    pub async fn start(fake: Fake) -> Self {
        let state = Arc::new(Mutex::new(fake));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/{*path}", post(handle))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { url, state, task }
    }
}
async fn handle(
    State(state): State<Arc<Mutex<Fake>>>,
    uri: Uri,
    Json(body): Json<Value>,
) -> Response {
    let mut f = state.lock().unwrap();
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or(uri.path())
        .to_string();
    *f.hits.entry(method.clone()).or_default() += 1;
    if f.down {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    if f.malformed {
        return (StatusCode::OK, "not json").into_response();
    }
    if f.oversized {
        return (StatusCode::OK, "x".repeat(4 * 1024 * 1024 + 1)).into_response();
    }
    let mut data = match method.as_str() {
        "get_info" => {
            json!({"status":"OK","height":10,"target_height":0,"synchronized":!f.syncing,"busy_syncing":f.syncing,"offline":false,"nettype":"mainnet","difficulty":100000,"target":120,"tx_pool_size":1})
        }
        "get_block_header_by_height" => {
            let mut h = header(body["params"]["height"].as_u64().unwrap(), f.reorg);
            if let Some(t) = f.tip_timestamp {
                h["timestamp"] = json!(t);
            }
            json!({"status":"OK","block_header":h})
        }
        "get_block_headers_range" => {
            let first = body["params"]["start_height"].as_u64().unwrap();
            let end = body["params"]["end_height"].as_u64().unwrap();
            json!({"status":"OK","headers":(first..=end).map(|h| header(h,f.reorg)).collect::<Vec<_>>()})
        }
        "get_block" => {
            let h = body["params"]["height"].as_u64().unwrap_or(8);
            let mut hashes = vec![if f.no_membership {
                "e".repeat(64)
            } else {
                TXID.to_string()
            }];
            for i in 1..f.block_tx_count {
                hashes.push(format!("{:064x}", 10_000 + i));
            }
            let mut h = header(h, f.reorg);
            h["num_txes"] = json!(hashes.len());
            if let Some(t) = f.block_timestamp {
                h["timestamp"] = json!(t);
            }
            json!({"status":"OK","block_header":h,"miner_tx_hash":"d".repeat(64),"tx_hashes":hashes})
        }
        "/get_transactions" => {
            // Make a regression to Cuprate's unsupported JSON decoder fail immediately.
            assert_eq!(body["decode_as_json"], false);
            assert!(body["txs_hashes"].as_array().unwrap().len() <= 25);
            if f.incomplete {
                json!({"status":"OK","txs":[]})
            } else if f.missing {
                // Cuprate skips the empty txs array entirely.
                json!({"status":"OK","missed_tx":body["txs_hashes"]})
            } else {
                let mut tx = tx();
                tx["in_pool"] = json!(f.pool);
                if let Some(t) = f.transaction_timestamp {
                    tx["block_timestamp"] = json!(t);
                }
                if f.wrong_blob {
                    tx["pruned_as_hex"] = json!("0000");
                }
                json!({"status":"OK","txs":[tx],"missed_tx":[]})
            }
        }
        "/get_transaction_pool_hashes" => {
            if f.no_pool {
                return StatusCode::NOT_FOUND.into_response();
            }
            json!({"status":"OK","tx_hashes":if f.empty_pool { vec![] } else { vec![TXID] }})
        }
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if uri.path() == "/json_rpc" {
        data = json!({"jsonrpc":"2.0","id":"explorer","result":data});
    }
    Json(data).into_response()
}
