//! prx — a high-performance reverse proxy built on Pingora.
//!
//! The binary in `src/main.rs` is a thin wrapper around this library so that
//! integration tests and benchmarks (`benches/`) can exercise the routing and
//! configuration internals directly.

pub mod admin;
pub mod config;
pub mod metrics;
pub mod proxy;
pub mod reload;
pub mod runtime;
