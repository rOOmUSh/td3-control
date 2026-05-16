//! Dependency-free time-sortable ID generator.
//!
//! Format: `<prefix>_<timestamp_ms>_<counter>_<pseudo_rand>`.
//!
//! The timestamp gives lexicographic-by-time sortability (useful when
//! displayed in a filtered list). The per-process counter prevents collisions
//! when two IDs are minted in the same millisecond. The pseudo-random suffix
//! further reduces collision risk if two processes happen to mint on the same
//! millisecond and counter wraps.
//!
//! This is deliberately not a ULID or a UUID - we want to keep this helper
//! dependency-free rather than pull in `ulid`, `uuid`, or `rand` for what is
//! effectively a catalog primary-key generator.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a new ID with the given prefix (e.g. `"item"`, `"snap"`, `"tag"`).
pub fn new_id(prefix: &str) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let rand = pseudo_rand(now_ms, counter);
    format!(
        "{}_{:013}_{:06}_{:08x}",
        prefix,
        now_ms,
        counter & 0xFFFFFF,
        rand
    )
}

/// Derive a 32-bit pseudo-random value from the time and counter.
/// Not cryptographic; only used to make collisions statistically unlikely.
fn pseudo_rand(seed_a: u64, seed_b: u64) -> u32 {
    let mut x = seed_a ^ seed_b.wrapping_mul(0x9E3779B97F4A7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51AFD7ED558CCD);
    x ^= x >> 33;
    x = x.wrapping_mul(0xC4CEB9FE1A85EC53);
    x ^= x >> 33;
    x as u32
}
