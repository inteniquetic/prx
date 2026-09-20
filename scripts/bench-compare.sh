#!/usr/bin/env bash
#
# Print a comparison table for one scenario across all targets that have a
# result file for the current commit (or for the sha given as $2).
#
# Usage: scripts/bench-compare.sh <scenario> [git-sha]
#        RATE=<rps> scripts/bench-compare.sh <scenario> [git-sha]   fixed-rate runs

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENARIO="${1:?scenario required}"
SHA="${2:-$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || echo unknown)}"

python3 - "${ROOT}/bench/results" "${SCENARIO}" "${SHA}" "${RATE:-}" <<'PY'
import json
import sys
from pathlib import Path

results_dir, scenario, sha = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
rate = int(sys.argv[4]) if sys.argv[4] else None
rows = []
for path in sorted(results_dir.glob(f"{scenario}-*-{sha}.json")):
    with path.open() as fh:
        row = json.load(fh)
    # The glob also matches <scenario>-q<rate>-<target>; a saturation table and
    # a fixed-rate table must never share rows.
    if row["scenario"] == scenario and row.get("rate") == rate:
        rows.append(row)

if not rows:
    print(f"no results for scenario={scenario} sha={sha} in {results_dir}")
    raise SystemExit(1)

def fmt(value, digits=2):
    return "-" if value is None else f"{value:,.{digits}f}"

header = f"{'target':<14}{'rps':>14}{'p50 ms':>10}{'p99 ms':>10}{'p999 ms':>10}{'cpu us/req':>12}{'cpu busy':>10}{'peak PSS MB':>13}{'success':>10}"
print(f"\nscenario: {scenario}   commit: {sha}   " + (f"fixed rate: {rate} rps" if rate else "saturation"))
print(header)
print("-" * len(header))

baseline = next((r for r in rows if r["target"] == "prx"), None)
for row in rows:
    lat = row["latency_ms"]
    res = row["resources"]
    print(
        f"{row['target']:<14}"
        f"{fmt(row['throughput']['requests_per_second']):>14}"
        f"{fmt(lat.get('p50')):>10}"
        f"{fmt(lat.get('p99')):>10}"
        f"{fmt(lat.get('p999')):>10}"
        f"{fmt(res.get('cpu_us_per_request'), 3):>12}"
        f"{('-' if res.get('cpu_utilisation') is None else format(res['cpu_utilisation'] * 100, '.0f') + '%'):>10}"
        f"{fmt((res.get('peak_rss_kb') or 0) / 1024):>13}"
        f"{row['throughput']['success_rate'] * 100:>9.2f}%"
    )

if baseline:
    print("\nprx relative to each reference (higher rps / lower p99 & RSS is better):")
    for row in rows:
        if row["target"] == "prx":
            continue
        rps_ref = row["throughput"]["requests_per_second"] or None
        p99_ref = row["latency_ms"].get("p99")
        rss_ref = row["resources"].get("peak_rss_kb") or None
        rps_prx = baseline["throughput"]["requests_per_second"]
        p99_prx = baseline["latency_ms"].get("p99")
        rss_prx = baseline["resources"].get("peak_rss_kb")
        def pct(prx, ref):
            if not prx or not ref:
                return "-"
            return f"{(prx / ref - 1) * 100:+.1f}%"
        print(
            f"  vs {row['target']:<8} rps {pct(rps_prx, rps_ref):>8}"
            f"   p99 {pct(p99_prx, p99_ref):>8}"
            f"   peak RSS {pct(rss_prx, rss_ref):>8}"
        )
print()
PY
