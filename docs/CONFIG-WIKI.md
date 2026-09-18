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
| `h2c` | `bool` | `true` | No | Accept HTTP/2 over cleartext (needed for gRPC without TLS) |
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
| `methods` | `array` | `[]` | No | allowed HTTP methods (empty = any) |
| `request_headers` | table | `{}` | No | header rules applied before the upstream sees the request |
| `response_headers` | table | `{}` | No | header rules applied to the upstream response |
| `is_default` | `bool` | `false` | No | Fallback route when no match |
| `lb` | enum | `"round_robin"` | No | `round_robin`, `random`, `hash`, `least_conn`, `p2c_ewma` |
| `max_retries` | `number` | `0` | No | Retries per request |
| `retry_backoff_ms` | `number` | `0` | No | Backoff before retry |
| `circuit_breaker` | `table` | defaults | No | passive circuit breaker |
| `upstream` | array | - | Yes | Upstream list |

Validation:
- `path_prefix` must not be empty and must start with `/`.
- At most one route can have `is_default = true`.
- Every entry in `methods` must be a known HTTP method (case-insensitive):
  `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, `TRACE`, `CONNECT`.

Host matching:
- `host = "api.example.com"`: exact match
- `host = "*.example.com"`: matches both `foo.example.com` and `example.com`.
- If `host` is `null` or `""`: matches all hosts.
- Host is normalized to lowercase and `:port` is stripped before matching.

Path matching:
- Uses `starts_with(path_prefix)`.

### Matching precedence

Requests are matched against an index built at load time, in this order:

1. **Host tier**: exact host wins over wildcard host, which wins over a route
   with no host. Among wildcards, the longest suffix wins
   (`*.api.example.com` before `*.example.com`).
2. **Path**: within the winning host, the longest `path_prefix` wins.
3. **Config order**: routes that tie on host and path are resolved by the order
   they appear in the file — the first one wins.
4. **Default**: a route with `is_default = true` is used when nothing matched.

The first host tier that covers the path decides the request. A more specific
host does not fall through to a broader one.

### Method matching

`methods` is a hard filter on the route that wins the path match:

- empty list (the default) accepts every method;
- otherwise, only the listed methods are accepted, and any other method gets
  `405 method_not_allowed`;
- prx does **not** fall back to a broader route or to the default route on a
  method mismatch, because that would route a request past the restriction
  you asked for;
- a method prx does not know (for example `PROPFIND`) only matches routes that
  list no methods at all.

### 3.3a `[server.tls]` — TLS listener and certificates

| Field | Type | Default | Description |
|---|---|---|---|
| `listen` | `string` | - | Address for the TLS listener |
| `enable_h2` | `bool` | `true` | Offer HTTP/2 through ALPN |
| `cert_path` / `key_path` | `string` | - | Single-certificate form, still supported |
| `[[server.tls.cert]]` | table array | - | One entry per certificate |

```toml
[server.tls]
listen = "0.0.0.0:8443"
enable_h2 = true

[[server.tls.cert]]
domains = ["example.com", "*.example.com"]
cert_path = "./certs/example.crt"
key_path = "./certs/example.key"
is_default = true

[[server.tls.cert]]
domains = ["api.other.com"]
cert_path = "./certs/other.crt"
key_path = "./certs/other.key"
```

prx picks the certificate from the SNI the client sent: an exact domain first,
then the longest matching wildcard (`*.example.com` also covers
`example.com`). A client that sends no SNI, or a name nothing covers, gets the
entry marked `is_default`, or the first one; refusing the handshake instead
would be a worse failure mode.

`domains` may be omitted, in which case the names are read from the
certificate's own SAN entries.

Certificates are loaded and checked **at startup**, not during a handshake, so
these stop the process instead of breaking every client:

- a cert or key file that cannot be read or parsed;
- a private key that does not match its certificate;
- a TLS listener with no certificate configured at all.

A certificate expiring within 14 days is logged as a warning at startup, and
`prx_tls_cert_expiry_seconds{domain}` reports the seconds remaining so it can
be alerted on.

**Build requirement:** the TLS listener needs pingora's `openssl` feature,
which `Cargo.toml` enables. Building needs `libssl-dev` and running needs
`libssl3`; the Dockerfile installs both. Without that feature pingora compiles
a stub whose handshake is `unimplemented!()`, so a TLS listener would accept
connections and then panic the worker.

### 3.3b `[compression]`

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `false` | Compress responses the client is willing to accept compressed |
| `level` | `number` | `4` | 1–11; higher costs CPU for a smaller body |
| `decompress_upstream` | `bool` | `false` | Decode an upstream response the client cannot accept |

```toml
[compression]
enabled = true
level = 4
```

prx uses pingora's streaming compressor, so a large response is compressed as
it flows through instead of being buffered first. `gzip`, `br` and `zstd` are
all negotiated from the client's `Accept-Encoding`; a client that sends none
gets the body untouched, and a response the upstream already encoded is passed
through rather than compressed twice.

Compression is a CPU cost, so measure before turning it on for everything:
`docs/BENCHMARKS.md` explains how to run the same scenario with and without it.

**Interaction with `[route.cache]`:** the cache stores the response as it came
from the upstream, and compression happens afterwards on the way to each
client, so a cached entry serves every encoding correctly and is stored once.

### 3.4 `[[service]]` — load balancing

| `lb` | Behavior |
|---|---|
| `round_robin` (default) | Walk the weighted ring in order |
| `random` | Start at a random point in the ring |
| `hash` | Pick from the ring by hashing host + path |
| `least_conn` | Of two randomly sampled upstreams, take the one with fewer requests in flight |
| `p2c_ewma` | Same, but scored by in-flight count times a moving average of latency |

`weight` repeats an upstream in the ring, so `weight = 3` against `weight = 1`
receives three times the traffic (verified by a distribution test).

`least_conn` and `p2c_ewma` sample **two distinct** upstreams and keep the
better one. Sampling two at random rather than scanning all of them keeps the
cost constant and avoids every worker piling onto whichever upstream currently
looks best; drawing without replacement matters because two or three upstreams
is the common case, and drawing with replacement would send a quarter of the
traffic to the worse of two.

`p2c_ewma` is the one to reach for when upstreams are not equally fast:
`least_conn` cannot tell a slow upstream from a quiet one, because a slow
upstream that still accepts connections looks idle.

Metrics: `prx_upstream_inflight{service,upstream}` and
`prx_upstream_ewma_ms{service,upstream}`.

### 3.4a2 `[service.sticky]` — session affinity

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `false` | Turn affinity on |
| `mode` | enum | `"cookie"` | `cookie`, `client_ip` or `header` |
| `name` | `string` | `"prx_upstream"` | Cookie name, or header name in `header` mode |
| `ttl_s` | `number` | `3600` | Cookie lifetime in `cookie` mode |

```toml
[service.sticky]
enabled = true
mode = "cookie"
name = "prx_upstream"
ttl_s = 3600
```

- **`cookie`** pins exactly: prx sets a cookie naming the chosen upstream
  (`HttpOnly`, `SameSite=Lax`) and honors it afterwards. It survives a client
  changing address, and a cookie for an upstream that no longer exists simply
  stops matching.
- **`client_ip`** and **`header`** hash the key onto the upstream ring, so no
  state is stored, but adding or removing an upstream reshuffles some clients.
  These modes ignore the service's `lb` setting by design: routing them through
  it would spread the client across upstreams, which is the opposite of what
  was asked for.

Affinity is a preference, never a requirement. When the pinned upstream is
unhealthy or gone, the request falls back to normal balancing. prx cannot know
an upstream died until it tries, so pair affinity with `max_retries >= 1` or
`[service.health_check]` if a client must not see an error when its upstream
disappears.

### 3.4a `[[service]]` — retries and time budget

| Field | Type | Default | Description |
|---|---|---|---|
| `max_retries` | `number` | `0` | Extra attempts per request |
| `retry_backoff_ms` | `number` | `0` | Upper bound of the wait before a retry |
| `retry_idempotent_only` | `bool` | `true` | Only replay idempotent methods after a mid-flight failure |
| `retry_budget_ratio` | `float` | `0.2` | Share of recent successes that may be spent on retries; `0.0` disables the budget |
| `retry_budget_min_per_window` | `number` | `10` | Retries always allowed per window, so an idle service is not locked out |
| `retry_budget_window_ms` | `number` | `10000` | Window the budget is measured over |
| `request_timeout_ms` | `number` | `0` | Total budget for a request including retries; `0` disables it |

**Backoff is jittered.** `retry_backoff_ms` is the upper bound, and the actual
wait is picked uniformly below it. A fixed backoff would send every request
that failed at the same moment back at the same moment.

**Retry budget.** Without one, an upstream that starts failing receives
`1 + max_retries` times its usual traffic exactly when it is least able to cope.
The budget allows retries while they stay under
`retry_budget_ratio * successes` in the current window, with
`retry_budget_min_per_window` as a floor. Denied retries are counted in
`prx_retry_denied_total{reason="budget"}`.

**Idempotency.** A connect failure means nothing reached the upstream, so any
method may be retried. Once the request may have been applied, only `GET`,
`HEAD`, `OPTIONS`, `TRACE`, `PUT` and `DELETE` are replayed while
`retry_idempotent_only = true`. Set it to `false` only if your upstream
deduplicates writes itself.

**Time budget.** `request_timeout_ms` is measured from the moment prx accepts
the request. When it runs out the client gets `504`, no further attempt is
started, and each attempt's connect/read/write timeouts are shortened so they
cannot outlive the budget. It does not abort a response that is already
streaming in slowly; per-read timeouts still govern that.

```toml
[[service]]
name = "payments"
max_retries = 2
retry_backoff_ms = 50
retry_idempotent_only = true    # never replay a POST that may have landed
retry_budget_ratio = 0.1        # retries may add at most 10% load
request_timeout_ms = 3000
```

### 3.4b `[[service]]` — HTTP version to the upstream

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `upstream_h2` | enum | `"never"` | No | `never`, `always`, `auto` |

- `never`: HTTP/1.1 to the upstream (the historical behavior).
- `always`: HTTP/2 only. Over TLS it is negotiated with ALPN; over cleartext prx
  trusts that the upstream speaks h2c.
- `auto`: prefer HTTP/2, fall back to HTTP/1.1. Over cleartext there is no ALPN
  to negotiate with, so this behaves like `never`.

**gRPC needs `always`**: gRPC carries its status in HTTP/2 trailers, which
HTTP/1.1 cannot express, so it must be HTTP/2 from client to upstream. Pair it
with `[server] h2c = true` for cleartext clients, or a TLS listener with
`enable_h2 = true`.

```toml
[server]
h2c = true

[[service]]
name = "grpc-api"
upstream_h2 = "always"

[[service.upstream]]
addr = "127.0.0.1:50051"
```

### 3.4b1 `[route.cache]` — short-lived response cache

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `false` | Turn caching on for this route |
| `ttl_ms` | `number` | `2000` | How long an entry stays fresh |
| `max_body_bytes` | `number` | `262144` | Larger responses are streamed but never stored |
| `max_entries` | `number` | `10000` | Cap on stored responses |
| `max_bytes` | `number` | `134217728` | Cap on stored bytes |
| `cache_status_codes` | array | `[200, 203, 300, 301, 404]` | Status codes worth storing |
| `key_query` | `bool` | `true` | Include the query string in the key |
| `vary_headers` | array | `["accept-encoding"]` | Request headers that take part in the key |
| `coalesce_wait_ms` | `number` | `2000` | How long a queued request waits for the in-flight fetch |
| `add_status_header` | `bool` | `true` | Add `X-Cache: HIT\|MISS` and `Age` |

```toml
[route.cache]
enabled = true
ttl_ms = 2000
```

**The coalescing is the main event.** When a popular object expires under load,
every request that arrives during the refetch would normally become its own
upstream request. prx sends one and queues the rest behind it; an end-to-end
test holds 30 simultaneous requests on a cold key to exactly one upstream
fetch. Even a one-second TTL is enough for this to matter.

A queued request never waits indefinitely: after `coalesce_wait_ms` it goes
upstream itself, so a stalled fetch cannot stall everyone behind it.

What prx refuses to cache, regardless of configuration:

- anything but `GET` and `HEAD`;
- requests carrying `Authorization`, or `Cache-Control: no-store`/`no-cache`;
- responses with `Cache-Control: no-store` or `private`, with `Set-Cookie`, or
  with `Vary: *` — storing any of these would hand one client's response to
  another;
- bodies over `max_body_bytes`, which are streamed through untouched.

Hop-by-hop headers (`Connection`, `Transfer-Encoding`, `Set-Cookie`, …) are
stripped before storing, so a stored entry never replays another connection's
state.

Admin endpoints: `GET /web/cache` reports entries, bytes, hits, misses,
coalesced requests and evictions per route; `DELETE /web/cache` purges
everything, or one route with `?route=<name>`.

Metrics: `prx_cache_total{route,result}` (`hit`, `miss`, `miss_follower`),
`prx_cache_entries{route}` and `prx_cache_bytes{route}`.

**Not implemented yet:** `stale-while-revalidate`. Serving a stale entry while
refreshing in the background needs a revalidation task, and the coalescing
above already removes most of the thundering-herd risk it would address.

### 3.4b2 `[route.rate_limit]` and `[route.concurrency_limit]`

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `false` | Turn rate limiting on for this route |
| `key` | `string` | `"client_ip"` | `client_ip`, `route`, or `header:<Name>` |
| `requests_per_second` | `number` | `100` | Sustained rate per key |
| `burst` | `number` | = `requests_per_second` | How many may arrive at once |
| `response_status` | `number` | `429` | Status used for rejections |
| `retry_after` | `bool` | `true` | Send a `Retry-After` header |
| `entry_ttl_ms` | `number` | `60000` | Forget a key after this long idle |
| `max_entries` | `number` | `100000` | Hard cap on tracked keys |

| `[route.concurrency_limit]` | Type | Default | Description |
|---|---|---|---|
| `max_concurrent` | `number` | `0` | Requests in flight allowed on this route; `0` is unlimited |
| `response_status` | `number` | `503` | Status used when shedding |

```toml
[route.rate_limit]
enabled = true
key = "header:X-Api-Key"     # one allowance per API key
requests_per_second = 100
burst = 200

[route.concurrency_limit]
max_concurrent = 500          # never let one route occupy every worker
```

How it behaves:

- A token bucket per key: `burst` controls how much a client may bank, and
  `requests_per_second` the sustained rate. An idle key never banks more than
  `burst`.
- Rejections carry `Retry-After` with the seconds until one token is back.
- Limits are checked before an upstream is chosen, so a rejected request costs
  a hash and nothing else.
- Buckets live in 64 shards, so unrelated keys do not share a lock, and nothing
  is allocated per request.
- Memory is bounded: once `max_entries` is reached, idle keys go first and then
  the least recently seen ones, so a flood of unique keys cannot push out a
  client that is actively being limited.
- Requests missing the header named by `header:<Name>` share one bucket, so a
  missing header cannot be used to escape the limit.
- `client_ip` buckets by address and ignores the source port, otherwise a
  client could reset its allowance by opening a new connection. Behind a CDN or
  another proxy, every request arrives from that proxy's address, so use
  `header:<Name>` with a header you control (see the header rules section about
  `set` versus `add` for why the value must not be client-supplied).

**A reload resets the counters.** Rate limit state lives in the config snapshot,
so a client that was being limited gets a fresh allowance after a reload. That
is a deliberate trade: config changes are rare, and carrying state across them
would mean keying it by something other than the route it belongs to.

Metrics: `prx_rate_limited_total{route,kind}` (`kind` is `rate` or
`concurrency`) and `prx_limiter_entries{route}`.

### 3.4c Header rules

Rules exist per route (`[route.request_headers]`, `[route.response_headers]`)
and globally (`[headers.request]`, `[headers.response]`). Global rules run
first, so a route can override them.

```toml
[headers.request]
set = { "X-Edge" = "prx" }

[[route]]
name = "api"
service = "api"
path_prefix = "/"

[route.request_headers]
set = { "X-Real-IP" = "$client_ip" }
add = { "X-Forwarded-For" = "$client_ip" }
remove = ["X-Internal-Token"]

[route.response_headers]
set = { "X-Frame-Options" = "DENY" }
remove = ["Server"]
```

Order within one rule set: `remove`, then `set`, then `add`.

- `set` replaces every existing value for that header.
- `add` appends another value, keeping what is already there.
- `remove` drops the header entirely.

**Spoofing:** a client can send any header it likes. Use `set` when the
upstream must trust the value (`set` overwrites whatever the client sent), and
`add` only when you intentionally want to extend a chain such as
`X-Forwarded-For` from a proxy you trust in front of prx.

Variables usable inside a value:

| Variable | Value |
|---|---|
| `$client_ip` | client IP address |
| `$client_port` | client source port |
| `$scheme` | `https` when the upstream connection uses TLS, otherwise `http` |
| `$host` | Host header the client sent |
| `$route_name` | name of the matched route |
| `$upstream_addr` | address of the upstream this request went to |
| `$request_id` | inbound `X-Request-Id` if present and usable, otherwise a generated id |

A header whose value needs a variable that is unavailable for this request is
skipped rather than written with a hole in it. Unknown variables and invalid
header names are rejected when the config loads.

### 3.4d `[service.health_check]` — active probing

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `false` | Turn the background prober on for this service |
| `kind` | enum | `"tcp"` | `tcp` (connect and close) or `http` (GET and check the status) |
| `path` | `string` | `"/healthz"` | Request path for `kind = "http"` |
| `interval_ms` | `number` | `2000` | How often each upstream is probed |
| `timeout_ms` | `number` | `1000` | Probe timeout; a timeout counts as a failure |
| `healthy_threshold` | `number` | `2` | Consecutive successes before a down upstream is used again |
| `unhealthy_threshold` | `number` | `3` | Consecutive failures before an upstream is taken out |
| `expected_status` | array | `[200]` | Status codes that count as healthy for `kind = "http"` |

```toml
[[service]]
name = "api"

[service.health_check]
enabled = true
kind = "http"
path = "/healthz"
interval_ms = 2000
unhealthy_threshold = 3
```

Without this, an upstream is only removed after real requests have failed
against it (the circuit breaker), so users absorb the first errors of every
outage, and it is let back in when a timer expires rather than because it
recovered. With it:

- a failing upstream is taken out before a request finds it;
- an upstream that comes back has to pass `healthy_threshold` probes before it
  receives traffic again, and passing them also clears the circuit breaker;
- `ready_path` reports `503` while a service has no usable upstream;
- the admin API and Web UI read this verdict instead of opening a TCP
  connection per upstream on every refresh (`source: "active_probe"` in
  `GET /web/health/routes`).

Probes run on their own thread and runtime, so they never compete with request
handling, and the checker reads the current config on each tick: a reload is
picked up without restarting anything.

Metrics: `prx_health_check_total{service,upstream,result}` and
`prx_upstream_healthy{service,upstream}`.

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
- If a route matched the path but rejected the request method, the response is
  `405` and the default route is not used.

### 4.2 Health/Readiness

`health_path` and `ready_path` are handled before route matching:
- `health_path` returns `200 ok`.
- `ready_path` returns:
  - `200 ready` when every route has at least one available upstream.
  - `503 not_ready` when any route has no available upstream.

### 4.3 Upgraded connections (WebSocket)

When a request carries an `Upgrade` header, prx does not apply the upstream
`read_timeout_ms`, `write_timeout_ms` or `idle_timeout_ms` to that connection.
Those timeouts describe request/response traffic, and an upgraded connection is
expected to sit idle; applying them would drop healthy websockets.
`connect_timeout_ms` still applies.

### 4.4 Retry + Circuit breaker

- Retry follows `max_retries` and does not select an upstream already tried within the same request.
- A retry is also refused when the retry budget is spent, when the request's
  time budget is gone, or when the method is not idempotent and the request may
  already have reached the upstream. Each case is counted separately in
  `prx_retry_denied_total{reason}`.
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
- `route '<name>' lists unsupported HTTP method '<method>'`
- `route '<name>' has an invalid header name '<name>'`
- `[server.tls] is configured but has no certificate`
- `[server.tls] cert_path and key_path must be set together`
- `only one [[server.tls.cert]] can be marked is_default = true`
- `private key <path> does not match certificate <path>`
- `compression.level must be between 1 and 11`
- `route '<name>' cache.ttl_ms must be > 0`
- `route '<name>' cache.max_body_bytes must be > 0`
- `route '<name>' cache.cache_status_codes must not be empty`
- `route '<name>' cache.vary_headers contains an invalid header name '<name>'`
- `route '<name>' rate_limit.key '<key>' is invalid`
- `route '<name>' rate_limit.requests_per_second must be > 0`
- `route '<name>' rate_limit.response_status must be a 4xx or 5xx code`
- `route '<name>' concurrency_limit.response_status must be a 4xx or 5xx code`
- `service '<name>' retry_budget_ratio must be between 0.0 and 10.0`
- `service '<name>' retry_budget_window_ms must be > 0`
- `service '<name>' health_check.interval_ms must be > 0`
- `service '<name>' health_check.timeout_ms must be > 0`
- `service '<name>' health_check thresholds must be > 0`
- `service '<name>' health_check.path must start with '/'`
- `service '<name>' sticky.name must not be empty`
- `service '<name>' sticky.name '<name>' is not a valid header name`
- `route '<name>' header '<name>' uses unknown variable '$<var>'`

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
