# prx Config Wiki

This document is a complete configuration guide for `Prx.toml`, based on actual behavior implemented in `src/config.rs`, `src/runtime.rs`, `src/proxy.rs`, and `src/reload.rs`.

## 1) Quick Start

The default config file is `Prx.toml`, and you can override the path with:

```bash
PRX_CONFIG=./Prx.toml cargo run
```

When the config file is saved, the system auto-reloads without restarting the process.

## 2) Main File Structure

```toml
[server]
[observability]

[[route]]
[route.circuit_breaker]
[[route.upstream]]
```

Minimum requirements:
- At least one `[[route]]` block is required.
- Each route must include at least one `[[route.upstream]]` block.

## 3) Field Reference

### 3.1 `[server]`

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `listen` | `string[]` | `["0.0.0.0:8080"]` | No | HTTP listeners |
| `health_path` | `string` | `"/healthz"` | No | Health endpoint path |
| `ready_path` | `string` | `"/readyz"` | No | Readiness endpoint path |
| `threads` | `number` | `null` | No | Number of Pingora worker threads |
| `grace_period_seconds` | `number` | `null` | No | Grace period before shutdown |
| `graceful_shutdown_timeout_seconds` | `number` | `null` | No | Timeout for graceful shutdown |
| `config_reload_debounce_ms` | `number` | `250` | No | Debounce for auto-reload |
| `tls` | `table` | `null` | No | Enable HTTPS listener |

Validation:
- `health_path` and `ready_path` must start with `/`.
- `health_path` and `ready_path` must be different.

### 3.2 `[server.tls]`

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `listen` | `string` | - | Yes (if `server.tls` exists) | HTTPS listener, e.g. `0.0.0.0:8443` |
| `cert_path` | `string` | - | Yes | Certificate path |
| `key_path` | `string` | - | Yes | Private key path |
| `enable_h2` | `bool` | `true` | No | Enable HTTP/2 on TLS listener |

### 3.3 `[observability]`

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `log_level` | `string` | `"info"` | No | logging level |
| `access_log` | `bool` | `true` | No | Enable/disable access log |
| `prometheus_listen` | `string` | `null` | No | Enable metrics endpoint (separate listener) |

### 3.4 `[[route]]`

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `name` | `string` | `"default"` | No | Route name |
| `host` | `string` | `null` | No | host matcher |
| `path_prefix` | `string` | `"/"` | No | path prefix matcher |
| `is_default` | `bool` | `false` | No | Fallback route when no match |
| `lb` | enum | `"round_robin"` | No | `round_robin`, `random`, `hash` |
| `max_retries` | `number` | `0` | No | Retries per request |
| `retry_backoff_ms` | `number` | `0` | No | Backoff before retry |
| `circuit_breaker` | `table` | defaults | No | passive circuit breaker |
| `upstream` | array | - | Yes | Upstream list |

Validation:
- `path_prefix` must not be empty and must start with `/`.
- At most one route can have `is_default = true`.

Host matching:
- `host = "api.example.com"`: exact match
- `host = "*.example.com"`: matches both `foo.example.com` and `example.com`.
- If `host` is `null`: matches all hosts.
- Host is normalized to lowercase and `:port` is stripped before matching.

Path matching:
- Uses `starts_with(path_prefix)`.
- Routes are sorted so longer `path_prefix` values match first.

### 3.5 `[route.circuit_breaker]`

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `enabled` | `bool` | `false` | No | Enable/disable circuit breaker |
| `consecutive_failures` | `number` | `3` | No | Consecutive failures before opening circuit |
| `open_ms` | `number` | `30000` | No | Open-state duration |

Validation (when `enabled = true`):
- `consecutive_failures > 0`
- `open_ms > 0`

### 3.6 `[[route.upstream]]`

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `addr` | `string` | - | Yes | Upstream address, e.g. `10.0.0.5:8080` |
| `tls` | `bool` | `false` | No | Connect to upstream via TLS |
| `sni` | `string` | auto | No | SNI for upstream TLS |
| `weight` | `number` | `1` | No | Load balancing weight |
| `verify_cert` | `bool` | runtime `true` | No | verify certificate |
| `verify_hostname` | `bool` | runtime `true` | No | verify hostname |
| `connect_timeout_ms` | `number` | `null` | No | connect timeout |
| `total_connect_timeout_ms` | `number` | `null` | No | total connection timeout |
| `read_timeout_ms` | `number` | `null` | No | read timeout |
| `write_timeout_ms` | `number` | `null` | No | write timeout |
| `idle_timeout_ms` | `number` | `null` | No | idle timeout |

Runtime notes:
- If `sni` is not set, the system derives it from `addr` when possible; otherwise it uses `"localhost"`.
- `weight` is clamped to `1..256`.
- Requests sent upstream rewrite the `Host` header to `upstream.sni`.

## 4) Important Behavior to Know

### 4.1 Route fallback

If no route matches `(host, path)`:
- If a route has `is_default = true`, that route is used.
- If no default route exists, the response is `404`.

### 4.2 Health/Readiness

`health_path` and `ready_path` are handled before route matching:
- `health_path` returns `200 ok`.
- `ready_path` returns:
  - `200 ready` when every route has at least one available upstream.
  - `503 not_ready` when any route has no available upstream.

### 4.3 Retry + Circuit breaker

- Retry follows `max_retries` and does not select an upstream already tried within the same request.
- On connect/proxy failure, failures are counted to trigger the route circuit breaker policy.
- If new config parsing/validation fails during reload, the previous config is kept.

## 5) Common Validation Errors

- `config must include at least one [[route]] block`
- `server.health_path must start with '/'`
- `server.ready_path must start with '/'`
- `server.health_path and server.ready_path must be different`
- `route '<name>' must include at least one [[route.upstream]]`
- `route '<name>' has empty path_prefix`
- `route '<name>' path_prefix must start with '/'`
- `route '<name>' includes upstream with empty addr`
- `only one route can be marked is_default = true`

## 6) Full Config Example (Production-style Baseline)

```toml
[server]
listen = ["0.0.0.0:8080"]
health_path = "/healthz"
ready_path = "/readyz"
threads = 4
grace_period_seconds = 10
graceful_shutdown_timeout_seconds = 30
config_reload_debounce_ms = 250

[server.tls]
listen = "0.0.0.0:8443"
cert_path = "/etc/prx/tls.crt"
key_path = "/etc/prx/tls.key"
enable_h2 = true

[observability]
log_level = "info"
access_log = true
prometheus_listen = "0.0.0.0:9090"

[[route]]
name = "api"
host = "api.example.com"
path_prefix = "/"
is_default = false
lb = "round_robin"
max_retries = 1
retry_backoff_ms = 25

[route.circuit_breaker]
enabled = true
consecutive_failures = 3
open_ms = 30000

[[route.upstream]]
addr = "10.0.1.10:8080"
weight = 2
connect_timeout_ms = 1000
read_timeout_ms = 30000
write_timeout_ms = 30000
idle_timeout_ms = 30000

[[route.upstream]]
addr = "10.0.1.11:8080"
weight = 1
connect_timeout_ms = 1000
read_timeout_ms = 30000
write_timeout_ms = 30000
idle_timeout_ms = 30000

[[route]]
name = "web-default"
host = "*.example.com"
path_prefix = "/"
is_default = true
lb = "hash"
max_retries = 1
retry_backoff_ms = 0

[route.circuit_breaker]
enabled = true
consecutive_failures = 3
open_ms = 30000

[[route.upstream]]
addr = "10.0.2.10:3000"
weight = 2
connect_timeout_ms = 1000

[[route.upstream]]
addr = "10.0.2.11:3000"
weight = 1
connect_timeout_ms = 1000
```

## 7) Related Docs

- `docs/CONFIG-PLAYBOOK.md`
- `ops/SLO.md`
- `ops/alerts/prx-alerts.yml`
- `ops/FAILURE-DRILLS.md`
- `ops/ROLLBACK.md`
