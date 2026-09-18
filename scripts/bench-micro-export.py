#!/usr/bin/env python3
"""Export criterion results into one JSON file for baselines and CI diffing.

criterion writes per-benchmark directories under target/criterion; this reduces
them to {benchmark: {median_ns, mean_ns, ...}} so a baseline can be committed
and compared later by scripts/perf-gate.sh.
"""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


def collect(criterion_dir: Path) -> dict:
    out: dict[str, dict] = {}
    for estimates in sorted(criterion_dir.glob("**/new/estimates.json")):
        # .../target/criterion/<group>/<bench>/new/estimates.json
        name = "/".join(estimates.relative_to(criterion_dir).parts[:-2])
        try:
            data = json.loads(estimates.read_text())
        except json.JSONDecodeError:
            continue
        out[name] = {
            "median_ns": round(data["median"]["point_estimate"], 3),
            "mean_ns": round(data["mean"]["point_estimate"], 3),
            "std_dev_ns": round(data["std_dev"]["point_estimate"], 3),
        }
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--criterion-dir", type=Path, default=Path("target/criterion"))
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    if not args.criterion_dir.exists():
        raise SystemExit(f"{args.criterion_dir} not found; run `make bench-micro` first")

    sha = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True
    ).stdout.strip() or "unknown"

    payload = {"schema": 1, "git_sha": sha, "benchmarks": collect(args.criterion_dir)}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"wrote {args.out} ({len(payload['benchmarks'])} benchmarks)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
