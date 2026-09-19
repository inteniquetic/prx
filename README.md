# prx

`prx` is a high-performance reverse proxy built on top of Pingora.

## Highlights

- Async Rust runtime
- HTTP/1.1 + HTTP/2 proxy path
- gRPC proxying (HTTP/2 end to end, trailers preserved)
- WebSocket proxying, including long-idle connections
- Load balancing: `round_robin`, `random`, `hash`, `least_conn`, `p2c_ewma` (power of two choices, latency aware)
- Session affinity by cookie, client IP or header
- Per-route request/response header rules (`X-Forwarded-For`, `X-Real-IP`, security headers)
- Per-route rate limiting and concurrency limiting
- Short-lived response cache with request coalescing (one upstream fetch per cold key)
- Response compression (gzip, brotli, zstd), streaming rather than buffered
- Route-level failover retry
- Passive per-route circuit breaker for unhealthy upstreams
- Active health checking that removes a failing upstream before a request finds it
- Graceful reload support from Pingora runtime
- Config-driven behavior via `Prx.toml`
- Auto config reload when `Prx.toml` is saved
- Built-in health and readiness endpoints
- Prometheus-compatible metrics, including custom prx routing/upstream metrics

## Run

```bash
cargo run
```

Optional:

```bash
PRX_CONFIG=./Prx.toml cargo run
```

Admin API is enabled by default on a dedicated listener (separate from proxy traffic):

```bash
cargo run
```

Endpoints:
- `GET /` embedded WebUI (SPA)
- `GET /web/config` read current `Prx.toml` (TOML text)
- `GET /web/config?format=json` read normalized config payload for WebUI
- `GET /web/health/routes` check route upstream TCP health status
- `POST /web/health/routes` check health from provided TOML payload (used by WebUI draft)
- `PUT /web/config` write new `Prx.toml` (validated before apply)
- `GET /web/cache` cache statistics per route
- `DELETE /web/cache[?route=<name>]` purge cached responses

Note: `webui/dist` is embedded at compile time. Rebuild `prx` after `webui` changes.

Optional override:

```bash
PRX_ADMIN_LISTEN=127.0.0.1:9091 cargo run
```

## gRPC and WebSocket

gRPC needs HTTP/2 from the client all the way to the upstream, because it
carries its status in trailers:

```toml
[server]
h2c = true                 # accept cleartext HTTP/2 (default: true)

[[service]]
name = "grpc-api"
upstream_h2 = "always"     # speak HTTP/2 to the upstream (default: "never")
```

WebSocket upgrades work with no extra configuration. The upstream
read/write/idle timeouts are not applied to an upgraded connection, so an idle
websocket is not dropped.

Both paths are covered by end-to-end tests: `tests/e2e_grpc.rs` and
`tests/e2e_websocket.rs`.

## Config

The proxy reads `Prx.toml` on startup and watches it for changes.
When the file is updated, the active routing/upstream config is reloaded without restarting the process.

Reference config: `Prx.toml`
Config wiki:
- `docs/CONFIG-WIKI.md` (full reference)
- `docs/CONFIG-PLAYBOOK.md` (ready-to-use examples)

## Roadmap

Planned work is broken down into small, independently shippable tasks:
`docs/tasks/README.md`

## Benchmarks

Measured numbers and how to reproduce them: `docs/BENCHMARKS.md`.
The harness that compares prx against nginx and haproxy lives in `bench/`:

```bash
scripts/bench.sh --all h1-keepalive   # needs docker + oha
make bench-micro                      # criterion micro-benchmarks, no docker
```

Profiling guide: `docs/PROFILING.md`.

No performance claim belongs in this README unless `docs/BENCHMARKS.md` carries
the number behind it.

Key config knobs:

- `[server].health_path` and `[server].ready_path` for liveness/readiness probes
- `[[route]].max_retries` and `[[route]].retry_backoff_ms` for retry behavior
- `[route.circuit_breaker]` for passive trip/open behavior per route
- `[observability].prometheus_listen` for `/metrics` endpoint

## Test

```bash
cargo test --all-targets
```

This includes end-to-end tests that run the real binary:

- `tests/e2e_proxy.rs`: route matching, host and path precedence, method
  filtering (`405`), `404` on no matching route, retry + upstream failover,
  health and readiness handlers
- `tests/e2e_websocket.rs`: upgrade handshake, text/binary frames, large
  frames, close handshake, idle connections
- `tests/e2e_grpc.rs`: unary calls with trailers, non-zero `grpc-status`,
  server streaming

## Release Gate

```bash
scripts/release-gate.sh
```

Security hardening note:
prx builds against the published pingora 0.9 crates — there is no `vendor/`
fork and no `[patch.crates-io]` section. `cargo audit` runs with no ignore
flags and exits 0; the warning-class advisories that remain are listed with
reasons in `ops/ZERO-EXCEPTION-POLICY.md`.

Operational references:

- SLO baseline: `ops/SLO.md`
- Alert rules: `ops/alerts/prx-alerts.yml`
- Failure drills: `ops/FAILURE-DRILLS.md`
- Rollback runbook: `ops/ROLLBACK.md`
- Zero-exception security policy: `ops/ZERO-EXCEPTION-POLICY.md`

## TLS

prx builds pingora with its `openssl` TLS backend, so the TLS listener is a
working one: building requires `libssl-dev` and running requires `libssl3`
(the Dockerfile installs both).

Certificates are chosen per connection from the client's SNI, so one listener
can serve several domains:

```toml
[server.tls]
listen = "0.0.0.0:8443"
enable_h2 = true

[[server.tls.cert]]
domains = ["example.com", "*.example.com"]
cert_path = "./certs/example.crt"
key_path = "./certs/example.key"
is_default = true
```

They are loaded and validated at startup: an unreadable file, or a key that
does not match its certificate, stops the process rather than failing every
handshake. `prx_tls_cert_expiry_seconds{domain}` reports how long each one has
left.

### Automatic certificates (ACME)

Or let prx obtain and renew them itself. It answers the HTTP-01 challenge on
its own plaintext listener and installs the issued certificate into the
running TLS listener without a restart:

```toml
[server]
listen = ["0.0.0.0:80"]

[server.tls]
listen = "0.0.0.0:443"

[server.tls.acme]
enabled = true
email = ["ops@example.com"]
domains = ["example.com", "www.example.com"]
directory_url = "https://acme-v02.api.letsencrypt.org/directory"
storage_dir = "/var/lib/prx/acme"
```

`directory_url` defaults to Let's Encrypt **staging**, so a misconfigured
deployment cannot burn through the production rate limits; switch it once a
staging run has issued successfully. If the ACME server is unreachable the
certificate already in use keeps serving and the error is reported by
`GET /web/tls/status` — a failing renewal never takes the proxy down. Only
HTTP-01 is implemented, so wildcard domains are rejected at config load.

See `docs/CONFIG-WIKI.md` for the full reference.
