//! Plugin core (T501).
//!
//! The request path already had five plugins in it — header rules, rate limit,
//! concurrency, micro-cache, compression — each one written by hand into
//! `request_filter` and ordered by whichever `if` came first. This is the place
//! a sixth goes without making that function longer, and without its position
//! in the order being an accident of where someone typed it.
//!
//! The design constraint that shapes everything here: **a route with no plugins
//! must pay nothing.** Not "almost nothing" — a config that never mentions
//! plugins has to benchmark the same as before this module existed. That is why
//! chains are compiled at reload rather than assembled per request, why every
//! phase is guarded by a bit test rather than a loop over an empty slice, and
//! why per-request state is not allocated until a plugin says it wants some.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use pingora::http::{RequestHeader, ResponseHeader};
use pingora::prelude::Result;

pub mod builtin;
pub mod registry;

pub use registry::{build_plugin, check_plugin_config, known_kinds};

// ---------------------------------------------------------------------------
// Phases
// ---------------------------------------------------------------------------

/// Which phases a plugin wants to be called in.
///
/// A chain ORs together what its members declare, and the proxy tests one bit
/// before touching the chain at all. A plugin that only inspects request
/// headers therefore costs a response-heavy route exactly one bit test per
/// phase it is not in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseMask(u8);

impl PhaseMask {
    pub const NONE: Self = Self(0);
    pub const REQUEST_HEAD: Self = Self(1 << 0);
    pub const UPSTREAM_REQUEST: Self = Self(1 << 1);
    pub const RESPONSE_HEAD: Self = Self(1 << 2);
    pub const RESPONSE_BODY: Self = Self(1 << 3);
    pub const LOG: Self = Self(1 << 4);
    /// Reserved for T503, which is what adds `request_body_filter` to the
    /// proxy. Declared here so the bit is not reused for something else.
    pub const REQUEST_BODY: Self = Self(1 << 5);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

// ---------------------------------------------------------------------------
// What a plugin can decide
// ---------------------------------------------------------------------------

/// What the chain should do after a plugin has had its turn.
#[derive(Debug, Clone)]
pub enum PluginDecision {
    /// Carry on to the next plugin, and then to the proxy's own work.
    Continue,
    /// Answer this request here. The rest of the chain does not run and the
    /// request never reaches an upstream — or, from a response phase, the
    /// upstream's answer is replaced by this one.
    Respond { status: u16, body: Bytes },
}

impl PluginDecision {
    /// A plain status with no body, which is what most rejections are.
    pub fn deny(status: u16) -> Self {
        Self::Respond {
            status,
            body: Bytes::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-request state
// ---------------------------------------------------------------------------

/// One `Option<Box<dyn Any>>` per plugin in the chain.
///
/// Indexed by position, so a plugin can only ever reach its own slot — there is
/// no shared bag for plugins to collide in, and no name lookup per request.
pub struct PluginState {
    slots: Box<[Option<Box<dyn Any + Send + Sync>>]>,
}

impl PluginState {
    fn new(len: usize) -> Self {
        let mut slots = Vec::with_capacity(len);
        slots.resize_with(len, || None);
        Self {
            slots: slots.into_boxed_slice(),
        }
    }

    fn slot(&mut self, index: usize) -> &mut Option<Box<dyn Any + Send + Sync>> {
        &mut self.slots[index]
    }
}

impl fmt::Debug for PluginState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginState")
            .field("slots", &self.slots.len())
            .finish()
    }
}

/// What a plugin is handed besides the thing it is inspecting.
pub struct PluginCtx<'a> {
    /// The route this request matched, for logs and metrics.
    pub route_name: &'a str,
    /// This plugin's own state for this request. `None` until it puts
    /// something there, and dropped with the request.
    pub state: &'a mut Option<Box<dyn Any + Send + Sync>>,
}

/// What `on_log` gets to see. Read-only: logging is the phase for recording
/// what happened, not for changing it.
#[derive(Debug, Clone, Copy)]
pub struct LogInfo<'a> {
    pub route_name: &'a str,
    pub status: u16,
    pub latency_ms: u64,
    /// The request ended in an error rather than a response.
    pub failed: bool,
}

// ---------------------------------------------------------------------------
// The trait
// ---------------------------------------------------------------------------

/// Something that can take part in handling a request.
///
/// Every phase has a default that does nothing, so a plugin writes only the
/// phases it cares about — but it must still declare them in [`Plugin::phases`],
/// because that is what the proxy tests instead of calling into the chain.
/// Declaring a phase and not implementing it is merely wasteful; implementing
/// one without declaring it means it never runs, which is why
/// `plugin_declares_every_phase_it_implements` exists in the tests of each
/// built-in.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// The name from `[[plugin]] name = "..."`, for logs and errors.
    fn name(&self) -> &str;

    /// The `kind` this was built from.
    fn kind(&self) -> &'static str;

    fn phases(&self) -> PhaseMask;

    /// Whether this plugin keeps anything in [`PluginCtx::state`]. A chain
    /// where nobody does never allocates the slot array.
    fn uses_state(&self) -> bool {
        false
    }

    async fn on_request_head(
        &self,
        _head: &mut RequestHeader,
        _ctx: &mut PluginCtx<'_>,
    ) -> Result<PluginDecision> {
        Ok(PluginDecision::Continue)
    }

    async fn on_upstream_request(
        &self,
        _head: &mut RequestHeader,
        _ctx: &mut PluginCtx<'_>,
    ) -> Result<()> {
        Ok(())
    }

    async fn on_response_head(
        &self,
        _head: &mut ResponseHeader,
        _ctx: &mut PluginCtx<'_>,
    ) -> Result<PluginDecision> {
        Ok(PluginDecision::Continue)
    }

    /// Synchronous because pingora's `response_body_filter` is: the body is
    /// being streamed and there is nowhere to await.
    fn on_response_body(
        &self,
        _body: &mut Option<Bytes>,
        _end_of_stream: bool,
        _ctx: &mut PluginCtx<'_>,
    ) -> Result<()> {
        Ok(())
    }

    fn on_log(&self, _info: &LogInfo<'_>, _ctx: &mut PluginCtx<'_>) {}
}

// ---------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------

/// A route's plugins, in the order the config listed them.
///
/// Built once per reload and shared by every request on that route, so the
/// per-request cost is the `Arc` deref and the vtable call — not the lookup,
/// the ordering or the config parse.
#[derive(Clone, Default)]
pub struct PluginChain {
    plugins: Arc<[Arc<dyn Plugin>]>,
    mask: PhaseMask,
    needs_state: bool,
}

impl PluginChain {
    pub fn new(plugins: Vec<Arc<dyn Plugin>>) -> Self {
        let mask = plugins
            .iter()
            .fold(PhaseMask::NONE, |acc, plugin| acc.union(plugin.phases()));
        let needs_state = plugins.iter().any(|plugin| plugin.uses_state());
        Self {
            plugins: plugins.into(),
            mask,
            needs_state,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    /// The one call on the hot path for a route with no plugins.
    #[inline]
    pub fn handles(&self, phase: PhaseMask) -> bool {
        self.mask.contains(phase)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn needs_state(&self) -> bool {
        self.needs_state
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Plugin>> {
        self.plugins.iter()
    }

    pub fn names(&self) -> Vec<&str> {
        self.plugins.iter().map(|plugin| plugin.name()).collect()
    }
}

impl Default for PhaseMask {
    fn default() -> Self {
        Self::NONE
    }
}

impl fmt::Debug for PluginChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginChain")
            .field("plugins", &self.names())
            .field("mask", &self.mask)
            .finish()
    }
}

/// Holds a chain's per-request state, allocating it only if somebody wants it.
///
/// A chain where no plugin keeps state hands out a slot on the stack instead,
/// so the common case — plugins that only look at headers — costs no
/// allocation at all.
pub struct StateSlots<'a> {
    real: &'a mut Option<Box<PluginState>>,
    scratch: Option<Box<dyn Any + Send + Sync>>,
    len: usize,
    needed: bool,
}

impl<'a> StateSlots<'a> {
    pub fn new(chain: &PluginChain, real: &'a mut Option<Box<PluginState>>) -> Self {
        Self {
            real,
            scratch: None,
            len: chain.len(),
            needed: chain.needs_state(),
        }
    }

    pub fn slot(&mut self, index: usize) -> &mut Option<Box<dyn Any + Send + Sync>> {
        if self.needed {
            self.real
                .get_or_insert_with(|| Box::new(PluginState::new(self.len)))
                .slot(index)
        } else {
            &mut self.scratch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Marker {
        name: String,
        phases: PhaseMask,
        stateful: bool,
    }

    #[async_trait]
    impl Plugin for Marker {
        fn name(&self) -> &str {
            &self.name
        }
        fn kind(&self) -> &'static str {
            "marker"
        }
        fn phases(&self) -> PhaseMask {
            self.phases
        }
        fn uses_state(&self) -> bool {
            self.stateful
        }
    }

    fn marker(name: &str, phases: PhaseMask, stateful: bool) -> Arc<dyn Plugin> {
        Arc::new(Marker {
            name: name.to_string(),
            phases,
            stateful,
        })
    }

    #[test]
    fn an_empty_chain_handles_nothing() {
        let chain = PluginChain::empty();
        assert!(chain.is_empty());
        for phase in [
            PhaseMask::REQUEST_HEAD,
            PhaseMask::UPSTREAM_REQUEST,
            PhaseMask::RESPONSE_HEAD,
            PhaseMask::RESPONSE_BODY,
            PhaseMask::LOG,
        ] {
            assert!(!chain.handles(phase));
        }
    }

    #[test]
    fn a_chain_answers_for_every_phase_its_members_declare() {
        let chain = PluginChain::new(vec![
            marker("a", PhaseMask::REQUEST_HEAD, false),
            marker("b", PhaseMask::LOG, false),
        ]);
        assert!(chain.handles(PhaseMask::REQUEST_HEAD));
        assert!(chain.handles(PhaseMask::LOG));
        // Nobody asked for these, so the proxy must skip them entirely.
        assert!(!chain.handles(PhaseMask::RESPONSE_BODY));
        assert!(!chain.handles(PhaseMask::UPSTREAM_REQUEST));
    }

    #[test]
    fn the_chain_keeps_the_order_the_config_listed() {
        let chain = PluginChain::new(vec![
            marker("first", PhaseMask::REQUEST_HEAD, false),
            marker("second", PhaseMask::REQUEST_HEAD, false),
            marker("third", PhaseMask::REQUEST_HEAD, false),
        ]);
        assert_eq!(chain.names(), vec!["first", "second", "third"]);
    }

    #[test]
    fn state_is_not_allocated_when_no_plugin_wants_any() {
        let chain = PluginChain::new(vec![marker("a", PhaseMask::REQUEST_HEAD, false)]);
        assert!(!chain.needs_state());

        let mut real: Option<Box<PluginState>> = None;
        let mut slots = StateSlots::new(&chain, &mut real);
        // Writing into the scratch slot must not reach for the heap slab.
        *slots.slot(0) = Some(Box::new(7u32));
        assert!(
            real.is_none(),
            "a chain nobody keeps state in allocated a slab anyway"
        );
    }

    #[test]
    fn state_is_allocated_once_and_kept_apart_per_plugin() {
        let chain = PluginChain::new(vec![
            marker("a", PhaseMask::REQUEST_HEAD, true),
            marker("b", PhaseMask::REQUEST_HEAD, false),
        ]);
        assert!(chain.needs_state());

        let mut real: Option<Box<PluginState>> = None;
        {
            let mut slots = StateSlots::new(&chain, &mut real);
            *slots.slot(0) = Some(Box::new(1u32));
            *slots.slot(1) = Some(Box::new(2u32));
        }
        let state = real.expect("a stateful chain should have allocated its slots");
        assert_eq!(
            state.slots[0]
                .as_ref()
                .and_then(|value| value.downcast_ref::<u32>()),
            Some(&1)
        );
        assert_eq!(
            state.slots[1]
                .as_ref()
                .and_then(|value| value.downcast_ref::<u32>()),
            Some(&2)
        );
    }

    #[test]
    fn phase_bits_do_not_overlap() {
        let all = [
            PhaseMask::REQUEST_HEAD,
            PhaseMask::REQUEST_BODY,
            PhaseMask::UPSTREAM_REQUEST,
            PhaseMask::RESPONSE_HEAD,
            PhaseMask::RESPONSE_BODY,
            PhaseMask::LOG,
        ];
        let mut seen = 0u8;
        for phase in all {
            assert_eq!(seen & phase.0, 0, "two phases share a bit");
            seen |= phase.0;
        }
    }

    #[test]
    fn deny_is_a_response_with_no_body() {
        match PluginDecision::deny(403) {
            PluginDecision::Respond { status, body } => {
                assert_eq!(status, 403);
                assert!(body.is_empty());
            }
            PluginDecision::Continue => panic!("deny should not continue"),
        }
    }
}
