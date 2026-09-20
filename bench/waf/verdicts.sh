#!/usr/bin/env bash
# Block/allow verdict per corpus URL, so speed is only compared between engines
# that decide the same thing. Status codes only: rule-ID conformance is go-ftw's
# job (T507), not this script's.
#
# usage: verdicts.sh <target>    start the target first (KEEP_UP=1 or compose up);
#                                writes bench/results/verdicts-<target>.tsv (status<TAB>url)
#        verdicts.sh --diff a b  URLs where targets a and b disagree on block/allow
set -euo pipefail
export LC_ALL=C   # sort and join must agree on collation
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RES="${ROOT}/bench/results"
CORPUS="${ROOT}/bench/waf/corpus"

if [ "${1:?target or --diff required}" = "--diff" ]; then
  # 403 = blocked, anything else = allowed. Compare the decision, not the code.
  join -t $'\t' -j 2 \
    <(sort -t $'\t' -k2 "${RES}/verdicts-${2:?}.tsv") \
    <(sort -t $'\t' -k2 "${RES}/verdicts-${3:?}.tsv") \
    | awk -F'\t' '(($2==403)!=($3==403))'
  exit 0
fi

UA='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36'
mkdir -p "${RES}"
cat "${CORPUS}/benign.txt" "${CORPUS}/attacks.txt" | while read -r url; do
  code="$(curl -g -s -o /dev/null --max-time 5 -w '%{http_code}' -H 'Host: bench.local' -A "${UA}" "${url}" || true)"
  printf '%s\t%s\n' "${code:-000}" "${url}"
done > "${RES}/verdicts-$1.tsv"
awk -F'\t' '{c[$1]++} END{for (k in c) print k, c[k]}' "${RES}/verdicts-$1.tsv" | sort
