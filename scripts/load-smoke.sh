#!/usr/bin/env bash
set -euo pipefail

TARGET_URL="${1:-http://127.0.0.1:8080/healthz}"
REQUESTS="${REQUESTS:-2000}"
CONCURRENCY="${CONCURRENCY:-32}"

if command -v oha >/dev/null 2>&1; then
  exec oha -n "${REQUESTS}" -c "${CONCURRENCY}" "${TARGET_URL}"
fi

if command -v hey >/dev/null 2>&1; then
  exec hey -n "${REQUESTS}" -c "${CONCURRENCY}" "${TARGET_URL}"
fi

echo "No load tool found (expected: oha or hey)."
echo "Install one of them, then run:"
echo "  REQUESTS=${REQUESTS} CONCURRENCY=${CONCURRENCY} scripts/load-smoke.sh ${TARGET_URL}"
exit 1
