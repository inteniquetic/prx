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
die() { echo "error: $*" >&2; exit 1; }
expected="$(cat "${CORPUS}/benign.txt" "${CORPUS}/attacks.txt" | wc -l | tr -d ' ')"

if [ "${1:?target or --diff required}" = "--diff" ]; then
  a="${RES}/verdicts-${2:?}.tsv" b="${RES}/verdicts-${3:?}.tsv"
  # join drops URLs that are in one file only, and an empty diff of an empty
  # join reads as agreement. Both files must cover the whole current corpus.
  for f in "${a}" "${b}"; do
    [ "$(wc -l < "${f}" | tr -d ' ')" = "${expected}" ] || die "${f} does not cover the ${expected}-URL corpus; rerun verdicts.sh for it"
  done
  joined="$(join -t $'\t' -j 2 <(sort -t $'\t' -k2 "${a}") <(sort -t $'\t' -k2 "${b}"))"
  [ "$(printf '%s\n' "${joined}" | wc -l | tr -d ' ')" = "${expected}" ] || die "the two files were taken from different corpora"
  # 403 = blocked, 200 = allowed; nothing else gets this far.
  printf '%s\n' "${joined}" | awk -F'\t' '$2 != $3'
  echo "compared ${expected} URLs" >&2
  exit 0
fi

# The file is named after the target, so make sure that target is what answers.
running="$(docker ps --filter publish=18080 --format '{{.Names}}')"
[ "${running}" = "bench-$1" ] || die "port 18080 is served by '${running:-nothing}', not bench-$1"

# Same headers the waf-* load scenarios send (scripts/bench.sh WAF_HEADERS):
# "benign is allowed" has to hold for the traffic that is actually benchmarked.
UA='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36'
mkdir -p "${RES}"
cat "${CORPUS}/benign.txt" "${CORPUS}/attacks.txt" | while read -r url; do
  code="$(curl -g -s -o /dev/null --max-time 5 -w '%{http_code}' -H 'Host: bench.local' -A "${UA}" \
    -H 'Accept: text/html,application/json;q=0.9,*/*;q=0.8' -H 'Accept-Language: en-US,en;q=0.9' \
    -H 'Cookie: sid=9f8a7c6b5d4e3f2a; theme=dark; lang=en' "${url}" || true)"
  printf '%s\t%s\n' "${code:-000}" "${url}"
done > "${RES}/verdicts-$1.tsv"
awk -F'\t' '{c[$1]++} END{for (k in c) print k, c[k]}' "${RES}/verdicts-$1.tsv" | sort
# A timeout or a 5xx is not a verdict. Counting it as "allowed" lets two broken
# targets agree with each other.
bad="$(awk -F'\t' '$1 != 200 && $1 != 403' "${RES}/verdicts-$1.tsv" | wc -l | tr -d ' ')"
[ "${bad}" = 0 ] || die "${bad} requests got neither 200 nor 403; verdicts-$1.tsv is not usable"
