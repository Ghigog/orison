//! The retrieval pipeline: metadata filter → BM25 and dense ANN in parallel →
//! reciprocal rank fusion → graph expansion → rerank → budget-aware truncation.
//!
//! Every stage is optional and separately measurable, because the handoff is
//! explicit that stages are earned rather than assumed: "add a stage only where
//! the number actually moves". `tests/retrieval_quality.rs` runs each
//! configuration against `ground_truth.json` and reports what each one did.

use crate::knowledge::{EntityId, KnowledgeGraph};

use super::bm25::LexicalIndex;
use super::dense::{DenseIndex, VectorOwner};
use super::error::RetrievalError;
use super::fusion::{reciprocal_rank_fusion, RankedList, DEFAULT_RRF_K};
use super::rerank::Reranker;
use super::types::{MetadataFilter, Scored};

/// How the pipeline is put together for one query.
#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    pub filter: MetadataFilter,
    /// Results to return.
    pub limit: usize,
    /// Candidates each retriever contributes before fusion. Larger than
    /// `limit` because fusion and reranking can only reorder what they are
    /// given.
    pub candidates: usize,
    pub bm25_weight: f32,
    pub dense_weight: f32,
    pub rrf_k: f32,
    /// Hops of graph expansion applied to the top results.
    ///
    /// `retrieve_context` expanded one degree from *every* match, which is half
    /// of why `large` returns 74 of 207 nodes for one query. Expanding from
    /// only the strongest few keeps the multi-hop reach without the flood.
    pub expand_hops: usize,
    /// How many top results seed the expansion.
    pub expand_from: usize,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            filter: MetadataFilter::default(),
            limit: 5,
            candidates: 32,
            bm25_weight: 1.0,
            dense_weight: 1.0,
            rrf_k: DEFAULT_RRF_K,
            expand_hops: 0,
            expand_from: 0,
        }
    }
}

/// What each stage contributed, for measurement and for debugging a surprising
/// result. Phase 1's lesson: prove the negative before believing it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageCounts {
    pub bm25: usize,
    pub dense: usize,
    pub fused: usize,
    pub expanded: usize,
    pub returned: usize,
}

#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub hits: Vec<Scored>,
    pub stages: StageCounts,
}

/// One query against the graph.
///
/// `query_vector` is `None` when no embedding model is configured or reachable.
/// That is a degraded pipeline, not a broken one — BM25 alone still answers —
/// and it is explicit in the signature rather than being an empty list that
/// silently contributes nothing, which is what the Godot build did.
#[allow(clippy::too_many_arguments)]
pub fn retrieve(
    query: &str,
    graph: &KnowledgeGraph,
    lexical: &LexicalIndex,
    dense: Option<(&DenseIndex<'_>, &[f32])>,
    reranker: &dyn Reranker,
    config: &RetrievalConfig,
) -> Result<RetrievalResult, RetrievalError> {
    let mut stages = StageCounts::default();

    let bm25 = lexical.search(query, config.candidates, &config.filter)?;
    stages.bm25 = bm25.len();

    let dense_hits = match dense {
        Some((index, vector)) => {
            let raw = index.search(vector, config.candidates, VectorOwner::Entity)?;
            // The vector index knows nothing about levels or kinds, so the
            // metadata filter is applied to its output rather than inside it.
            raw.into_iter()
                .filter(|hit| {
                    graph
                        .get(&hit.id)
                        .is_some_and(|e| config.filter.admits(e.level, e.kind))
                })
                .collect()
        }
        None => Vec::new(),
    };
    stages.dense = dense_hits.len();

    let mut lists = vec![RankedList {
        results: &bm25,
        weight: config.bm25_weight,
    }];
    if !dense_hits.is_empty() {
        lists.push(RankedList {
            results: &dense_hits,
            weight: config.dense_weight,
        });
    }
    let mut fused = reciprocal_rank_fusion(&lists, config.rrf_k);
    stages.fused = fused.len();

    if config.expand_hops > 0 && config.expand_from > 0 {
        let added = expand(graph, &mut fused, config);
        stages.expanded = added;
    }

    let reranked = reranker.rerank(query, fused, graph);
    let hits: Vec<Scored> = reranked.into_iter().take(config.limit).collect();
    stages.returned = hits.len();

    Ok(RetrievalResult { hits, stages })
}

/// Pull in the graph neighbourhood of the strongest results.
///
/// This is what reaches a fact that lives one note away from the one that
/// matched. `large`'s `"who keeps the accord"` is exactly this: the Accord is
/// retrieved on the word "accord", and the chapel that holds it says nothing
/// about accords at all. No amount of lexical or dense scoring gets there; only
/// the edge does.
///
/// A neighbour inherits its seed's score decayed by distance, so it competes
/// with the weak tail of the matched set rather than being appended below all
/// of it — appended entries never survive a `take(limit)` and the expansion
/// would be inert. It still cannot outrank its own seed.
///
/// Seeding from only the strongest few is the difference from
/// `retrieve_context`, which expanded one degree from *every* match. That is
/// half of why `large` returns 74 of 207 nodes for one query.
fn expand(graph: &KnowledgeGraph, fused: &mut Vec<Scored>, config: &RetrievalConfig) -> usize {
    /// Ranks of penalty per hop away from the seed.
    const HOP_RANK_PENALTY: usize = 2;

    // Scored on RRF's own scale, at a virtual rank just below the seed's.
    //
    // Scaling the seed's *score* instead does not work, and the reason is worth
    // recording: RRF deliberately throws magnitude away. On `large`, `"who
    // keeps the accord"` puts the Accord first with a BM25 score of 38.8
    // against 16.8 for the runner-up and 2.7 for the tail — and after fusion
    // all of them sit within a few ten-thousandths of each other. Any multiplier
    // below 1.0 therefore drops a neighbour beneath the entire tail, and the
    // expansion becomes inert. Ranks are what survived the fusion, so ranks are
    // what the expansion has to speak in.
    let seeds: Vec<EntityId> = fused
        .iter()
        .take(config.expand_from)
        .map(|s| s.id.clone())
        .collect();

    let mut added = 0;
    for (seed_rank, seed) in seeds.iter().enumerate() {
        let mut position_in_hop = 0usize;
        for (neighbour, hop) in graph.within_hops(seed, config.expand_hops) {
            if fused.iter().any(|s| s.id == neighbour) {
                continue;
            }
            let virtual_rank = seed_rank + 1 + hop * HOP_RANK_PENALTY + position_in_hop;
            position_in_hop += 1;
            fused.push(Scored {
                id: neighbour,
                score: config.bm25_weight / (config.rrf_k + virtual_rank as f32),
            });
            added += 1;
        }
    }
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    added
}

/// Format retrieved entities into prompt context, stopping before the budget.
///
/// `count_tokens` is passed in rather than assumed: the real one is
/// `crate::prompt::budget::count_tokens` with the backend's own tokenizer, and
/// nothing here may fall back to `length / 4` (B-4). Truncation drops whole
/// entities from the tail rather than cutting mid-entity, so what reaches the
/// model is a shorter list of complete facts instead of a complete list of
/// half-facts.
pub fn format_context(
    hits: &[Scored],
    graph: &KnowledgeGraph,
    max_tokens: usize,
    count_tokens: impl Fn(&str) -> usize,
) -> String {
    const HEADER: &str = "### Retrieved Memory & World Context:\n";
    let mut out = String::from(HEADER);
    let mut used = count_tokens(HEADER);
    let mut included: Vec<&EntityId> = Vec::new();

    for hit in hits {
        let Some(entity) = graph.get(&hit.id) else {
            continue;
        };
        let description = if entity.description.trim().is_empty() {
            "No description."
        } else {
            entity.description.trim()
        };
        let line = format!(
            "- **{}** ({}): {}\n",
            entity.label,
            entity.kind.as_str(),
            description
        );
        let cost = count_tokens(&line);
        if used + cost > max_tokens {
            break;
        }
        out.push_str(&line);
        used += cost;
        included.push(&entity.id);
    }

    // Relationships between the entities that made it in. Ported from
    // `retrieve_context`, which is the one part of that function worth keeping:
    // the edges are why a knowledge graph beats a list of documents.
    let mut relations = String::new();
    for edge in graph.edges() {
        if !included.contains(&&edge.from) || !included.contains(&&edge.to) {
            continue;
        }
        let (Some(from), Some(to)) = (graph.get(&edge.from), graph.get(&edge.to)) else {
            continue;
        };
        relations.push_str(&format!(
            "  - Relation: {} is [{}] -> {}\n",
            from.label,
            edge.kind.as_str(),
            to.label
        ));
    }
    if !relations.is_empty() {
        let block = format!("\nRelationships:\n{relations}");
        if used + count_tokens(&block) <= max_tokens {
            out.push_str(&block);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{Edge, EdgeKind, Entity, EntityKind};
    use crate::retrieval::rerank::NoRerank;

    fn graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        for (name, body) in [
            ("The Quillion Accord", "The Quillion Accord ended the boundary war. Its sole surviving copy is held at Pale Reach Chapel."),
            ("Pale Reach Chapel", "A half-abandoned place in the Pale Reach. It floods in spring."),
            ("On Debt Bondage", "A person who cannot pay may sell a year of labour."),
        ] {
            let mut e = Entity::new(EntityId::slug(name), name, EntityKind::Lore);
            e.body = body.to_string();
            e.description = body.to_string();
            g.insert(e);
        }
        g.connect(Edge {
            from: EntityId::slug("The Quillion Accord"),
            to: EntityId::slug("Pale Reach Chapel"),
            kind: EdgeKind::AssociatedWith,
            weight: 1.0,
        });
        g
    }

    /// A whitespace count, for tests only. The production counter is the
    /// model's own tokenizer; character division is the defect B-4 names.
    fn words(text: &str) -> usize {
        text.split_whitespace().count()
    }

    #[test]
    fn bm25_alone_answers_without_a_model() {
        let g = graph();
        let lexical = LexicalIndex::build(&g).unwrap();
        let out = retrieve(
            "what stopped the boundary war",
            &g,
            &lexical,
            None,
            &NoRerank,
            &RetrievalConfig {
                filter: MetadataFilter::everything(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out.hits[0].id, EntityId::slug("The Quillion Accord"));
        assert_eq!(out.stages.dense, 0, "no vector was supplied");
        assert!(out.stages.bm25 > 0);
    }

    #[test]
    fn expansion_reaches_the_neighbour_and_never_outranks_the_match() {
        let g = graph();
        let lexical = LexicalIndex::build(&g).unwrap();
        let out = retrieve(
            "Quillion",
            &g,
            &lexical,
            None,
            &NoRerank,
            &RetrievalConfig {
                filter: MetadataFilter::everything(),
                expand_hops: 1,
                expand_from: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let ids: Vec<&str> = out.hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids[0], "the_quillion_accord");
        assert!(ids.contains(&"pale_reach_chapel"));
        assert!(out.stages.expanded > 0);
    }

    #[test]
    fn context_stops_before_the_budget_rather_than_after_it() {
        let g = graph();
        let hits: Vec<Scored> = [
            "The Quillion Accord",
            "Pale Reach Chapel",
            "On Debt Bondage",
        ]
        .iter()
        .map(|n| Scored {
            id: EntityId::slug(n),
            score: 1.0,
        })
        .collect();

        let full = format_context(&hits, &g, 10_000, words);
        assert!(full.contains("Quillion"));
        assert!(full.contains("Relationships:"));

        // Enough for the header and the first entity, not the second.
        let clipped = format_context(&hits, &g, 35, words);
        assert!(words(&clipped) <= 35, "budget exceeded: {clipped}");
        assert!(
            clipped.contains("Quillion"),
            "the strongest hit should survive"
        );
        assert!(
            !clipped.contains("Debt Bondage"),
            "the tail should be dropped whole"
        );
    }

    #[test]
    fn a_budget_of_nothing_yields_only_the_header() {
        let g = graph();
        let hits = vec![Scored {
            id: EntityId::slug("On Debt Bondage"),
            score: 1.0,
        }];
        let out = format_context(&hits, &g, 1, words);
        assert!(out.starts_with("### Retrieved"));
        assert!(!out.contains("Debt Bondage"));
    }
}
