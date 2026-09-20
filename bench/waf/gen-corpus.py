#!/usr/bin/env python3
"""Seeded WAF corpus: URL lists for `oha --urls-from-file`, and POST bodies.

usage: gen-corpus.py <crs-dir> <out-dir> <host:port>
   eg: bench/waf/gen-corpus.py bench/waf/crs bench/waf/corpus 127.0.0.1:18080

Needs PyYAML (pip install pyyaml) and a CRS checkout (bench/waf/fetch-crs.sh).
"""
import json
import random
import re
import sys
from pathlib import Path
from urllib.parse import urlencode

import yaml

crs, out, host = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
out.mkdir(parents=True, exist_ok=True)
rng = random.Random(8508)  # fixed: every target and every rerun sees the same traffic
WORDS = "alpha bravo charlie delta echo foxtrot golf hotel india juliet".split()

benign = [
    f"http://{host}/1k?"
    + urlencode(
        {
            "id": rng.randint(1, 10**6),
            "q": " ".join(rng.sample(WORDS, 2)),
            "page": rng.randint(1, 50),
            "sort": rng.choice(["name", "date", "price"]),
        }
    )
    for _ in range(1000)
]

# Paranoia level of every rule, read from the rule files themselves.
level = {}
for f in crs.glob("rules/*.conf"):
    for block in re.split(r"\n(?=SecRule|SecAction)", f.read_text()):
        rule, pl = re.search(r"id:(\d+)", block), re.search(r"paranoia-level/(\d)", block)
        if rule and pl:
            level[int(rule.group(1))] = int(pl.group(1))

# GET-only, query-string requests from CRS's own regression suite. oha cannot vary
# method or body per request, so body-borne attacks are not under load here.
#   attacks: every such request, including negative tests and PL2+ cases that PL1
#            lets through. For verdicts.sh, where an "allow" is as telling as a block.
#   blocked: the ones CRS itself expects a PL1 rule to fire on (~98 % are blocked).
#            For load, where "attack traffic" has to mean traffic that gets blocked.
attacks, blocked = set(), set()
for f in sorted(crs.glob("tests/regression/tests/**/*.yaml")):
    for test in (yaml.safe_load(f.read_text()) or {}).get("tests", []):
        for stage in test.get("stages", []):
            i = stage.get("input") or stage.get("stage", {}).get("input", {})
            uri = i.get("uri") or ""
            if (
                i.get("method", "GET") == "GET"
                and not i.get("data")
                and not i.get("encoded_request")
                and "?" in uri
                and uri.startswith("/")
                and uri.isascii()
                and uri.isprintable()
                and " " not in uri
                and "#" not in uri  # curl and oha drop everything after a fragment
            ):
                attacks.add(f"http://{host}{uri}")
                ids = ((stage.get("output") or {}).get("log") or {}).get("expect_ids") or []
                if ids and all(level.get(r) == 1 for r in ids):
                    blocked.add(f"http://{host}{uri}")
attacks, blocked = sorted(attacks), sorted(blocked)

mix = benign * 9 + [rng.choice(blocked) for _ in range(len(benign))]
rng.shuffle(mix)

(out / "benign.txt").write_text("\n".join(benign) + "\n")
(out / "attacks.txt").write_text("\n".join(attacks) + "\n")
(out / "blocked.txt").write_text("\n".join(blocked) + "\n")
(out / "mix.txt").write_text("\n".join(mix) + "\n")
(out / "form.txt").write_text(
    urlencode({f"field{n}": " ".join(rng.choices(WORDS, k=12)) for n in range(20)})
)
for name, size in (("json-16k", 16 << 10), ("json-128k", 128 << 10)):
    items, length = [], 2
    while length < size - 200:
        item = {
            "id": rng.randint(1, 10**6),
            "name": " ".join(rng.choices(WORDS, k=3)),
            "tags": rng.sample(WORDS, 3),
        }
        items.append(item)
        length += len(json.dumps(item)) + 2
    (out / f"{name}.json").write_text(json.dumps(items))
print(f"benign={len(benign)} attacks={len(attacks)} blocked={len(blocked)} mix={len(mix)}")
