//! Header rules, as plugins (T502).
//!
//! Split in two — one for each direction — because a chain is a single ordered
//! list and the two directions want different places in it. Request rules run
//! before anything a route's own plugins do to the upstream request; response
//! rules run after the cache has decided what to store, because that is the
//! order the hand-written version had and storing a response with the route's
//! own headers already on it would change what a later cache hit serves.
//!
//! This is the first thing the migration turned up that the API did not
//! anticipate: a plugin is one position in one list, and "runs first on the way
//! out, last on the way back" is two positions.

use async_trait::async_trait;
use pingora::http::{RequestHeader, ResponseHeader};
use pingora::prelude::Result;
use std::sync::Arc;

use crate::headers::{CompiledHeaderRules, HeaderContext};
use crate::plugin::{NeedsMask, PhaseMask, Plugin, PluginCtx, PluginDecision};

/// What both directions share: the rules, and what they say they need.
fn declared_needs(rules: &CompiledHeaderRules) -> NeedsMask {
    let mut needs = NeedsMask::NONE;
    if rules.needs_client_addr() {
        needs = needs.union(NeedsMask::CLIENT_ADDR);
    }
    if rules.needs_request_id() {
        needs = needs.union(NeedsMask::REQUEST_ID);
    }
    needs
}

fn header_context<'a>(ctx: &'a PluginCtx<'a>) -> HeaderContext<'a> {
    HeaderContext {
        client_ip: ctx.facts.client_ip,
        client_port: ctx.facts.client_port,
        scheme: ctx.facts.scheme,
        host: ctx.facts.host,
        route_name: Some(ctx.route_name),
        upstream_addr: ctx.facts.upstream_addr,
        request_id: ctx.facts.request_id,
    }
}

/// Rules applied to the request on its way to the upstream.
#[derive(Debug)]
pub struct RequestHeaders {
    rules: Arc<CompiledHeaderRules>,
    needs: NeedsMask,
}

impl RequestHeaders {
    pub fn new(rules: Arc<CompiledHeaderRules>) -> Self {
        let needs = declared_needs(&rules);
        Self { rules, needs }
    }
}

#[async_trait]
impl Plugin for RequestHeaders {
    fn name(&self) -> &str {
        "request-headers"
    }

    fn kind(&self) -> &'static str {
        "request-headers"
    }

    fn phases(&self) -> PhaseMask {
        PhaseMask::UPSTREAM_REQUEST
    }

    fn needs(&self) -> NeedsMask {
        self.needs
    }

    async fn on_upstream_request(
        &self,
        head: &mut RequestHeader,
        ctx: &mut PluginCtx<'_>,
    ) -> Result<()> {
        let header_ctx = header_context(ctx);
        self.rules.apply(head, &header_ctx);
        Ok(())
    }
}

/// Rules applied to the response on its way back to the client.
#[derive(Debug)]
pub struct ResponseHeaders {
    rules: Arc<CompiledHeaderRules>,
    needs: NeedsMask,
}

impl ResponseHeaders {
    pub fn new(rules: Arc<CompiledHeaderRules>) -> Self {
        let needs = declared_needs(&rules);
        Self { rules, needs }
    }
}

#[async_trait]
impl Plugin for ResponseHeaders {
    fn name(&self) -> &str {
        "response-headers"
    }

    fn kind(&self) -> &'static str {
        "response-headers"
    }

    fn phases(&self) -> PhaseMask {
        PhaseMask::RESPONSE_HEAD
    }

    fn needs(&self) -> NeedsMask {
        self.needs
    }

    async fn on_response_head(
        &self,
        head: &mut ResponseHeader,
        ctx: &mut PluginCtx<'_>,
    ) -> Result<PluginDecision> {
        let header_ctx = header_context(ctx);
        self.rules.apply(head, &header_ctx);
        Ok(PluginDecision::Continue)
    }
}
