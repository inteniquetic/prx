//! Plugins that ship with prx.
//!
//! Four of the five that used to live inside `request_filter` are here (T502),
//! which is what proved the API in [`super`] can carry real work before
//! anything outside the repo depends on it. The fifth, response compression,
//! is not here on purpose — see `docs/tasks/T502-builtins-as-plugins.md`.
//!
//! These are built from a route's own config rather than from a `[[plugin]]`
//! block, so a config written before plugins existed keeps working untouched.

pub mod cache;
pub mod concurrency;
pub mod echo_header;
pub mod headers;
pub mod rate_limit;
