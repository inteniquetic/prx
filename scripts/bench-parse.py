#!/usr/bin/env python3
"""Normalize load-generator output into the JSON result format used by T001/T004.

oha --json and h2load's text output carry the same facts in different shapes;
everything downstream (bench-compare.sh, perf-gate.sh, docs/BENCHMARKS.md)
reads only the normalized form produced here.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# A tick is 1/100s on every platform this harness supports (Linux, USER_HZ=100).
USER_HZ = 100.0


def parse_oha(text: str) -> dict:
    data = json.loads(text)
    summary = data.get("summary", {})
    latency = data.get("latencyPercentiles", {})
    total = summary.get("total") or 0.0
    # null when nothing completed (a WAF can take seconds per request).
    success = summary.get("successRate") or 0.0
    # oha reports successRate as a fraction in recent versions and as a
    # percentage in older ones; normalize to a fraction.
    if success > 1.0:
        success = success / 100.0
    # Count responses, not attempts: oha's requestsPerSec includes requests it
    # aborted at the deadline, which flatters a target that answered nothing.
    completed = sum((data.get("statusCodeDistribution") or {}).values())
    return {
        "requests_per_second": round(completed / total, 2) if total else 0.0,
        "requests_total": completed,
        "success_rate": success,
        "latency_ms": {
            "p50": _sec_to_ms(latency.get("p50")),
            "p90": _sec_to_ms(latency.get("p90")),
            "p99": _sec_to_ms(latency.get("p99")),
            "p999": _sec_to_ms(latency.get("p99.9")),
        },
        "status_codes": data.get("statusCodeDistribution", {}),
    }


def _sec_to_ms(value) -> float | None:
    return round(value * 1000.0, 3) if isinstance(value, (int, float)) else None


def parse_h2load(text: str) -> dict:
    rps = _search(r"finished in [^,]+, ([0-9.]+) req/s", text)
    succeeded = _search(r"requests: \d+ total, \d+ started, \d+ done, (\d+) succeeded", text)
    total = _search(r"requests: (\d+) total", text)
    mean, sd, minimum, maximum = None, None, None, None
    m = re.search(r"time for request:\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)", text)
    if m:
        minimum, maximum, mean, sd = (_duration_to_ms(g) for g in m.groups())
    return {
        "requests_per_second": rps or 0.0,
        "requests_total": int(total or 0),
        "success_rate": (succeeded / total) if (succeeded and total) else 0.0,
        # h2load does not print percentiles; mean/sd/min/max is what it gives.
        "latency_ms": {"mean": mean, "sd": sd, "min": minimum, "max": maximum},
        "status_codes": {},
    }


def _search(pattern: str, text: str) -> float | None:
    m = re.search(pattern, text)
    return float(m.group(1)) if m else None


def _duration_to_ms(token: str) -> float | None:
    m = re.match(r"([0-9.]+)(us|ms|s|m)$", token)
    if not m:
        return None
    value, unit = float(m.group(1)), m.group(2)
    return round(value * {"us": 0.001, "ms": 1.0, "s": 1000.0, "m": 60000.0}[unit], 3)


def main() -> int:
    ap = argparse.ArgumentParser()
    for flag in ("target", "scenario", "tool", "url", "sha", "timestamp"):
        ap.add_argument(f"--{flag}", required=True)
    ap.add_argument("--connections", type=int, required=True)
    ap.add_argument("--duration", type=int, required=True)
    ap.add_argument("--load-output", type=Path, required=True)
    ap.add_argument("--stats", type=Path, required=True)
    args = ap.parse_args()

    raw = args.load_output.read_text(errors="replace")
    try:
        load = parse_oha(raw) if args.tool == "oha" else parse_h2load(raw)
    except (json.JSONDecodeError, ValueError) as exc:
        print(f"failed to parse {args.tool} output: {exc}", file=sys.stderr)
        print(raw[:2000], file=sys.stderr)
        return 1

    stats = {}
    if args.stats.exists():
        try:
            stats = json.loads(args.stats.read_text())
        except json.JSONDecodeError:
            stats = {}

    cpu_ticks = float(stats.get("cpu_ticks", 0) or 0)
    requests = load.get("requests_total") or 0
    cpu_seconds = cpu_ticks / USER_HZ
    result = {
        "schema": 1,
        "timestamp": args.timestamp,
        "git_sha": args.sha,
        "target": args.target,
        "scenario": args.scenario,
        "tool": args.tool,
        "url": args.url,
        "connections": args.connections,
        "duration_s": args.duration,
        "throughput": {
            "requests_per_second": round(load["requests_per_second"], 2),
            "requests_total": requests,
            "success_rate": round(load["success_rate"], 6),
        },
        "latency_ms": load["latency_ms"],
        "resources": {
            "peak_rss_kb": int(stats.get("peak_rss_kb", 0) or 0),
            "cpu_seconds": round(cpu_seconds, 3),
            "cpu_us_per_request": round(cpu_seconds * 1e6 / requests, 3) if requests else None,
        },
        "status_codes": load["status_codes"],
    }
    json.dump(result, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
