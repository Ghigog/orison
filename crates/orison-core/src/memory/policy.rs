//! When to compact and when to distill.

/// The thresholds, all of them configuration.
#[derive(Debug, Clone, Copy)]
pub struct MemoryPolicy {
    /// Compact when a character's live transcript exceeds this share of the
    /// history budget the prompt allocates it.
    ///
    /// A share rather than a token count, because the budget is derived from
    /// the backend's real context window (B-1) and a literal here would go
    /// stale the moment a model changed. At 1.0 compaction fires exactly when
    /// the transcript stops fitting, which is late; the default leaves room
    /// for the compaction to run before the prompt starts dropping turns.
    pub compact_at_history_fraction: f32,
    /// Turns kept verbatim after a compaction. The short-term tier.
    pub keep_recent_entries: usize,
    /// Never summarise fewer than this many entries: a summary of two lines
    /// costs a model call and saves nothing.
    pub min_entries_to_summarise: usize,
    /// Distill into long-term memory once this many undistilled summaries
    /// have accumulated. `MemoryManager.LTM_THRESHOLD`.
    pub distill_after_summaries: usize,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            compact_at_history_fraction: 0.75,
            keep_recent_entries: 10,
            min_entries_to_summarise: 6,
            distill_after_summaries: 10,
        }
    }
}

impl MemoryPolicy {
    /// The token count at which compaction is due, given this turn's history
    /// allocation.
    pub fn compaction_threshold(&self, history_budget_tokens: usize) -> usize {
        ((history_budget_tokens as f32) * self.compact_at_history_fraction).max(0.0) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_threshold_tracks_the_budget_rather_than_a_literal() {
        let policy = MemoryPolicy::default();
        // A 3B model with a 4k window and an 8B with a 32k one do not get the
        // same threshold, which is the whole point of not writing `30` here.
        assert_eq!(policy.compaction_threshold(1000), 750);
        assert_eq!(policy.compaction_threshold(8000), 6000);
        assert_eq!(policy.compaction_threshold(0), 0);
    }
}
