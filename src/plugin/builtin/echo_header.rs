//! The plugin that exists so the plugin system can be tested (T501).
//!
//! It is deliberately dull: it appends a value to a request header, to a
//! response header, or both, and can be told to answer the request itself. That
//! is enough to check the three things the core promises — that plugins run in
//! the order the config lists them, that a plugin can stop the chain, and that
//! both the request and response phases are reached — without waiting for the
//! WAF to exist.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use bytes::Bytes;
use pingora::http::{RequestHeader, ResponseHeader};
use pingora::prelude::Result as PingoraResult;

use crate::plugin::{PhaseMask, Plugin, PluginCtx, PluginDecision};

#[derive(Debug)]
pub struct EchoHeader {
    name: String,
    /// Header appended to on the way in, and on the way to the upstream.
    request_header: Option<String>,
    /// Header appended to on the way back.
    response_header: Option<String>,
    value: String,
    /// When set, this plugin answers the request instead of letting it through.
    respond_status: Option<u16>,
    respond_body: String,
    phases: PhaseMask,
}

impl EchoHeader {
    pub fn from_config(name: &str, settings: &toml::Value) -> Result<Self> {
        if let Some(problem) = Self::check(settings) {
            bail!(problem);
        }

        let get = |key: &str| -> Option<String> {
            settings
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::to_string)
        };

        let request_header = get("request_header");
        let response_header = get("response_header");
        let value = get("value").context("`value` is required")?;
        let respond_status = settings
            .get("respond_status")
            .and_then(toml::Value::as_integer)
            .map(|status| status as u16);

        let mut phases = PhaseMask::NONE;
        // Responding has to happen before the upstream is chosen, so a plugin
        // that answers the request is in the request phase whether or not it
        // writes a header.
        if request_header.is_some() || respond_status.is_some() {
            phases = phases
                .union(PhaseMask::REQUEST_HEAD)
                .union(PhaseMask::UPSTREAM_REQUEST);
        }
        if response_header.is_some() {
            phases = phases.union(PhaseMask::RESPONSE_HEAD);
        }

        Ok(Self {
            name: name.to_string(),
            request_header,
            response_header,
            value,
            respond_status,
            respond_body: get("respond_body").unwrap_or_default(),
            phases,
        })
    }

    /// The settings check, shared with `PrxConfig::check` so a bad block is
    /// reported in the editor rather than at boot.
    pub fn check(settings: &toml::Value) -> Option<String> {
        if settings
            .get("value")
            .and_then(toml::Value::as_str)
            .is_none()
        {
            return Some("`value` is required and must be a string".to_string());
        }
        if settings.get("request_header").is_none()
            && settings.get("response_header").is_none()
            && settings.get("respond_status").is_none()
        {
            return Some(
                "set at least one of `request_header`, `response_header` or `respond_status`, \
                 or this plugin does nothing"
                    .to_string(),
            );
        }
        if let Some(status) = settings.get("respond_status") {
            match status.as_integer() {
                Some(code) if (100..=599).contains(&code) => {}
                _ => {
                    return Some(
                        "`respond_status` must be an HTTP status between 100 and 599".to_string(),
                    );
                }
            }
        }
        None
    }

    /// Appends to the header rather than replacing it, so two of these in a
    /// chain leave evidence of the order they ran in.
    fn append(header: &mut RequestHeader, name: &str, value: &str) {
        let existing = header
            .headers
            .get(name)
            .and_then(|current| current.to_str().ok())
            .map(str::to_string);
        let next = match existing {
            Some(current) => format!("{current},{value}"),
            None => value.to_string(),
        };
        let _ = header.insert_header(name.to_string(), next);
    }
}

#[async_trait]
impl Plugin for EchoHeader {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &'static str {
        "echo-header"
    }

    fn phases(&self) -> PhaseMask {
        self.phases
    }

    async fn on_request_head(
        &self,
        head: &mut RequestHeader,
        _ctx: &mut PluginCtx<'_>,
    ) -> PingoraResult<PluginDecision> {
        if let Some(name) = &self.request_header {
            Self::append(head, name, &self.value);
        }
        match self.respond_status {
            Some(status) => Ok(PluginDecision::Respond {
                status,
                body: Bytes::from(self.respond_body.clone()),
            }),
            None => Ok(PluginDecision::Continue),
        }
    }

    async fn on_upstream_request(
        &self,
        head: &mut RequestHeader,
        _ctx: &mut PluginCtx<'_>,
    ) -> PingoraResult<()> {
        // The header rules in `upstream_request_filter` rebuild the upstream
        // request from the route's config, so a mark added in the request phase
        // is not automatically there. Adding it again is what makes the
        // upstream phase observable in a test.
        if let Some(name) = &self.request_header {
            Self::append(head, name, &self.value);
        }
        Ok(())
    }

    async fn on_response_head(
        &self,
        head: &mut ResponseHeader,
        _ctx: &mut PluginCtx<'_>,
    ) -> PingoraResult<PluginDecision> {
        if let Some(name) = &self.response_header {
            let existing = head
                .headers
                .get(name)
                .and_then(|current| current.to_str().ok())
                .map(str::to_string);
            let next = match existing {
                Some(current) => format!("{current},{}", self.value),
                None => self.value.clone(),
            };
            let _ = head.insert_header(name.to_string(), next);
        }
        Ok(PluginDecision::Continue)
    }
}
