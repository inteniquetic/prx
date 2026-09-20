//! The route's rate limit, as a plugin (T502).
//!
//! This is the built-in that made `PluginResponse` grow headers: a limit that
//! cannot say `Retry-After` is a limit the client has to guess at.

use async_trait::async_trait;
use pingora::http::RequestHeader;
use pingora::prelude::Result;
use std::sync::Arc;

use crate::config::RateLimitKey;
use crate::limiter::Decision;
use crate::metrics;
use crate::plugin::{
    LogInfo, NeedsMask, PhaseMask, Plugin, PluginCtx, PluginDecision, PluginResponse,
};
use crate::runtime::{RouteRateLimit, hash_key};

#[derive(Debug)]
pub struct RateLimit {
    limit: Arc<RouteRateLimit>,
    needs: NeedsMask,
}

impl RateLimit {
    pub fn new(limit: Arc<RouteRateLimit>) -> Self {
        // Only the client_ip key looks at the connection, and it buckets by
        // address without rendering one — so it asks for the address, not the
        // string (T502).
        let needs = match limit.key {
            RateLimitKey::ClientIp => NeedsMask::PEER_ADDR,
            RateLimitKey::Route | RateLimitKey::Header(_) => NeedsMask::NONE,
        };
        Self { limit, needs }
    }

    /// The bucket a request falls in. Nothing is allocated for the common
    /// `client_ip` and `route` cases.
    fn bucket(&self, head: &RequestHeader, ctx: &PluginCtx<'_>) -> u64 {
        match &self.limit.key {
            RateLimitKey::Route => hash_key(&["route"]) ^ ctx.facts.route_idx as u64,
            RateLimitKey::ClientIp => match ctx.facts.peer_addr {
                Some(addr) => match addr.as_inet() {
                    // Bucket per address, ignoring the source port: a client
                    // opening new connections must not get a fresh allowance
                    // each time.
                    Some(inet) => {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        std::hash::Hash::hash(&inet.ip(), &mut hasher);
                        std::hash::Hasher::finish(&hasher)
                    }
                    None => hash_key(&[addr.to_string().as_str()]),
                },
                None => 0,
            },
            RateLimitKey::Header(name) => head
                .headers
                .get(name.as_str())
                .and_then(|value| value.to_str().ok())
                .map(|value| hash_key(&[value]))
                .unwrap_or(0),
        }
    }
}

#[async_trait]
impl Plugin for RateLimit {
    fn name(&self) -> &str {
        "rate-limit"
    }

    fn kind(&self) -> &'static str {
        "rate-limit"
    }

    fn phases(&self) -> PhaseMask {
        PhaseMask::REQUEST_HEAD.union(PhaseMask::LOG)
    }

    fn needs(&self) -> NeedsMask {
        self.needs
    }

    async fn on_request_head(
        &self,
        head: &mut RequestHeader,
        ctx: &mut PluginCtx<'_>,
    ) -> Result<PluginDecision> {
        let key = self.bucket(head, ctx);
        match self.limit.limiter.check(key) {
            Decision::Allow => Ok(PluginDecision::Continue),
            Decision::Deny { retry_after_s } => {
                metrics::inc_rate_limited(ctx.route_name, "rate");
                let mut response = PluginResponse::new(self.limit.response_status);
                if self.limit.retry_after {
                    response = response.with_header("retry-after", retry_after_s.to_string());
                }
                Ok(PluginDecision::Respond(response))
            }
        }
    }

    fn on_log(&self, info: &LogInfo<'_>, _ctx: &mut PluginCtx<'_>) {
        metrics::set_limiter_entries(info.route_name, self.limit.limiter.entries());
    }
}
