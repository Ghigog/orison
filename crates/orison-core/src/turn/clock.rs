//! Where "now" comes from.
//!
//! `orison-core` never reads a wall clock it cannot control: `Campaign::new`
//! already takes `now` as an argument for that reason. A turn writes several
//! timestamped rows, so passing the string down every call would be noise;
//! this is the same discipline as one injectable dependency.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The source of timestamps for rows a turn writes.
pub trait Clock: Send + Sync {
    /// Seconds since the Unix epoch.
    fn unix_seconds(&self) -> u64;

    /// An RFC 3339 / ISO 8601 UTC timestamp, the format the Godot save used
    /// (`Time.get_datetime_string_from_system(true)` plus a `Z`).
    fn timestamp(&self) -> String {
        format_rfc3339(self.unix_seconds())
    }
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// A clock that starts at a fixed instant and advances only when told to.
///
/// Tests that assert on ordering need timestamps that differ; tests that
/// assert on content need them not to. This gives both without a sleep.
#[derive(Debug)]
pub struct FixedClock {
    seconds: AtomicU64,
}

impl FixedClock {
    pub fn new(unix_seconds: u64) -> Self {
        Self {
            seconds: AtomicU64::new(unix_seconds),
        }
    }

    pub fn advance(&self, seconds: u64) {
        self.seconds.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Default for FixedClock {
    fn default() -> Self {
        // 2026-01-01T00:00:00Z, comfortably inside the range every branch of
        // `civil_from_days` is exercised by.
        Self::new(1_767_225_600)
    }
}

impl Clock for FixedClock {
    fn unix_seconds(&self) -> u64 {
        self.seconds.load(Ordering::SeqCst)
    }
}

/// Format seconds-since-epoch as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Hand-rolled rather than pulling in a date library for one function: the
/// civil-from-days conversion is Howard Hinnant's, which is the same algorithm
/// every such library uses, and the alternative is a dependency whose only use
/// in this crate would be this line.
fn format_rfc3339(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let secs_of_day = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    )
}

/// Days since 1970-01-01 to a civil (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_rfc3339(1_767_225_600), "2026-01-01T00:00:00Z");
        // A leap day, which is the case a hand-rolled conversion gets wrong.
        assert_eq!(format_rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(
            format_rfc3339(1_767_225_600 + 45_296),
            "2026-01-01T12:34:56Z"
        );
    }

    #[test]
    fn timestamps_sort_lexicographically_in_time_order() {
        // History and emotion rows are ordered by their `id`, but a reader
        // comparing timestamps must not get a different answer.
        let clock = FixedClock::default();
        let first = clock.timestamp();
        clock.advance(86_400 * 40);
        let later = clock.timestamp();
        assert!(first < later, "{first} should sort before {later}");
    }
}
