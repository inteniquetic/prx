//! Route index (T102).
//!
//! Replaces the linear scan in `RuntimeConfig::select_route` with a structure
//! built once per config reload:
//!
//! ```text
//! host ──► exact map      (hash lookup on the whole host)
//!      ──► wildcard map    (hash lookup per label suffix, longest first)
//!      ──► any-host bucket
//!                └─► path radix trie ─► route entries (file order)
//! ```
//!
//! Lookup cost is O(len(path)) per host tier instead of O(number of routes),
//! and no allocation happens on the lookup path (T101).
//!
//! # Matching rules
//!
//! 1. An exact host match beats a wildcard host match, which beats a route
//!    that declares no host.
//! 2. Among wildcard hosts, the longest suffix wins (`*.api.example.com`
//!    before `*.example.com`).
//! 3. Within one host, the longest path prefix wins.
//! 4. If several routes tie on host and path prefix, the first one in the
//!    config file wins.
//! 5. `methods` is a hard filter on the route that wins the path match. If
//!    that route rejects the request method the result is `MethodNotAllowed`
//!    (405): prx does not fall through to a broader route or to the default
//!    route, because that would silently route a method past the restriction
//!    the operator asked for.
//! 6. A route marked `is_default` is the fallback when nothing else matches.

use rustc_hash::FxHashMap;

/// Outcome of a route lookup. `Copy`, so the hot path never allocates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMatch {
    /// Index into `RuntimeConfig::routes`.
    Matched(usize),
    /// A route matched host and path, but not the request method.
    MethodNotAllowed,
    /// Nothing matched and no default route exists.
    NotFound,
}

/// Bitmask of the HTTP methods a route accepts. `0` means "any method".
pub type MethodMask = u16;

pub const METHOD_ANY: MethodMask = 0;

const METHODS: [(&str, MethodMask); 9] = [
    ("GET", 1 << 0),
    ("POST", 1 << 1),
    ("PUT", 1 << 2),
    ("PATCH", 1 << 3),
    ("DELETE", 1 << 4),
    ("HEAD", 1 << 5),
    ("OPTIONS", 1 << 6),
    ("TRACE", 1 << 7),
    ("CONNECT", 1 << 8),
];

/// Returns the bit for a method name, or `None` when prx does not know it.
pub fn method_bit(method: &str) -> Option<MethodMask> {
    METHODS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(method))
        .map(|(_, bit)| *bit)
}

/// Builds a mask from configured method names. An empty list means "any".
pub fn method_mask(methods: &[String]) -> MethodMask {
    methods
        .iter()
        .filter_map(|m| method_bit(m))
        .fold(METHOD_ANY, |acc, bit| acc | bit)
}

#[derive(Debug, Clone, Copy)]
struct RouteEntry {
    route_idx: usize,
    methods: MethodMask,
}

impl RouteEntry {
    fn accepts(&self, method_bit: Option<MethodMask>) -> bool {
        if self.methods == METHOD_ANY {
            return true;
        }
        match method_bit {
            Some(bit) => self.methods & bit != 0,
            // A method prx does not know can only match a route that accepts
            // every method.
            None => false,
        }
    }
}

/// What a single host bucket concluded about a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BucketOutcome {
    /// A route matched host, path and method.
    Hit(usize),
    /// A route matched the path but rejects the method.
    MethodMismatch,
    /// No route in this bucket covers the path.
    NoPathMatch,
}

/// A radix trie over path prefixes. Nodes live in one `Vec` so the whole trie
/// is a couple of allocations and stays cache friendly.
#[derive(Debug)]
struct PathTrie {
    nodes: Vec<TrieNode>,
}

impl Default for PathTrie {
    /// Never derive this: the lookup and insert paths both assume node 0 is the
    /// root, so an empty `nodes` vector would be an invalid trie.
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct TrieNode {
    /// Edge label from the parent node.
    label: Box<[u8]>,
    /// Child node indices, kept sorted by the first byte of their label.
    children: Vec<u32>,
    /// Routes registered at exactly this prefix, in config order.
    entries: Vec<RouteEntry>,
}

impl TrieNode {
    fn new(label: &[u8]) -> Self {
        Self {
            label: label.into(),
            children: Vec::new(),
            entries: Vec::new(),
        }
    }
}

impl PathTrie {
    fn new() -> Self {
        Self {
            nodes: vec![TrieNode::new(b"")],
        }
    }

    fn insert(&mut self, prefix: &[u8], entry: RouteEntry) {
        let mut node = 0usize;
        let mut rest = prefix;

        loop {
            if rest.is_empty() {
                self.nodes[node].entries.push(entry);
                return;
            }

            match self.child_sharing_first_byte(node, rest[0]) {
                None => {
                    let child = self.push_node(rest);
                    self.nodes[child].entries.push(entry);
                    self.link_child(node, child as u32);
                    return;
                }
                Some(child) => {
                    let shared = common_prefix_len(&self.nodes[child].label, rest);
                    if shared == self.nodes[child].label.len() {
                        // The whole edge is consumed: continue below it.
                        node = child;
                        rest = &rest[shared..];
                        continue;
                    }

                    // Split the edge: parent -> [shared] -> old tail.
                    let split = self.split_child(node, child, shared);
                    if shared == rest.len() {
                        self.nodes[split].entries.push(entry);
                    } else {
                        let leaf = self.push_node(&rest[shared..]);
                        self.nodes[leaf].entries.push(entry);
                        self.link_child(split, leaf as u32);
                    }
                    return;
                }
            }
        }
    }

    /// Longest-prefix lookup.
    ///
    /// The deepest node that carries routes decides the outcome: if none of its
    /// routes accepts the method, the request is a method mismatch rather than
    /// a match against some shorter prefix. Walks the path once and allocates
    /// nothing.
    fn lookup(&self, path: &[u8], method_bit: Option<MethodMask>) -> BucketOutcome {
        let mut node = 0usize;
        let mut rest = path;
        let mut outcome = BucketOutcome::NoPathMatch;

        loop {
            if !self.nodes[node].entries.is_empty() {
                outcome = match self.nodes[node]
                    .entries
                    .iter()
                    .find(|entry| entry.accepts(method_bit))
                {
                    Some(entry) => BucketOutcome::Hit(entry.route_idx),
                    None => BucketOutcome::MethodMismatch,
                };
            }

            if rest.is_empty() {
                return outcome;
            }

            let Some(child) = self.child_sharing_first_byte(node, rest[0]) else {
                return outcome;
            };
            let label = &self.nodes[child].label;
            if rest.len() < label.len() || &rest[..label.len()] != label.as_ref() {
                return outcome;
            }
            rest = &rest[label.len()..];
            node = child;
        }
    }

    fn child_sharing_first_byte(&self, node: usize, byte: u8) -> Option<usize> {
        let children = &self.nodes[node].children;
        children
            .binary_search_by(|probe| self.nodes[*probe as usize].label[0].cmp(&byte))
            .ok()
            .map(|pos| children[pos] as usize)
    }

    fn push_node(&mut self, label: &[u8]) -> usize {
        self.nodes.push(TrieNode::new(label));
        self.nodes.len() - 1
    }

    fn link_child(&mut self, parent: usize, child: u32) {
        let byte = self.nodes[child as usize].label[0];
        let pos = self.nodes[parent]
            .children
            .binary_search_by(|probe| self.nodes[*probe as usize].label[0].cmp(&byte))
            .unwrap_or_else(|pos| pos);
        self.nodes[parent].children.insert(pos, child);
    }

    /// Splits `child`'s edge after `shared` bytes and returns the new node that
    /// now sits between `parent` and `child`.
    fn split_child(&mut self, parent: usize, child: usize, shared: usize) -> usize {
        let label = self.nodes[child].label.clone();
        let split = self.push_node(&label[..shared]);
        self.nodes[child].label = label[shared..].into();

        // Re-point the parent at the new intermediate node.
        let byte = label[0];
        let children = &mut self.nodes[parent].children;
        if let Some(pos) = children.iter().position(|c| *c as usize == child) {
            children[pos] = split as u32;
        } else {
            debug_assert!(false, "split_child called with a non-child node");
            let _ = byte;
        }
        self.nodes[split].children.push(child as u32);
        split
    }
}

fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

#[derive(Debug, Default)]
struct HostBucket {
    paths: PathTrie,
}

/// The route index built once per config reload.
#[derive(Debug)]
pub struct RouteIndex {
    /// Host lookup uses FxHash rather than SipHash: the map only ever holds
    /// hosts from the config, so a crafted Host header can at worst collide
    /// with one of them and walk a bucket of a handful of entries, while the
    /// per-request hashing cost drops substantially.
    exact: FxHashMap<Box<str>, HostBucket>,
    /// Keyed by the part after `*.`. Lookups strip one label at a time, so the
    /// longest suffix is tried first and the cost is one hash per label in the
    /// request host instead of a scan over every wildcard route.
    wildcard: FxHashMap<Box<str>, HostBucket>,
    any: HostBucket,
    default_route: Option<usize>,
}

/// One route as the index needs it, in config order.
pub struct IndexedRoute<'a> {
    pub host: Option<&'a str>,
    pub path_prefix: &'a str,
    pub methods: MethodMask,
    pub is_default: bool,
}

impl RouteIndex {
    pub fn build<'a, I>(routes: I) -> Self
    where
        I: IntoIterator<Item = IndexedRoute<'a>>,
    {
        let mut index = Self {
            exact: FxHashMap::default(),
            wildcard: FxHashMap::default(),
            any: HostBucket::default(),
            default_route: None,
        };

        for (route_idx, route) in routes.into_iter().enumerate() {
            if route.is_default && index.default_route.is_none() {
                index.default_route = Some(route_idx);
            }

            let entry = RouteEntry {
                route_idx,
                methods: route.methods,
            };
            let prefix = route.path_prefix.as_bytes();

            match route.host {
                // An empty host pattern means "any host", same as omitting it.
                None | Some("") => index.any.paths.insert(prefix, entry),
                Some(pattern) => match pattern.strip_prefix("*.") {
                    Some(suffix) => index
                        .wildcard
                        .entry(suffix.into())
                        .or_default()
                        .paths
                        .insert(prefix, entry),
                    None => index
                        .exact
                        .entry(pattern.into())
                        .or_default()
                        .paths
                        .insert(prefix, entry),
                },
            }
        }

        index
    }

    /// Looks up a route. `host` must already be normalized (lowercase, no port).
    ///
    /// The first host tier that covers the path decides: a more specific host
    /// does not fall through to a broader one just because the method was
    /// rejected.
    pub fn select(&self, host: &str, path: &str, method: Option<&str>) -> RouteMatch {
        let method_bit = method.map(method_bit).unwrap_or(Some(METHOD_ANY));
        let path_bytes = path.as_bytes();

        if let Some(bucket) = self.exact.get(host) {
            match bucket.paths.lookup(path_bytes, method_bit) {
                BucketOutcome::Hit(idx) => return RouteMatch::Matched(idx),
                BucketOutcome::MethodMismatch => return RouteMatch::MethodNotAllowed,
                BucketOutcome::NoPathMatch => {}
            }
        }

        // `*.example.com` also matches `example.com`, so the first candidate is
        // the whole host; each following one drops the leading label. Trying
        // them in that order means the longest suffix wins.
        if !self.wildcard.is_empty() {
            let mut candidate = host;
            loop {
                if let Some(bucket) = self.wildcard.get(candidate) {
                    match bucket.paths.lookup(path_bytes, method_bit) {
                        BucketOutcome::Hit(idx) => return RouteMatch::Matched(idx),
                        BucketOutcome::MethodMismatch => return RouteMatch::MethodNotAllowed,
                        BucketOutcome::NoPathMatch => {}
                    }
                }
                match candidate.find('.') {
                    Some(dot) => candidate = &candidate[dot + 1..],
                    None => break,
                }
            }
        }

        match self.any.paths.lookup(path_bytes, method_bit) {
            BucketOutcome::Hit(idx) => return RouteMatch::Matched(idx),
            BucketOutcome::MethodMismatch => return RouteMatch::MethodNotAllowed,
            BucketOutcome::NoPathMatch => {}
        }

        match self.default_route {
            Some(idx) => RouteMatch::Matched(idx),
            None => RouteMatch::NotFound,
        }
    }

    pub fn default_route(&self) -> Option<usize> {
        self.default_route
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route<'a>(host: Option<&'a str>, path: &'a str) -> IndexedRoute<'a> {
        IndexedRoute {
            host,
            path_prefix: path,
            methods: METHOD_ANY,
            is_default: false,
        }
    }

    fn with_methods<'a>(
        host: Option<&'a str>,
        path: &'a str,
        methods: &[&str],
    ) -> IndexedRoute<'a> {
        let owned: Vec<String> = methods.iter().map(|m| m.to_string()).collect();
        IndexedRoute {
            host,
            path_prefix: path,
            methods: method_mask(&owned),
            is_default: false,
        }
    }

    fn default_route<'a>(path: &'a str) -> IndexedRoute<'a> {
        IndexedRoute {
            host: None,
            path_prefix: path,
            methods: METHOD_ANY,
            is_default: true,
        }
    }

    fn matched(index: &RouteIndex, host: &str, path: &str) -> Option<usize> {
        match index.select(host, path, Some("GET")) {
            RouteMatch::Matched(idx) => Some(idx),
            _ => None,
        }
    }

    #[test]
    fn longest_path_prefix_wins() {
        let index = RouteIndex::build(vec![
            route(Some("api.local"), "/"),
            route(Some("api.local"), "/v1"),
            route(Some("api.local"), "/v1/users"),
        ]);

        assert_eq!(matched(&index, "api.local", "/anything"), Some(0));
        assert_eq!(matched(&index, "api.local", "/v1/orders"), Some(1));
        assert_eq!(matched(&index, "api.local", "/v1/users/42"), Some(2));
    }

    #[test]
    fn exact_host_beats_wildcard_and_any() {
        let index = RouteIndex::build(vec![
            route(None, "/"),
            route(Some("*.example.com"), "/"),
            route(Some("api.example.com"), "/"),
        ]);

        assert_eq!(matched(&index, "api.example.com", "/x"), Some(2));
        assert_eq!(matched(&index, "www.example.com", "/x"), Some(1));
        assert_eq!(matched(&index, "other.test", "/x"), Some(0));
    }

    #[test]
    fn longer_wildcard_suffix_wins() {
        let index = RouteIndex::build(vec![
            route(Some("*.example.com"), "/"),
            route(Some("*.api.example.com"), "/"),
        ]);

        assert_eq!(matched(&index, "a.api.example.com", "/x"), Some(1));
        assert_eq!(matched(&index, "a.example.com", "/x"), Some(0));
    }

    #[test]
    fn wildcard_matches_the_bare_domain_too() {
        let index = RouteIndex::build(vec![route(Some("*.example.com"), "/")]);

        assert_eq!(matched(&index, "example.com", "/x"), Some(0));
        assert_eq!(matched(&index, "a.b.example.com", "/x"), Some(0));
        // Must not match a domain that merely ends with the same letters.
        assert_eq!(matched(&index, "notexample.com", "/x"), None);
    }

    #[test]
    fn first_route_in_config_order_wins_a_tie() {
        let index = RouteIndex::build(vec![
            route(Some("api.local"), "/v1"),
            route(Some("api.local"), "/v1"),
        ]);

        assert_eq!(matched(&index, "api.local", "/v1/x"), Some(0));
    }

    #[test]
    fn empty_host_pattern_means_any_host() {
        let index = RouteIndex::build(vec![route(Some(""), "/")]);

        assert_eq!(matched(&index, "anything.local", "/x"), Some(0));
    }

    #[test]
    fn default_route_is_the_fallback() {
        let index = RouteIndex::build(vec![route(Some("api.local"), "/v1"), default_route("/")]);

        assert_eq!(matched(&index, "other.local", "/nothing"), Some(1));
        assert_eq!(index.default_route(), Some(1));
    }

    #[test]
    fn no_match_without_a_default_route() {
        let index = RouteIndex::build(vec![route(Some("api.local"), "/v1")]);

        assert_eq!(
            index.select("other.local", "/x", Some("GET")),
            RouteMatch::NotFound
        );
    }

    #[test]
    fn method_mismatch_returns_method_not_allowed() {
        let index = RouteIndex::build(vec![
            with_methods(Some("api.local"), "/v1", &["POST"]),
            default_route("/"),
        ]);

        assert_eq!(
            index.select("api.local", "/v1/x", Some("POST")),
            RouteMatch::Matched(0)
        );
        // The default route must not silently swallow a method mismatch.
        assert_eq!(
            index.select("api.local", "/v1/x", Some("GET")),
            RouteMatch::MethodNotAllowed
        );
    }

    #[test]
    fn a_rejected_method_does_not_fall_through_to_a_broader_route() {
        let index = RouteIndex::build(vec![
            route(Some("api.local"), "/"),
            with_methods(Some("api.local"), "/v1", &["POST"]),
        ]);

        // /v1 is the longest prefix and only takes POST. Falling back to "/"
        // here would route a GET past the restriction, so this is a 405.
        assert_eq!(
            index.select("api.local", "/v1/x", Some("GET")),
            RouteMatch::MethodNotAllowed
        );
        // Paths outside /v1 are unaffected.
        assert_eq!(
            index.select("api.local", "/other", Some("GET")),
            RouteMatch::Matched(0)
        );
    }

    #[test]
    fn a_specific_host_does_not_fall_through_to_a_wildcard_on_method_mismatch() {
        let index = RouteIndex::build(vec![
            with_methods(Some("api.example.com"), "/", &["POST"]),
            route(Some("*.example.com"), "/"),
        ]);

        assert_eq!(
            index.select("api.example.com", "/x", Some("GET")),
            RouteMatch::MethodNotAllowed
        );
        assert_eq!(
            index.select("www.example.com", "/x", Some("GET")),
            RouteMatch::Matched(1)
        );
    }

    #[test]
    fn unknown_request_method_only_matches_unrestricted_routes() {
        let index = RouteIndex::build(vec![
            with_methods(Some("api.local"), "/v1", &["GET"]),
            route(Some("api.local"), "/"),
        ]);

        // A method prx does not know cannot satisfy a route that lists methods.
        assert_eq!(
            index.select("api.local", "/v1/x", Some("PROPFIND")),
            RouteMatch::MethodNotAllowed
        );
        // A route without a method list accepts it.
        assert_eq!(
            index.select("api.local", "/other", Some("PROPFIND")),
            RouteMatch::Matched(1)
        );
    }

    #[test]
    fn method_matching_is_case_insensitive_in_config() {
        let index = RouteIndex::build(vec![with_methods(Some("api.local"), "/", &["get", "Post"])]);

        assert_eq!(
            index.select("api.local", "/x", Some("GET")),
            RouteMatch::Matched(0)
        );
        assert_eq!(
            index.select("api.local", "/x", Some("POST")),
            RouteMatch::Matched(0)
        );
        assert_eq!(
            index.select("api.local", "/x", Some("DELETE")),
            RouteMatch::MethodNotAllowed
        );
    }

    #[test]
    fn trie_handles_shared_and_split_prefixes() {
        let index = RouteIndex::build(vec![
            route(Some("api.local"), "/api/v1/users"),
            route(Some("api.local"), "/api/v1/orders"),
            route(Some("api.local"), "/api/v2"),
            route(Some("api.local"), "/api"),
        ]);

        assert_eq!(matched(&index, "api.local", "/api/v1/users/1"), Some(0));
        assert_eq!(matched(&index, "api.local", "/api/v1/orders"), Some(1));
        assert_eq!(matched(&index, "api.local", "/api/v2/x"), Some(2));
        assert_eq!(matched(&index, "api.local", "/api/v1/other"), Some(3));
        assert_eq!(matched(&index, "api.local", "/apiary"), Some(3));
        assert_eq!(matched(&index, "api.local", "/other"), None);
    }

    #[test]
    fn path_shorter_than_the_prefix_does_not_match() {
        let index = RouteIndex::build(vec![route(Some("api.local"), "/v1/users")]);

        assert_eq!(matched(&index, "api.local", "/v1"), None);
        assert_eq!(matched(&index, "api.local", "/v1/user"), None);
    }
}
