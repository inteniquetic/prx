#!/usr/bin/env bash
#
# T003: record a flamegraph of prx while the T001 harness drives load at it.
#
# Usage: scripts/profile.sh [scenario]        (default: h1-keepalive)
#
# Requires: cargo-flamegraph (cargo install flamegraph) and perf.
# On Linux you also need:  sysctl -w kernel.perf_event_paranoid=1
#
# The proxy is built with the `profiling` profile (release + debug symbols) so
# frames carry real names. Output: bench/results/flamegraph-<scenario>-<sha>.svg

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENARIO="${1:-h1-keepalive}"
DURATION="${DURATION:-30}"
CONNECTIONS="${CONNECTIONS:-64}"
LISTEN="${LISTEN:-127.0.0.1:8080}"
SHA="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || echo unknown)"
OUT="${ROOT}/bench/results/flamegraph-${SCENARIO}-${SHA}.svg"

die() { echo "error: $*" >&2; exit 1; }

command -v cargo-flamegraph >/dev/null || command -v flamegraph >/dev/null \
  || die "cargo-flamegraph is required: cargo install flamegraph"
command -v oha >/dev/null || die "oha is required: cargo install oha"

paranoid="$(cat /proc/sys/kernel/perf_event_paranoid 2>/dev/null || echo 2)"
[ "${paranoid}" -le 1 ] || die "perf_event_paranoid=${paranoid}; run: sudo sysctl -w kernel.perf_event_paranoid=1"

# Backends: reuse the harness backend, run locally (no docker needed here).
( cd "${ROOT}/bench/backend" && cargo build --release >/dev/null )
LISTEN=127.0.0.1:8001 "${ROOT}/bench/backend/target/release/prx-bench-backend" >/dev/null 2>&1 &
B1=$!
LISTEN=127.0.0.1:8002 "${ROOT}/bench/backend/target/release/prx-bench-backend" >/dev/null 2>&1 &
B2=$!
cleanup() { kill "${B1}" "${B2}" 2>/dev/null || true; rm -f "${CONFIG}"; }
trap cleanup EXIT

CONFIG="$(mktemp /tmp/prx-profile-XXXXXX.toml)"
cat > "${CONFIG}" <<EOF
[server]
listen = ["${LISTEN}"]
threads = 4

[observability]
log_level = "error"
access_log = false

[[service]]
name = "bench"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:8001"

[[service.upstream]]
addr = "127.0.0.1:8002"

[[route]]
name = "bench"
service = "bench"
path_prefix = "/"
is_default = true
EOF

echo "==> profiling ${SCENARIO} for ${DURATION}s -> ${OUT}"
# Drive load in the background; flamegraph owns the proxy process lifetime.
(
  sleep 3
  oha --no-tui -z "${DURATION}s" -c "${CONNECTIONS}" "http://${LISTEN}/1k" >/dev/null
  pkill -INT -f 'target/profiling/prx' || true
) &

PRX_CONFIG="${CONFIG}" PRX_ADMIN_LISTEN=127.0.0.1:9099 \
  cargo flamegraph --profile profiling --bin prx --output "${OUT}"

echo "==> wrote ${OUT}"
echo "    open it in a browser and check which frames dominate; see docs/PROFILING.md"
