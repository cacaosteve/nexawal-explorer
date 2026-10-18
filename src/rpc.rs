use crate::model::{
    hash, unix_time, Block, Error, Header, Info, Result, Snapshot, TxEntry,
    DEFAULT_MAX_TIP_AGE_SECS,
};
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};

const MAX_RESPONSE: usize = 4 * 1024 * 1024;
const CACHE_BYTES: usize = 8 * 1024 * 1024;
const CACHE_ENTRIES: usize = 128;
const RPC_TIMEOUT: Duration = Duration::from_secs(8);
const TTL: Duration = Duration::from_secs(5);

struct Cached {
    at: Instant,
    bytes: usize,
    value: Arc<Value>,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Cached>,
    bytes: usize,
}
impl Cache {
    fn get(&self, key: &str) -> Option<Arc<Value>> {
        self.entries
            .get(key)
            .filter(|e| e.at.elapsed() < TTL)
            .map(|e| e.value.clone())
    }
    fn insert(&mut self, key: String, value: Arc<Value>, bytes: usize) {
        self.entries.retain(|_, e| e.at.elapsed() < TTL);
        self.bytes = self.entries.values().map(|e| e.bytes).sum();
        if bytes > CACHE_BYTES / 2 {
            return;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        while self.bytes + bytes > CACHE_BYTES || self.entries.len() >= CACHE_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.at)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                if let Some(v) = self.entries.remove(&k) {
                    self.bytes -= v.bytes;
                }
            } else {
                break;
            }
        }
        self.bytes += bytes;
        self.entries.insert(
            key,
            Cached {
                at: Instant::now(),
                bytes,
                value,
            },
        );
    }
}

struct Inner {
    base: Url,
    client: Client,
    cache: Mutex<Cache>,
    // Fixed stripes bound single-flight metadata even under arbitrary hash searches.
    flights: Vec<Mutex<()>>,
    permits: Semaphore,
    snapshot: Mutex<Option<(Instant, Snapshot)>>,
    max_tip_age_secs: u64,
}
#[derive(Clone)]
pub struct Rpc(Arc<Inner>);

impl Rpc {
    pub fn new(base: &str) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Self::with_max_tip_age(base, DEFAULT_MAX_TIP_AGE_SECS)
    }

    pub fn with_max_tip_age(
        base: &str,
        max_tip_age_secs: u64,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        if !(120..=86_400).contains(&max_tip_age_secs) {
            return Err("EXPLORER_MAX_TIP_AGE_SECS must be between 120 and 86400".into());
        }
        let mut base = Url::parse(base)?;
        if !matches!(base.scheme(), "http" | "https")
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || base.path() != "/"
        {
            return Err("EXPLORER_RPC_URL must be an HTTP(S) origin without credentials, path, query, or fragment".into());
        }
        base.set_path("/");
        let client = Client::builder()
            .timeout(RPC_TIMEOUT)
            .connect_timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent("nexawal-explorer/0.1")
            .build()?;
        Ok(Self(Arc::new(Inner {
            base,
            client,
            cache: Mutex::new(Cache::default()),
            flights: (0..32).map(|_| Mutex::new(())).collect(),
            permits: Semaphore::new(4),
            snapshot: Mutex::new(None),
            max_tip_age_secs,
        })))
    }

    async fn request(
        &self,
        path: &str,
        payload: Value,
        epoch: &str,
        cached: bool,
    ) -> Result<Arc<Value>> {
        // Private method, called only by the fixed read-only operations below.
        let key = format!("{epoch}:{path}:{payload}");
        if cached {
            if let Some(v) = self.0.cache.lock().await.get(&key) {
                return Ok(v);
            }
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let stripe = hasher.finish() as usize % self.0.flights.len();
        let _flight = tokio::time::timeout(RPC_TIMEOUT, self.0.flights[stripe].lock())
            .await
            .map_err(|_| Error::Busy)?;
        if cached {
            if let Some(v) = self.0.cache.lock().await.get(&key) {
                return Ok(v);
            }
        }
        let _permit = tokio::time::timeout(Duration::from_millis(500), self.0.permits.acquire())
            .await
            .map_err(|_| Error::Busy)?
            .map_err(|_| Error::Busy)?;
        let url = self.0.base.join(path).map_err(|_| Error::Unavailable)?;
        let mut response = self
            .0
            .client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::Unsupported);
        }
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(Error::Unsupported);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Unavailable)? {
            if body.len() + chunk.len() > MAX_RESPONSE {
                return Err(Error::Unsupported);
            }
            body.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| Error::Inconsistent)?;
        if value.get("error").is_some() {
            return Err(Error::Unavailable);
        }
        let result = if path == "json_rpc" {
            value.get("result").cloned().ok_or(Error::Inconsistent)?
        } else {
            value
        };
        if result.get("status").and_then(Value::as_str) != Some("OK") {
            return Err(Error::Unavailable);
        }
        let bytes = body.len();
        let result = Arc::new(result);
        if cached {
            self.0.cache.lock().await.insert(key, result.clone(), bytes);
        }
        Ok(result)
    }

    async fn json(&self, method: &str, params: Value, epoch: &str) -> Result<Arc<Value>> {
        self.request(
            "json_rpc",
            json!({"jsonrpc":"2.0","id":"explorer","method":method,"params":params}),
            epoch,
            true,
        )
        .await
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        let mut slot = self.0.snapshot.lock().await;
        if let Some((at, s)) = slot.as_ref() {
            if at.elapsed() < TTL {
                return Ok(s.clone());
            }
        }
        // Never use stale node state when a refresh fails.
        *slot = None;
        let info_value = self
            .request(
                "json_rpc",
                json!({"jsonrpc":"2.0","id":"explorer","method":"get_info"}),
                "",
                false,
            )
            .await?;
        let info: Info = parse(&info_value)?;
        if info.height == 0 {
            return Err(Error::Unavailable);
        }
        let tip_value = self.request("json_rpc", json!({"jsonrpc":"2.0","id":"explorer","method":"get_block_header_by_height","params":{"height":info.height - 1}}), "", false).await?;
        let tip: Header = field(&tip_value, "block_header")?;
        tip.validate()?;
        if tip.height != info.height - 1 || tip.orphan_status {
            return Err(Error::Inconsistent);
        }
        let s = Snapshot {
            info,
            tip,
            observed_at: unix_time(),
            max_tip_age_secs: self.0.max_tip_age_secs,
        };
        *slot = Some((Instant::now(), s.clone()));
        Ok(s)
    }

    pub async fn headers(&self, s: &Snapshot, start: u64, end: u64) -> Result<Vec<Header>> {
        if start > end || end >= s.info.height || end - start >= 25 {
            return Err(Error::InvalidInput);
        }
        let data = self
            .json(
                "get_block_headers_range",
                json!({"start_height":start,"end_height":end}),
                &s.epoch(),
            )
            .await?;
        let headers: Vec<Header> = field(&data, "headers")?;
        if headers.len() as u64 != end - start + 1 {
            return Err(Error::Inconsistent);
        }
        for (idx, h) in headers.iter().enumerate() {
            h.validate()?;
            if h.height != start + idx as u64
                || h.orphan_status
                || (idx > 0 && h.prev_hash != headers[idx - 1].hash)
                || (h.height == s.tip.height && h.hash != s.tip.hash)
            {
                return Err(Error::Inconsistent);
            }
        }
        Ok(headers)
    }

    pub async fn block(&self, s: &Snapshot, id: &str) -> Result<Block> {
        let params = if id.len() == 64 {
            json!({"hash": hash(id)?})
        } else {
            let height = crate::model::height(id)?;
            if height >= s.info.height {
                return Err(if s.ready() {
                    Error::NotFound
                } else {
                    Error::Unavailable
                });
            }
            json!({"height":height})
        };
        // Daemons do not standardize errors for unknown hashes. Leave ambiguous RPC
        // failures as unavailable rather than misclassifying a node failure as absence.
        let data = self
            .json("get_block", params, &s.epoch())
            .await
            .map_err(|e| {
                if id.len() == 64 && e == Error::Unavailable {
                    Error::BlockUnavailable
                } else {
                    e
                }
            })?;
        let b: Block = parse(&data)?;
        b.validate()?;
        if id.len() == 64 && b.block_header.hash != id {
            return Err(Error::Inconsistent);
        }
        if id.len() != 64 && b.block_header.height != crate::model::height(id)? {
            return Err(Error::Inconsistent);
        }
        if b.block_header.height == s.tip.height && b.block_header.hash != s.tip.hash {
            return Err(Error::Inconsistent);
        }
        Ok(b)
    }

    pub async fn transaction(&self, s: &Snapshot, txid: &str) -> Result<Option<TxEntry>> {
        let txid = hash(txid)?;
        // decode_as_json is deliberately false: Cuprate does not implement it.
        let value = self
            .request(
                "get_transactions",
                json!({"txs_hashes":[txid],"decode_as_json":false,"prune":false}),
                &s.epoch(),
                true,
            )
            .await?;
        let txs: Vec<TxEntry> = value.get("txs").map(parse).transpose()?.unwrap_or_default();
        if txs.len() > 1 {
            return Err(Error::Inconsistent);
        }
        if let Some(tx) = txs.into_iter().next() {
            if tx.tx_hash != txid {
                return Err(Error::Inconsistent);
            }
            return Ok(Some(tx));
        }
        // An empty/partial malformed response must not become a false "not found".
        let missed: Vec<String> = field(&value, "missed_tx")?;
        if missed != vec![txid] {
            return Err(Error::Inconsistent);
        }
        Ok(None)
    }

    pub async fn pool_hashes(&self, s: &Snapshot) -> Result<Vec<String>> {
        let value = self
            .request("get_transaction_pool_hashes", json!({}), &s.epoch(), true)
            .await?;
        let mut hashes: Vec<String> = field(&value, "tx_hashes")?;
        if hashes.len() > 40_000 {
            return Err(Error::Unsupported);
        }
        for h in &hashes {
            hash(h).map_err(|_| Error::Inconsistent)?;
        }
        hashes.sort_unstable();
        hashes.dedup();
        Ok(hashes)
    }

    pub async fn pool_entries(&self, s: &Snapshot, ids: &[String]) -> Result<Vec<TxEntry>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        if ids.len() > 25 {
            return Err(Error::InvalidInput);
        }
        let value = self
            .request(
                "get_transactions",
                json!({"txs_hashes":ids,"decode_as_json":false,"prune":false}),
                &s.epoch(),
                true,
            )
            .await?;
        let txs: Vec<TxEntry> = value.get("txs").map(parse).transpose()?.unwrap_or_default();
        let missed: Vec<String> = value
            .get("missed_tx")
            .map(parse)
            .transpose()?
            .unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        for tx in &txs {
            if !ids.contains(&tx.tx_hash) || !seen.insert(tx.tx_hash.clone()) {
                return Err(Error::Inconsistent);
            }
        }
        for id in missed {
            if !ids.contains(&id) || !seen.insert(id) {
                return Err(Error::Inconsistent);
            }
        }
        if seen.len() != ids.len() {
            return Err(Error::Inconsistent);
        }
        Ok(txs)
    }
}

fn parse<T: DeserializeOwned>(value: &Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|_| Error::Inconsistent)
}
fn field<T: DeserializeOwned>(value: &Value, name: &str) -> Result<T> {
    parse(value.get(name).ok_or(Error::Inconsistent)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{Fake, Node, TXID};

    #[tokio::test]
    async fn snapshot_refresh_observes_same_height_reorg() {
        let node = Node::start(Fake::default()).await;
        let rpc = Rpc::new(&node.url).unwrap();
        let old = rpc.snapshot().await.unwrap();
        let before = rpc.block(&old, "8").await.unwrap();
        node.state.lock().unwrap().reorg = true;
        rpc.0.snapshot.lock().await.as_mut().unwrap().0 = Instant::now() - Duration::from_secs(6);
        let next = rpc.snapshot().await.unwrap();
        let after = rpc.block(&next, "8").await.unwrap();
        assert_eq!(old.info.height, next.info.height);
        assert_ne!(old.epoch(), next.epoch());
        assert_ne!(before.block_header.hash, after.block_header.hash);
        assert_eq!(node.state.lock().unwrap().hits["get_info"], 2);
    }

    #[tokio::test]
    async fn expired_snapshot_is_not_served_when_node_fails() {
        let node = Node::start(Fake::default()).await;
        let rpc = Rpc::new(&node.url).unwrap();
        rpc.snapshot().await.unwrap();
        node.state.lock().unwrap().down = true;
        rpc.0.snapshot.lock().await.as_mut().unwrap().0 = Instant::now() - Duration::from_secs(6);
        assert!(matches!(rpc.snapshot().await, Err(Error::Unavailable)));
        assert!(rpc.0.snapshot.lock().await.is_none());
    }

    #[tokio::test]
    async fn failed_rpc_results_are_not_cached_and_recover_without_restart() {
        let node = Node::start(Fake::default()).await;
        let rpc = Rpc::new(&node.url).unwrap();
        let s = rpc.snapshot().await.unwrap();
        node.state.lock().unwrap().down = true;
        assert!(rpc.transaction(&s, TXID).await.is_err());
        node.state.lock().unwrap().down = false;
        assert!(rpc.transaction(&s, TXID).await.unwrap().is_some());
        assert_eq!(node.state.lock().unwrap().hits["/get_transactions"], 2);
    }
    #[test]
    fn cache_is_bounded_and_expires() {
        let mut cache = Cache::default();
        for i in 0..300 {
            cache.insert(i.to_string(), Arc::new(json!(i)), 100_000);
        }
        assert!(cache.bytes <= CACHE_BYTES);
        assert!(cache.entries.len() <= CACHE_ENTRIES);
        let entry = cache.entries.values_mut().next().unwrap();
        entry.at = Instant::now() - Duration::from_secs(6);
        let key = cache
            .entries
            .iter()
            .find(|(_, e)| e.at.elapsed() > TTL)
            .unwrap()
            .0
            .clone();
        assert!(cache.get(&key).is_none());
    }
    #[test]
    fn rejects_unsafe_upstream_configuration() {
        for url in [
            "file:///tmp/x",
            "https://user:pass@example.com",
            "https://a/b",
            "https://a/?q=x",
        ] {
            assert!(Rpc::new(url).is_err());
        }
    }
}
