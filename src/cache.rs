//! Short-lived response cache with request coalescing (T109).
//!
//! The throughput win here is not the cache hits on their own: it is that a
//! burst of identical requests arriving during a miss produces **one** upstream
//! request instead of hundreds. That is what keeps an upstream standing when a
//! popular object expires under load.
//!
//! Entries are kept in sharded maps with a hard cap on both count and total
//! bytes, so the cache cannot grow into the proxy's memory budget.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use http::{HeaderName, HeaderValue, StatusCode};
use tokio::sync::Notify;

const SHARDS: usize = 32;

/// Headers that must never be replayed from a cache entry: they describe one
/// connection or one client, not the object.
const HOP_BY_HOP: [&str; 9] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "set-cookie",
];

/// A stored response.
#[derive(Debug, Clone)]
pub struct CachedResponse {
    pub status: u16,
    pub headers: Vec<(HeaderName, HeaderValue)>,
    pub body: Bytes,
    expires_at_ms: u64,
    stored_at_ms: u64,
}

impl CachedResponse {
    pub fn age_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.stored_at_ms)
    }

    fn is_fresh(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms
    }

    fn size_bytes(&self) -> usize {
        self.body.len()
            + self
                .headers
                .iter()
                .map(|(name, value)| name.as_str().len() + value.len())
                .sum::<usize>()
    }
}

/// Result of asking the cache for a key.
#[derive(Debug)]
pub enum Lookup {
    /// A fresh entry was found.
    Hit(Arc<CachedResponse>),
    /// Nothing cached and this request owns the upstream fetch. It must call
    /// [`ResponseCache::finish`] when it is done, however that goes.
    MissLeader,
    /// Another request is already fetching this key; this one waited and still
    /// found nothing, so it should go upstream too rather than wait forever.
    MissFollower,
}

#[derive(Debug, Default)]
struct Shard {
    entries: HashMap<u64, Arc<CachedResponse>>,
}

/// A short-lived response cache.
#[derive(Debug)]
pub struct ResponseCache {
    ttl: Duration,
    max_entries: usize,
    max_bytes: usize,
    /// How long a coalesced request waits for the leader before going upstream
    /// itself. Bounded so a stuck leader cannot stall everyone behind it.
    coalesce_wait: Duration,
    shards: Vec<Mutex<Shard>>,
    inflight: Mutex<HashMap<u64, Arc<Notify>>>,
    entries: AtomicUsize,
    bytes: AtomicUsize,
    hits: AtomicU64,
    misses: AtomicU64,
    coalesced: AtomicU64,
    evictions: AtomicU64,
}

impl ResponseCache {
    pub fn new(
        ttl: Duration,
        max_entries: usize,
        max_bytes: usize,
        coalesce_wait: Duration,
    ) -> Self {
        Self {
            ttl,
            max_entries: max_entries.max(SHARDS),
            max_bytes: max_bytes.max(64 * 1024),
            coalesce_wait,
            shards: (0..SHARDS).map(|_| Mutex::new(Shard::default())).collect(),
            inflight: Mutex::new(HashMap::new()),
            entries: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            coalesced: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    pub fn coalesced(&self) -> u64 {
        self.coalesced.load(Ordering::Relaxed)
    }

    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    pub fn entries(&self) -> usize {
        self.entries.load(Ordering::Relaxed)
    }

    pub fn bytes(&self) -> usize {
        self.bytes.load(Ordering::Relaxed)
    }

    fn shard(&self, key: u64) -> &Mutex<Shard> {
        &self.shards[(key as usize) & (SHARDS - 1)]
    }

    fn lock_shard(&self, key: u64) -> std::sync::MutexGuard<'_, Shard> {
        // A poisoned lock means a panic elsewhere; serving traffic matters more
        // than the cache's internal consistency, so recover and carry on.
        match self.shard(key).lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Reads a fresh entry, if there is one.
    pub fn peek(&self, key: u64, now_ms: u64) -> Option<Arc<CachedResponse>> {
        let shard = self.lock_shard(key);
        shard
            .entries
            .get(&key)
            .filter(|entry| entry.is_fresh(now_ms))
            .cloned()
    }

    /// Looks a key up, coalescing concurrent misses onto one upstream request.
    pub async fn lookup(&self, key: u64) -> Lookup {
        let now_ms = now_epoch_ms();
        if let Some(entry) = self.peek(key, now_ms) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Lookup::Hit(entry);
        }
        self.misses.fetch_add(1, Ordering::Relaxed);

        // Either become the leader for this key, or get the handle to wait on.
        let notify = {
            let mut inflight = match self.inflight.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            match inflight.get(&key) {
                Some(existing) => Some(existing.clone()),
                None => {
                    inflight.insert(key, Arc::new(Notify::new()));
                    None
                }
            }
        };

        let Some(notify) = notify else {
            return Lookup::MissLeader;
        };

        self.coalesced.fetch_add(1, Ordering::Relaxed);
        // Wait for the leader, but never indefinitely: if it dies or takes too
        // long, this request goes upstream on its own.
        let _ = tokio::time::timeout(self.coalesce_wait, notify.notified()).await;

        match self.peek(key, now_epoch_ms()) {
            Some(entry) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Lookup::Hit(entry)
            }
            None => Lookup::MissFollower,
        }
    }

    /// Releases leadership of a key and wakes everyone waiting on it. Must be
    /// called by a leader whether the fetch succeeded, failed or was cached.
    pub fn finish(&self, key: u64) {
        let notify = {
            let mut inflight = match self.inflight.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            inflight.remove(&key)
        };
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }

    /// Stores a response. Oversized bodies are rejected by the caller.
    pub fn insert(
        &self,
        key: u64,
        status: u16,
        headers: Vec<(HeaderName, HeaderValue)>,
        body: Bytes,
    ) {
        let now_ms = now_epoch_ms();
        let entry = Arc::new(CachedResponse {
            status,
            headers,
            body,
            expires_at_ms: now_ms + self.ttl.as_millis() as u64,
            stored_at_ms: now_ms,
        });
        let size = entry.size_bytes();

        {
            let mut shard = self.lock_shard(key);
            if let Some(previous) = shard.entries.insert(key, entry) {
                self.bytes
                    .fetch_sub(previous.size_bytes().min(self.bytes()), Ordering::Relaxed);
            } else {
                self.entries.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.bytes.fetch_add(size, Ordering::Relaxed);

        if self.entries() > self.max_entries || self.bytes() > self.max_bytes {
            self.evict(now_ms);
        }
    }

    /// Drops expired entries first, then the oldest ones, until the cache is
    /// back inside both caps.
    fn evict(&self, now_ms: u64) {
        for shard in &self.shards {
            let mut shard = match shard.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            shard.entries.retain(|_, entry| {
                if entry.is_fresh(now_ms) {
                    true
                } else {
                    self.entries.fetch_sub(1, Ordering::Relaxed);
                    self.bytes
                        .fetch_sub(entry.size_bytes().min(self.bytes()), Ordering::Relaxed);
                    self.evictions.fetch_add(1, Ordering::Relaxed);
                    false
                }
            });
        }

        if self.entries() <= self.max_entries && self.bytes() <= self.max_bytes {
            return;
        }

        // Still over: drop the oldest entries shard by shard.
        for shard in &self.shards {
            if self.entries() <= self.max_entries && self.bytes() <= self.max_bytes {
                return;
            }
            let mut shard = match shard.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let mut by_age: Vec<(u64, u64)> = shard
                .entries
                .iter()
                .map(|(key, entry)| (*key, entry.stored_at_ms))
                .collect();
            by_age.sort_unstable_by_key(|(_, stored)| *stored);

            for (key, _) in by_age {
                if self.entries() <= self.max_entries / 2 && self.bytes() <= self.max_bytes / 2 {
                    break;
                }
                if let Some(entry) = shard.entries.remove(&key) {
                    self.entries.fetch_sub(1, Ordering::Relaxed);
                    self.bytes
                        .fetch_sub(entry.size_bytes().min(self.bytes()), Ordering::Relaxed);
                    self.evictions.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Empties the cache, returning how many entries were dropped.
    pub fn purge(&self) -> usize {
        let mut removed = 0;
        for shard in &self.shards {
            let mut shard = match shard.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            removed += shard.entries.len();
            shard.entries.clear();
        }
        self.entries.store(0, Ordering::Relaxed);
        self.bytes.store(0, Ordering::Relaxed);
        self.evictions.fetch_add(removed as u64, Ordering::Relaxed);
        removed
    }
}

/// Whether a response may be stored, given the rules prx enforces regardless of
/// configuration.
pub fn is_cacheable_response(
    status: u16,
    cacheable_status: &[u16],
    headers: impl Iterator<Item = (HeaderName, HeaderValue)>,
) -> bool {
    if !cacheable_status.contains(&status) {
        return false;
    }
    if StatusCode::from_u16(status).is_err() {
        return false;
    }

    for (name, value) in headers {
        let value = value.to_str().unwrap_or("").to_ascii_lowercase();
        match name.as_str() {
            "cache-control" => {
                if value.contains("no-store") || value.contains("private") {
                    return false;
                }
            }
            // A response that varies on everything cannot be replayed safely.
            "vary" if value.trim() == "*" => return false,
            // Storing a response that sets a session cookie would hand that
            // session to the next client.
            "set-cookie" => return false,
            _ => {}
        }
    }
    true
}

/// Strips headers that belong to one connection or client before storing.
pub fn storable_headers(
    headers: impl Iterator<Item = (HeaderName, HeaderValue)>,
) -> Vec<(HeaderName, HeaderValue)> {
    headers
        .filter(|(name, _)| !HOP_BY_HOP.contains(&name.as_str()))
        .collect()
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(name: &str, value: &str) -> (HeaderName, HeaderValue) {
        (
            HeaderName::from_bytes(name.as_bytes()).expect("valid name"),
            HeaderValue::from_str(value).expect("valid value"),
        )
    }

    fn cache(ttl_ms: u64) -> ResponseCache {
        ResponseCache::new(
            Duration::from_millis(ttl_ms),
            1_000,
            10 * 1024 * 1024,
            Duration::from_millis(200),
        )
    }

    #[tokio::test]
    async fn a_stored_response_is_served_until_it_expires() {
        let cache = cache(120);
        cache.insert(
            1,
            200,
            vec![header("content-type", "text/plain")],
            Bytes::from_static(b"body"),
        );

        match cache.lookup(1).await {
            Lookup::Hit(entry) => {
                assert_eq!(entry.status, 200);
                assert_eq!(entry.body, Bytes::from_static(b"body"));
            }
            other => panic!("expected a hit, got {other:?}"),
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            cache.peek(1, now_epoch_ms()).is_none(),
            "the entry should have expired"
        );
    }

    /// The point of the whole module: a burst of identical misses must produce
    /// one upstream fetch, not one per request.
    #[tokio::test]
    async fn concurrent_misses_are_coalesced_into_one_fetch() {
        let cache = Arc::new(ResponseCache::new(
            Duration::from_secs(5),
            1_000,
            1024 * 1024,
            Duration::from_secs(2),
        ));
        let upstream_calls = Arc::new(AtomicUsize::new(0));

        let mut waiters = Vec::new();
        for _ in 0..50 {
            let cache = cache.clone();
            let calls = upstream_calls.clone();
            waiters.push(tokio::spawn(async move {
                match cache.lookup(42).await {
                    Lookup::Hit(_) => "hit",
                    Lookup::MissLeader => {
                        // Simulate the upstream fetch.
                        calls.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        cache.insert(42, 200, vec![], Bytes::from_static(b"payload"));
                        cache.finish(42);
                        "leader"
                    }
                    Lookup::MissFollower => {
                        calls.fetch_add(1, Ordering::Relaxed);
                        "follower"
                    }
                }
            }));
        }

        let mut hits = 0;
        for waiter in waiters {
            if waiter.await.expect("task panicked") == "hit" {
                hits += 1;
            }
        }

        assert_eq!(
            upstream_calls.load(Ordering::Relaxed),
            1,
            "only the leader should have gone upstream"
        );
        assert!(
            hits >= 40,
            "most waiters should have been served from the cache, got {hits}"
        );
    }

    #[tokio::test]
    async fn a_follower_gives_up_rather_than_waiting_forever() {
        let cache = Arc::new(ResponseCache::new(
            Duration::from_secs(5),
            1_000,
            1024 * 1024,
            Duration::from_millis(80),
        ));

        // A leader that never finishes.
        assert!(matches!(cache.lookup(7).await, Lookup::MissLeader));

        let started = std::time::Instant::now();
        let follower = cache.lookup(7).await;
        assert!(
            matches!(follower, Lookup::MissFollower),
            "a follower must proceed on its own when the leader stalls"
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "the follower waited too long: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn eviction_keeps_the_cache_inside_its_caps() {
        let cache = ResponseCache::new(
            Duration::from_secs(60),
            64,
            64 * 1024,
            Duration::from_millis(100),
        );

        for key in 0..5_000u64 {
            cache.insert(key, 200, vec![], Bytes::from_static(&[b'x'; 512]));
        }

        assert!(
            cache.entries() <= 64,
            "entry cap was exceeded: {}",
            cache.entries()
        );
        assert!(
            cache.bytes() <= 64 * 1024,
            "byte cap was exceeded: {}",
            cache.bytes()
        );
        assert!(cache.evictions() > 0);
    }

    #[test]
    fn purge_empties_the_cache() {
        let cache = cache(60_000);
        for key in 0..10u64 {
            cache.insert(key, 200, vec![], Bytes::from_static(b"x"));
        }
        assert_eq!(cache.purge(), 10);
        assert_eq!(cache.entries(), 0);
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn responses_that_must_not_be_shared_are_refused() {
        let allowed = [200u16, 404];

        assert!(is_cacheable_response(
            200,
            &allowed,
            vec![header("content-type", "text/plain")].into_iter()
        ));

        assert!(
            !is_cacheable_response(
                200,
                &allowed,
                vec![header("cache-control", "no-store")].into_iter()
            ),
            "no-store must be honored"
        );
        assert!(
            !is_cacheable_response(
                200,
                &allowed,
                vec![header("cache-control", "private, max-age=60")].into_iter()
            ),
            "a private response belongs to one client"
        );
        assert!(
            !is_cacheable_response(
                200,
                &allowed,
                vec![header("set-cookie", "session=abc")].into_iter()
            ),
            "caching a Set-Cookie would hand a session to the next client"
        );
        assert!(
            !is_cacheable_response(200, &allowed, vec![header("vary", "*")].into_iter()),
            "Vary: * cannot be replayed"
        );
        assert!(
            !is_cacheable_response(500, &allowed, std::iter::empty()),
            "a status outside the allowed list is not cached"
        );
    }

    #[test]
    fn connection_headers_are_not_stored() {
        let stored = storable_headers(
            vec![
                header("content-type", "application/json"),
                header("connection", "keep-alive"),
                header("transfer-encoding", "chunked"),
                header("set-cookie", "a=b"),
            ]
            .into_iter(),
        );

        let names: Vec<&str> = stored.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, vec!["content-type"]);
    }
}
