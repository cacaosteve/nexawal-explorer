//! Bounded, read-only public transaction observations. Never accepts wallet material.
use crate::{
    model::{self, decode_transaction, Error},
    rpc::Rpc,
};
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TransactionObservation {
    pub api_version: u8,
    pub network: String,
    pub txid: String,
    pub status: &'static str,
    /// Decimal string: never a floating-point amount, never a recipient payment amount.
    pub fee_piconero: Option<String>,
    pub block_height: Option<u64>,
    pub block_hash: Option<String>,
    pub block_timestamp: Option<u64>,
    pub confirmations: u64,
    pub chain_height: u64,
    pub tip_hash: String,
    pub node_ready: bool,
    pub observed_at: u64,
}

pub async fn transaction(rpc: &Rpc, raw: &str) -> model::Result<TransactionObservation> {
    let id = model::hash(raw)?;
    let s = rpc.snapshot().await?;
    let entry = rpc.transaction(&s, &id).await?.ok_or_else(|| {
        if s.ready() {
            Error::NotFound
        } else {
            Error::Unavailable
        }
    })?;
    let details = decode_transaction(&entry, &id)?;
    let (block_height, block_hash, block_timestamp, confirmations) = if entry.in_pool {
        (None, None, None, 0)
    } else {
        let height = entry.block_height.ok_or(Error::Inconsistent)?;
        let block = rpc.block(&s, &height.to_string()).await?;
        if block.block_header.orphan_status || !block.contains(&id) {
            return Err(Error::Inconsistent);
        }
        (
            Some(height),
            Some(block.block_header.hash),
            Some(block.block_header.timestamp),
            s.confirmations(height)?,
        )
    };
    Ok(TransactionObservation {
        api_version: 1,
        network: s.info.nettype.clone(),
        txid: id,
        status: if entry.in_pool {
            "pending"
        } else {
            "confirmed"
        },
        fee_piconero: details.fee.map(|f| f.to_string()),
        block_height,
        block_hash,
        block_timestamp,
        confirmations,
        chain_height: s.info.height,
        tip_hash: s.tip.hash.clone(),
        node_ready: s.ready(),
        observed_at: s.observed_at,
    })
}

pub fn fail(error: Error) -> Response {
    let (status, code) = match error {
        Error::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_request"),
        Error::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        Error::Unsupported => (StatusCode::BAD_GATEWAY, "unsupported"),
        Error::Inconsistent => (StatusCode::SERVICE_UNAVAILABLE, "inconsistent"),
        Error::Busy => (StatusCode::SERVICE_UNAVAILABLE, "busy"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    };
    let mut response = (
        status,
        Json(serde_json::json!({"api_version":1,"error":{"code":code}})),
    )
        .into_response();
    if status == StatusCode::SERVICE_UNAVAILABLE {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
    }
    response
}
