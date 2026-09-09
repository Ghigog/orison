//! Measuring retrieval quality.
//!
//! Recall is what `eval_baseline.md` records. Precision is what it says is
//! missing and known bad: on `large`, `"what stopped the boundary war"`
//! retrieves the right note and 73 others, and the existing baseline cannot
//! tell you whether a change made that worse while keeping recall the same. So
//! both are computed here, always, and reported together.
//!
//! This is not the eval harness. Porting `eval/EvalRunner.tscn` is Phase 5's
//! job and building `orison-eval` this phase is explicitly out of scope; these
//! are the few functions the §3.4 tests need to assert on numbers rather than
//! on "did this print something plausible".

use crate::knowledge::EntityId;

use super::types::Scored;

/// The fraction of the relevant set that appears in the top `k`.
pub fn recall_at_k(hits: &[Scored], relevant: &[EntityId], k: usize) -> f32 {
    if relevant.is_empty() {
        return 1.0;
    }
    let found = hits
        .iter()
        .take(k)
        .filter(|h| relevant.contains(&h.id))
        .count();
    found as f32 / relevant.len() as f32
}

/// The fraction of the top `k` that is relevant.
///
/// Measured against `min(k, hits)` rather than `k`, so returning three correct
/// results out of a possible three does not score 0.6 for having been asked for
/// five.
pub fn precision_at_k(hits: &[Scored], relevant: &[EntityId], k: usize) -> f32 {
    let considered = hits.len().min(k);
    if considered == 0 {
        return 0.0;
    }
    let found = hits
        .iter()
        .take(k)
        .filter(|h| relevant.contains(&h.id))
        .count();
    found as f32 / considered as f32
}

/// Reciprocal rank of the first relevant result, or 0.0 if there is none.
///
/// Recall and precision at `k` are set measures: they cannot see a reordering
/// *within* the top `k`, which is exactly what a reranker does. Without this,
/// "the reranker changed nothing" and "the reranker moved the right answer from
/// ninth to first" are the same two numbers.
pub fn reciprocal_rank(hits: &[Scored], relevant: &[EntityId]) -> f32 {
    hits.iter()
        .position(|h| relevant.contains(&h.id))
        .map(|i| 1.0 / (i + 1) as f32)
        .unwrap_or(0.0)
}

/// Aggregated numbers for one pipeline configuration over one fixture.
#[derive(Debug, Clone, Default)]
pub struct QualityReport {
    pub label: String,
    pub queries: usize,
    pub recall: f32,
    pub precision: f32,
    /// Mean reciprocal rank. The measure that can see a reordering.
    pub mrr: f32,
    /// Queries that returned nothing at all. The headline as-found figure in
    /// `eval_baseline.md` was 13 of 14, so this is worth its own line.
    pub empty: usize,
    /// Per-query recall, in the fixture's own order, for reporting the two
    /// documented hard cases individually.
    pub per_query: Vec<(String, f32, f32)>,
}

impl QualityReport {
    pub fn accumulate(&mut self, query: &str, hits: &[Scored], relevant: &[EntityId], k: usize) {
        let r = recall_at_k(hits, relevant, k);
        let p = precision_at_k(hits, relevant, k);
        self.queries += 1;
        self.recall += r;
        self.precision += p;
        self.mrr += reciprocal_rank(&hits[..hits.len().min(k)], relevant);
        if hits.is_empty() {
            self.empty += 1;
        }
        self.per_query.push((query.to_string(), r, p));
    }

    pub fn finish(mut self) -> Self {
        if self.queries > 0 {
            self.recall /= self.queries as f32;
            self.precision /= self.queries as f32;
            self.mrr /= self.queries as f32;
        }
        self
    }

    pub fn recall_of(&self, query: &str) -> Option<f32> {
        self.per_query
            .iter()
            .find(|(q, _, _)| q == query)
            .map(|(_, r, _)| *r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(ids: &[&str]) -> Vec<Scored> {
        ids.iter()
            .map(|id| Scored {
                id: EntityId::from_stored(*id),
                score: 1.0,
            })
            .collect()
    }

    fn relevant(ids: &[&str]) -> Vec<EntityId> {
        ids.iter().map(|id| EntityId::from_stored(*id)).collect()
    }

    #[test]
    fn recall_counts_only_the_top_k() {
        let hits = hits(&["a", "b", "c", "d", "e", "target"]);
        assert_eq!(recall_at_k(&hits, &relevant(&["target"]), 5), 0.0);
        assert_eq!(recall_at_k(&hits, &relevant(&["target"]), 6), 1.0);
    }

    #[test]
    fn precision_is_measured_against_what_was_returned() {
        let hits = hits(&["target"]);
        // One result, and it is right: that is precision 1.0, not 0.2.
        assert_eq!(precision_at_k(&hits, &relevant(&["target"]), 5), 1.0);
    }

    #[test]
    fn reciprocal_rank_sees_a_reordering_that_recall_cannot() {
        let relevant = relevant(&["target"]);
        let ninth = hits(&["a", "b", "c", "d", "e", "f", "g", "h", "target"]);
        let first = hits(&["target", "a", "b", "c", "d", "e", "f", "g", "h"]);

        // Identical to both set measures at k = 9.
        assert_eq!(
            recall_at_k(&ninth, &relevant, 9),
            recall_at_k(&first, &relevant, 9)
        );
        assert_eq!(
            precision_at_k(&ninth, &relevant, 9),
            precision_at_k(&first, &relevant, 9)
        );
        // Not identical here, which is the point.
        assert!(reciprocal_rank(&first, &relevant) > reciprocal_rank(&ninth, &relevant));
    }

    #[test]
    fn an_empty_result_scores_zero_on_both_and_is_counted() {
        let mut report = QualityReport::default();
        report.accumulate("nothing", &[], &relevant(&["target"]), 5);
        let report = report.finish();
        assert_eq!(report.recall, 0.0);
        assert_eq!(report.precision, 0.0);
        assert_eq!(report.mrr, 0.0);
        assert_eq!(report.empty, 1);
    }
}
