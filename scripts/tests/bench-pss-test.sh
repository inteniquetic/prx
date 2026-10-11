#!/usr/bin/env bash
#
# Self-test for the PSS sampler in scripts/bench.sh. Uses a fake `docker` on PATH,
# so it needs no containers. Covers: a failing exec must yield 0 samples (the
# caller then marks the run invalid), and a working exec must yield >0 samples
# with the peak taken from Pss: lines.
#
# Usage: bash scripts/tests/bench-pss-test.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

BIN="${WORK}/bin"
mkdir -p "${BIN}"
# fake docker: FAKE_DOCKER_MODE=fail exits 1 with no output; ok prints smaps-like text
cat > "${BIN}/docker" <<'EOF'
#!/usr/bin/env bash
if [ "${FAKE_DOCKER_MODE:-ok}" = fail ]; then
  echo "Error: No such container" >&2
  exit 1
fi
printf 'Rss:  100 kB\nPss:  %s kB\n' "${FAKE_PSS:-1000}"
printf 'Pss:  %s kB\n' "$(( ${FAKE_PSS:-1000} / 2 ))"
EOF
chmod +x "${BIN}/docker"

# Load only the function definitions (no main run).
BENCH_SOURCE_ONLY=1 source "${ROOT}/scripts/bench.sh"

fail=0
check() { # name, expected, actual
  if [ "$2" = "$3" ]; then echo "ok   - $1"; else echo "FAIL - $1: expected '$2' got '$3'"; fail=1; fi
}

run_sampler() { # mode, pss -> prints "samples peak"
  local dir="${WORK}/run-$RANDOM"
  mkdir -p "${dir}"
  touch "${dir}/stop" 2>/dev/null || true
  rm -f "${dir}/stop"
  ( PATH="${BIN}:${PATH}" FAKE_DOCKER_MODE="$1" FAKE_PSS="$2" \
      sample_peak_pss fake-container "${dir}/out" "${dir}/stop" ) &
  local pid=$!
  sleep 3
  touch "${dir}/stop"
  wait "${pid}"
  cat "${dir}/out"
}

read -r s_fail p_fail <<< "$(run_sampler fail 0)"
check "failing docker exec -> zero samples" "0" "${s_fail}"
check "failing docker exec -> zero peak" "0" "${p_fail}"

read -r s_ok p_ok <<< "$(run_sampler ok 1000)"
[ "${s_ok}" -gt 0 ] && r=yes || r=no
check "working docker exec -> >0 samples" "yes" "${r}"
# each sample sums both Pss lines: 1000 + 500 = 1500
check "peak is the summed Pss of a sample" "1500" "${p_ok}"

[ "${fail}" -eq 0 ] && echo "PASS: bench PSS sampler self-test" || { echo "FAIL: bench PSS sampler self-test"; exit 1; }
