//! Plugins that ship with prx.
//!
//! Only the test plugin for now. T502 moves the five that currently live
//! inside `request_filter` here, which is what proves the API in [`super`] can
//! carry real work before anything outside the repo depends on it.

pub mod echo_header;
