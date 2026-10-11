# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`prx` is a reverse proxy built on Pingora (Rust), with an embedded Svelte WebUI for managing its config. Config lives in `Prx.toml`, hot-reloads on file change, and can also be edited through an admin HTTP API that the WebUI calls.

## Commands

All common tasks are in the `Makefile` — prefer `make <target>` over raw `cargo` invocations.

```bash
make run          # cargo run (uses ./Prx.toml, override with PRX_CONFIG)
make build         # cargo build
make fmt           # cargo fmt --all
make fmt-check     # cargo fmt --all --check
make clippy        # cargo clippy --all-targets -- -D warnings
make test          # cargo test --all-targets -- --test-threads=1
make audit         # cargo audit (installs cargo-audit if missing)
make gate          # full release gate: fmt-check + clippy + test + audit (scripts/release-gate.sh)
```

Run a single test: `cargo test --all-targets -- --test-threads=1 <test_name>`

Tests **must** run single-threaded (`--test-threads=1`) — `tests/e2e_proxy.rs` binds real TCP ports for upstream/proxy servers per test, and parallel runs collide on ports.

Before considering Rust work done, run `make gate` (or at minimum `fmt-check`, `clippy`, `test`) — CI (`.github/workflows/ci.yml`) runs the same four steps and `cargo audit` must stay at **zero exceptions** (see `ops/ZERO-EXCEPTION-POLICY.md`); do not add `#[allow]` or audit ignores to work around a failure.

### WebUI (`webui/`)

```bash
cd webui
npm install
npm run dev        # vite dev server
npm run build       # vite build -> webui/dist
npm run check       # svelte-check
```

`webui/dist` is embedded into the Rust binary at compile time via `include_dir!` in `src/admin.rs`. **After changing anything in `webui/`, rebuild `webui` (`npm run build`) and then rebuild `prx`** — the running admin server serves whatever was embedded at last compile, not live files.

## Architecture

### Rust proxy core (`src/`)

- `main.rs` — process entrypoint. Loads `Prx.toml`, builds the Pingora `Server`, wires up the proxy service (`PrxProxy`), the admin service (`AdminAxumService`), the config file watcher, and optionally a Prometheus metrics service. Everything is composed here; there's no DI framework.
- `config.rs` — `PrxConfig` and friends: the serde/TOML-deserializable shape of `Prx.toml` (`server`, `observability`, `[[service]]`, `[[route]]`). `PrxConfig::validate()` is the single source of truth for config invariants (unique service names, routes reference existing services, at most one default route, circuit breaker thresholds > 0, etc.) — both file-load and the admin `PUT /web/config` path go through this.
- `runtime.rs` — `RuntimeConfig`: the *resolved* runtime form of `PrxConfig`, built once via `RuntimeConfig::from_config`. Route service names are resolved to indices, routes are pre-sorted by path-prefix length (longest-prefix-wins matching), and each upstream gets a weighted selection ring (`build_selection_ring`) plus atomic circuit-breaker state (`UpstreamState`). This is the hot-path data structure the proxy reads per-request — never mutated in place, only atomically swapped (see below).
- `proxy.rs` — `PrxProxy`, the Pingora `ProxyHttp` implementation. Per-request flow: `request_filter` matches host+path to a route via `RuntimeConfig::select_route` (also short-circuits health/ready paths), `upstream_peer` picks an upstream via the service's LB strategy and applies per-upstream timeouts, `fail_to_connect`/`error_while_proxy` record failures against the circuit breaker and decide retry via `should_retry` (bounded by `max_retries` and by number of distinct upstreams already attempted), and `logging` emits access logs + Prometheus metrics.
- `admin.rs` — Axum-based admin HTTP service, run as a second Pingora `Service` on its own listener (`PRX_ADMIN_LISTEN`, default `127.0.0.1:9090`). Serves the embedded WebUI (SPA fallback to `index.html` for extensionless paths), `GET/PUT /web/config` (raw TOML or normalized JSON), `GET/POST /web/health/routes` (live TCP health probe per upstream), and REST CRUD for services/routes (`/admin/services*`, `/admin/routes*`). All config mutations go through `ConfigAdmin`, which: takes a `Mutex` write lock, atomically replaces `Prx.toml` (temp file + `rename` + fsync), re-parses+validates, and on failure **rolls back** the file to its previous bytes before returning an error. `modify_config` (used by the CRUD handlers) is a read-modify-validate-write-swap cycle around a closure.
- `reload.rs` — a dedicated OS thread (not tokio) watching the config file's parent directory via `notify`, debounced, that reloads+validates `Prx.toml` and atomically swaps the active `RuntimeConfig` on success. Invalid edits are logged and ignored — the previous config keeps running.
- `metrics.rs` — Prometheus metric definitions (`prx_requests_total`, `prx_request_latency_ms`, `prx_upstream_errors_total`, `prx_circuit_breaker_open_total`, `prx_upstream_circuit_open`) as lazily-registered globals, exposed via `prometheus_http_service` when `observability.prometheus_listen` is set.

**Key pattern: config is never mutated in place.** The active config is `Arc<ArcSwap<RuntimeConfig>>`, shared between the proxy hot path, the admin API, and the file watcher. Any config change — from disk edit or admin API write — builds a brand-new `RuntimeConfig` and atomically swaps the `Arc`. In-flight requests keep using the snapshot they loaded at the start of `request_filter`.

**Key pattern: config validation has one entrypoint.** `PrxConfig::validate()` in `config.rs` is called by `from_toml_str` (used by file load and admin PUT) — extend it rather than adding ad hoc checks elsewhere. The admin CRUD handlers (`admin.rs`) additionally do payload-shape checks (e.g. non-empty addr) before calling into `modify_config`, which re-validates the whole config anyway.

### Vendored dependencies (`vendor/`)

`Cargo.toml` patches `pingora-core` and `pingora-load-balancing` to local copies under `vendor/` to satisfy the zero-exception `cargo audit` policy (patched `prometheus`/`protobuf` versions, removed `daemonize`/`derivative`). See `ops/ZERO-EXCEPTION-POLICY.md` before touching these — don't revert to crates.io versions without checking whether upstream has shipped the same fixes.

### WebUI (`webui/`)

Svelte 4 + TypeScript + Tailwind SPA, structured by feature:

- `src/lib/api/admin.ts` — fetch wrappers for the admin HTTP API.
- `src/lib/stores/config.ts`, `navigation.ts` — Svelte stores for loaded config state and current page.
- `src/lib/configCodec.ts` / `configNormalize.ts` — conversions between the admin API's JSON shape and the form/UI model.
- `src/lib/components/pages/` — top-level pages (Dashboard, Routes, Services, Settings), composed from `components/dashboard/`, `components/routes/`, `components/services/` widgets.

The WebUI talks only to the admin API (`/web/config`, `/web/health/routes`, `/admin/services*`, `/admin/routes*`) — it has no separate backend.

## Config model (`Prx.toml`)

Reference: `Prx.toml` in repo root, `docs/CONFIG-WIKI.md` (full reference), `docs/CONFIG-PLAYBOOK.md` (examples). Shape in brief:

- `[server]` — listeners, health/ready paths, TLS, thread/graceful-shutdown tuning, reload debounce.
- `[observability]` — log level, access log toggle, Prometheus listener.
- `[[service]]` — named upstream pool: LB strategy (`round_robin` | `random` | `hash`), retry count/backoff, circuit breaker, one or more `[[service.upstream]]` (addr, TLS, weight, per-connection timeouts).
- `[[route]]` — maps `host` (supports `*.suffix` wildcard) + `path_prefix` to a `service` by name; at most one route may set `is_default = true` as the catch-all.

## Operational references

Not something to derive from code — check these before making changes with production impact:

- `ops/SLO.md` — SLO baseline
- `ops/alerts/prx-alerts.yml` — alert rules
- `ops/FAILURE-DRILLS.md` — failure drills
- `ops/ROLLBACK.md` — rollback runbook
- `ops/ZERO-EXCEPTION-POLICY.md` — security audit policy (see Vendored dependencies above)
