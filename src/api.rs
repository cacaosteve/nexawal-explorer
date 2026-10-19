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
    pub double_spend_seen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<AdvancedDetails>,
}

pub const PAGE_SIZE: usize = 10;
pub const MAX_INDICES: usize = 128;

#[derive(Debug, Serialize)]
pub struct AdvancedInput {
    pub index: usize,
    pub key_image: String,
    pub ring_size: usize,
    pub absolute_indices: Vec<String>,
    pub indices_truncated: bool,
}
#[derive(Debug, Serialize)]
pub struct AdvancedOutput {
    pub index: usize,
    pub key: String,
    pub public_amount_piconero: Option<String>,
    pub view_tag: Option<u8>,
    pub global_index: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct AdvancedDetails {
    pub page: usize,
    pub page_size: usize,
    pub page_count: usize,
    pub version: u8,
    pub coinbase: bool,
    pub serialized_size_bytes: Option<usize>,
    pub downloaded_size_bytes: usize,
    pub weight: Option<usize>,
    pub unlock_kind: &'static str,
    pub unlock_value: String,
    pub input_count: usize,
    pub output_count: usize,
    pub inputs: Vec<AdvancedInput>,
    pub outputs: Vec<AdvancedOutput>,
    pub extra_hex: String,
    pub extra_truncated: bool,
    pub received_timestamp: Option<u64>,
}

pub async fn transaction(rpc: &Rpc, raw: &str) -> model::Result<TransactionObservation> {
    transaction_page(rpc, raw, None).await
}

pub async fn transaction_page(
    rpc: &Rpc,
    raw: &str,
    page: Option<usize>,
) -> model::Result<TransactionObservation> {
    if page.is_some_and(|p| p >= 410) {
        return Err(Error::InvalidInput);
    }
    let id = model::hash(raw)?;
    let s = rpc.snapshot().await?;
    if !matches!(s.info.nettype.as_str(), "mainnet" | "testnet" | "stagenet") {
        return Err(Error::Inconsistent);
    }
    let entry = rpc.transaction(&s, &id).await?.ok_or_else(|| {
        if s.ready() {
            Error::NotFound
        } else {
            Error::Unavailable
        }
    })?;
    let details = decode_transaction(&entry, &id)?;
    let advanced = page
        .map(|page| advanced_page(&details, &entry, page))
        .transpose()?;
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
        double_spend_seen: entry.double_spend_seen,
        details: advanced,
    })
}

fn advanced_page(
    details: &model::TxDetails,
    entry: &model::TxEntry,
    page: usize,
) -> model::Result<AdvancedDetails> {
    let input_count = if details.coinbase {
        1
    } else {
        details.inputs.len()
    };
    let page_count = input_count
        .max(details.outputs.len())
        .div_ceil(PAGE_SIZE)
        .max(1);
    if page >= page_count {
        return Err(Error::InvalidInput);
    }
    Ok(AdvancedDetails {
        page,
        page_size: PAGE_SIZE,
        page_count,
        version: details.version,
        coinbase: details.coinbase,
        serialized_size_bytes: details.full_size,
        downloaded_size_bytes: details.downloaded_size,
        weight: details.weight,
        unlock_kind: details.unlock_kind,
        unlock_value: details.unlock_value.to_string(),
        input_count,
        output_count: details.outputs.len(),
        inputs: details
            .inputs
            .iter()
            .enumerate()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|(index, input)| AdvancedInput {
                index,
                key_image: input.key_image.clone(),
                ring_size: input.ring_indices.len(),
                absolute_indices: input
                    .ring_indices
                    .iter()
                    .take(MAX_INDICES)
                    .map(u64::to_string)
                    .collect(),
                indices_truncated: input.ring_indices.len() > MAX_INDICES,
            })
            .collect(),
        outputs: details
            .outputs
            .iter()
            .enumerate()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|(index, output)| AdvancedOutput {
                index,
                key: output.key.clone(),
                public_amount_piconero: output.amount.map(|n| n.to_string()),
                view_tag: output.view_tag,
                global_index: entry.output_indices.get(index).map(u64::to_string),
            })
            .collect(),
        extra_hex: details.extra[..details.extra.len().min(4096)].to_string(),
        extra_truncated: details.extra.len() > 4096,
        received_timestamp: if entry.in_pool {
            entry.received_timestamp.filter(|t| *t > 0)
        } else {
            None
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advanced_pages_bound_worst_case_rows_indices_hex_and_response_bytes() {
        let entry: model::TxEntry =
            serde_json::from_str(include_str!("../tests/fixtures/transaction.json")).unwrap();
        let mut data = decode_transaction(&entry, &entry.tx_hash).unwrap();
        let mut input = data.inputs[0].clone();
        input.ring_indices = (0..200).map(|i| u64::MAX - 200 + i).collect();
        data.inputs = vec![input; 4096];
        data.outputs = vec![data.outputs[0].clone(); 4096];
        data.extra = "aa".repeat(65_536);
        let first = advanced_page(&data, &entry, 0).unwrap();
        assert_eq!(first.page_count, 410);
        assert_eq!(first.inputs.len(), 10);
        assert_eq!(first.outputs.len(), 10);
        assert_eq!(first.inputs[0].absolute_indices.len(), 128);
        assert!(first.inputs[0].indices_truncated && first.extra_truncated);
        assert_eq!(first.extra_hex.len(), 4096);
        assert!(serde_json::to_vec(&first).unwrap().len() < 60_000); // leaves room for summary
        let last = advanced_page(&data, &entry, 409).unwrap();
        assert_eq!(last.inputs.len(), 6);
        assert_eq!(last.inputs[0].index, 4090);
        assert!(matches!(
            advanced_page(&data, &entry, 410),
            Err(Error::InvalidInput)
        ));
        assert!(matches!(
            advanced_page(&data, &entry, usize::MAX),
            Err(Error::InvalidInput)
        ));
    }

    #[test]
    fn coinbase_legacy_public_amounts_and_unknown_pending_time_are_explicit() {
        let mut entry: model::TxEntry =
            serde_json::from_str(include_str!("../tests/fixtures/transaction.json")).unwrap();
        let mut data = decode_transaction(&entry, &entry.tx_hash).unwrap();
        data.coinbase = true;
        data.inputs.clear();
        data.outputs[0].amount = Some(u64::MAX);
        entry.in_pool = true;
        entry.received_timestamp = Some(0);
        let page = advanced_page(&data, &entry, 0).unwrap();
        assert_eq!(page.input_count, 1);
        assert!(page.inputs.is_empty());
        assert_eq!(
            page.outputs[0].public_amount_piconero.as_deref(),
            Some("18446744073709551615")
        );
        assert!(page.received_timestamp.is_none());
    }
}
