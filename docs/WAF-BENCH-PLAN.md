# prx + WAF vs nginx + ModSecurity vs Coraza — Comparative Benchmark & Optimization Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure prx + WAF plugin against nginx + ModSecurity v3 and Caddy + Coraza on identical rules and traffic, then use the numbers to make prx + WAF the fastest of the three without detecting less.

**Architecture:** Extend the existing `bench/` harness (docker compose profiles, one target at a time, pinned cpusets, `oha` load, JSON results) with two WAF competitors and their no-WAF twins, a single shared SecLang config + pinned CRS checkout mounted into every target, WAF-shaped workloads, and a verdict-equivalence gate. Phase A produces competitor numbers **before** the prx WAF exists, so T505–T508 are built against a measured target instead of an assumed budget. Phase B adds `prx-waf` to the same matrix. Phase C is a profile → hypothesis → measure loop that feeds T508.

**Tech Stack:** docker compose, `oha`, bash, Python 3 (+ PyYAML), OWASP CRS v4.21.0, `owasp/modsecurity-crs` nginx image (libmodsecurity3), Caddy + `coraza-caddy` v2, criterion, `scripts/profile.sh`.

**Spec:** [`docs/PLUGIN-WAF-PLAN.md`](PLUGIN-WAF-PLAN.md) §5 (how the WAF gets fast, budgets), [`docs/decisions/0001-waf-engine.md`](decisions/0001-waf-engine.md) (T504 measurements), [`docs/tasks/T508-waf-performance.md`](tasks/T508-waf-performance.md), [`bench/README.md`](../bench/README.md).

## Global Constraints

- CRS is **v4.21.0, commit `2ac6c00`** for every target — the same ruleset T504 measured. No target may use a CRS copy embedded in its image or module.
- One shared SecLang engine config (`bench/waf/main.conf`) for every target. Any per-engine deviation is written to `bench/waf/DEVIATIONS.md` with the reason; an unrecorded deviation invalidates the run.
- Paranoia level 1, inbound anomaly threshold 5, blocking mode, request body limit 262144, response body inspection **off**, audit log **off**, access log **off**, debug log off — everywhere.
- One target runs at a time; proxy cpuset `0,1`, backend cpuset `2,3` (existing harness rule). Worker/thread count is 4 for every target (matches `bench/configs/prx.toml` and `nginx.conf`).
- A faster result that changes any block/allow verdict is a regression, not a win (same rule as T508).
- Every performance claim lands in `docs/BENCHMARKS.md` with machine, date, git sha, image digests. Numbers from Docker Desktop on macOS are for debugging the harness only and must not be published — see `bench/README.md`.
- Win criteria below are fixed **before** measuring. Changing them later requires numbers and a written reason (same convention as T504/T508).
- `make gate` stays green; zero new `#[allow]`, zero audit exceptions.

---

## 1. What "faster" means (fixed before measuring)

Absolute numbers mix two things: how fast the proxy is and how fast the WAF is. Both are reported, separately:

- **Absolute** = what a user of `<proxy>+WAF` experiences.
- **WAF tax** = `<proxy>+WAF` minus the same proxy with the WAF off. This is the number that tells us whether the *engine* is good, and the only one that survives an argument like "Caddy is just a slower proxy".

| # | Criterion | Pass condition |
|---|---|---|
| W1 | Verdict equivalence (gate, not a score) | On the verdict corpus, prx-waf returns the same block/allow as **both** competitors, or every difference is listed and explained in `BENCHMARKS.md` |
| W2 | Saturation throughput (RPS on 2 proxy cores) | ≥ 1.0× the best competitor on every scenario; ≥ 1.2× on `waf-get` and `waf-attack-mix` |
| W3 | p99 at the same fixed request rate | ≤ 1.0× the best competitor on every scenario; ≤ 0.8× on `waf-get` |
| W4 | WAF tax (added p99, added CPU µs/request) | Lower than both competitors' tax on every scenario |
| W5 | Memory with CRS loaded (sum of PSS, all processes) | Lower than both competitors, idle and peak |
| W6 | Cold start to first 200 with CRS loaded | Lower than both competitors |

The T508 absolute budgets (p99 +1 ms no body, +5 ms at 128 KB, RSS +50 MB) stay in force. Task 8 replaces "assumed" with "measured" next to each of them.

**Prior, to be tested, not claimed:** Rust `regex` (lazy DFA) should beat Go `regexp` (Coraza) and PCRE-backed libmodsecurity on raw matching. If prx loses, the likely place is variable extraction, transformations and allocation, not matching — T504 already showed matching is 82 µs for 312 patterns on one string, and that the real cost scales with the number of variables.

## 2. Targets

| Target | What it is | Why it is in the matrix |
|---|---|---|
| `prx` | existing | WAF-off twin of `prx-waf` |
| `prx-waf` | prx + `kind = "waf"` plugin (Phase B) | the thing we are making fast |
| `nginx` | existing | WAF-off twin of `nginx-modsec` |
| `nginx-modsec` | nginx + ModSecurity-nginx connector + libmodsecurity3 | the incumbent; what T508 names as the thing to beat |
| `caddy` | Caddy reverse proxy | WAF-off twin of `caddy-coraza` |
| `caddy-coraza` | Caddy + `coraza-caddy` v2 (Coraza in-process) | Coraza's most direct in-process deployment, same shape as a prx plugin |

Skipped: `coraza-spoa` (HAProxy, out-of-process hop — measures IPC, not the engine) and `coraza-proxy-wasm` (Envoy, measures the Wasm VM). Add one only if a user asks how prx compares to that specific deployment.

## 3. Workloads

All requests carry the same browser-like headers (`Host: bench.local`, a Chrome `User-Agent`, `Accept`, `Accept-Language`, a 3-value `Cookie`). A numeric-IP `Host` or a tool `User-Agent` scores CRS points on every request and would turn the benign scenarios into attack scenarios.

| Scenario | Request | What it isolates |
|---|---|---|
| `waf-get` | GET `/1k?id=…&q=…&page=…&sort=…`, 1000 distinct benign URLs | dominant real traffic: headers + a few args, no body |
| `waf-form` | POST urlencoded, 20 fields, ~2 KB | `ARGS_POST` extraction |
| `waf-json-16k` | POST JSON 16 KB | JSON body processor |
| `waf-json-128k` | POST JSON 128 KB | the T508 "+5 ms at 128 KB" budget row |
| `waf-attack-mix` | 90 % benign URLs / 10 % CRS regression-suite URLs, seeded shuffle | realistic hostile background |
| `waf-attack-only` | 100 % CRS regression-suite URLs | blocked path and worst-case CPU (the DoS-relevant number) |

Each scenario runs twice per target: **saturation** (unthrottled, 64 conns → RPS, CPU µs/req) and **fixed rate** (`RATE` = 50 % of the slowest target's saturation RPS for that scenario, `--latency-correction` → p50/p99/p99.9 free of coordinated omission). Latencies from saturation runs are not compared.

Skipped: multipart uploads, response-body inspection, H2, TLS. Add when a criterion above passes and someone needs the next one.

## 4. File structure

| File | Responsibility |
|---|---|
| `scripts/bench.sh` (modify) | new targets, WAF scenarios, `RATE` mode, all-process resource sampling |
| `bench/backend/src/main.rs` (modify) | drain request bodies so POST scenarios work over keep-alive |
| `bench/docker-compose.yml` (modify) | `nginx-modsec`, `caddy`, `caddy-coraza`, later `prx-waf` profiles |
| `bench/waf/fetch-crs.sh` (create) | pinned CRS checkout into `bench/waf/crs/` (git-ignored) |
| `bench/waf/main.conf` (create) | the one shared SecLang engine config |
| `bench/waf/modsec.conf` (create) | include list: `main.conf` + CRS, used by ModSecurity and Coraza |
| `bench/waf/DEVIATIONS.md` (create) | per-engine departures from `main.conf` |
| `bench/waf/gen-corpus.py` (create) | benign/attack/mix URL lists + POST bodies, seeded |
| `bench/waf/verdicts.sh` (create) | status code per corpus URL per target, and the diff |
| `bench/configs/nginx-modsec.conf` (create) | `nginx.conf` + 3 ModSecurity lines |
| `bench/caddy/Dockerfile` (create) | xcaddy build with `coraza-caddy` pinned |
| `bench/configs/Caddyfile`, `Caddyfile.coraza` (create) | twin configs differing only by the `coraza_waf` block |
| `bench/configs/prx-waf.toml` (create, Phase B) | `prx.toml` + the `[[plugin]]` block |
| `docs/BENCHMARKS.md` (modify) | new §7 "WAF comparison" |
| `docs/tasks/T508-waf-performance.md` (modify) | budgets annotated with measured competitor numbers |

---

# Phase A — competitor baseline (start now; needs no prx WAF)

### Task 1: Count every process in the container, and only the measured window

The sampler reads `/proc/1` only. In nginx PID 1 is the master; the workers do the work and hold the rules, so nginx CPU and RSS are under-reported today. It also reads CPU once at the end, so container start-up (CRS compile, for WAF targets) is billed to the run.

**Files:** Modify `scripts/bench.sh` (`sample_process_stats`)

**Interfaces:** Produces the same stats JSON (`peak_rss_kb`, `cpu_ticks`) so `bench-parse.py`, `bench-compare.sh`, `perf-gate.sh` need no change. `peak_rss_kb` becomes summed PSS; `cpu_ticks` becomes a start→end delta over all processes.

- [ ] **Step 1: See the bug**

```bash
KEEP_UP=1 DURATION=10 WARMUP=2 scripts/bench.sh nginx h1-keepalive
docker exec bench-nginx sh -c 'for p in /proc/[0-9]*; do echo "$(cat $p/comm) $(awk "{print \$14+\$15}" $p/stat)"; done'
```
Expected: PID 1 shows ~0 ticks, the four workers show all of it.

- [ ] **Step 2: Replace the body of `sample_process_stats`**

```bash
sample_process_stats() {
  local container="$1" out="$2" seconds="$3"
  (
    # Every process, not PID 1: nginx does its work (and holds its rules) in workers.
    # PSS, not RSS: workers share pages, and summing RSS would count them once each.
    local ticks='cat /proc/[0-9]*/stat 2>/dev/null | awk "{s+=\$14+\$15} END{print s+0}"'
    local pss='cat /proc/[0-9]*/smaps_rollup 2>/dev/null | awk "/^Pss:/{s+=\$2} END{print s+0}"'
    local peak_rss_kb=0 cpu_start cpu_end rss
    cpu_start="$(docker exec -u 0 "${container}" sh -c "${ticks}" 2>/dev/null || echo 0)"
    for _ in $(seq 1 "${seconds}"); do
      rss="$(docker exec -u 0 "${container}" sh -c "${pss}" 2>/dev/null || echo 0)"
      [ -n "${rss}" ] && [ "${rss}" -gt "${peak_rss_kb}" ] && peak_rss_kb="${rss}"
      sleep 1
    done
    cpu_end="$(docker exec -u 0 "${container}" sh -c "${ticks}" 2>/dev/null || echo 0)"
    printf '{"peak_rss_kb":%s,"cpu_ticks":%s}\n' "${peak_rss_kb:-0}" "$(( cpu_end - cpu_start ))" > "${out}"
  ) &
  echo $!
}
```

- [ ] **Step 3: Verify** — rerun Step 1's bench; `cpu_us_per_request` in `bench/results/h1-keepalive-nginx-*.json` must now be non-trivial and `peak_rss_kb` larger than the master alone. Run `scripts/bench.sh prx h1-keepalive` and confirm prx numbers are within noise of before (single process, so only the start-up CPU disappears).

- [ ] **Step 4: Commit** — `fix(bench): sample all container processes over the measured window only`

### Task 2: Backend drains request bodies

`bench/backend` never reads bodies ("the harness only issues GETs"). With POST over keep-alive the body bytes would be parsed as the next request head.

**Files:** Modify `bench/backend/src/main.rs`

- [ ] **Step 1: Write the failing test** (append to `main.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::content_length;

    #[test]
    fn reads_content_length_case_insensitively() {
        assert_eq!(content_length(b"POST / HTTP/1.1\r\ncontent-LENGTH: 42\r\n\r\n"), 42);
    }

    #[test]
    fn no_header_means_no_body() {
        assert_eq!(content_length(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"), 0);
    }
}
```

- [ ] **Step 2: Run it** — `cargo test --manifest-path bench/backend/Cargo.toml` → FAIL, `content_length` not found.

- [ ] **Step 3: Implement**

```rust
// ponytail: Content-Length only. Every proxy under test forwards a length for
// the fixed-size bodies oha sends; add chunked decoding if a scenario needs it.
fn content_length(head: &[u8]) -> usize {
    for line in head.split(|b| *b == b'\n') {
        let Some(colon) = line.iter().position(|b| *b == b':') else { continue };
        if line[..colon].eq_ignore_ascii_case(b"content-length") {
            return std::str::from_utf8(&line[colon + 1..])
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
        }
    }
    0
}
```

In `serve`, read the length next to `path`, and replace the carry-over block at the end of the loop:

```rust
        let body_len = content_length(&buf[..head_end]);
        // ... existing path / delay / `let body = match path { .. }` unchanged ...

        // Drop the request body: what is already buffered, then the rest off the wire.
        let buffered = (filled - head_end).min(body_len);
        buf.copy_within(head_end + buffered..filled, 0);
        filled -= head_end + buffered;
        let mut remaining = body_len - buffered;
        while remaining > 0 {
            let n = stream.read(&mut buf[..remaining.min(buf.len())]).await?;
            if n == 0 {
                return Ok(());
            }
            remaining -= n;
        }

        stream.write_all(&body).await?;
```

Also fix the stale comment above the head-read loop ("Bodies are not consumed").

- [ ] **Step 4: Run** — tests PASS; then end to end:
```bash
KEEP_UP=1 DURATION=5 WARMUP=1 scripts/bench.sh prx h1-keepalive
head -c 131072 /dev/zero | tr '\0' 'a' > /tmp/body
oha --no-tui -n 2000 -c 8 -m POST -D /tmp/body http://127.0.0.1:18080/1k | rg 'Success rate|\[200\]'
```
Expected: 100 % success, 2000 × 200.

- [ ] **Step 5: Commit** — `feat(bench): drain request bodies in the mock backend`

### Task 3: One ruleset, one engine config, for everyone

**Files:** Create `bench/waf/fetch-crs.sh`, `bench/waf/main.conf`, `bench/waf/modsec.conf`, `bench/waf/DEVIATIONS.md`; add `bench/waf/crs/` to `.gitignore`

- [ ] **Step 1: `bench/waf/fetch-crs.sh`**

```bash
#!/usr/bin/env bash
# CRS pinned to the release T504 measured. Never track main.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/crs"
TAG=v4.21.0 COMMIT=2ac6c00
rm -rf "${DIR}"
git clone --quiet --depth 1 --branch "${TAG}" https://github.com/coreruleset/coreruleset "${DIR}"
got="$(git -C "${DIR}" rev-parse --short=7 HEAD)"
[ "${got}" = "${COMMIT}" ] || { echo "CRS ${TAG} is ${got}, expected ${COMMIT}" >&2; exit 1; }
cp "${DIR}/crs-setup.conf.example" "${DIR}/crs-setup.conf"   # defaults: PL1, inbound threshold 5
```

- [ ] **Step 2: `bench/waf/main.conf`** — only directives all three engines implement

```apache
SecRuleEngine On
SecRequestBodyAccess On
SecRequestBodyLimit 262144
SecRequestBodyNoFilesLimit 262144
SecRequestBodyLimitAction Reject
SecResponseBodyAccess Off
SecAuditEngine Off
SecDebugLogLevel 0
SecRule REQUEST_HEADERS:Content-Type "^application/json" \
    "id:200001,phase:1,t:none,t:lowercase,pass,nolog,ctl:requestBodyProcessor=JSON"
```

- [ ] **Step 3: `bench/waf/modsec.conf`**

```apache
Include /bench/waf/main.conf
Include /bench/waf/crs/crs-setup.conf
Include /bench/waf/crs/rules/*.conf
```

- [ ] **Step 4: `bench/waf/DEVIATIONS.md`** — header plus an empty table `| Target | Directive | What differs | Why |`. It must exist and be empty before the first run, so every later row is a visible decision.

- [ ] **Step 5: Run** `bash bench/waf/fetch-crs.sh && ls bench/waf/crs/rules/*.conf | wc -l` → non-zero, script exits 0. Commit — `feat(bench): shared seclang config and pinned crs for waf targets`

### Task 4: `nginx-modsec` target

The image's entrypoint templates its own nginx and ModSecurity config from env vars. We bypass it and start nginx on our config, so `nginx` and `nginx-modsec` differ by exactly the ModSecurity lines.

**Files:** Create `bench/configs/nginx-modsec.conf`; modify `bench/configs/nginx.conf`, `bench/docker-compose.yml`, `scripts/bench.sh`

- [ ] **Step 1: Pin the image and confirm the module path**

```bash
docker pull owasp/modsecurity-crs:nginx-alpine
docker inspect --format '{{index .RepoDigests 0}}' owasp/modsecurity-crs:nginx-alpine
docker run --rm --entrypoint sh owasp/modsecurity-crs:nginx-alpine -c \
  'nginx -v; ls /etc/nginx/modules/ | grep -i modsecurity; strings /usr/local/lib/libmodsecurity.so* 2>/dev/null | grep -m1 "ModSecurity v"'
```
Put the printed `owasp/modsecurity-crs@sha256:…` digest in compose (Step 3). If the module is not under `/etc/nginx/modules/`, use the path `find / -name 'ngx_http_modsecurity_module.so'` prints and note it in the config comment.

- [ ] **Step 2: Configs.** In `bench/configs/nginx.conf` add to the `http {}` block (both twins need it — nginx's 16k default spills the 128 KB body to a temp file, which would bill disk I/O to the WAF comparison):

```nginx
    client_max_body_size 1m;
    client_body_buffer_size 256k;
```

`bench/configs/nginx-modsec.conf` = a copy of the updated `nginx.conf` with, as the first line:

```nginx
load_module modules/ngx_http_modsecurity_module.so;
```
and inside `server {}`:
```nginx
        modsecurity on;
        modsecurity_rules_file /bench/waf/modsec.conf;
```
Confirm the twins differ by nothing else: `diff bench/configs/nginx.conf bench/configs/nginx-modsec.conf` → exactly those 3 lines plus the header comment.

- [ ] **Step 3: Compose service**

```yaml
  nginx-modsec:
    profiles: ["nginx-modsec"]
    container_name: bench-nginx-modsec
    image: owasp/modsecurity-crs@sha256:<digest from Step 1>
    entrypoint: ["nginx", "-c", "/bench/nginx-modsec.conf", "-g", "daemon off;"]
    volumes:
      - ./configs/nginx-modsec.conf:/bench/nginx-modsec.conf:ro
      - ./waf:/bench/waf:ro
    ports:
      - "18080:8080"
    cpuset: "${BENCH_PROXY_CPUS:-0,1}"
    ulimits:
      nofile: { soft: 65535, hard: 65535 }
    depends_on: [backend1, backend2]
    networks: [bench]
```

- [ ] **Step 4: Teach `bench.sh` the target** — add `nginx-modsec) echo "bench-nginx-modsec" ;;` to the container-name `case`, and the name to the usage comment.

- [ ] **Step 5: Verify it blocks and passes**

```bash
docker compose -f bench/docker-compose.yml --profile nginx-modsec up -d
docker logs bench-nginx-modsec 2>&1 | tail -5          # no emerg/permission errors
curl -s -o /dev/null -w '%{http_code}\n' -H 'Host: bench.local' -A 'Mozilla/5.0' 'http://127.0.0.1:18080/1k?id=1'                        # 200
curl -s -o /dev/null -w '%{http_code}\n' -H 'Host: bench.local' -A 'Mozilla/5.0' 'http://127.0.0.1:18080/1k?id=1%27%20OR%20%271%27=%271%27--' # 403
```
If nginx fails on a temp-path or pid permission (image runs unprivileged), add the failing `*_temp_path /tmp/...;` directive to **both** nginx configs and rerun.

- [ ] **Step 6: Commit** — `feat(bench): nginx + modsecurity target on the shared ruleset`

### Task 5: `caddy` and `caddy-coraza` targets

**Files:** Create `bench/caddy/Dockerfile`, `bench/configs/Caddyfile`, `bench/configs/Caddyfile.coraza`; modify `bench/docker-compose.yml`, `scripts/bench.sh`

- [ ] **Step 1: Resolve and pin the module version**

```bash
curl -s https://api.github.com/repos/corazawaf/coraza-caddy/releases/latest | jq -r .tag_name
```

- [ ] **Step 2: `bench/caddy/Dockerfile`** (put the tag from Step 1 in `CORAZA_CADDY`)

```dockerfile
FROM caddy:2-builder AS build
ARG CORAZA_CADDY
RUN xcaddy build --with github.com/corazawaf/coraza-caddy/v2@${CORAZA_CADDY}

FROM caddy:2
COPY --from=build /usr/bin/caddy /usr/bin/caddy
```

- [ ] **Step 3: `bench/configs/Caddyfile`**

```caddyfile
{
	admin off
	auto_https off
	log {
		level ERROR
	}
}

:8080 {
	respond /healthz "ok" 200
	reverse_proxy backend1:8000 backend2:8000 {
		lb_policy round_robin
		lb_retries 0
		transport http {
			keepalive 60s
			keepalive_idle_conns 256
			dial_timeout 1s
			response_header_timeout 5s
		}
	}
}
```

`Caddyfile.coraza` = the same file with `order coraza_waf first` added to the global block and, as the first directive of the site block:

```caddyfile
	coraza_waf {
		directives `
			Include /bench/waf/modsec.conf
		`
	}
```

- [ ] **Step 4: Compose** — two services from one build, same shape as Task 4's (ports, cpuset, ulimits, depends_on, networks), differing only in the mounted Caddyfile; `caddy-coraza` also mounts `./waf:/bench/waf:ro`. Both set `environment: { GOMAXPROCS: "4" }` to match `threads = 4` / `worker_processes 4`. Container names `bench-caddy`, `bench-caddy-coraza`; add both to the `bench.sh` container-name `case`.

```yaml
  caddy: &caddy
    profiles: ["caddy"]
    container_name: bench-caddy
    build:
      context: ./caddy
      args: { CORAZA_CADDY: "<tag from Step 1>" }
    command: ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
    environment: { GOMAXPROCS: "4" }
    volumes:
      - ./configs/Caddyfile:/etc/caddy/Caddyfile:ro
    ports: ["18080:8080"]
    cpuset: "${BENCH_PROXY_CPUS:-0,1}"
    ulimits:
      nofile: { soft: 65535, hard: 65535 }
    depends_on: [backend1, backend2]
    networks: [bench]

  caddy-coraza:
    <<: *caddy
    profiles: ["caddy-coraza"]
    container_name: bench-caddy-coraza
    volumes:
      - ./configs/Caddyfile.coraza:/etc/caddy/Caddyfile:ro
      - ./waf:/bench/waf:ro
```

- [ ] **Step 5: Verify** — same two `curl`s as Task 4 Step 5 against `--profile caddy-coraza` (200 then 403), and both return 200 against `--profile caddy`. If Coraza rejects a directive from `main.conf`, do **not** edit `main.conf`; add the smallest per-engine override and a row in `DEVIATIONS.md`.

- [ ] **Step 6: Commit** — `feat(bench): caddy and caddy + coraza targets on the shared ruleset`

### Task 6: WAF workloads

**Files:** Create `bench/waf/gen-corpus.py`; modify `scripts/bench.sh`; add `bench/waf/corpus/` to `.gitignore`

- [ ] **Step 1: Confirm the load generator can do this** (oha is not installed on the dev laptop today)

```bash
cargo install oha
oha --help | rg -e '--urls-from-file' -e '--latency-correction' -e '--host' -e '--output-format'
```
All four must match (oha ≥ 1.0; `-j` from older versions is gone, the harness uses `--output-format json`). If `--urls-from-file` is missing, upgrade oha; do not substitute a different tool for some targets only.

- [ ] **Step 2: `bench/waf/gen-corpus.py`**

```python
#!/usr/bin/env python3
"""Seeded WAF corpus: URL lists for `oha --urls-from-file`, and POST bodies.

usage: gen-corpus.py <crs-dir> <out-dir> <host:port>
"""
import json
import random
import sys
from pathlib import Path
from urllib.parse import urlencode

import yaml  # pip install pyyaml

crs, out, host = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
out.mkdir(parents=True, exist_ok=True)
rng = random.Random(8508)  # fixed: every target and every rerun sees the same traffic
WORDS = "alpha bravo charlie delta echo foxtrot golf hotel india juliet".split()

benign = [
    f"http://{host}/1k?" + urlencode({
        "id": rng.randint(1, 10**6), "q": " ".join(rng.sample(WORDS, 2)),
        "page": rng.randint(1, 50), "sort": rng.choice(["name", "date", "price"]),
    })
    for _ in range(1000)
]

# GET-only, query-string attacks from CRS's own regression suite. oha cannot vary
# method/body per request, so body-borne attacks are covered by verdicts.sh only.
attacks = []
for f in sorted(crs.glob("tests/regression/tests/**/*.yaml")):
    for test in (yaml.safe_load(f.read_text()) or {}).get("tests", []):
        for stage in test.get("stages", []):
            i = stage.get("input") or stage.get("stage", {}).get("input", {})
            uri = i.get("uri") or ""
            if (i.get("method", "GET") == "GET" and not i.get("data")
                    and not i.get("encoded_request") and "?" in uri
                    and uri.startswith("/") and not any(c.isspace() for c in uri)):
                attacks.append(f"http://{host}{uri}")
attacks = sorted(set(attacks))

mix = benign * 9 + [rng.choice(attacks) for _ in range(len(benign))]
rng.shuffle(mix)

(out / "benign.txt").write_text("\n".join(benign) + "\n")
(out / "attacks.txt").write_text("\n".join(attacks) + "\n")
(out / "mix.txt").write_text("\n".join(mix) + "\n")
(out / "form.txt").write_text(urlencode({f"field{n}": " ".join(rng.choices(WORDS, k=12)) for n in range(20)}))
for name, size in (("json-16k", 16 << 10), ("json-128k", 128 << 10)):
    items = []
    while len(json.dumps(items)) < size - 200:
        items.append({"id": rng.randint(1, 10**6), "name": " ".join(rng.choices(WORDS, k=3)), "tags": rng.sample(WORDS, 3)})
    (out / f"{name}.json").write_text(json.dumps(items))
print(f"benign={len(benign)} attacks={len(attacks)} mix={len(mix)}")
```

- [ ] **Step 3: Run it** — `python3 bench/waf/gen-corpus.py bench/waf/crs bench/waf/corpus 127.0.0.1:18080` → `benign=1000 attacks=861 mix=10000` on CRS v4.21.0. Running it twice must produce byte-identical files (`shasum bench/waf/corpus/*` before/after).

- [ ] **Step 4: Scenarios in `scripts/bench.sh`** — extend `run_load`'s `case` (before the `*)` arm), and add `RATE`:

```bash
CORPUS="${ROOT}/bench/waf/corpus"
WAF_HEADERS=(--host bench.local
  -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36'
  -H 'Accept: text/html,application/json;q=0.9,*/*;q=0.8' -H 'Accept-Language: en-US,en;q=0.9'
  -H 'Cookie: sid=9f8a7c6b5d4e3f2a; theme=dark; lang=en')
# RATE set => fixed-rate run with coordinated-omission correction (latency numbers).
# RATE unset => saturation run (throughput and CPU numbers).
RATE_ARGS=(); [ -n "${RATE:-}" ] && RATE_ARGS=(-q "${RATE}" --latency-correction)
```
```bash
    waf-get)         oha --no-tui -j -z "${seconds}s" -c "${conns}" "${RATE_ARGS[@]}" "${WAF_HEADERS[@]}" --urls-from-file "${CORPUS}/benign.txt"  > "${out}" ;;
    waf-attack-mix)  oha --no-tui -j -z "${seconds}s" -c "${conns}" "${RATE_ARGS[@]}" "${WAF_HEADERS[@]}" --urls-from-file "${CORPUS}/mix.txt"     > "${out}" ;;
    waf-attack-only) oha --no-tui -j -z "${seconds}s" -c "${conns}" "${RATE_ARGS[@]}" "${WAF_HEADERS[@]}" --urls-from-file "${CORPUS}/attacks.txt" > "${out}" ;;
    waf-form)        oha --no-tui -j -z "${seconds}s" -c "${conns}" "${RATE_ARGS[@]}" "${WAF_HEADERS[@]}" -m POST -T application/x-www-form-urlencoded -D "${CORPUS}/form.txt" "${url}" > "${out}" ;;
    waf-json-16k)    oha --no-tui -j -z "${seconds}s" -c "${conns}" "${RATE_ARGS[@]}" "${WAF_HEADERS[@]}" -m POST -T application/json -D "${CORPUS}/json-16k.json"  "${url}" > "${out}" ;;
    waf-json-128k)   oha --no-tui -j -z "${seconds}s" -c "${conns}" "${RATE_ARGS[@]}" "${WAF_HEADERS[@]}" -m POST -T application/json -D "${CORPUS}/json-128k.json" "${url}" > "${out}" ;;
```
Name fixed-rate result files `<scenario>-q${RATE}-<target>-<sha>.json` so they never overwrite a saturation run. The scenarios also run against the WAF-off twins — that is where the WAF tax comes from. Update the usage comment.

- [ ] **Step 5: Smoke** — `DURATION=10 WARMUP=2 scripts/bench.sh nginx-modsec waf-attack-mix`; the JSON's status distribution must show both 200 and 403, in roughly 9:1. Against `nginx` it must be 100 % 200. Commit — `feat(bench): waf workloads and fixed-rate latency mode`

### Task 7: Verdict-equivalence gate

Speed is only comparable between engines that decide the same thing. This is a cheap status-code diff; full rule-ID conformance (go-ftw) stays T507's job.

**Files:** Create `bench/waf/verdicts.sh`

- [ ] **Step 1: Script**

```bash
#!/usr/bin/env bash
# usage: verdicts.sh <target>   -> bench/results/verdicts-<target>.tsv   (status<TAB>url)
#        verdicts.sh --diff a b -> lines where the two targets disagree on block/allow
set -euo pipefail
export LC_ALL=C   # sort and join must agree on collation
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RES="${ROOT}/bench/results"
if [ "$1" = "--diff" ]; then
  # 403 = blocked; anything else = allowed. Compare the decision, not the exact code.
  join -t $'\t' -j 2 <(sort -t $'\t' -k2 "${RES}/verdicts-$2.tsv") <(sort -t $'\t' -k2 "${RES}/verdicts-$3.tsv") \
    | awk -F'\t' '(($2==403)!=($3==403))'
  exit 0
fi
UA='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36'
cat "${ROOT}/bench/waf/corpus/benign.txt" "${ROOT}/bench/waf/corpus/attacks.txt" | while read -r url; do
  code="$(curl -g -s -o /dev/null -w '%{http_code}' -H 'Host: bench.local' -A "${UA}" "${url}" || echo 000)"
  printf '%s\t%s\n' "${code}" "${url}"
done > "${RES}/verdicts-$1.tsv"
awk -F'\t' '{c[$1]++} END{for (k in c) print k, c[k]}' "${RES}/verdicts-$1.tsv"
```

- [ ] **Step 2: Run for both competitors** (start each profile with `docker compose --profile <t> up -d`, run `verdicts.sh <t>`, tear down), then `bench/waf/verdicts.sh --diff nginx-modsec caddy-coraza`.
Expected: every `benign.txt` URL is 200 on both; the diff is short. Each differing URL is a real ModSecurity-vs-Coraza behaviour difference — record the count and the top rule families in `BENCHMARKS.md`. This list is the noise floor prx-waf will be judged against in Task 10.

- [ ] **Step 3: Commit** — `feat(bench): block/allow verdict diff across waf targets`

### Task 8: Record the competitor baseline and turn budgets into measurements

Run on the reference Linux machine from `bench/README.md`, not on a laptop.

- [ ] **Step 1: Saturation runs** — for each scenario in §3:
```bash
TARGETS="nginx nginx-modsec caddy caddy-coraza" scripts/bench.sh --all <scenario>
```
- [ ] **Step 2: Fixed-rate runs** — per scenario, `RATE` = half the lowest saturation RPS from Step 1 among the two WAF targets, same value for all four targets:
```bash
RATE=<n> TARGETS="nginx nginx-modsec caddy caddy-coraza" scripts/bench.sh --all <scenario>
```
- [ ] **Step 3: Cold start (W6)** — per WAF target, three times, take the median:
```bash
docker compose -f bench/docker-compose.yml --profile <t> down
s=$(date +%s.%N); docker compose -f bench/docker-compose.yml --profile <t> up -d
until curl -sf -o /dev/null -H 'Host: bench.local' http://127.0.0.1:18080/healthz; do :; done
echo "$(date +%s.%N) - $s" | bc
```
- [ ] **Step 4: Idle memory (W5)** — after start and one warm-up request, read summed PSS with the `pss` one-liner from Task 1.
- [ ] **Step 5: Write `docs/BENCHMARKS.md` §7 "WAF comparison"** — machine, date, sha, image digests, CRS commit, `DEVIATIONS.md` contents, then one table per criterion W2–W6 with columns `nginx-modsec | caddy-coraza | prx-waf (not yet measured)`, and the WAF-tax table (`X+WAF − X`). Copy raw JSON to `bench/results/baseline/`.
- [ ] **Step 6: Annotate T508** — next to each budget row in `docs/tasks/T508-waf-performance.md`, add the best competitor's measured value for the matching scenario. If a competitor already beats a budget (e.g. adds < 1 ms p99 on `waf-get`), tighten the budget to "better than that" and say so.
- [ ] **Step 7: Commit** — `docs: waf competitor baseline and measured targets for T508`

---

# Phase B — prx joins the matrix (after T503 + T505–T507)

### Task 9: `prx-waf` target

**Files:** Create `bench/configs/prx-waf.toml`; modify `bench/docker-compose.yml`, `scripts/bench.sh`

- [ ] **Step 1: Config** — a copy of `bench/configs/prx.toml` plus (shape from `PLUGIN-WAF-PLAN.md` §4.3), and `plugins = ["waf-main"]` on the `bench` route:

```toml
[[plugin]]
name = "waf-main"
kind = "waf"
enabled = true

[plugin.config]
rules = ["/bench/waf/main.conf", "/bench/waf/crs/crs-setup.conf", "/bench/waf/crs/rules/*.conf"]
paranoia_level = 1
anomaly_threshold = 5
mode = "blocking"
request_body_limit = 262144
audit_log = false
```
If the shipped config shape differs from §4.3, follow the shipped one. If prx takes engine settings from TOML rather than from `main.conf`, add one `DEVIATIONS.md` row listing each TOML key and the `main.conf` directive it stands in for.

- [ ] **Step 2: Compose** — copy the `prx` service as `prx-waf` (`container_name: bench-prx-waf`, mount `prx-waf.toml` and `./waf:/bench/waf:ro`); add the name to `bench.sh`.
- [ ] **Step 3: Verify** — the two `curl`s from Task 4 Step 5 → 200, 403. Commit — `feat(bench): prx + waf target on the shared ruleset`

### Task 10: Gate, then measure, then write down the gap

- [ ] **Step 1: W1 gate** — `bench/waf/verdicts.sh prx-waf`, then `--diff prx-waf nginx-modsec` and `--diff prx-waf caddy-coraza`. Any URL where prx-waf disagrees with **both** competitors is a prx bug: fix it in T506/T507 before benchmarking. URLs where the competitors already disagree with each other (Task 7) are judged case by case and listed. **Do not run Step 2 until this passes** — benchmarking an engine that checks less produces a number that flatters us and means nothing.
- [ ] **Step 2: Full matrix** — Task 8 Steps 1–4 with `TARGETS="prx prx-waf"` on the same machine, same `RATE` values.
- [ ] **Step 3: Gap table** — fill the `prx-waf` column in `BENCHMARKS.md` §7 and add a W1–W6 pass/fail row per scenario. Every failing cell becomes an input to Phase C, ranked by how far it misses.
- [ ] **Step 4: Commit** — `docs: prx + waf measured against modsecurity and coraza`

---

# Phase C — close the gap (this is the body of T508)

### Task 11: Attribution before optimization

An end-to-end number says *that* prx-waf is slow somewhere; it does not say *where*. Two cheap instruments, no new infrastructure:

- [ ] **Step 1: Flamegraph under the worst failing scenario** — `scripts/profile.sh` (see `docs/PROFILING.md`) against `prx-waf` while `scripts/bench.sh prx-waf <scenario>` runs. Bucket self-time into: regex matching (`regex_automata::hybrid` vs `::pikevm` — note which), transformations, variable extraction / body parsing, allocation (`malloc`/`free`/`realloc`), plugin/chain overhead, everything else. Paste the percentages into the T508 task file.
- [ ] **Step 2: Engine-only bench** — `benches/waf.rs` (already T508 scope item 4): feed the engine parsed requests built from `bench/waf/corpus/` with no sockets, reporting µs/request for `waf-get`, `waf-form`, `waf-json-128k`, `waf-attack-only` shapes. This is the inner loop for every hypothesis below; the e2e harness is only re-run to confirm.
- [ ] **Step 3 (only if Step 1 is ambiguous about engine vs integration):** a ~60-line Go program that replays the same corpus through `coraza.NewWAF` + `tx.ProcessURI/AddRequestHeader/ProcessRequestHeaders/ProcessRequestBody` and prints µs/request. It gives an engine-to-engine number with both proxies removed. Skipped by default: the WAF-tax column already approximates it.

### Task 12: Hypothesis loop

One hypothesis per branch. For each: (1) state the expected gain and the profile bucket it attacks, (2) change, (3) `cargo bench --bench waf` before/after, (4) T507 tests + `verdicts.sh --diff` unchanged, (5) e2e rerun of the failing scenario, (6) record kept/reverted **with numbers** in the T508 file. A change that does not move its bucket is reverted, not kept "because it can't hurt".

Ranked by expected payoff from what T504 already measured; the flamegraph from Task 11 overrides this order.

| # | Hypothesis | Evidence it matters | Confirm in profile | Risk / gate |
|---|---|---|---|---|
| H1 | **Constant-fold the ruleset at load.** Drop rules above the configured paranoia level, resolve `skipAfter`/`SecMarker` to jump indices, and fold comparisons on `TX` variables that are set once in `crs-setup.conf` and never written per request | 190 of 678 rules are `@lt`, almost all paranoia-level gates evaluated per request by interpreting engines | time in non-`@rx` operator eval and rule dispatch | a `TX` var that *is* mutated per request (anomaly scores) must never be folded — fold only vars with no `setvar` outside phase-less `SecAction` |
| H2 | **Memoize transformations per (variable, transform chain) per request** (plan §5.3) | CRS repeats `t:urlDecodeUni,t:lowercase,…` chains across hundreds of rules; cost is × number of variables | transformation bucket > regex bucket | cache key must include the full ordered chain |
| H3 | **Lazy variable extraction from a load-time needs mask** (plan §5.4) — never build `ARGS_POST`/JSON/XML/`FILES` collections that no enabled rule targets | `waf-json-128k` budget row; body parsing is pure overhead on rules that look only at headers | extraction/body-parse bucket on body scenarios | none for verdicts if the mask is derived from the compiled ruleset |
| H4 | **`is_match` on bytes, captures only on demand.** Use `regex::bytes`, never run capture groups unless the rule has the `capture` action, skip UTF-8 validation of inputs | capture search is several times slower than `is_match`; few CRS rules use `capture` | `captures`/`pikevm`/`backtrack` frames under rules without `capture` | none |
| H5 | **Keep the lazy DFA from thrashing.** CRS patterns are large; if the hybrid cache overflows, `regex` clears it and eventually falls back to the PikeVM. Raise `dfa_size_limit` for the big patterns and measure memory (W5) | T504: the `RegexSet` could not even build under the 10 MB default — these are big automata | `pikevm` self-time, cache-clear counts | trades RSS for speed; W5 must still pass |
| H6 | **Aho-Corasick where CRS already asks for it, prefilter where it pays.** `@pm`/`@pmFromFile` (30 rules) as one automaton each; literal prefilter only for the 110 of 312 patterns that yield a usable literal | T504 measured 2× (34.8 % of regex runs skipped), not more | regex bucket still dominant after H4/H5 | prefilter must be conservative: a missed literal is a missed attack |
| H7 | **Per-request scratch reuse** — transformation buffers and collection vectors come from the plugin's per-request state slot and are reused, not reallocated per rule | only if the allocation bucket is > ~10 % | `malloc`/`free` self-time | none |
| H8 | **Short-circuit in blocking mode** once the inbound score has crossed the threshold (T508 scope 3) | helps `waf-attack-only` and the DoS case only | — | semantic change vs CRS unless it mirrors `tx.early_blocking`; enable the same behaviour on **all** targets or none, and detection mode always runs everything |

Out of scope unless W2–W4 still fail after H1–H8, and then only as a new task with numbers attached: Hyperscan/Vectorscan, hand-written SIMD, a custom regex engine (same line T508 already draws).

### Task 13: Keep it fast

- [ ] Add `waf-get` and `waf-json-128k` for `prx-waf` to the macro perf gate: `scripts/perf-gate.sh macro waf-get` against `bench/results/baseline/`. The gate compares prx-waf to **its own** baseline (the existing mechanism); it cannot compare against competitors, whose numbers are only valid on the machine that produced them.
- [ ] Prove the gate can go red: add a deliberate `std::thread::sleep(Duration::from_micros(200))` in the WAF's request-head hook on a scratch branch, run the gate, confirm failure, discard the branch (T508 acceptance criterion).
- [ ] Re-run Task 8 + Task 10 on each CRS bump and each ModSecurity/Coraza major release; the claim "faster than X" carries the versions it was measured against.

---

## Phase A execution log

Status on 2026-09-20 (branch `bench/waf-comparison`, macOS + OrbStack — harness debugging only, no publishable numbers):

| Task | State | Notes |
|---|---|---|
| 1 sampling | done, verified on `nginx` | Also fixed three bugs that meant the harness had never produced a valid run: `oha -j` no longer exists (→ `--output-format json`); the sampler was started inside `$(...)`, which blocks until the background job ends, so it sampled an **idle** proxy before the load began; the `RETURN` trap fired again in `main` with `tmp` unbound. nginx now reports 27 µs CPU/request instead of 0.03 |
| 2 backend bodies | done, verified | 2000 × 128 KB POST over keep-alive through nginx → 100 % 200 |
| 3 shared ruleset | done | The T504 decision record named commit `8d06076`; that is CRS `main`. The `v4.21.0` tag is `2ac6c00` and has the 678 `SecRule`s T504 counted, so T504 measured the tag and only the hash was mislabelled. Pin and record corrected |
| 4 `nginx-modsec` | done, verified | nginx 1.30.5 + ModSecurity-nginx 1.0.4 + libmodsecurity 3.0.16, 826 rules loaded, pinned by digest. Benign → 200; SQLi/XSS in query and in a JSON body → 403. The plain `nginx` twin was moved from 1.27 to 1.30.5 so the pair differs by the WAF only. The image's healthcheck is disabled (it would add requests to the run) |
| 5 `caddy`, `caddy-coraza` | done, verified | Caddy 2.11.4 + `coraza-caddy` v2.6.1 (`http.handlers.waf`). Loads the shared `modsec.conf` unchanged — `DEVIATIONS.md` is still empty. Same 200/403 results as `nginx-modsec`, including SQLi inside a JSON body |
| 6 workloads | done, verified on `nginx` and `nginx-modsec` | `bench-parse.py` now counts responses rather than attempts (oha's `requestsPerSec` includes requests it aborted at the deadline) and survives a run where nothing completed. PSS needed `cap_add: SYS_PTRACE`: without it root cannot read `smaps_rollup` of the unprivileged workers and memory silently read as ~0 |
| 7 verdict diff | done | Both engines: 1000/1000 benign → 200; of 861 attack URLs 580 → 403 and 281 pass at PL1. **`--diff nginx-modsec caddy-coraza` is empty: 0 disagreements in 1,861 URLs.** There is no noise floor to hide behind — in Task 10 prx-waf must match all 1,861 |
| 8 baseline | procedure run once here (provisional tables below); **publishable run still owed** | needs the reference Linux machine. `BENCHMARKS.md` §7 holds the empty table; T508 budgets are annotated with the provisional competitor numbers |

### Provisional competitor baseline (laptop — orders of magnitude only, do not publish)

macOS + OrbStack, proxy on 2 CPUs, `oha` on the same host, git `f2bc3b1`. The full Task 8 procedure was run once to prove it end to end: 5 targets × 6 scenarios at saturation, then 5 scenarios at a fixed rate (half the slower WAF target's saturation rps). 20 s runs at 64 connections; JSON scenarios 45–60 s at 8 and 4 connections. No errors, `DEVIATIONS.md` still empty.

**Saturation — throughput, CPU and memory**

| Scenario | nginx → +ModSecurity rps | caddy → +Coraza rps | CPU tax µs/req (ModSec / Coraza) | peak PSS MB (ModSec / Coraza) |
|---|---|---|---|---|
| `waf-get` | 70,613 → 1,443 | 23,430 → 2,115 | 1,350 / **851** | 85 / 172 |
| `waf-attack-mix` | 67,301 → 1,598 | 23,996 → 2,097 | 1,217 / **858** | 85 / 184 |
| `waf-attack-only` | 66,605 → 1,804 | 23,407 → 2,302 | 1,074 / **770** | 84 / 522 |
| `waf-form` (20 fields) | 63,779 → 497 | 21,251 → 265 | **3,973** / 7,395 | 87 / 226 |
| `waf-json-16k` (970 leaves, 8 conns) | 21,598 → 10.6 | 17,082 → 12.2 | 189 ms / **163 ms** | 88 / 1,406 |
| `waf-json-128k` (7,815 leaves, 4 conns) | 11,505 → 0.62 (+5 × 504) | 7,832 → 1.28 | 3.43 s / **1.56 s** | 105 / 5,528 |

**Fixed rate — latency added by the WAF (on − off, ms)**

| Scenario @ rate | ModSecurity p50 / p99 | Coraza p50 / p99 |
|---|---|---|
| `waf-get` @ 721 rps | +0.54 / **+1.2** | **+0.11** / +9.3 |
| `waf-attack-mix` @ 798 | +0.75 / **+5.8** | **+0.14** / +8.6 |
| `waf-attack-only` @ 902 | +0.35 / +25.7 | **+0.21** / **+8.5** |
| `waf-form` @ 132 | **+3.8** / **+2.5** | +5.6 / +24.2 |
| `waf-json-16k` @ 5 | +180 / +367 | **+157** / **+270** |

**What prx-waf has to beat** (best competitor per cell, to be re-measured on the reference machine): on `waf-get` more than 2,115 rps (W2 asks 1.2× → ~2,540), a CPU tax under ~850 µs/request, and under +1.2 ms p99 at light load; on `waf-form` under ~4 ms CPU and +2.5 ms p99; on `waf-json-16k` more than 12 rps. Neither incumbent wins everywhere: Coraza is the cheaper engine on GETs and JSON but pays for it in p99 (Go GC) and memory; ModSecurity is cheaper on forms and flat on memory. T504 measured 82 µs for all 312 regexes against one string, so there is an order of magnitude of room on `waf-get`.

What this already says about where to aim:

1. **ModSecurity's cost is per variable, not per byte.** 20 form fields cost ~3× a bare GET; 970 JSON leaves cost ~360×; 7,815 leaves take seconds and grow faster than linearly (8× the leaves, 14× the time). This is the `82 µs × number of variables` warning from T504 at full size. The T508 budget of +5 ms at 128 KB is ~500× tighter than the incumbent on this body shape — prx does not need heroics to win here, it needs to not repeat the per-variable × per-rule loop. H2 (transformation memo), H3 (lazy extraction) and H6 (one Aho-Corasick pass per value before any regex) are the hypotheses that attack exactly this.
2. **Body shape is part of the scenario.** "128 KB JSON" means nothing without the leaf count; the corpus is argument-dense (5 short leaves per ~84 bytes), which is realistic for API traffic and close to worst case for a WAF. Report leaf counts next to body sizes, and add one sparse body (few large values) before publishing so both ends are visible.
3. **A WAF this slow is a DoS lever.** One 128 KB request pins a worker for seconds; 4 of them stall the proxy. prx must bound WAF work per request (a variable-count cap and/or a time budget that fails closed or open by config) — this belongs in T503/T507, not only in T508.
4. **Coraza's memory is unbounded under concurrent bodies.** 64 concurrent 16 KB JSON posts drove `caddy-coraza` to 7.8 GB PSS (~120 MB per in-flight request) with nothing completed in 10 s. W5 needs a third number besides idle and peak-on-GET: **peak under concurrent body inspection**, and prx should hold it flat by construction (bounded per-request scratch, H7) rather than by luck.
5. **A synchronous WAF stalls its whole worker.** `nginx-modsec` returned 504s on `waf-json-128k` although the backend answers instantly: while a worker spends seconds inside libmodsecurity, every other connection on that worker waits, including upstream reads that then hit `proxy_read_timeout`. This is the "synchronous API in an async worker" objection from `PLUGIN-WAF-PLAN.md` §2, observed. prx has the same exposure if rule evaluation runs inline on a pingora runtime thread: evaluation that can exceed ~1 ms must yield or run off the I/O threads. Decide this in T503/T506, before T508.
6. **Slow scenarios need long runs.** With seconds per request, a 10 s window completes almost nothing and requests aborted during warm-up are still being chewed on when measurement starts. Use `DURATION=120 CONNECTIONS=8` for the JSON scenarios on WAF targets.

**W5 idle memory and W6 cold start** (3 runs each, identical to ±0.05 s / ±2 MB):

| Target | Cold start to first 200 | Idle PSS | CRS cost |
|---|---|---|---|
| `nginx` → `nginx-modsec` | 0.5 s → 0.4 s | 34 → 76 MB | **+42 MB** |
| `caddy` → `caddy-coraza` | 0.4 s → 0.4 s | 48 → 90 MB | **+42 MB** |
| `prx` | 0.35 s | 17 MB | — |

- **W6 as written does not discriminate.** Both engines parse all of CRS in well under the ~0.3 s it takes Docker to create the container; T504 measured 236 ms for a pure-Rust load. Keep W6 as "not slower than both", and stop treating start-up as a place to win. If load time ever matters it will be on hot reload under traffic, which needs its own measurement.
- **W5 has a concrete number: CRS costs both incumbents +42 MB idle.** T508's assumed +50 MB budget is therefore not a win condition; tighten it to **< +42 MB**. prx starts 17–31 MB below the others, so it can win W5 in absolute terms even at parity on the delta.

**WAF-off proxies on the same scenarios** (same caveats): the floor each WAF sits on.

| Scenario | `nginx` rps / CPU µs | `caddy` rps / CPU µs | `prx` rps / CPU µs | `prx` PSS MB |
|---|---|---|---|---|
| `waf-get` | 70,613 / 27 | 23,430 / 77 | 18,050 / 85 | 28 |
| `waf-attack-mix` | 67,301 / 28 | 23,996 / 75 | 18,279 / 85 | 29 |
| `waf-form` | 63,779 / 30 | 21,251 / 86 | 17,109 / 94 | 29 |
| `waf-json-16k` (8 conns) | 21,598 / 50 | 17,082 / 101 | 15,530 / 100 | 24 |
| `waf-json-128k` (4 conns) | 11,505 / 81 | 7,832 / 152 | 7,839 / 158 | 23 |

This is the first proxy-vs-proxy run the harness has produced, and bare prx spends ~3× nginx's CPU per request. For the WAF goal it is small change — both incumbents add 850–1,400 µs per benign GET, so prx-waf wins W2–W4 as long as its WAF tax stays under roughly 750 µs, and T504 suggests far less. But it caps how large the win can look in absolute terms, it is a miss against the repo's own "RPS per core equal or better" goal, and it must be confirmed on the reference machine and profiled (`docs/PROFILING.md`) as its own task — not inside this plan.

**Resolved blockers.** (1) The Docker disk filled because `Dockerfile` does `COPY . .` and `.dockerignore` only excluded the top-level `target/`, so an image build copied `.claude/worktrees/*/target` (2.8 GB) into the build cache; `.dockerignore` fixed, cache pruned. (2) The prx `Dockerfile` pinned `rust:1.85` while `rcgen`/`time` need ≥ 1.88; now 1.93 and the image builds. (3) The `prx` compose service passed `-c <toml>`, which pingora reads as its own YAML server conf, so the bench prx had never started; the path goes in `PRX_CONFIG` only.

### Review of Phase A (independent reviewer, 2026-09-20) — what changed and what it invalidates

An independent read-only review returned 17 findings. One was checked and dismissed: the `nginx` and `nginx-modsec` images run a **byte-identical** `/usr/sbin/nginx` (same sha256, same configure arguments), so the twins already differ by the WAF module only. The rest were fixed:

- **`verdicts.sh` could certify a comparison it never made.** `join` drops URLs present in one file only, timeouts were booked as "allowed", and nothing tied a file to the target that produced it. It now refuses files that do not cover the whole current corpus, treats anything but 200/403 as a failed run, checks which container holds the port, and sends the same headers as the load scenarios. The "0 disagreements in 1,861 URLs" above was re-checked and holds (both files had 1,861 rows, only 200/403), but it must be re-run on the new corpus.
- **The CPU window was longer than the load window** (one run reported 127 CPU-seconds on a 120 CPU-second budget). CPU is now read synchronously right before and after the load; PSS is sampled separately every 2 s. Results carry `cpu_window_s`, `proxy_cpus` and **`cpu_utilisation`**.
- **Saturation was never checked.** At 64 connections prx was only ~77 % CPU-busy, so its "18k rps" is **not a saturation number** and not comparable to nginx's rps; only CPU µs/request is. The cause, reported by the separate profiling task and consistent with the code: `upstream_peer` builds `HttpPeer::new(upstream.addr.clone(), …)` from a *hostname* on every request (`src/proxy.rs:815`), which is a blocking `getaddrinfo` on a runtime thread — the workers were waiting on DNS, not on CPU or connections. With IP-literal upstreams that task measured prx at ~36k rps and ~53 µs/request instead of ~16k and ~93. **Every prx row in this document predates that finding**; re-measure prx after it is fixed (resolve at config load / cache the resolved address), and until then read the prx floor as pessimistic. The fix belongs to that task, not to this plan. Treat any saturation row under ~90 % `cpu_utilisation` as not a W2 data point, and raise `CONNECTIONS` until the proxy is the limit.
- **`waf-attack-only` was one-third allowed traffic.** The corpus now has two lists: `attacks.txt` (all 859, for verdicts, where an allow is as telling as a block) and `blocked.txt` (533 URLs CRS itself expects a PL1 rule to fire on; 98 % are blocked), which the load scenarios use. **The provisional `waf-attack-*` rows above predate this and will move.**
- `bench.sh` now fails a run whose status mix is wrong for its scenario (benign blocked, attacks not blocked, a no-WAF target blocking), refuses to start when another run holds the port, rejects `RATE` on non-waf scenarios, clamps negative CPU deltas, and parses `/proc/*/stat` correctly when `comm` contains spaces. `bench-compare.sh` no longer mixes fixed-rate and saturation rows.
- Both Caddyfiles set `keepalive_idle_conns_per_host 256` for parity with nginx's pool. The reviewer suspected Go was redialling upstream connections without it; **an A/B run shows no effect** (23,051 vs 23,676 rps, p50 1.58 vs 1.56 ms, both ~91 % CPU-busy), so the provisional Caddy numbers above stand.
- The backend closes a chunked request instead of mis-framing it into meaningless 200s. `#` URIs are dropped from the corpus (curl and oha truncate at the fragment).
- Proxy-configuration differences that cannot be removed are now recorded in `bench/waf/DEVIATIONS.md`, including why the nginx 504s on `waf-json-128k` are evidence of a stalled worker rather than an isolated proof, and that prx is built without LTO.

**Re-verified after the fixes** (10 s runs, same laptop caveats):

- Verdicts on the new 1,859-URL corpus, now sent with the full set of benchmark headers: both engines 1,279 × 200 and 580 × 403, **`--diff` compared 1,859 URLs and found 0 disagreements**. W1's bar for prx-waf is unchanged: match all of them.
- `waf-attack-only` on `blocked.txt`: 98 % blocked on both engines (ModSecurity 1,780 rps, 1,127 µs/req; Coraza 2,150 rps, 907 µs/req). `waf-attack-mix`: ~10 % blocked on both. The status-mix check passes on all four runs.
- `cpu_utilisation` at 64 connections: nginx-modsec 1.00, caddy-coraza 0.98, nginx 0.92, caddy 0.91 — saturated. **prx 0.77 at 64 and at 256 connections with the same rps**, i.e. not connection-bound: consistent with workers blocked in the per-request DNS lookup described above.
- The port guard was exercised for real: four runs refused to start while another session's `bench-prx` held 18080, and `verdicts.sh` refused to write a file for a target that was not the one answering. One lesson the guard does not cover: both sessions shared the compose project, so a manual `docker compose down` from one removed the other's backends. Tear down with `rm -sf <target>`, never `down`, while anyone else may be using the project.

## Threats to validity (say these out loud in `BENCHMARKS.md`)

- **Same host for load and proxy.** `oha` competes for CPU. On Linux pin it with `taskset -c 4-7`; on a 4-core box the proxy numbers are a lower bound.
- **Caddy is not nginx.** A slow `caddy-coraza` absolute number may be Caddy, not Coraza. That is why every criterion is also read through the WAF-tax column.
- **Status-code verdicts are coarse.** Two engines can both return 403 for different rules. Good enough to stop "faster because it checks less"; not a conformance claim. Conformance is go-ftw in T507.
- **GET-only attack traffic under load.** Body-borne attacks are exercised for verdicts, not for throughput. The POST scenarios are benign-only.
- **Loopback + in-memory backend** makes the proxy's share of latency look larger than in production (already noted in `BENCHMARKS.md` §4). Fine for a relative comparison; do not quote the absolute overheads as production numbers.
- **One ruleset configuration.** PL1 only. PL2+ multiplies rule count and may reorder the ranking; measure it before claiming anything about it.

## Self-review

- Spec coverage: T508 scope items 1, 3, 4, 5, 6 map to H6, H8, Task 11 Step 2, Task 13, W5; item 2 (`RegexSet`) stays removed per T504. T508's "compare with nginx+modsecurity on the same CRS" is Tasks 3–4, 8, 10.
- Unverified externals are turned into explicit verification steps rather than assumed: the ModSecurity image's module path and digest (Task 4 Step 1), the `coraza-caddy` tag (Task 5 Step 1), `oha` flags (Task 6 Step 1), CRS test-YAML shape (Task 6 Step 3 checks the attack count).
- Names used across tasks: targets `nginx-modsec`, `caddy`, `caddy-coraza`, `prx-waf`; scenarios `waf-get`, `waf-form`, `waf-json-16k`, `waf-json-128k`, `waf-attack-mix`, `waf-attack-only`; paths `/bench/waf/main.conf`, `/bench/waf/modsec.conf`, `/bench/waf/crs/`; corpus files `benign.txt`, `attacks.txt`, `mix.txt`, `form.txt`, `json-16k.json`, `json-128k.json`.
