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
  "observed_at": 1700000010
}
```

Illustrative values only. `chain_height` counts blocks, so confirmations include the containing block: `chain_height - block_height`. Fees are decimal **strings of integer piconero**, never floating-point XMR or private payment amounts. Unknown fee is null, never assumed zero. Pending entries have `status: "pending"`, null block fields, and zero confirmations.

Confirmed entries reuse blob-ID verification and containing-block membership checks. This is still one node's observation, not independent consensus verification. It does not prove recipient identity, private RingCT amount, spendability, finality, or receipt by a wallet. Cached snapshots may lag up to five seconds and are not atomic across a reorg. `node_ready: false` warns that the node is syncing/offline or tip freshness is unverified. `observed_at` is the snapshot's Unix time in seconds; it is not transaction time. A block timestamp is taken from the containing block, not daemon transaction metadata.

## Errors

JSON errors have `{"api_version":1,"error":{"code":"unavailable"}}`.

- 400 `invalid_request`: invalid ID, query parameters, malformed framework request.
- 404 `not_found`: explicitly absent from a caught-up node; **not proof that a payment never existed**. Unknown API routes also return 404.
- 502 `unsupported`: unsupported operation or safety limit.
- 503 `unavailable`, `inconsistent`, or `busy`: node failure, syncing/uncertain absence, inconsistent/reorg data, or request limits. Retry-After is five seconds.

Responses have `Cache-Control: no-store`, no cookies, no CORS permission, and the same privacy/security headers as HTML. No application visitor/IP/transaction-ID logging is added. Proxy/provider/node operators can still observe IPs and queried transaction IDs. Native clients must not interpret an HTML proxy error or unknown API version as a transaction result.

## NexaWal clients

The independent **Enable explorer details** preference defaults to false on iOS, Android, and GPUI, including existing installations. Enabling the setting alone performs no lookup. A user must open **Explorer details** for one transaction. Refresh is manual. No startup/refresh/history-list/balance/send path uses this API. Copying an explorer URL does not request it.

Clients use a separate clearnet HTTPS connection even when wallet RPC uses I2P. No seed, key, address, amount, entire history, account, or authentication token is included. Responses are bounded to 16 KiB with 12-second deadlines, no redirects/cookies/persistent cache, strict schema/ID/network/status/confirmation/fee validation, and observation-age checks. The current public explorer is mainnet-only; mobile non-mainnet wallets do not make lookups. Close/background/disable invalidates results and cancels work. Foregrounding alone does not poll. Explorer data never enters WalletCore or overrides wallet records or send decisions.

`client-strings.json` contains draft app copy for all 54 existing locales. Run `node Scripts/sync-client-localizations.mjs` from the umbrella checkout to mechanically update all three clients; run `node Scripts/check-client-localizations.mjs` to check coverage/parity. The explorer website's own UI remains English. Human review of safety/privacy translations is recommended.
