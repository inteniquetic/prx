# Zero-Exception Policy

`prx` enforces a zero-exception security policy:

1. No ignored RustSec advisories in CI or release scripts
2. `cargo audit` must pass clean before release

## Current Status

As of February 12, 2026, the release gate passes with zero RustSec exceptions.

## Remediation Implemented

To satisfy zero-exception without waiting on upstream Pingora changes, this repository vendors patched crates:

1. `vendor/pingora-core`
2. `vendor/pingora-load-balancing`

Applied changes:

1. Upgraded `prometheus` dependency chain to `0.14.x` (`protobuf >= 3.7.2`)
2. Removed `daemonize` dependency and replaced daemonization with in-tree double-fork logic
3. Removed `derivative` dependency and replaced macro-based derives with explicit trait impls

These patches are wired through `[patch.crates-io]` in `Cargo.toml`.

## Enforcement

`scripts/release-gate.sh` and `.github/workflows/ci.yml` run plain `cargo audit` with no ignore flags.
Any advisory or unmaintained-crate warning fails the release path.

## Maintenance

When upstream Pingora ships official fixes, remove the local vendored patches and revalidate:

1. `cargo update`
2. `scripts/release-gate.sh`
