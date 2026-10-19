# Read-only transaction API v1

`GET /api/v1/transactions/{txid}` returns a single public transaction observation.
The transaction ID must be exactly 64 hexadecimal characters. Query parameters are rejected.
There are no wallet, address, seed, view-key, broadcast, arbitrary RPC, bulk-history, or write endpoints.
The same resource limits, single-flight/cache layer, HTTPS ingress, and IP rate limits as the website apply.
The website remains server-rendered and needs no JavaScript.

## Success (200, application/json)

```json
{
  "api_version": 1,
  "network": "mainnet",
  "txid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "status": "confirmed",
  "fee_piconero": "44540000",
  "block_height": 8,
  "block_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "block_timestamp": 1700000000,
  "confirmations": 2,
  "chain_height": 10,
  "tip_hash": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
  "node_ready": true,
  "observed_at": 1700000010,
  "double_spend_seen": false
}
```

Illustrative values only. `chain_height` counts blocks, so confirmations include the containing block: `chain_height - block_height`. Fees are decimal **strings of integer piconero**, never floating-point XMR or private payment amounts. Unknown fee is null, never assumed zero. Pending entries have `status: "pending"`, null block fields, and zero confirmations.

Confirmed entries reuse blob-ID verification and containing-block membership checks. This is still one node's observation, not independent consensus verification. It does not prove recipient identity, private RingCT amount, spendability, finality, or receipt by a wallet. Cached snapshots may lag up to five seconds and are not atomic across a reorg. `node_ready: false` warns that the node is syncing/offline or tip freshness is unverified. `observed_at` is the snapshot's Unix time in seconds; it is not transaction time. A block timestamp is taken from the containing block, not daemon transaction metadata.

## Advanced details (manual, paged)

`double_spend_seen` is a node-reported warning, not a wallet or consensus verdict.
Clients show it prominently even with Advanced details collapsed. Missing this
additive field on an older summary server is tolerated; it is not proof of safety.

`GET /api/v1/transactions/{txid}/details/{page}` returns the same verified summary
plus a `details` object. Pages start at zero; no query parameters are accepted.
This is additive: the compact endpoint does not return these lists.

The object contains `page`, `page_size` (10), `page_count`, transaction `version`,
`coinbase`, `serialized_size_bytes`, `downloaded_size_bytes`, `weight`,
`unlock_kind` (`none`, `height`, `timestamp`), decimal-string `unlock_value`,
`input_count`, `output_count`, `inputs`, `outputs`, `extra_hex`,
`extra_truncated`, and nullable `received_timestamp` (pending only; unknown zero
timestamps are normalized to null). A pruned blob does not reveal full size or
weight: those fields remain null, never estimated as zero.

- Each input has its zero-based `index`, `key_image`, `ring_size`,
  `absolute_indices` (decimal **strings**) and `indices_truncated`.
- Each output has its zero-based `index`, one-time `key`, nullable
  `public_amount_piconero` (decimal string), nullable integer `view_tag` (0–255),
  and nullable decimal-string `global_index`. Public amounts are shown only for
  legacy v1/coinbase outputs; a RingCT transfer's amount is null, not zero.
- At most 10 inputs and 10 outputs are returned per page. Inputs and outputs
  share a page number; counts determine which side has remaining rows. Coinbase
  has one logical input but an empty key-image input list. Clients replace pages
  rather than accumulating the entire transaction.
- At most 128 ring indices per input and 2,048 bytes of extra (4,096 hex digits)
  are previewed. Truncation is explicit. These match the website's extra preview;
  unusually large legacy rings have an additional native/API index-preview cap.
- Maximum supported input/output count is 4,096 each (same decoder as the site),
  hence at most 410 pages. Invalid/out-of-range pages return 400, not empty data.
  The response stays below 64 KiB, including worst-case u64 index strings.

Every page repeats identity/network/status/block checks and an observation time;
observations across pages may change. No atomic cross-page snapshot is promised.
All three clients validate requested page, counts/contiguous row indices, integer
types/limits, hex, status, nullability and preview flags. Closing/collapsing,
backgrounding, disabling or changing wallet session cancels/invalidates work.
No next-page prefetch or retry happens automatically. A page failure does not
erase a previously displayed summary or its conflicting-spend warning. If an
older deployed server lacks the details endpoint, clients show unavailable, not
a claim that the transaction is missing.

## Errors

JSON errors have `{"api_version":1,"error":{"code":"unavailable"}}`.

- 400 `invalid_request`: invalid ID, query parameters, malformed framework request.
- 404 `not_found`: explicitly absent from a caught-up node; **not proof that a payment never existed**. Unknown API routes also return 404.
- 502 `unsupported`: unsupported operation or safety limit.
- 503 `unavailable`, `inconsistent`, or `busy`: node failure, syncing/uncertain absence, inconsistent/reorg data, or request limits. Retry-After is five seconds.

Responses have `Cache-Control: no-store`, no cookies, no CORS permission, and the same privacy/security headers as HTML. No application visitor/IP/transaction-ID logging is added. Proxy/provider/node operators can still observe IPs and queried transaction IDs. Native clients must not interpret an HTML proxy error or unknown API version as a transaction result.

## NexaWal clients

The independent **Enable explorer details** preference defaults to false on iOS, Android, and GPUI, including existing installations. Enabling the setting alone performs no lookup. A user must open **Explorer details** for one transaction. Refresh is manual. No startup/refresh/history-list/balance/send path uses this API. Copying an explorer URL does not request it.

Clients use a separate clearnet HTTPS connection even when wallet RPC uses I2P. Requests include no seed, private key, wallet address, private amount, entire history, account, or authentication token. Responses are bounded to 16 KiB (summary) or 64 KiB (advanced page), with 12-second deadlines, no redirects/cookies/persistent cache, strict schema/ID/network/status/confirmation/fee validation, and observation-age checks. The current public explorer is mainnet-only; mobile non-mainnet wallets do not make lookups. Close/background/disable invalidates results and cancels work. Foregrounding alone does not poll. Explorer data never enters WalletCore or overrides wallet records or send decisions.

`client-strings.json` contains draft app copy for all 54 existing locales. Run `node Scripts/sync-client-localizations.mjs` from the umbrella checkout to mechanically update all three clients; run `node Scripts/check-client-localizations.mjs` to check coverage/parity. The explorer website's own UI remains English. Human review of safety/privacy translations is recommended.

`client-advanced-strings.json` adds 25 advanced labels/warnings in all 54 locales.
`node Scripts/check-client-fixtures.mjs` checks that API/native contract fixtures
are byte-identical; native tests validate the same synthetic page independently.

The checker compares Android's actual generated translations and observed-time
format against the canonical catalog, not just resource names. Run
`node --test Scripts/client-localization-checks.test.mjs` to test rejection of
translation drift/English fallback, missing/duplicate/empty resources and damaged
format tokens. Client lifecycle test commands and physical-device smoke checks are
in each app's `docs/EXPLORER.md`.
