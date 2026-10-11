#!/usr/bin/env python3
"""Median and spread across h1-keepalive reference runs.

Usage: python3 scripts/bench-stability.py <dir-with-run-JSON> [--glob 'stab-run-*.json']

Spread is (max - min) / median, in percent. A run is valid only if its
peak_rss_kb > 0 (a 0 means PSS sampling failed, which the harness now refuses
to write, so this is a second check). Exits 1 if fewer than 2 valid runs or if
rps spread > 3%.
"""
import argparse
import glob
import json
import os
import statistics
import sys


def spread_pct(values):
    med = statistics.median(values)
    return (max(values) - min(values)) / med * 100 if med else float("inf")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("dir")
    ap.add_argument("--glob", default="stab-run-*.json")
    ap.add_argument("--rps-limit", type=float, default=3.0)
    args = ap.parse_args()

    files = sorted(glob.glob(os.path.join(args.dir, args.glob)))
    runs = []
    for path in files:
        d = json.load(open(path))
        runs.append({
            "file": os.path.basename(path),
            "rps": d["throughput"]["requests_per_second"],
            "p99": d["latency_ms"]["p99"],
            "rss": d["resources"]["peak_rss_kb"],
            "cpu_us": d["resources"]["cpu_us_per_request"],
        })

    print(f"{'run':<22}{'rps':>12}{'p99 ms':>10}{'peak_rss_kb':>14}{'cpu_us/req':>12}")
    for r in runs:
        print(f"{r['file']:<22}{r['rps']:>12,.1f}{r['p99']:>10.3f}{r['rss']:>14,}{r['cpu_us']:>12.2f}")

    valid = [r for r in runs if r["rss"] > 0]
    if len(valid) < len(runs):
        print(f"\nWARNING: {len(runs) - len(valid)} run(s) have peak_rss_kb == 0 and are invalid")
    if len(valid) < 2:
        print("\nNO VALID BASELINE: fewer than 2 valid runs")
        return 1

    metrics = {"rps": "rps", "p99": "p99", "peak_rss_kb": "rss", "cpu_us_per_req": "cpu_us"}
    print(f"\n{'metric':<16}{'median':>14}{'min':>14}{'max':>14}{'spread %':>12}")
    stats = {}
    for name, key in metrics.items():
        vals = [r[key] for r in valid]
        med = statistics.median(vals)
        sp = spread_pct(vals)
        stats[name] = (med, sp)
        print(f"{name:<16}{med:>14,.3f}{min(vals):>14,.3f}{max(vals):>14,.3f}{sp:>11.2f}%")

    rps_sp = stats["rps"][1]
    ok = rps_sp <= args.rps_limit
    print(f"\nrps spread {rps_sp:.2f}% vs limit {args.rps_limit}%: {'STABLE' if ok else 'NOT STABLE'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
