#!/usr/bin/env bash
# CRS pinned to the release T504 measured. Never track main.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/crs"
TAG=v4.21.0 COMMIT=2ac6c00
rm -rf "${DIR}"
git -c advice.detachedHead=false clone --quiet --depth 1 --branch "${TAG}" https://github.com/coreruleset/coreruleset "${DIR}"
got="$(git -C "${DIR}" rev-parse --short=7 HEAD)"
[ "${got}" = "${COMMIT}" ] || { echo "CRS ${TAG} is ${got}, expected ${COMMIT}" >&2; exit 1; }
cp "${DIR}/crs-setup.conf.example" "${DIR}/crs-setup.conf"   # defaults: PL1, inbound threshold 5
echo "CRS ${TAG} (${got}) -> ${DIR}"
