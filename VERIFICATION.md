# Local verification — 2026-10-18

Initial implementation; not a public deployment or independent security audit.

- Rust 1.96.0 on macOS: formatting, Clippy with warnings denied, and **34 offline tests** passed, including a subprocess SIGTERM shutdown regression.
- Opt-in read-only integration test passed against **https://rpc.nexatrode.com** (Cuprate): node snapshot, header range, block by height/hash, normal transfer, miner transaction, genesis, explicit missing transaction, and two-entry mempool sample. No wallet keys or funds used.
- Live node behavior checked: `target_height: 0` while synchronized is valid; `decode_as_json: true` is unsupported; empty block/transaction arrays may be omitted. Compatibility tests cover these cases.
- Docker image built on the local ARM64 Docker engine, then started with non-root/read-only/capability-drop and 256 MiB / 1 CPU Compose limits. `/readyz`, `/mempool` and `/block/0` returned 200. SIGTERM handling is explicit for container PID 1, with 25-second stop grace exceeding the 20-second page deadline. This is not a sustained load or worst-case memory benchmark; AMD64 image execution was not tested.
- Browser checks: desktop home, HTML-form transaction search, transaction detail, and 390 × 844 responsive layout. Native disclosure elements are used, not script-dependent controls.
- OSV querybatch check of **190 crates.io package/version pairs** in the resolved dependency graph returned no advisories. Git-sourced monero-oxide packages were not covered by that registry query. This check is a dated database observation, not an assurance of vulnerability-free software. `cargo-audit` was not installed; no false audit-tool pass is claimed.

## Before public exposure

1. Review the initial source, choose/create the GitHub repository, and commit/push it. No remote repository has been created by this task.
2. Run the opt-in smoke against the exact private RPC origin / deployed Cuprate build. Test monerod separately before advertising verified compatibility with it.
3. Choose DNS and hosting; merge only the example explorer site block, preserving RPC/rates configuration. Do not start a replacement daemon or mount the live node database.
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
