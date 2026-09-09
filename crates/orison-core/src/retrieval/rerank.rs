//! Reranking.
//!
//! ## What this is not
//!
//! The migration plan specifies a cross-encoder here, and a cross-encoder is a
//! model: it reads the query and one candidate together and scores the pair.
//! No such model was reachable from the environment this was built in — no
//! weights, no network path to fetch any — so shipping something *called* a
//! cross-encoder would repeat Phase 2's `LlamaCppBackend` situation, where a
//! dependency compiled cleanly and had never once run.
//!
//! So [`Reranker`] is a trait, and what ships behind it is
//! [`PassageReranker`]: model-free, measurable offline, and aimed at the one
//! failure a cross-encoder is being asked to fix here.
//!
//! ## The failure it targets
//!
//! `eval_baseline.md` records that on `large`, `"what stopped the boundary
//! war"` retrieves the right note and drags in 74 of 207 alongside it. That is
//! not a ranking subtlety; it is that a whole-note score rewards a long note
//! for mentioning a query term anywhere in it. `The Quillion Accord` is three
//! sentences and every one of them is about the query. A character file that
//! mentions "war" in passing is not.
//!
//! [`PassageReranker`] therefore scores the *best window* of a candidate rather
//! than the whole of it: how many distinct query terms appear inside a short
//! span, weighted by how rare each term is across the candidate set. A note
//! whose match is concentrated wins; a note that merely contains the words
//! somewhere loses. That is the same intuition a cross-encoder acts on,
//! arrived at without a model.
//!
//! Whether it earns its place is a measured question, not an assumed one. See
//! `tests/retrieval_quality.rs`, which reports the pipeline with and without
//! it.

use std::collections::{HashMap, HashSet};

use crate::knowledge::KnowledgeGraph;

use super::types::Scored;

/// Reorder candidates for a query.
pub trait Reranker {
    fn rerank(&self, query: &str, candidates: Vec<Scored>, graph: &KnowledgeGraph) -> Vec<Scored>;

    /// A short name for measurement output.
    fn name(&self) -> &'static str;
}

/// The control: leave the fused order alone. Not a placeholder — it is what
/// every "does the reranker help?" measurement is compared against.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoRerank;

impl Reranker for NoRerank {
    fn rerank(
        &self,
        _query: &str,
        candidates: Vec<Scored>,
        _graph: &KnowledgeGraph,
    ) -> Vec<Scored> {
        candidates
    }

    fn name(&self) -> &'static str {
        "none"
    }
}

/// Rescore by the best-matching passage rather than the whole note.
#[derive(Debug, Clone, Copy)]
pub struct PassageReranker {
    /// Window width in words. Roughly a long sentence: wide enough that a fact
    /// stated across a clause boundary still counts, narrow enough that two
    /// unrelated mentions in one note do not add up to a match.
    pub window: usize,
    /// How much the reranked score displaces the fused one. Fusion already
    /// carries real evidence, so this blends rather than replaces.
    pub weight: f32,
}

impl Default for PassageReranker {
    fn default() -> Self {
        Self {
            window: 40,
            weight: 0.7,
        }
    }
}

impl Reranker for PassageReranker {
    fn name(&self) -> &'static str {
        "passage"
    }

    fn rerank(&self, query: &str, candidates: Vec<Scored>, graph: &KnowledgeGraph) -> Vec<Scored> {
        let terms = terms_of(query);
        if terms.is_empty() || candidates.is_empty() {
            return candidates;
        }

        // Rarity across the candidate set, not the whole corpus: these are the
        // documents actually being told apart, so a term every candidate shares
        // discriminates nothing here whatever its corpus frequency.
        let mut containing: HashMap<&str, usize> = HashMap::new();
        let texts: Vec<Vec<String>> = candidates
            .iter()
            .map(|c| {
                let text = graph
                    .get(&c.id)
                    .map(|e| e.searchable_text())
                    .unwrap_or_default();
                words_of(&text)
            })
            .collect();
        for words in &texts {
            let present: HashSet<&String> = words.iter().collect();
            for term in &terms {
                if present.contains(term) {
                    *containing.entry(term.as_str()).or_insert(0) += 1;
                }
            }
        }
        let n = candidates.len() as f32;
        // Smoothed so that a term every candidate happens to share still counts
        // for something. An unsmoothed IDF goes to zero there, and "every
        // candidate contains both query terms" is the ordinary case after
        // fusion, not an edge one — it would leave every candidate scoring
        // identically and the rerank doing nothing at all.
        //
        // A term no candidate contains is dropped rather than weighted: it
        // cannot separate them, and counting it would only scale every score
        // down by the same factor.
        let rarity: HashMap<&str, f32> = terms
            .iter()
            .filter_map(|t| {
                let df = *containing.get(t.as_str()).unwrap_or(&0) as f32;
                if df == 0.0 {
                    return None;
                }
                Some((t.as_str(), ((n + 1.0) / (df + 0.5)).ln().max(0.05)))
            })
            .collect();

        let best = candidates
            .first()
            .map(|c| c.score)
            .filter(|s| *s > 0.0)
            .unwrap_or(1.0);

        let mut rescored: Vec<Scored> = candidates
            .into_iter()
            .zip(texts)
            .map(|(candidate, words)| {
                let passage = best_window_score(&words, &terms, &rarity, self.window);
                let fused = candidate.score / best;
                Scored {
                    id: candidate.id,
                    score: (1.0 - self.weight) * fused + self.weight * passage,
                }
            })
            .collect();

        rescored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        rescored
    }
}

/// The share of the query's rarity mass that the densest window covers.
fn best_window_score(
    words: &[String],
    terms: &[String],
    rarity: &HashMap<&str, f32>,
    window: usize,
) -> f32 {
    let total: f32 = terms.iter().filter_map(|t| rarity.get(t.as_str())).sum();
    if total <= 0.0 || words.is_empty() {
        return 0.0;
    }

    let mut best = 0.0f32;
    let mut start = 0usize;
    // A single pass with a sliding window: the covered mass is recomputed from
    // the window's distinct terms, which is cheap because a query has few.
    while start < words.len() {
        let end = (start + window).min(words.len());
        let present: HashSet<&str> = words[start..end].iter().map(String::as_str).collect();
        let covered: f32 = terms
            .iter()
            .filter(|t| present.contains(t.as_str()))
            .filter_map(|t| rarity.get(t.as_str()))
            .sum();
        best = best.max(covered / total);
        if end == words.len() {
            break;
        }
        start += window / 2;
    }
    best
}

fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Query terms, minus stopwords.
///
/// Stopwords matter as much here as they did in the Phase 1 lexical fix:
/// without them, "who keeps the archive" matches every note containing "the",
/// which is all of them.
fn terms_of(query: &str) -> Vec<String> {
    const STOPWORDS: [&str; 62] = [
        "a", "an", "and", "are", "as", "at", "be", "but", "by", "do", "does", "for", "from", "had",
        "has", "have", "he", "her", "his", "how", "i", "in", "is", "it", "its", "me", "my", "of",
        "on", "or", "our", "she", "so", "that", "the", "their", "them", "then", "there", "these",
        "they", "this", "to", "up", "was", "we", "were", "what", "when", "where", "which", "who",
        "whom", "why", "will", "with", "you", "your", "about", "did", "would", "could",
    ];
    let mut out: Vec<String> = Vec::new();
    for word in words_of(query) {
        if word.len() < 2 || STOPWORDS.contains(&word.as_str()) || out.contains(&word) {
            continue;
        }
        out.push(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{Entity, EntityId, EntityKind};

    fn graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();

        let mut focused = Entity::new(EntityId::slug("focused"), "Focused", EntityKind::Lore);
        focused.body = "The Accord ended the boundary war between two houses.".into();
        g.insert(focused);

        // Same terms, scattered across a long note. This is the `large`
        // precision failure in miniature: 74 of 207 notes look like this.
        let mut scattered = Entity::new(EntityId::slug("scattered"), "Scattered", EntityKind::Lore);
        let filler =
            "He walked the low road at dusk and thought of nothing much at all. ".repeat(12);
        scattered.body = format!("The boundary stone stands here. {filler} A war was fought elsewhere, long ago. {filler}");
        g.insert(scattered);

        g
    }

    #[test]
    fn a_concentrated_match_beats_a_scattered_one() {
        let graph = graph();
        let candidates = vec![
            Scored {
                id: EntityId::slug("scattered"),
                score: 1.0,
            },
            Scored {
                id: EntityId::slug("focused"),
                score: 0.9,
            },
        ];
        let reranked =
            PassageReranker::default().rerank("what stopped the boundary war", candidates, &graph);
        assert_eq!(
            reranked[0].id,
            EntityId::slug("focused"),
            "the note whose match is concentrated should win"
        );
    }

    #[test]
    fn the_control_changes_nothing() {
        let graph = graph();
        let candidates = vec![
            Scored {
                id: EntityId::slug("scattered"),
                score: 1.0,
            },
            Scored {
                id: EntityId::slug("focused"),
                score: 0.9,
            },
        ];
        let out = NoRerank.rerank("boundary war", candidates.clone(), &graph);
        assert_eq!(out, candidates);
    }

    #[test]
    fn a_query_of_nothing_but_stopwords_leaves_the_order_alone() {
        let graph = graph();
        let candidates = vec![
            Scored {
                id: EntityId::slug("scattered"),
                score: 1.0,
            },
            Scored {
                id: EntityId::slug("focused"),
                score: 0.9,
            },
        ];
        let out = PassageReranker::default().rerank("what is it", candidates.clone(), &graph);
        assert_eq!(out, candidates);
    }
}
