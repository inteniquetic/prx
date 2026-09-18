#!/usr/bin/env bash
#
# T004: fail when a change makes prx measurably slower.
#
# Modes:
#   scripts/perf-gate.sh micro [baseline.json] [current.json]
#       compare criterion medians (default threshold: 5% slower fails)
#   scripts/perf-gate.sh macro <scenario> [baseline-dir] [current-dir]
#       compare harness results (default: 7% less rps or 10% more RSS fails)
#
# Thresholds are deliberately loose: shared CI runners are noisy, so this gate
# is for catching real regressions, not for measuring small wins. See
# bench/README.md.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:?mode required: micro|macro}"

case "${MODE}" in
  micro)
    BASE="${2:-${ROOT}/bench/results/baseline/micro-bench.json}"
    CURRENT="${3:-${ROOT}/bench/results/micro-bench-current.json}"
    MICRO_THRESHOLD="${MICRO_THRESHOLD:-5}" python3 - "${BASE}" "${CURRENT}" <<'PY'
import json, os, sys

threshold = float(os.environ["MICRO_THRESHOLD"])
base = json.load(open(sys.argv[1]))["benchmarks"]
current = json.load(open(sys.argv[2]))["benchmarks"]

rows, failures = [], []
for name, cur in sorted(current.items()):
    ref = base.get(name)
    if not ref:
        rows.append((name, None, cur["median_ns"], None, "new"))
        continue
    delta = (cur["median_ns"] / ref["median_ns"] - 1) * 100
    status = "SLOWER" if delta > threshold else ("faster" if delta < -threshold else "ok")
    if status == "SLOWER":
        failures.append((name, delta))
    rows.append((name, ref["median_ns"], cur["median_ns"], delta, status))

width = max(len(r[0]) for r in rows) if rows else 10
print(f"{'benchmark':<{width}}{'baseline ns':>15}{'current ns':>15}{'delta':>10}  status")
for name, ref, cur, delta, status in rows:
    ref_s = "-" if ref is None else f"{ref:,.1f}"
    delta_s = "-" if delta is None else f"{delta:+.1f}%"
    print(f"{name:<{width}}{ref_s:>15}{cur:>15,.1f}{delta_s:>10}  {status}")

missing = sorted(set(base) - set(current))
if missing:
    print(f"\nnote: {len(missing)} benchmark(s) present in the baseline are missing here")

if failures:
    print(f"\nFAIL: {len(failures)} benchmark(s) regressed by more than {threshold}%:")
    for name, delta in failures:
        print(f"  {name}  {delta:+.1f}%")
    sys.exit(1)
print(f"\nOK: no benchmark regressed by more than {threshold}%")
PY
    ;;

  macro)
    SCENARIO="${2:?scenario required}"
    BASE_DIR="${3:-${ROOT}/bench/results/baseline}"
    CUR_DIR="${4:-${ROOT}/bench/results}"
    RPS_THRESHOLD="${RPS_THRESHOLD:-7}" RSS_THRESHOLD="${RSS_THRESHOLD:-10}" \
      python3 - "${BASE_DIR}" "${CUR_DIR}" "${SCENARIO}" <<'PY'
import json, os, sys
from pathlib import Path

rps_threshold = float(os.environ["RPS_THRESHOLD"])
rss_threshold = float(os.environ["RSS_THRESHOLD"])
base_dir, cur_dir, scenario = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]

def latest(directory: Path) -> dict | None:
    files = sorted(directory.glob(f"{scenario}-prx-*.json"), key=lambda p: p.stat().st_mtime)
    return json.load(files[-1].open()) if files else None

base, cur = latest(base_dir), latest(cur_dir)
if base is None:
    print(f"no baseline for scenario={scenario} in {base_dir}; run the harness on a known-good commit first")
    sys.exit(1)
if cur is None:
    print(f"no current result for scenario={scenario} in {cur_dir}")
    sys.exit(1)

rps_base = base["throughput"]["requests_per_second"]
rps_cur = cur["throughput"]["requests_per_second"]
rss_base = base["resources"]["peak_rss_kb"] or 0
rss_cur = cur["resources"]["peak_rss_kb"] or 0

rps_delta = (rps_cur / rps_base - 1) * 100 if rps_base else 0.0
rss_delta = (rss_cur / rss_base - 1) * 100 if rss_base else 0.0

print(f"scenario {scenario}: rps {rps_base:,.0f} -> {rps_cur:,.0f} ({rps_delta:+.1f}%), "
      f"peak RSS {rss_base/1024:,.1f}MB -> {rss_cur/1024:,.1f}MB ({rss_delta:+.1f}%)")

failed = False
if rps_delta < -rps_threshold:
    print(f"FAIL: throughput dropped more than {rps_threshold}%")
    failed = True
if rss_delta > rss_threshold:
    print(f"FAIL: peak RSS grew more than {rss_threshold}%")
    failed = True
sys.exit(1 if failed else 0)
PY
    ;;

  *)
    echo "unknown mode: ${MODE} (expected micro|macro)" >&2
    exit 2
    ;;
esac
