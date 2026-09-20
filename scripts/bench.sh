#!/usr/bin/env bash
#
# T001 benchmark harness: run one scenario against one target and write a JSON
# result under bench/results/.
#
# Usage:
#   scripts/bench.sh <target> <scenario>
#   TARGETS="prx nginx haproxy" scripts/bench.sh --all <scenario>
#
# Targets:   prx | nginx | haproxy
#            nginx-modsec | caddy | caddy-coraza   (WAF comparison)
# Scenarios: h1-keepalive | h1-close | h2 | large-body | many-conns | slow-upstream
#            waf-get | waf-form | waf-json-16k | waf-json-128k | waf-attack-mix
#            | waf-attack-only   (docs/WAF-BENCH-PLAN.md; run them against the
#            WAF-off twins too: the difference is the WAF tax)
#
# The waf-* scenarios need bench/waf/fetch-crs.sh and bench/waf/gen-corpus.py
# to have been run once.
#
# Env knobs:
#   DURATION=60        measurement seconds (after warmup)
#   WARMUP=10          warmup seconds, discarded
#   CONNECTIONS=64     concurrent connections
#   BENCH_HOST         host:port of the proxy under test (default 127.0.0.1:18080)
#   KEEP_UP=1          leave containers running after the run
#   RATE=<rps>         waf-* only: fixed request rate with coordinated-omission
#                      correction. Use it for latency numbers; leave it unset
#                      (saturation) for throughput and CPU numbers.
#
# Results: bench/results/<scenario>[-q<RATE>]-<target>-<git-sha>.json
#
# Needs oha >= 1.0 (--output-format, --urls-from-file).
#
# Read bench/README.md before trusting any number this produces.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_DIR="${ROOT}/bench"
RESULT_DIR="${ROOT}/bench/results"

DURATION="${DURATION:-60}"
WARMUP="${WARMUP:-10}"
CONNECTIONS="${CONNECTIONS:-64}"
BENCH_HOST="${BENCH_HOST:-127.0.0.1:18080}"
KEEP_UP="${KEEP_UP:-0}"
RATE="${RATE:-}"

# Every waf-* request looks like a browser. A numeric-IP Host or a tool
# User-Agent scores CRS points on its own and would turn the benign scenarios
# into attack scenarios.
CORPUS="${ROOT}/bench/waf/corpus"
BROWSER_UA='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36'
WAF_HEADERS=(--host bench.local
  -H "User-Agent: ${BROWSER_UA}"
  -H 'Accept: text/html,application/json;q=0.9,*/*;q=0.8'
  -H 'Accept-Language: en-US,en;q=0.9'
  -H 'Cookie: sid=9f8a7c6b5d4e3f2a; theme=dark; lang=en')

die() { echo "error: $*" >&2; exit 1; }

compose() { docker compose -f "${COMPOSE_DIR}/docker-compose.yml" "$@"; }

require_tools() {
  command -v docker >/dev/null || die "docker is required"
  docker compose version >/dev/null 2>&1 || die "docker compose v2 is required"
  case "$1" in
    h2) command -v h2load >/dev/null || die "h2load (nghttp2-client) is required for the h2 scenario" ;;
    *)  command -v oha >/dev/null || die "oha is required (cargo install oha)" ;;
  esac
}

scenario_path() {
  case "$1" in
    large-body) echo "/64k" ;;
    slow-upstream) echo "/slow/50" ;;
    *) echo "/1k" ;;
  esac
}

scenario_connections() {
  case "$1" in
    many-conns) echo "${CONNECTIONS_MANY:-10000}" ;;
    *) echo "${CONNECTIONS}" ;;
  esac
}

start_target() {
  local target="$1"
  # Another run (or another checkout of this repo) on the same port would be
  # measured instead of this target, silently.
  local holder
  holder="$(docker ps --filter "publish=${BENCH_HOST##*:}" --format '{{.Names}}' | grep -v "^$(container_for "${target}")\$" || true)"
  [ -z "${holder}" ] || die "port ${BENCH_HOST##*:} is held by ${holder}; another benchmark is running"
  compose --profile "${target}" up -d --build >/dev/null
  # Wait for the proxy to answer before the warmup starts.
  for _ in $(seq 1 60); do
    if curl -fsS -o /dev/null --max-time 1 -H 'Host: bench.local' -A "${BROWSER_UA}" "http://${BENCH_HOST}/1k" 2>/dev/null; then return 0; fi
    sleep 1
  done
  die "target ${target} did not become ready on ${BENCH_HOST}"
}

stop_target() {
  local target="$1"
  [ "${KEEP_UP}" = "1" ] && return 0
  compose --profile "${target}" down --remove-orphans >/dev/null 2>&1 || true
}

container_for() {
  case "$1" in
    prx) echo "bench-prx" ;;
    nginx) echo "bench-nginx" ;;
    haproxy) echo "bench-haproxy" ;;
    nginx-modsec) echo "bench-nginx-modsec" ;;
    caddy) echo "bench-caddy" ;;
    caddy-coraza) echo "bench-caddy-coraza" ;;
    *) die "unknown target: $1" ;;
  esac
}

# CPU and memory of the proxy inside its container. Container-level stats are
# avoided on purpose: they include the runtime's own overhead, which differs
# between images and would not be a fair comparison.
#
# Every process is counted, not PID 1: nginx does its work (and holds its rules)
# in workers, so PID 1 alone reports a master that did nothing.

# utime+stime of every process, in ticks. comm (field 2) may contain spaces, so
# fields are counted from the closing parenthesis, not from the start.
cpu_ticks() {
  docker exec -u 0 "$1" sh -c 'cat /proc/[0-9]*/stat 2>/dev/null' 2>/dev/null \
    | awk '{n=split(substr($0, index($0, ") ") + 2), f, " "); s += f[12] + f[13]} END {print s + 0}'
}

# Peak PSS until the stop file appears. PSS, not RSS: workers share pages, and
# summing RSS would count them once each. Every 2 s: smaps_rollup walks the whole
# address space inside the proxy's own cpuset, which is not free on a large heap.
sample_peak_pss() {
  local container="$1" out="$2" stop="$3" peak=0 pss
  while [ ! -e "${stop}" ]; do
    pss="$(docker exec -u 0 "${container}" sh -c 'cat /proc/[0-9]*/smaps_rollup 2>/dev/null' 2>/dev/null \
      | awk '/^Pss:/{s+=$2} END{print s+0}')"
    [ "${pss:-0}" -gt "${peak}" ] && peak="${pss}"
    sleep 2
  done
  echo "${peak}" > "${out}"
}

# One waf-* load run. "$@" is what varies: the URL list, or the body and URL.
waf_load() {
  local conns="$1" seconds="$2" out="$3"; shift 3
  local rate=()
  [ -n "${RATE}" ] && rate=(-q "${RATE}" --latency-correction)
  # ${rate[@]+...}: bash 3.2 (macOS) treats an empty array as unbound under set -u.
  oha --no-tui --output-format json -z "${seconds}s" -c "${conns}" \
    ${rate[@]+"${rate[@]}"} "${WAF_HEADERS[@]}" "$@" > "${out}"
}

run_load() {
  local scenario="$1" url="$2" conns="$3" seconds="$4" out="$5"
  case "${scenario}" in
    waf-*) [ -s "${CORPUS}/blocked.txt" ] || die "no corpus: run bench/waf/gen-corpus.py (see its docstring)" ;;
  esac
  case "${scenario}" in
    waf-get)         waf_load "${conns}" "${seconds}" "${out}" --urls-from-file "${CORPUS}/benign.txt" ;;
    waf-attack-mix)  waf_load "${conns}" "${seconds}" "${out}" --urls-from-file "${CORPUS}/mix.txt" ;;
    waf-attack-only) waf_load "${conns}" "${seconds}" "${out}" --urls-from-file "${CORPUS}/blocked.txt" ;;
    waf-form)        waf_load "${conns}" "${seconds}" "${out}" -m POST -T application/x-www-form-urlencoded -D "${CORPUS}/form.txt" "${url}" ;;
    waf-json-16k)    waf_load "${conns}" "${seconds}" "${out}" -m POST -T application/json -D "${CORPUS}/json-16k.json" "${url}" ;;
    waf-json-128k)   waf_load "${conns}" "${seconds}" "${out}" -m POST -T application/json -D "${CORPUS}/json-128k.json" "${url}" ;;
    h2)
      h2load -n 0 -c "${conns}" -m 32 -D "${seconds}" "${url}" > "${out}" 2>&1
      ;;
    h1-close)
      oha --no-tui --output-format json -z "${seconds}s" -c "${conns}" --disable-keepalive "${url}" > "${out}"
      ;;
    *)
      oha --no-tui --output-format json -z "${seconds}s" -c "${conns}" "${url}" > "${out}"
      ;;
  esac
}

# A WAF target that blocks benign traffic answers without touching the upstream
# and looks fast; one whose rules did not load looks fast too. Neither is a result.
check_status_mix() {
  local scenario="$1" target="$2" file="$3" blocked
  blocked="$(python3 -c 'import json,sys; c=json.load(open(sys.argv[1]))["status_codes"]; t=sum(c.values()) or 1; print(round(100*c.get("403",0)/t))' "${file}")"
  case "${scenario}:${target}" in
    waf-get:*|waf-form:*|waf-json-*:*)
      [ "${blocked}" -eq 0 ] || die "${target}/${scenario}: ${blocked}% of benign requests were blocked" ;;
    waf-attack-only:nginx-modsec|waf-attack-only:caddy-coraza|waf-attack-only:prx-waf)
      [ "${blocked}" -ge 90 ] || die "${target}/${scenario}: only ${blocked}% blocked; are the rules loaded?" ;;
    waf-attack-mix:nginx-modsec|waf-attack-mix:caddy-coraza|waf-attack-mix:prx-waf)
      { [ "${blocked}" -ge 5 ] && [ "${blocked}" -le 15 ]; } || die "${target}/${scenario}: ${blocked}% blocked, expected ~10%" ;;
    waf-attack-*:*)
      [ "${blocked}" -eq 0 ] || die "${target}/${scenario}: ${blocked}% blocked by a target with no WAF" ;;
  esac
}

run_one() {
  local target="$1" scenario="$2"
  container_for "${target}" >/dev/null   # reject an unknown target before starting anything
  case "${scenario}" in
    waf-*) ;;
    *) [ -z "${RATE}" ] || die "RATE only applies to waf-* scenarios" ;;
  esac
  require_tools "${scenario}"

  local path conns url sha stamp
  path="$(scenario_path "${scenario}")"
  conns="$(scenario_connections "${scenario}")"
  url="http://${BENCH_HOST}${path}"
  sha="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  mkdir -p "${RESULT_DIR}"
  local tmp; tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"; trap - RETURN' RETURN

  echo "==> ${target} / ${scenario}  (${conns} conns, warmup ${WARMUP}s, measure ${DURATION}s)"
  start_target "${target}"

  echo "    warmup..."
  run_load "${scenario}" "${url}" "${conns}" "${WARMUP}" "${tmp}/warmup.out" || true

  echo "    measuring..."
  # CPU is read right before and right after the load, in this shell, so the
  # window is the load and not a sampler loop that outlives it.
  local container pss_pid cpu_start cpu_end t_start t_end cpu_delta
  container="$(container_for "${target}")"
  sample_peak_pss "${container}" "${tmp}/pss" "${tmp}/stop" &
  pss_pid=$!
  cpu_start="$(cpu_ticks "${container}")"; t_start="$(date +%s)"
  run_load "${scenario}" "${url}" "${conns}" "${DURATION}" "${tmp}/load.out"
  cpu_end="$(cpu_ticks "${container}")"; t_end="$(date +%s)"
  touch "${tmp}/stop"; wait "${pss_pid}" 2>/dev/null || true
  # A worker that died mid-run takes its ticks with it; never report negative CPU.
  cpu_delta=$(( cpu_end - cpu_start )); [ "${cpu_delta}" -ge 0 ] || cpu_delta=0
  local cpus="${BENCH_PROXY_CPUS:-0,1}"
  printf '{"peak_rss_kb":%s,"cpu_ticks":%s,"cpu_window_s":%s,"proxy_cpus":%s}\n' \
    "$(cat "${tmp}/pss" 2>/dev/null || echo 0)" "${cpu_delta}" "$(( t_end - t_start ))" \
    "$(echo "${cpus}" | tr ',' '\n' | wc -l | tr -d ' ')" > "${tmp}/stats.json"

  local out="${RESULT_DIR}/${scenario}${RATE:+-q${RATE}}-${target}-${sha}.json"
  python3 "${ROOT}/scripts/bench-parse.py" \
    --target "${target}" \
    --scenario "${scenario}" \
    --tool "$([ "${scenario}" = "h2" ] && echo h2load || echo oha)" \
    --connections "${conns}" \
    --duration "${DURATION}" \
    --url "${url}" \
    --sha "${sha}" \
    --timestamp "${stamp}" \
    --load-output "${tmp}/load.out" \
    --stats "${tmp}/stats.json" \
    ${RATE:+--rate "${RATE}"} \
    > "${out}"
  check_status_mix "${scenario}" "${target}" "${out}"

  stop_target "${target}"
  echo "    wrote ${out}"
}

main() {
  local scenario target
  if [ "${1:-}" = "--all" ]; then
    scenario="${2:?scenario required}"
    for target in ${TARGETS:-prx nginx haproxy}; do
      run_one "${target}" "${scenario}"
    done
    RATE="${RATE}" "${ROOT}/scripts/bench-compare.sh" "${scenario}"
    return
  fi
  target="${1:?target required: prx|nginx|haproxy}"
  scenario="${2:?scenario required}"
  run_one "${target}" "${scenario}"
}

main "$@"
