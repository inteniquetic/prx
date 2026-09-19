//! Turning a `kind` in the config into something that can run (T501).
//!
//! The registry is a `match`, not a lazily-populated map: every kind prx can
//! build is known at compile time, so an unknown one is a config error with a
//! line number rather than a surprise at boot. That is also what lets
//! `validate.rs` report it in the editor before anyone applies the file.

use std::sync::Arc;

use anyhow::{Context, Result, bail};

use super::Plugin;
use super::builtin::echo_header::EchoHeader;
use crate::config::PluginConfig;

/// Every `kind` this build understands, in the order they are listed to the
/// user when they get one wrong.
pub const KINDS: &[&str] = &["echo-header"];

pub fn known_kinds() -> &'static [&'static str] {
    KINDS
}

/// Builds the plugin a `[[plugin]]` block describes.
pub fn build_plugin(config: &PluginConfig) -> Result<Arc<dyn Plugin>> {
    match config.kind.as_str() {
        "echo-header" => Ok(Arc::new(
            EchoHeader::from_config(&config.name, &config.config)
                .with_context(|| format!("plugin '{}'", config.name))?,
        )),
        other => bail!(
            "unknown plugin kind '{other}' (known kinds: {})",
            KINDS.join(", ")
        ),
    }
}

/// Checks a plugin's settings without building it, for `PrxConfig::check`.
///
/// Returns the message to put in the `ConfigIssue`, or `None` when the block
/// is fine. Kept separate from [`build_plugin`] so validating a draft never
/// allocates the runtime objects a plugin might hold.
pub fn check_plugin_config(kind: &str, settings: &toml::Value) -> Option<String> {
    match kind {
        "echo-header" => EchoHeader::check(settings),
        other => Some(format!(
            "unknown plugin kind '{other}' (known kinds: {})",
            KINDS.join(", ")
        )),
    }
}
