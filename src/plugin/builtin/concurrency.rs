//! The route's concurrency limit, as a plugin (T502).
//!
//! The one that proves a plugin can hold something it has to give back: a slot
//! taken in the request phase is released in the log phase, which runs for
//! every request however it ended — including the ones another plugin answered.

use async_trait::async_trait;
use pingora::http::RequestHeader;
use pingora::prelude::Result;
use std::sync::Arc;

use crate::config::ConcurrencyLimitConfig;
use crate::limiter::ConcurrencyLimiter;
use crate::metrics;
use crate::plugin::{LogInfo, PhaseMask, Plugin, PluginCtx, PluginDecision, PluginResponse};

/// Whether this request is holding a slot. Kept in plugin state rather than in
/// the request context, so the accounting lives with the thing that does it.
struct Held;

#[derive(Debug)]
pub struct Concurrency {
    limiter: Arc<ConcurrencyLimiter>,
    config: ConcurrencyLimitConfig,
}

impl Concurrency {
    pub fn new(limiter: Arc<ConcurrencyLimiter>, config: ConcurrencyLimitConfig) -> Self {
        Self { limiter, config }
    }
}

#[async_trait]
impl Plugin for Concurrency {
    fn name(&self) -> &str {
        "concurrency"
    }

    fn kind(&self) -> &'static str {
        "concurrency"
    }

    fn phases(&self) -> PhaseMask {
        PhaseMask::REQUEST_HEAD.union(PhaseMask::LOG)
    }

    fn uses_state(&self) -> bool {
        true
    }

    async fn on_request_head(
        &self,
        _head: &mut RequestHeader,
        ctx: &mut PluginCtx<'_>,
    ) -> Result<PluginDecision> {
        if !self.limiter.try_acquire(self.config.max_concurrent) {
            metrics::inc_rate_limited(ctx.route_name, "concurrency");
            return Ok(PluginDecision::Respond(PluginResponse::new(
                self.config.response_status,
            )));
        }
        if self.config.max_concurrent > 0 {
            *ctx.state = Some(Box::new(Held));
        }
        Ok(PluginDecision::Continue)
    }

    fn on_log(&self, _info: &LogInfo<'_>, ctx: &mut PluginCtx<'_>) {
        // Taking the marker out is what makes the release happen exactly once,
        // even if the log phase were ever to run twice.
        if ctx.state.take().is_some() {
            self.limiter.release();
        }
    }
}
