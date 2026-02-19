#!/usr/bin/env bash
set -euo pipefail

echo "[1/4] cargo fmt -- --check"
cargo fmt -- --check

echo "[2/4] cargo clippy --all-targets -- -D warnings"
cargo clippy --all-targets -- -D warnings

echo "[3/4] cargo test --all-targets -- --test-threads=1"
cargo test --all-targets -- --test-threads=1

echo "[4/4] cargo audit"
if ! command -v cargo-audit >/dev/null 2>&1; then
  cargo install cargo-audit --locked
fi
cargo audit

echo "release gate passed"
