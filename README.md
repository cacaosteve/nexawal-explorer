# NexaWal Explorer

A standalone, server-rendered Monero explorer for **Cuprate RPC**, with monerod-compatible operations. No JavaScript, cookies, external assets, analytics, wallet keys, database mounts, or transaction broadcasts. Inspired by the restraint of xmrchain.net, not a fork of its C++ implementation.

## Run locally

```sh
cd /Users/steve/supermegamartandwalletcore/nexawal-explorer
EXPLORER_BIND=127.0.0.1:18099 EXPLORER_RPC_URL=https://rpc.nexatrode.com cargo run --locked --release
```

Open **http://127.0.0.1:18099**. The example avoids a local nginx already using port 8080. The application's default is `127.0.0.1:8080`. `EXPLORER_BIND` accepts an IP and port; the Docker image binds `0.0.0.0:8080` inside its container. The configured RPC URL must be an HTTP(S) origin, with no path, credentials, query, or fragment. It is operator configuration, never a visitor-supplied URL. Outbound HTTP redirects and environment HTTP proxies are disabled deliberately.

## Pages

| Route | Purpose |
| --- | --- |
| `/` | Network snapshot, latest 10 blocks, and five-ID mempool preview |
| `/blocks?page=0` | Block browsing, 25 rows per page |
| `/block/<height-or-hash>?page=0` | Header, reward, miner transaction, paginated transaction IDs |
| `/tx/<txid>?page=0` | Exact integer fee, status, canonical-block membership, parsed inputs/outputs/extra |
| `/mempool?page=0` | Sorted pool IDs, fees, weights, observed state; 25 rows per page |
| `/search?q=<height-or-hash>` | Standard HTML search form |
| `/about` | Limitations and privacy disclosure |
| `/healthz` | Process liveness only; no RPC; bypasses the page admission budget |
| `/readyz` | 200 only when node reports synchronized, online, caught up, and has a recent tip |

All timestamps are UTC. Confirmed transaction times come from the containing block, not duplicate transaction metadata. A node height of N means the latest block index is N−1. Confirmation counts include the containing block. We do not label a transaction "final" after an arbitrary number of confirmations. The miner reward includes fees and is not labeled pure subsidy. Payment amounts and balances cannot be inferred from public RingCT data.

Tip freshness defaults to **one hour**, configurable with `EXPLORER_MAX_TIP_AGE_SECS` (120–86400 seconds). Unknown/zero tip times, tips more than ten minutes in the future, and expired tips make readiness return 503, even if the daemon claims synchronization. Existing historical pages remain readable with a warning; uncertain absence is not a 404. This is a clock-dependent operational heuristic, not proof of the latest canonical chain. Keep the server clock synchronized; a genuine long block interval can also trigger the warning.

Native HTML `<details>` provides technical disclosure without scripts. Full hashes are selectable. Input/output rows are paginated; transaction extra previews stop at 2,048 bytes. No clipboard JavaScript, auto-refresh, third-party links, or view-key upload forms.

## Node compatibility

Fixed read-only RPC operations:

- `get_info`, `get_block_header_by_height`, `get_block_headers_range`, `get_block` through `/json_rpc`.
- `/get_transactions` with **`decode_as_json: false`**, and `/get_transaction_pool_hashes`.

Cuprate currently rejects `decode_as_json: true`, and its full `/get_transaction_pool` endpoint is not available on the existing node. We do not depend on either. Pool details are fetched in one bounded batch using the pool-hash endpoint. Pool entries can disappear between calls and are labeled accordingly. Pool sorting is lexical by ID, not arrival order; it is not a whole-network view.

Transaction blobs are parsed with the **transaction-only `monero-oxide` crate**, pinned to the same explicit revision used by WalletCore v0.1.18 (`34201e58…`). There is **no WalletCore, scanner, seed, key-storage, FFI, or signing dependency**. Full transaction IDs are recomputed; pruned v2 IDs are verified with their prunable hash. Unsupported pruned legacy v1 data is rejected, not trusted by its label. For pruned responses, full serialized size and weight are reported unavailable rather than guessed.

RPC compatibility is not assumed from the word "Monero": run the opt-in smoke test against each deployed Cuprate/monerod version. No second daemon or second blockchain copy is needed. The old MAGIC Grants database-reader fork is not used.

## Cache and reorg model

- Demand-driven, successful-response-only memory cache: **5-second TTL**, at most **128 entries / 8 MiB serialized-response weight** (not a total-process RAM guarantee). No stale-on-error fallback.
- Node snapshot refresh is single-flight. Other operations use 32 fixed single-flight stripes to prevent unbounded lock metadata. At most four upstream requests are active.
- Chain-dependent keys include snapshot height **and tip hash**, so same-height reorgs also change the namespace at the next snapshot refresh. Old entries expire or are evicted; no persistent index is maintained.
- A confirmed transaction must actually be listed in its containing block returned by the node; impossible heights, truncated header ranges, parent-link mismatches and mismatched blob IDs fail explicitly.
- Pages are observations, not atomic database snapshots or independent consensus verification. Concurrent reorgs may cause a recoverable 503; cached node state can lag up to five seconds. No irreversible-finality promise.
- No node failure is converted into an empty chain or zero fee. Definitive transaction absence requires the node's explicit `missed_tx` response plus a caught-up node snapshot. Unknown block-hash RPC failures are conservatively unavailable because daemon errors are not standardized.

## Resource and privacy boundaries

- Upstream connect timeout: 3 seconds; request timeout: 8 seconds; response cap: 4 MiB. Four concurrent RPC requests, 32 active web requests, 20-second page deadline. Blob cap: 1 MiB; 4,096 input/output cap; extra cap: 64 KiB. Limits fail visibly.
- All dynamic HTML text/attributes are escaped; internal links use validated IDs/numbers. CSP forbids scripts, frames, remote assets, and connections. `Referrer-Policy: no-referrer`, no-store HTML, noindex robots policy. No public RPC relay or arbitrary method/path endpoint exists.
- No application access logging of IPs, paths, queries or transaction IDs. The Caddy example discards both explorer access logs and request-associated HTTP error logs: disabling access logs alone does not protect proxy-failure logs. It preserves unrelated sites' and general operational logs. Merge its scoped global logger settings as well as the site block, exclude this namespace from any additional catch-all loggers, and never enable global debug/handler tracing. Reverse proxies, infrastructure providers, and upstream node operators can still observe requests; **no-JS is not anonymity**. Audit provider logging separately before making privacy claims. Use liveness/readiness monitoring rather than visitor request logs.
- Request limits are **not** a distributed DDoS defense or per-IP rate limiter. Configure appropriate ingress limits and monitoring before public launch. Shared-host resource limits matter because wallets use the same node.
- HTTPS is terminated by your reverse proxy. HSTS is an operator decision; not forced for local HTTP / I2P access. A separate I2P/Tor tunnel can point to the same service later; none is created automatically.

## Docker / Incus

```sh
docker compose build
docker compose up -d
curl http://127.0.0.1:8088/readyz
```

The container is non-root, read-only, capability-dropped and limited to 256 MiB / 1 CPU. No chain volume is mounted. Public HTTP is **not** exposed by Compose; port 8088 binds host loopback. Set `EXPLORER_RPC_URL` to your existing node's private reachable RPC origin when co-located. Inside Docker, `127.0.0.1` refers to that container, not the host or another container. Do not replace or restart the running Cuprate to add this service.

`deploy/Caddyfile.example` contains **scoped global logging settings plus an example site for explorer.nexawal.com**, not a replacement for existing rates/RPC Caddy configuration. Merge settings into the one existing global block and preserve other sites' configuration.

The Mini deployment is live at **https://explorer.nexawal.com**. Its separate container runs in `cuprate-vm` and host Caddy proxies to the private VM interface. See [the deployment runbook](deploy/README.md), `deploy/compose.incus.yml`, and the custom rate-limited `deploy/Caddyfile.incus.example`. The local Compose file above is not the VM runtime file. No chain database is mounted and Cuprate need not restart. I2P setup and changes to wallet explorer links remain separate work; the wallet apps still point to xmrchain.net.

Railway can run the same image with `EXPLORER_BIND=0.0.0.0:8080`, target port 8080 and an explicitly configured RPC URL. No Railway deployment is performed by this repo. Docker bases are digest-pinned, but Debian package repositories are not snapshotted: this is not a byte-reproducible-image claim. Update base digests deliberately for security maintenance.

## Verification

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
# Optional local proxy-outage regression; Docker + Node 18+, synthetic data only.
node Scripts/test-proxy-privacy.mjs
# Opt in: reads a small number of public node records; never sends funds.
EXPLORER_TEST_RPC_URL=https://rpc.nexatrode.com cargo test --locked --test live -- --ignored --nocapture
```

`Cargo.lock` is included for binary dependency resolution and should be committed with the source. There are no automatic GitHub Actions workflows. Offline fixture tests cover missing/pending/confirmed transactions, parsing/fees, same-height tip changes, cache coalescing, upstream errors, output escaping and security headers. Live tests do not prove future daemon compatibility or a production security audit.

## Deliberately deferred

Chain-wide emission charts / historical analytics (would need an explorer-owned reorg-aware index), resolved ring-member drilldown, transaction-proof tools, transaction broadcasting, address/view-key scanning, automatic node failover, and translations. The initial interface is English; no incomplete translations are advertised. No production/public deployment or independent audit is implied.

## License and references

MIT. Third-party dependency licenses still apply. Design references: [Onion Monero Explorer](https://github.com/moneroexamples/onion-monero-blockchain-explorer), [Cuprate RPC architecture](https://architecture.cuprate.org/rpc/intro.html), [Monero daemon RPC](https://www.getmonero.org/resources/developer-guides/daemon-rpc.html). No code copied from other explorers.
