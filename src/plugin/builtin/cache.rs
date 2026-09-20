//! The route's micro-cache, as a plugin (T502).
//!
//! The hardest of the five, and the one that decided the shape of the API. It
//! is in four phases at once: it answers from the request phase, decides
//! storability from the response phase, collects the body as it streams, and
//! has to wake the requests waiting behind it from the log phase whether the
//! fetch worked, failed, or turned out not to be cacheable at all.
//!
//! It is also why `PluginResponse` carries headers: a cache hit is not a status
//! and a body, it is the response that was stored.

use async_trait::async_trait;
use bytes::Bytes;
use http::Method;
use pingora::http::{RequestHeader, ResponseHeader};
use pingora::prelude::Result;
use std::sync::Arc;

use crate::cache::{Lookup, is_cacheable_response, storable_headers};
use crate::metrics;
use crate::plugin::{LogInfo, PhaseMask, Plugin, PluginCtx, PluginDecision, PluginResponse};
use crate::runtime::RouteCache;

/// What this request is doing with the cache, for the phases that come later.
#[derive(Default)]
struct CacheState {
    key: Option<u64>,
    /// This request owns the upstream fetch for `key` and has to wake the
    /// requests waiting behind it.
    leader: bool,
    /// The response being assembled, when it is worth storing.
    pending: Option<Pending>,
}

struct Pending {
    status: u16,
    headers: Vec<(http::HeaderName, http::HeaderValue)>,
    body: bytes::BytesMut,
    too_large: bool,
}

#[derive(Debug)]
pub struct Cache {
    cache: Arc<RouteCache>,
}

impl Cache {
    pub fn new(cache: Arc<RouteCache>) -> Self {
        Self { cache }
    }

    /// Whether a request may be answered from the cache at all.
    fn is_cacheable_request(head: &RequestHeader) -> bool {
        if !matches!(head.method, Method::GET | Method::HEAD) {
            return false;
        }
        // An authenticated response belongs to one client.
        if head.headers.contains_key("authorization") {
            return false;
        }
        head.headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                let value = value.to_ascii_lowercase();
                !value.contains("no-store") && !value.contains("no-cache")
            })
            .unwrap_or(true)
    }

    /// The key: route, host, path, optionally the query, and the headers the
    /// route varies on.
    fn key(&self, head: &RequestHeader, route_idx: usize) -> u64 {
        use std::hash::{Hash, Hasher};

        let host = head
            .headers
            .get("host")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        route_idx.hash(&mut hasher);
        host.hash(&mut hasher);
        head.uri.path().hash(&mut hasher);
        head.uri.query().unwrap_or("").hash(&mut hasher);
        head.method.as_str().hash(&mut hasher);
        for name in &self.cache.vary_headers {
            name.hash(&mut hasher);
            head.headers
                .get(name.as_str())
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .hash(&mut hasher);
        }
        hasher.finish()
    }

    fn state<'a>(ctx: &'a mut PluginCtx<'_>) -> &'a mut CacheState {
        ctx.state
            .get_or_insert_with(|| Box::new(CacheState::default()))
            .downcast_mut::<CacheState>()
            .expect("the cache plugin owns its own slot")
    }
}

#[async_trait]
impl Plugin for Cache {
    fn name(&self) -> &str {
        "cache"
    }

    fn kind(&self) -> &'static str {
        "cache"
    }

    fn phases(&self) -> PhaseMask {
        PhaseMask::REQUEST_HEAD
            .union(PhaseMask::RESPONSE_HEAD)
            .union(PhaseMask::RESPONSE_BODY)
            .union(PhaseMask::LOG)
    }

    fn uses_state(&self) -> bool {
        true
    }

    async fn on_request_head(
        &self,
        head: &mut RequestHeader,
        ctx: &mut PluginCtx<'_>,
    ) -> Result<PluginDecision> {
        if !Self::is_cacheable_request(head) {
            return Ok(PluginDecision::Continue);
        }

        let key = self.key(head, ctx.facts.route_idx);
        let route_name = ctx.route_name.to_string();
        let lookup = self.cache.store.lookup(key).await;
        let state = Self::state(ctx);
        state.key = Some(key);

        match lookup {
            Lookup::Hit(entry) => {
                metrics::inc_cache(&route_name, "hit");
                let mut response = PluginResponse {
                    status: entry.status,
                    headers: entry.headers.clone(),
                    body: entry.body.clone(),
                };
                if self.cache.config.add_status_header {
                    response = response
                        .with_header("x-cache", "HIT")
                        .with_header("age", (entry.age_ms(now_epoch_ms()) / 1000).to_string());
                }
                Ok(PluginDecision::Respond(response))
            }
            Lookup::MissLeader => {
                metrics::inc_cache(&route_name, "miss");
                state.leader = true;
                Ok(PluginDecision::Continue)
            }
            Lookup::MissFollower => {
                // The leader did not finish in time; this request fetches on
                // its own rather than wait.
                metrics::inc_cache(&route_name, "miss_follower");
                Ok(PluginDecision::Continue)
            }
        }
    }

    async fn on_response_head(
        &self,
        head: &mut ResponseHeader,
        ctx: &mut PluginCtx<'_>,
    ) -> Result<PluginDecision> {
        let add_status_header = self.cache.config.add_status_header;
        let status = head.status.as_u16();
        let headers = || {
            head.headers
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
        };
        let storable =
            is_cacheable_response(status, &self.cache.config.cache_status_codes, headers());
        let stored_headers = storable.then(|| storable_headers(headers()));

        let state = Self::state(ctx);
        if state.key.is_none() {
            return Ok(PluginDecision::Continue);
        }
        if let Some(stored_headers) = stored_headers {
            state.pending = Some(Pending {
                status,
                headers: stored_headers,
                body: bytes::BytesMut::new(),
                too_large: false,
            });
        }
        if add_status_header {
            let _ = head.insert_header("x-cache", "MISS");
        }
        Ok(PluginDecision::Continue)
    }

    fn on_response_body(
        &self,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut PluginCtx<'_>,
    ) -> Result<()> {
        let max_body_bytes = self.cache.config.max_body_bytes;
        let state = Self::state(ctx);
        let Some(pending) = state.pending.as_mut() else {
            return Ok(());
        };
        let Some(key) = state.key else {
            return Ok(());
        };

        if let Some(chunk) = body.as_ref()
            && !pending.too_large
        {
            if pending.body.len() + chunk.len() > max_body_bytes {
                // Streaming a large response is fine; storing it is not. Drop
                // what was collected so the memory goes back immediately.
                pending.too_large = true;
                pending.body = bytes::BytesMut::new();
            } else {
                pending.body.extend_from_slice(chunk);
            }
        }

        if end_of_stream && !pending.too_large {
            let entry = state.pending.take().expect("checked above");
            self.cache
                .store
                .insert(key, entry.status, entry.headers, entry.body.freeze());
        }
        Ok(())
    }

    fn on_log(&self, info: &LogInfo<'_>, ctx: &mut PluginCtx<'_>) {
        let state = Self::state(ctx);
        // Waiters must be woken whether the fetch succeeded, failed or was
        // never cacheable; otherwise they sit until their timeout.
        if state.leader
            && let Some(key) = state.key
        {
            self.cache.store.finish(key);
            state.leader = false;
        }
        metrics::set_cache_size(
            info.route_name,
            self.cache.store.entries(),
            self.cache.store.bytes(),
        );
    }
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
