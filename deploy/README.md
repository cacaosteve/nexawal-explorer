# Mini / Incus deployment

The explorer is a separate Docker Compose project inside `cuprate-vm`, alongside
but not part of the Cuprate Compose project. It mounts no chain data and uses
only the existing restricted RPC at `http://10.62.67.10:18092`.

Host Caddy terminates HTTPS for `explorer.nexawal.com` and proxies to
`10.62.67.10:8088`. That listener is on the private Incus interface, not a WAN
port forward. Do not expose port 8088 on the router. The public DNS record is
`explorer.nexawal.com CNAME mini.nexatrode.com`.

## Build and install

Build a clean, committed source revision on the Mini host, not inside the node
VM. The Docker build defaults to two Cargo jobs to limit build contention.
Record the full source SHA and image ID with each deployment. The initial
runtime image is `nexawal-explorer:0057061`.

```sh
cd /home/juan/nexawal-explorer
git status --short
git rev-parse HEAD
docker build --build-arg CARGO_BUILD_JOBS=2 -t nexawal-explorer:0057061 .
docker save nexawal-explorer:0057061 | incus exec -T cuprate-vm -- docker load
incus exec -n cuprate-vm -- install -d -m 0755 /opt/nexawal-explorer
incus file push deploy/compose.incus.yml cuprate-vm/opt/nexawal-explorer/compose.yml
incus file push deploy/production.env.example cuprate-vm/opt/nexawal-explorer/.env
incus exec -n cuprate-vm -- docker compose \
  --env-file /opt/nexawal-explorer/.env \
  -f /opt/nexawal-explorer/compose.yml config --quiet
incus exec -n cuprate-vm -- docker compose \
  --env-file /opt/nexawal-explorer/.env \
  -f /opt/nexawal-explorer/compose.yml up -d --no-build
curl --fail http://10.62.67.10:8088/readyz
```

For future releases, substitute the actual source commit for `0057061` and
update the runtime `.env` before `up`. Do not retag a different source image
with an existing commit tag. `deploy/compose.incus.yml` is standalone; do not
combine it with the local loopback-only `compose.yml`. The runtime has a
read-only filesystem, non-root UID, dropped capabilities, 128-PID limit,
256 MiB memory limit, and one-CPU limit. Docker's restart policy and the
existing VM autostart configuration handle process/host restarts.

## Caddy

Merge `Caddyfile.incus.example` into the existing host configuration at
`/home/juan/supermegamart/caddy/Caddyfile`, preserving every unrelated site and
existing local edit. Merge logging into the one global block, not a second
global block. The existing custom Caddy image includes `mholt/caddy-ratelimit`.

The explorer allows 10 requests/second and 120/minute per remote IP (IPv6 /64),
with a 1 KiB request-body limit. This is not distributed DDoS protection. HTTP
error and access logs for the explorer are discarded. The rate-limit module
itself always logs `remote_ip`, so its logger removes `remote_ip` and `key`
across all zones while preserving zone/wait diagnostics. Other HTTP access,
error, and operational namespaces are unchanged. Additional catch-all
loggers, global debug/handler tracing, and provider logs must be audited
separately. Do not enable per-key metrics without reviewing privacy.

Before applying, back up the current file outside Git, validate a candidate
using the installed Caddy binary, then reload without restarting the container:

```sh
docker exec caddy caddy validate --config /tmp/Caddyfile.explorer-candidate --adapter caddyfile
docker exec caddy caddy reload --config /etc/caddy/Caddyfile --adapter caddyfile
```

The candidate must be copied into the container before validation. Never replace
the live Caddyfile with an incomplete explorer-only example. The backup for the
initial deployment is recorded with the deployment verification notes.

The initial backup is
`/home/juan/supermegamart/caddy/Caddyfile.before-explorer-20261018T2156Z`.

On the Mini, test the actual custom proxy image without touching live Caddy:

```sh
python3 Scripts/test-incus-proxy.py
```

This runs only synthetic requests against a disposable loopback-bound container,
checks outage and rate-limit behavior, verifies log redaction, and removes the
test container. The existing Node regression separately tests the generic
stock-Caddy example.

## Checks and rollback

Check `/healthz` for liveness and `/readyz` for node readiness. A stale tip can
make readiness fail even though historical pages are still usable with warnings;
do not continuously restart a healthy process to fix a stale node. Monitor memory,
CPU, upstream availability, certificate renewal, and disk usage without retaining
visitor searches. No recurring monitoring is configured by these files.

Verify the home page, a block, a transaction, mempool, HTML search redirect, CSS,
privacy headers, HTTP-to-HTTPS redirect, and the public RPC after any change.
Compare Cuprate's container ID/start time to ensure it was not replaced.

For an explorer-only rollback, restore and validate the saved Caddyfile, reload
Caddy, then stop just the explorer project. For an application update rollback,
restore the previous image tag in the explorer `.env` and run `up -d --no-build`.
Do not stop Cuprate, alter its config/database, remove volumes, restart the VM,
or run `docker compose down -v`. Wallet apps still use their existing external
explorer links until a separate explicitly requested change.
