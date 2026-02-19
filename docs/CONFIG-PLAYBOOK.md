# prx Config Playbook

This document focuses on practical `Prx.toml` examples you can copy and use immediately.

## 1) Minimal HTTP Proxy

```toml
[server]
listen = ["0.0.0.0:8080"]

[[route]]
name = "default"
path_prefix = "/"
is_default = true

[[route.upstream]]
addr = "127.0.0.1:3000"
```

Test:

```bash
curl http://127.0.0.1:8080/
curl http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/readyz
```

## 2) Host-based Routing

```toml
[server]
listen = ["0.0.0.0:8080"]

[[route]]
name = "grpc"
host = "grpc.local"
path_prefix = "/"
is_default = false
lb = "round_robin"

[[route.upstream]]
addr = "127.0.0.1:50051"

[[route]]
name = "web"
host = "*.local"
path_prefix = "/"
is_default = true
lb = "hash"

[[route.upstream]]
addr = "127.0.0.1:3000"
weight = 2

[[route.upstream]]
addr = "127.0.0.1:3001"
weight = 1
```

Test host routing:

```bash
curl -H "Host: grpc.local" http://127.0.0.1:8080/
curl -H "Host: app.local" http://127.0.0.1:8080/
```

## 3) Retry + Failover + Circuit Breaker

```toml
[server]
listen = ["0.0.0.0:8080"]

[[route]]
name = "api"
host = "api.local"
path_prefix = "/"
is_default = false
lb = "round_robin"
max_retries = 1
retry_backoff_ms = 50

[route.circuit_breaker]
enabled = true
consecutive_failures = 3
open_ms = 30000

[[route.upstream]]
addr = "127.0.0.1:8081"
connect_timeout_ms = 1000
read_timeout_ms = 30000
write_timeout_ms = 30000

[[route.upstream]]
addr = "127.0.0.1:8082"
connect_timeout_ms = 1000
read_timeout_ms = 30000
write_timeout_ms = 30000
```

Concept:
- If the first upstream fails, retry moves to the next upstream.
- If failures keep happening until the threshold, the circuit opens temporarily.

## 4) TLS Frontend + Metrics

```toml
[server]
listen = ["0.0.0.0:8080"]

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
name = "default"
path_prefix = "/"
is_default = true

[[route.upstream]]
addr = "10.0.0.10:8080"
```

Test:

```bash
curl -k https://127.0.0.1:8443/healthz
curl http://127.0.0.1:9090/metrics
```

## 5) Upstream TLS (mTLS/strict TLS not included in this config)

```toml
[[route]]
name = "secure-upstream"
host = "secure.local"
path_prefix = "/"
is_default = false
lb = "round_robin"

[[route.upstream]]
addr = "upstream.internal:443"
tls = true
sni = "upstream.internal"
verify_cert = true
verify_hostname = true
connect_timeout_ms = 1000
read_timeout_ms = 30000
```

Note:
- If `verify_cert`/`verify_hostname` are not set, runtime defaults to `true`.

## 6) Go-live Checklist (Config)

- Define `host` and `path_prefix` clearly for every route.
- Have no more than one default route, and make sure it is intentionally used as fallback.
- Set at least `connect_timeout_ms` and `read_timeout_ms`.
- Tune `max_retries` to fit your latency budget.
- Enable circuit breaker only on routes that need fail-fast behavior.
- Enable `prometheus_listen` and wire alert rules.
- Ensure `health_path`/`ready_path` do not conflict with main app paths.
- Test auto-reload by editing `Prx.toml` and verifying reload success in logs.

## 7) Quick Troubleshooting

### Getting `404` even though a route should match

- Check the incoming `Host` header (including `curl` calls missing `-H "Host: ..."`).
- Check whether `path_prefix` actually covers the request path.
- Check that no other route with a longer path matches first.

### `readyz` returns `503 not_ready`

- At least one route has all upstreams in open circuit.
- Check `prx_upstream_circuit_open` and `prx_circuit_breaker_open_total`.

### Config does not change after editing file

- Parser/validation may have failed, and the system kept the previous config.
- Check logs for `failed to reload config, keeping previous version`.
