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
# Scenarios: h1-keepalive | h1-close | h2 | large-body | many-conns | slow-upstream
#
# Env knobs:
#   DURATION=60        measurement seconds (after warmup)
#   WARMUP=10          warmup seconds, discarded
#   CONNECTIONS=64     concurrent connections
#   BENCH_HOST         host:port of the proxy under test (default 127.0.0.1:18080)
#   KEEP_UP=1          leave containers running after the run
#
# Results: bench/results/<scenario>-<target>-<git-sha>.json
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
  compose --profile "${target}" up -d --build >/dev/null
  # Wait for the proxy to answer before the warmup starts.
  for _ in $(seq 1 60); do
    if curl -fsS -o /dev/null --max-time 1 "http://${BENCH_HOST}/1k"; then return 0; fi
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
  esac
}

# Peak RSS of the proxy process inside its container, sampled once per second.
# Container-level stats are avoided on purpose: they include the runtime's own
# overhead, which differs between images and would not be a fair comparison.
sample_process_stats() {
  local container="$1" out="$2" seconds="$3"
  (
    local peak_rss_kb=0 utime_start=0 utime_end=0
    for _ in $(seq 1 "${seconds}"); do
      # PID 1 in each of these images is the proxy itself.
      local rss
      rss="$(docker exec "${container}" sh -c "awk '/VmRSS/{print \$2}' /proc/1/status" 2>/dev/null || echo 0)"
      [ -n "${rss}" ] && [ "${rss}" -gt "${peak_rss_kb}" ] && peak_rss_kb="${rss}"
      sleep 1
    done
    local cpu
    cpu="$(docker exec "${container}" sh -c "awk '{print \$14+\$15}' /proc/1/stat" 2>/dev/null || echo 0)"
    printf '{"peak_rss_kb":%s,"cpu_ticks":%s}\n' "${peak_rss_kb:-0}" "${cpu:-0}" > "${out}"
  ) &
  echo $!
}

run_load() {
  local scenario="$1" url="$2" conns="$3" seconds="$4" out="$5"
  case "${scenario}" in
    h2)
      h2load -n 0 -c "${conns}" -m 32 -D "${seconds}" "${url}" > "${out}" 2>&1
      ;;
    h1-close)
      oha --no-tui -j -z "${seconds}s" -c "${conns}" --disable-keepalive "${url}" > "${out}"
      ;;
    *)
      oha --no-tui -j -z "${seconds}s" -c "${conns}" "${url}" > "${out}"
      ;;
  esac
}

run_one() {
  local target="$1" scenario="$2"
  require_tools "${scenario}"

  local path conns url sha stamp
  path="$(scenario_path "${scenario}")"
  conns="$(scenario_connections "${scenario}")"
  url="http://${BENCH_HOST}${path}"
  sha="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  mkdir -p "${RESULT_DIR}"
  local tmp; tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  echo "==> ${target} / ${scenario}  (${conns} conns, warmup ${WARMUP}s, measure ${DURATION}s)"
  start_target "${target}"

  echo "    warmup..."
  run_load "${scenario}" "${url}" "${conns}" "${WARMUP}" "${tmp}/warmup.out" || true

  echo "    measuring..."
  local stats_pid
  stats_pid="$(sample_process_stats "$(container_for "${target}")" "${tmp}/stats.json" "${DURATION}")"
  run_load "${scenario}" "${url}" "${conns}" "${DURATION}" "${tmp}/load.out"
  wait "${stats_pid}" 2>/dev/null || true

  local out="${RESULT_DIR}/${scenario}-${target}-${sha}.json"
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
    > "${out}"

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
    "${ROOT}/scripts/bench-compare.sh" "${scenario}"
    return
  fi
  target="${1:?target required: prx|nginx|haproxy}"
  scenario="${2:?scenario required}"
  run_one "${target}" "${scenario}"
}

main "$@"
