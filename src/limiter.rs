//! Rate limiting and concurrency limiting (T108).
//!
//! The limiter has to be cheap enough to leave on permanently, so the hot path
//! is a hash, one short mutex on a shard, and integer arithmetic - no
//! allocation, and no lock shared across all keys.
//!
//! Buckets are kept in a fixed number of shards. Each shard holds plain values
//! rather than `Arc`s, so a lookup that finds an existing key touches one cache
//! line and returns.

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// Number of shards. A power of two so the shard index is a mask, and large
/// enough that unrelated keys rarely share a lock.
const SHARDS: usize = 64;

/// Tokens are tracked in thousandths so that rates below one request per
/// second, and fractional refills between calls, do not round away.
const MILLI: u64 = 1_000;

#[derive(Debug, Clone, Copy)]
struct Bucket {
    /// Available tokens, in thousandths.
    tokens_milli: u64,
    /// When the bucket was last refilled, in epoch milliseconds.
    last_refill_ms: u64,
}

/// Verdict for one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Denied; the value is how long the caller should wait, in seconds,
    /// rounded up and at least 1, for a `Retry-After` header.
    Deny {
        retry_after_s: u64,
    },
}

impl Decision {
    pub fn is_allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// A token bucket per key, sharded.
#[derive(Debug)]
pub struct RateLimiter {
    /// Sustained rate, in tokens per second.
    rate_per_s: u64,
    /// Maximum tokens a key can bank, which is what allows a burst.
    burst: u64,
    /// Entries idle for longer than this may be dropped.
    ttl_ms: u64,
    /// Upper bound on tracked keys, so a flood of unique keys cannot grow the
    /// map without limit.
    max_entries: usize,
    shards: Vec<Mutex<HashMap<u64, Bucket>>>,
    entries: AtomicUsize,
}

impl RateLimiter {
    pub fn new(rate_per_s: u64, burst: u64, ttl_ms: u64, max_entries: usize) -> Self {
        let shards = (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect();
        Self {
            rate_per_s: rate_per_s.max(1),
            burst: burst.max(1),
            ttl_ms: ttl_ms.max(1_000),
            max_entries: max_entries.max(SHARDS),
            shards,
            entries: AtomicUsize::new(0),
        }
    }

    /// Number of keys currently tracked.
    pub fn entries(&self) -> usize {
        self.entries.load(Ordering::Relaxed)
    }

    /// Takes one token for `key`, refilling first.
    pub fn check(&self, key: u64) -> Decision {
        self.check_at(key, now_epoch_ms())
    }

    /// Same as [`Self::check`] with an explicit clock, for tests.
    pub fn check_at(&self, key: u64, now_ms: u64) -> Decision {
        let shard_idx = (key as usize) & (SHARDS - 1);
        let mut shard = match self.shards[shard_idx].lock() {
            Ok(shard) => shard,
            // A poisoned shard means a panic while holding the lock. Rate
            // limiting is not worth failing requests over, so recover the data
            // and carry on.
            Err(poisoned) => poisoned.into_inner(),
        };

        let burst_milli = self.burst.saturating_mul(MILLI);
        let bucket = shard.entry(key).or_insert_with(|| {
            self.entries.fetch_add(1, Ordering::Relaxed);
            Bucket {
                tokens_milli: burst_milli,
                last_refill_ms: now_ms,
            }
        });

        // Refill for the time that passed, capped at the burst size.
        let elapsed_ms = now_ms.saturating_sub(bucket.last_refill_ms);
        if elapsed_ms > 0 {
            let refill = elapsed_ms.saturating_mul(self.rate_per_s);
            bucket.tokens_milli = bucket.tokens_milli.saturating_add(refill).min(burst_milli);
            bucket.last_refill_ms = now_ms;
        }

        let decision = if bucket.tokens_milli >= MILLI {
            bucket.tokens_milli -= MILLI;
            Decision::Allow
        } else {
            // Time until one whole token is available again.
            let missing_milli = MILLI - bucket.tokens_milli;
            let wait_ms = missing_milli.div_ceil(self.rate_per_s.max(1));
            Decision::Deny {
                retry_after_s: wait_ms.div_ceil(1_000).max(1),
            }
        };

        if shard.len() > self.max_entries / SHARDS {
            self.evict(&mut shard, now_ms);
        }

        decision
    }

    /// Drops idle entries, and if that is not enough, drops the least recently
    /// used ones until the shard is back within its share of `max_entries`.
    fn evict(&self, shard: &mut HashMap<u64, Bucket>, now_ms: u64) {
        let before = shard.len();
        shard.retain(|_, bucket| now_ms.saturating_sub(bucket.last_refill_ms) < self.ttl_ms);

        let limit = self.max_entries / SHARDS;
        if shard.len() > limit {
            // Keep the freshest entries: an attacker cycling through keys
            // should not be able to push out a legitimate client that is
            // actively being limited.
            let mut by_age: Vec<(u64, u64)> = shard
                .iter()
                .map(|(key, bucket)| (*key, bucket.last_refill_ms))
                .collect();
            by_age.sort_unstable_by_key(|(_, last)| *last);
            for (key, _) in by_age.into_iter().take(shard.len() - limit) {
                shard.remove(&key);
            }
        }

        let removed = before.saturating_sub(shard.len());
        if removed > 0 {
            self.entries.fetch_sub(removed, Ordering::Relaxed);
        }
    }
}

/// Counts requests in flight against a route, so a single route cannot occupy
/// every worker.
#[derive(Debug, Default)]
pub struct ConcurrencyLimiter {
    inflight: AtomicUsize,
}

impl ConcurrencyLimiter {
    /// Takes a slot when one is free. `max` of 0 means unlimited.
    pub fn try_acquire(&self, max: usize) -> bool {
        if max == 0 {
            return true;
        }
        let taken = self.inflight.fetch_add(1, Ordering::Relaxed);
        if taken < max {
            true
        } else {
            self.inflight.fetch_sub(1, Ordering::Relaxed);
            false
        }
    }

    pub fn release(&self) {
        let _ = self
            .inflight
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(1))
            });
    }

    pub fn inflight(&self) -> usize {
        self.inflight.load(Ordering::Relaxed)
    }
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

    #[test]
    fn a_burst_is_allowed_then_the_rate_applies() {
        // 10 requests per second, burst of 5.
        let limiter = RateLimiter::new(10, 5, 60_000, 1_000);
        let start = 1_000_000;

        for i in 0..5 {
            assert!(
                limiter.check_at(7, start).is_allowed(),
                "burst request {i} should be allowed"
            );
        }
        assert!(
            !limiter.check_at(7, start).is_allowed(),
            "the burst is spent, so this must be denied"
        );

        // 100ms later exactly one token has been refilled at 10/s.
        assert!(limiter.check_at(7, start + 100).is_allowed());
        assert!(!limiter.check_at(7, start + 100).is_allowed());
    }

    #[test]
    fn keys_are_limited_independently() {
        let limiter = RateLimiter::new(1, 1, 60_000, 1_000);
        let now = 5_000_000;

        assert!(limiter.check_at(1, now).is_allowed());
        assert!(!limiter.check_at(1, now).is_allowed());
        // A different key still has its own bucket.
        assert!(limiter.check_at(2, now).is_allowed());
    }

    #[test]
    fn a_denial_says_how_long_to_wait() {
        let limiter = RateLimiter::new(2, 1, 60_000, 1_000);
        let now = 9_000_000;

        assert!(limiter.check_at(3, now).is_allowed());
        match limiter.check_at(3, now) {
            Decision::Deny { retry_after_s } => assert_eq!(retry_after_s, 1),
            Decision::Allow => panic!("expected a denial"),
        }
    }

    #[test]
    fn tokens_never_bank_beyond_the_burst() {
        let limiter = RateLimiter::new(100, 3, 60_000, 1_000);
        let start = 2_000_000;

        // Idle for an hour: the bucket must still only hold `burst` tokens.
        assert!(limiter.check_at(11, start).is_allowed());
        let much_later = start + 3_600_000;
        for _ in 0..3 {
            assert!(limiter.check_at(11, much_later).is_allowed());
        }
        assert!(
            !limiter.check_at(11, much_later).is_allowed(),
            "an idle bucket must not bank an hour of tokens"
        );
    }

    #[test]
    fn memory_is_bounded_when_keys_never_repeat() {
        let max_entries = 640;
        let limiter = RateLimiter::new(10, 10, 60_000, max_entries);
        let now = 4_000_000;

        // Every key is unique, as it would be under a flood of random IPs.
        for key in 0..100_000u64 {
            limiter.check_at(key, now);
        }

        assert!(
            limiter.entries() <= max_entries,
            "the limiter tracked {} keys with a cap of {max_entries}",
            limiter.entries()
        );
    }

    #[test]
    fn idle_entries_are_dropped() {
        let limiter = RateLimiter::new(10, 10, 1_000, 640);
        let start = 8_000_000;

        for key in 0..200u64 {
            limiter.check_at(key, start);
        }
        let tracked = limiter.entries();
        assert!(tracked > 0);

        // Long after the TTL, touching enough keys triggers a sweep.
        for key in 1_000..1_200u64 {
            limiter.check_at(key, start + 10_000);
        }
        assert!(
            limiter.entries() <= tracked + 200,
            "stale entries were never reclaimed"
        );
    }

    #[test]
    fn concurrency_limiter_releases_slots() {
        let limiter = ConcurrencyLimiter::default();

        assert!(limiter.try_acquire(2));
        assert!(limiter.try_acquire(2));
        assert!(!limiter.try_acquire(2), "the third request must be refused");
        assert_eq!(limiter.inflight(), 2);

        limiter.release();
        assert!(limiter.try_acquire(2));

        // Unlimited when max is 0.
        let unlimited = ConcurrencyLimiter::default();
        for _ in 0..1_000 {
            assert!(unlimited.try_acquire(0));
        }
    }

    #[test]
    fn releasing_more_than_acquired_does_not_wrap() {
        let limiter = ConcurrencyLimiter::default();
        limiter.release();
        limiter.release();
        assert_eq!(limiter.inflight(), 0);
        assert!(limiter.try_acquire(1));
    }
}
