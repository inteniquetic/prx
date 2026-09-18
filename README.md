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
`Cargo.toml` patches `pingora-core` and `pingora-load-balancing` to local vendored copies under `vendor/` so `cargo audit` can run with zero exceptions.

Operational references:

- SLO baseline: `ops/SLO.md`
- Alert rules: `ops/alerts/prx-alerts.yml`
- Failure drills: `ops/FAILURE-DRILLS.md`
- Rollback runbook: `ops/ROLLBACK.md`
- Zero-exception security policy: `ops/ZERO-EXCEPTION-POLICY.md`

## TLS backend

TLS provider support depends on Pingora build features.
By default this project uses Pingora defaults; adjust Cargo features if you need a different TLS backend.
