# Local verification — 2026-10-18

Initial implementation checks followed by the deployment record below. Not an independent security audit.

## Advanced API / native parity — local changes, not deployed

- **41 offline explorer tests** and strict Clippy passed. Coverage includes verified
  advanced responses, explicit conflict flags, malformed/out-of-range pages,
  pruned/coinbase/legacy metadata, and worst-case row/index/hex/response bounds.
- A read-only Cuprate smoke with the new local API implementation passed against
  `https://rpc.nexawal.com` at height 3,787,124. No wallet material or funds were
  used; the production explorer binary was not changed by this test.
- Native verification passed: **106 Swift logic tests**, iOS Release simulator
  build (signing disabled), **73 Android app tests** and unsigned Release APK,
  **121 GPUI tests** and Release build. Delayed page replies are tested through
  cancellation/collapse/background paths without changing wallet state.
- API and all three native parsers use byte-identical synthetic contract fixtures
  (`node Scripts/check-client-fixtures.mjs`). Shared parsing tests cover last
  pages, exact u64 amounts, hidden RingCT amounts, nullable metadata and bad types.
- Summary plus 25 advanced labels/warnings cover all 54 existing app locales.
  Actual Android values and iOS/GPUI parity checks pass; three Node regression
  tests also pass. Draft translations still need human safety-wording review.
- Existing default-off/manual-only consent, clearnet disclosure, deadlines,
  no persistent response cache and no wallet-record overrides remain in place.
  No WalletCore pins, Xcode scheme, secrets, stores, nodes or DNS were changed.
- These updates remain local/uncommitted and need a server deployment before
  the live `/details/{page}` endpoint is available. Physical-device/native UI
  smoke testing still applies; no claim of signed-device QA is made here.

## Initial implementation checks

- Rust 1.96.0 on macOS: formatting, Clippy with warnings denied, and **34 offline tests** passed, including a subprocess SIGTERM shutdown regression.
- Opt-in read-only integration test passed against **https://rpc.nexatrode.com** (Cuprate): node snapshot, header range, block by height/hash, normal transfer, miner transaction, genesis, explicit missing transaction, and two-entry mempool sample. No wallet keys or funds used.
- Live node behavior checked: `target_height: 0` while synchronized is valid; `decode_as_json: true` is unsupported; empty block/transaction arrays may be omitted. Compatibility tests cover these cases.
- Docker image built on the local ARM64 Docker engine, then started with non-root/read-only/capability-drop and 256 MiB / 1 CPU Compose limits. `/readyz`, `/mempool` and `/block/0` returned 200. SIGTERM handling is explicit for container PID 1, with 25-second stop grace exceeding the 20-second page deadline. This is not a sustained load or worst-case memory benchmark; AMD64 image execution was not tested.
- Browser checks: desktop home, HTML-form transaction search, transaction detail, and 390 × 844 responsive layout. Native disclosure elements are used, not script-dependent controls.
- OSV querybatch check of **190 crates.io package/version pairs** in the resolved dependency graph returned no advisories. Git-sourced monero-oxide packages were not covered by that registry query. This check is a dated database observation, not an assurance of vulnerability-free software. `cargo-audit` was not installed; no false audit-tool pass is claimed.

## Before broader public promotion

1. Continue source review. The public source repository is https://github.com/cacaosteve/nexawal-explorer.
2. Repeat the smoke against the exact private RPC origin whenever the deployed Cuprate build changes. Test monerod separately before advertising verified compatibility with it.
3. Keep RPC/rates configuration and logging protections intact on future Caddy changes. Do not start a replacement daemon or mount the live node database.
4. Review ingress request limits, slow-client defenses, resource isolation, proxy/provider logs, HTTPS policy and operational monitoring. Existing concurrency limits are not distributed DDoS protection.
5. Test concurrent visitors and representative large blocks/transactions under the intended resource limits. No production throughput or memory guarantee is made.
6. Review all privacy claims for the chosen hosting provider. If publishing I2P/Tor addresses, verify those tunnels independently.
7. Only after the site is healthy, decide whether to change the three wallet apps' explorer URL. No wallet repository was changed here.

Deferred features and limitations are recorded in README.md. No full xmrchain.net feature-parity claim.

## Review fixes verified locally

- `/healthz` stays 200 with all 32 admission permits held, retains privacy headers, and makes no RPC request. Ordinary pages and readiness remain budgeted.
- Daemon synchronization flags alone no longer pass readiness: zero, stale, and implausibly future tip timestamps fail. Tests cover inclusive age/future boundaries, validated configuration limits, warning banners, and conservative 503 instead of false absence. Default maximum tip age is one hour, configurable; this is an operational heuristic, not independent chain verification.
- A confirmed transaction's displayed time comes from its membership-checked containing block. A regression deliberately supplies a transaction timestamp of zero and a different valid block timestamp.
- Actual Caddy **2.10.2** Docker proxy-outage tests passed for both a handled 503 and unhandled 502. Synthetic TXID/query/header/IP canaries were absent from explorer logs; an unrelated site's error request remained logged. This checks the scoped global logging configuration, not merely the absence of access logging. Run `node Scripts/test-proxy-privacy.mjs` to repeat. Infrastructure/provider logs and custom additional loggers still require a deployment-specific audit.
- The opt-in Cuprate smoke was repeated successfully after the fixes, at height 3,786,937. No writes, funds, wallet secrets, daemon configuration changes, or production deployment were involved.
- Rebuilt the release Docker image after these fixes and ran a separate disposable loopback-only container: `/healthz`, `/readyz`, `/block/0`, and `/mempool` all returned 200 with privacy headers. Runtime inspection confirmed UID/GID 10001, read-only filesystem, and 256 MiB limit. The test container was stopped and removed; the pre-existing local explorer container was not restarted.

The Sites local-only workflow kept hosting and DNS unchanged. These checks did not deploy the explorer or change the wallet apps.

## Authorized Mini deployment — 2026-10-18

- Public URL: https://explorer.nexawal.com. User added the CNAME to `mini.nexatrode.com`; authoritative DNS was checked. Host Caddy obtained a Let's Encrypt certificate, and HTTP returned a 308 redirect to HTTPS.
- AMD64 runtime image `nexawal-explorer:0057061` was built from source `005706198b4b1d34cffc117a0ab8568be25ec5c2` on the Mini with two Cargo jobs, then loaded into `cuprate-vm`. Build-host and running-VM binaries both hash to SHA-256 `5ae0109e88df7fcf154383d10ce70a559cc99c4c647192315860634e24cbdfb4`; root filesystem layers/configuration match. The VM runtime image ID is `sha256:db21175290ca8aad185392415dbd1ec8d55e6cd8a827b5d9246984c6088e1341`.
- Runtime files: `/opt/nexawal-explorer/compose.yml` and `.env` inside the VM. Private listener: `10.62.67.10:8088`, upstream restricted RPC: `http://10.62.67.10:18092`. Inspection confirmed read-only filesystem, UID/GID 10001, all capabilities dropped, 256 MiB memory, one CPU, 128 PIDs, and `unless-stopped` restart policy. No chain volume is mounted.
- The existing host Caddyfile was checksum-checked, backed up to `/home/juan/supermegamart/caddy/Caddyfile.before-explorer-20261018T2156Z`, merged preserving existing local edits/sites, validated against Caddy v2.11.4, and reloaded without restarting Caddy.
- `Scripts/test-incus-proxy.py` passed on the Mini's actual custom Caddy image: synthetic outage details absent from explorer logs; rate limits return 429 with Retry-After; rate-module diagnostics retain zone/wait but not remote IP/key; unrelated-site errors remain logged. The test container was removed.
- Public HTTPS home, block list, genesis block, confirmed transaction, mempool, About, CSS, robots, liveness, and readiness all returned 200. HTML transaction search returned 303 to the exact TXID. HTML privacy/CSP/no-store/no-cookie checks passed. The live home page was also verified in a browser.
- Cuprate's container ID and start time were unchanged, as were Caddy's. Public wallet RPC remained synchronized and returned `status: OK` at height 3,787,052 after the change. No wallet app links were changed. This is a functional smoke, not a sustained public-load test or an independent audit.
