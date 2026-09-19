# Zero-Exception Policy

`prx` enforces a zero-exception security policy for RustSec **vulnerabilities**:

1. No ignored RustSec advisories in CI or release scripts
2. `cargo audit` must pass clean before release

## Current Status

As of September 18, 2026, `cargo audit` exits 0 with **zero vulnerabilities and
zero ignore flags**.

Three `warning`-class advisories remain. They are unsound/unmaintained notices
rather than exploitable vulnerabilities, plain `cargo audit` does not fail on
them, and every one of them comes from inside pingora rather than from prx:

| Crate | Advisory | Class | Comes from | Why it is not ignored away |
|---|---|---|---|---|
| `derivative` 2.2.0 | RUSTSEC-2024-0388 | unmaintained | `pingora-core` | No prx-side fix exists; upstream has to drop the dependency |
| `rand` 0.8.5 | RUSTSEC-2026-0097 | unsound | `pingora-core`, `pingora-cache` | Unsound only with a custom logger calling `rand::rng()`; prx installs none |
| `rand` 0.9.2 | RUSTSEC-2026-0097 | unsound | `pingora-core` | Same |

Review date: **2026-12-18**. If upstream has not moved by then, re-evaluate
whether to fork or to pin.

## How this was reached

Earlier this repository vendored patched copies of `pingora-core` and
`pingora-load-balancing` under `vendor/`, wired in through
`[patch.crates-io]`, to work around advisories in pingora 0.7's dependency
chain. That left `pingora-cache` 0.7.0 exposed to
[RUSTSEC-2026-0035](https://rustsec.org/advisories/RUSTSEC-2026-0035) (cache
poisoning, 8.4 high), which the advisory could only fix by upgrading.

pingora 0.9 carries the fix, so the vendored fork was deleted outright and prx
now builds against the published crates:

- `pingora` 0.7 → **0.9**, which brings `pingora-cache` to 0.9.0 and closes
  RUSTSEC-2026-0035
- the `initgroups` type mismatch patched locally during T001 is fixed upstream
  in 0.9, so there is nothing left to carry
- `pingora-core` 0.9 moved the Prometheus listener into its own crate, so prx
  depends on `pingora-prometheus` and calls
  `pingora_prometheus::prometheus_http_service()`
- `anyhow` bumped to 1.0.104, clearing RUSTSEC-2026-0190

**There is no `vendor/` directory any more, and no `[patch.crates-io]`
section.** A dependency problem is now fixed by upgrading or by reporting it
upstream, never by a local fork that nobody remembers to revisit.

## Enforcement

`scripts/release-gate.sh` and `.github/workflows/ci.yml` run plain `cargo audit`
with no ignore flags, so any new *vulnerability* fails the release path.
Warning-class advisories are reported but do not fail the build; they are
tracked in the table above instead.

## Maintenance

On every dependency bump:

1. `cargo update`
2. `cargo audit` — a non-zero exit is a release blocker, no exceptions
3. If a new warning appears, add it to the table above with the reason it is
   acceptable and a review date, or fix it
