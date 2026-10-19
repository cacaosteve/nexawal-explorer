use monero_oxide::transaction::{Input, Pruned, Transaction};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    NotFound,
    Unavailable,
    BlockUnavailable,
    Unsupported,
    Inconsistent,
    Busy,
}
pub type Result<T> = std::result::Result<T, Error>;

pub const DEFAULT_MAX_TIP_AGE_SECS: u64 = 3600;
const MAX_FUTURE_TIP_SECS: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipFreshness {
    Fresh,
    Stale,
    Future,
    Unknown,
}

pub fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

pub fn hash(s: &str) -> Result<String> {
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidInput);
    }
    Ok(s.to_ascii_lowercase())
}

pub fn height(s: &str) -> Result<u64> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidInput);
    }
    s.parse().map_err(|_| Error::InvalidInput)
}

#[derive(Clone, Debug, Deserialize)]
pub struct Info {
    pub height: u64,
    #[serde(default)]
    pub target_height: u64,
    #[serde(default)]
    pub synchronized: bool,
    #[serde(default)]
    pub busy_syncing: bool,
    #[serde(default)]
    pub offline: bool,
    pub nettype: String,
    pub difficulty: u64,
    #[serde(default)]
    pub wide_difficulty: String,
    pub target: u64,
    pub tx_pool_size: u64,
}
impl Info {
    pub fn ready(&self) -> bool {
        self.height > 0
            && self.synchronized
            && !self.busy_syncing
            && !self.offline
            && (self.target_height == 0 || self.height >= self.target_height)
    }
    pub fn difficulty(&self) -> Option<u128> {
        if !self.wide_difficulty.is_empty() {
            u128::from_str_radix(self.wide_difficulty.trim_start_matches("0x"), 16).ok()
        } else {
            Some(self.difficulty.into())
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Header {
    pub hash: String,
    pub height: u64,
    pub timestamp: u64,
    pub prev_hash: String,
    pub num_txes: u64,
    pub reward: u64,
    pub block_size: u64,
    pub block_weight: u64,
    pub difficulty: u64,
    pub major_version: u8,
    #[serde(default)]
    pub orphan_status: bool,
}
impl Header {
    pub fn validate(&self) -> Result<()> {
        hash(&self.hash).map_err(|_| Error::Inconsistent)?;
        hash(&self.prev_hash).map_err(|_| Error::Inconsistent)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub info: Info,
    pub tip: Header,
    pub observed_at: u64,
    pub max_tip_age_secs: u64,
}
impl Snapshot {
    pub fn freshness_at(&self, now: u64) -> TipFreshness {
        if now == 0 || self.tip.timestamp == 0 {
            TipFreshness::Unknown
        } else if self.tip.timestamp > now.saturating_add(MAX_FUTURE_TIP_SECS) {
            TipFreshness::Future
        } else if now.saturating_sub(self.tip.timestamp) > self.max_tip_age_secs {
            TipFreshness::Stale
        } else {
            TipFreshness::Fresh
        }
    }
    pub fn ready_at(&self, now: u64) -> bool {
        self.info.ready() && self.freshness_at(now) == TipFreshness::Fresh
    }
    pub fn ready(&self) -> bool {
        self.ready_at(unix_time())
    }
    pub fn epoch(&self) -> String {
        format!("{}:{}", self.info.height, self.tip.hash)
    }
    pub fn confirmations(&self, h: u64) -> Result<u64> {
        self.info
            .height
            .checked_sub(h)
            .filter(|n| *n > 0)
            .ok_or(Error::Inconsistent)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Block {
    pub block_header: Header,
    pub miner_tx_hash: String,
    // Cuprate omits this field for miner-only blocks. Header count still validates it.
    #[serde(default)]
    pub tx_hashes: Vec<String>,
}
impl Block {
    pub fn validate(&self) -> Result<()> {
        self.block_header.validate()?;
        hash(&self.miner_tx_hash).map_err(|_| Error::Inconsistent)?;
        if self.tx_hashes.len() > 20_000
            || self.block_header.num_txes != self.tx_hashes.len() as u64
        {
            return Err(Error::Inconsistent);
        }
        let mut unique = std::collections::HashSet::new();
        unique.insert(&self.miner_tx_hash);
        for h in &self.tx_hashes {
            hash(h).map_err(|_| Error::Inconsistent)?;
            if !unique.insert(h) {
                return Err(Error::Inconsistent);
            }
        }
        Ok(())
    }
    pub fn contains(&self, txid: &str) -> bool {
        self.miner_tx_hash == txid || self.tx_hashes.iter().any(|h| h == txid)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct TxEntry {
    pub tx_hash: String,
    #[serde(default)]
    pub as_hex: String,
    #[serde(default)]
    pub pruned_as_hex: String,
    #[serde(default)]
    pub prunable_hash: String,
    pub in_pool: bool,
    pub block_height: Option<u64>,
    pub block_timestamp: Option<u64>,
    pub received_timestamp: Option<u64>,
    #[serde(default)]
    pub double_spend_seen: bool,
    #[serde(default)]
    pub output_indices: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct TxInput {
    pub key_image: String,
    pub ring_indices: Vec<u64>,
}
#[derive(Clone, Debug)]
pub struct TxOutput {
    pub key: String,
    pub amount: Option<u64>,
    pub view_tag: Option<u8>,
}
#[derive(Clone, Debug)]
pub struct TxDetails {
    pub version: u8,
    pub coinbase: bool,
    pub fee: Option<u64>,
    pub weight: Option<usize>,
    pub full_size: Option<usize>,
    pub downloaded_size: usize,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub extra: String,
    pub unlock: String,
    pub unlock_kind: &'static str,
    pub unlock_value: u64,
}

/// Parse public transaction data only. No wallet scanning, keys, or signing code is used.
/// Verify the returned blob's transaction ID, not merely the daemon's tx_hash label.
pub fn decode_transaction(entry: &TxEntry, expected: &str) -> Result<TxDetails> {
    if hash(&entry.tx_hash).map_err(|_| Error::Inconsistent)? != expected {
        return Err(Error::Inconsistent);
    }
    let full = !entry.as_hex.is_empty();
    let encoded = if full {
        &entry.as_hex
    } else {
        &entry.pruned_as_hex
    };
    if encoded.len() > 2 * 1_048_576 {
        return Err(Error::Unsupported);
    }
    let bytes = hex::decode(encoded).map_err(|_| Error::Inconsistent)?;
    let mut reader = bytes.as_slice();
    let (tx, weight): (Transaction<Pruned>, Option<usize>) = if full {
        let tx = Transaction::read(&mut reader).map_err(|_| Error::Inconsistent)?;
        if hex::encode(tx.hash()) != expected {
            return Err(Error::Inconsistent);
        }
        let weight = tx.weight();
        (tx.into(), Some(weight))
    } else {
        let tx = Transaction::<Pruned>::read(&mut reader).map_err(|_| Error::Inconsistent)?;
        let prunable: [u8; 32] = hex::decode(&entry.prunable_hash)
            .map_err(|_| Error::Inconsistent)?
            .try_into()
            .map_err(|_| Error::Inconsistent)?;
        // Legacy v1 ID verification needs the signatures: don't pretend a pruned prefix suffices.
        let id = tx
            .hash_with_prunable_hash(prunable)
            .ok_or(Error::Unsupported)?;
        if hex::encode(id) != expected {
            return Err(Error::Inconsistent);
        }
        (tx, None)
    };
    if !reader.is_empty() {
        return Err(Error::Inconsistent);
    }
    let prefix = tx.prefix();
    if prefix.inputs.len() > 4096 || prefix.outputs.len() > 4096 || prefix.extra.len() > 65_536 {
        return Err(Error::Unsupported);
    }
    let coinbase = matches!(prefix.inputs.first(), Some(Input::Gen(_)));
    let fee = if coinbase {
        Some(0)
    } else {
        match &tx {
            Transaction::V2 {
                proofs: Some(proofs),
                ..
            } => Some(proofs.base.fee),
            Transaction::V1 { prefix, .. } => {
                let ins = prefix
                    .inputs
                    .iter()
                    .try_fold(0u64, |sum, input| match input {
                        Input::ToKey {
                            amount: Some(amount),
                            ..
                        } => sum.checked_add(*amount),
                        _ => None,
                    });
                let outs = prefix
                    .outputs
                    .iter()
                    .try_fold(0u64, |sum, out| sum.checked_add(out.amount.unwrap_or(0)));
                ins.zip(outs).and_then(|(a, b)| a.checked_sub(b))
            }
            _ => None,
        }
    };
    let mut inputs = Vec::new();
    for input in &prefix.inputs {
        if let Input::ToKey {
            key_image,
            key_offsets,
            ..
        } = input
        {
            let mut index = 0u64;
            let mut ring_indices = Vec::new();
            for offset in key_offsets {
                index = index.checked_add(*offset).ok_or(Error::Inconsistent)?;
                ring_indices.push(index);
            }
            inputs.push(TxInput {
                key_image: hex::encode(key_image.to_bytes()),
                ring_indices,
            });
        }
    }
    let (unlock_kind, unlock_value) = match prefix.additional_timelock {
        monero_oxide::transaction::Timelock::None => ("none", 0),
        monero_oxide::transaction::Timelock::Block(h) => ("height", h as u64),
        monero_oxide::transaction::Timelock::Time(t) => ("timestamp", t),
    };
    Ok(TxDetails {
        version: tx.version(),
        coinbase,
        fee,
        weight,
        full_size: full.then_some(bytes.len()),
        downloaded_size: bytes.len(),
        inputs,
        outputs: prefix
            .outputs
            .iter()
            .map(|out| TxOutput {
                key: hex::encode(out.key.to_bytes()),
                // Zero is represented as None by the shared parser. On legacy/coinbase
                // transactions it is a public zero, not an encrypted RingCT amount.
                amount: if tx.version() == 1 || coinbase {
                    Some(out.amount.unwrap_or(0))
                } else {
                    out.amount
                },
                view_tag: out.view_tag,
            })
            .collect(),
        extra: hex::encode(&prefix.extra),
        unlock: format!("{:?}", prefix.additional_timelock),
        unlock_kind,
        unlock_value,
    })
}
