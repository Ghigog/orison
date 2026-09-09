//! Reciprocal rank fusion.
//!
//! `score(doc) = Σ weight_i / (k + rank_i)`, over each ranked list the doc
//! appears in. Rank-based rather than score-based, which is the point: BM25
//! scores and cosine similarities are on unrelated scales and normalising them
//! against each other requires a calibration nobody has.
//!
//! The Godot build had the right shape here (`retrieve_context` fused with
//! k=60) but nothing worth fusing: one list came from an inverted containment
//! test and the other only existed if `nomic-embed-text` happened to be
//! installed.

use std::collections::HashMap;

use crate::knowledge::EntityId;

use super::types::Scored;

/// The RRF smoothing constant. 60 is the value from the original paper and the
/// one the Godot build used; keeping it means the fusion behaviour is
/// comparable across the port.
pub const DEFAULT_RRF_K: f32 = 60.0;

/// One ranked list going into the fusion, and how much it counts.
pub struct RankedList<'a> {
    pub results: &'a [Scored],
    pub weight: f32,
}

/// Fuse ranked lists. Output is sorted by fused score, then by id, so equal
/// scores order deterministically rather than by hash iteration.
pub fn reciprocal_rank_fusion(lists: &[RankedList<'_>], k: f32) -> Vec<Scored> {
    let mut totals: HashMap<&EntityId, f32> = HashMap::new();
    for list in lists {
        for (i, hit) in list.results.iter().enumerate() {
            let rank = (i + 1) as f32;
            *totals.entry(&hit.id).or_insert(0.0) += list.weight / (k + rank);
        }
    }

    let mut fused: Vec<Scored> = totals
        .into_iter()
        .map(|(id, score)| Scored {
            id: id.clone(),
            score,
        })
        .collect();
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scored(ids: &[&str]) -> Vec<Scored> {
        ids.iter()
            .enumerate()
            .map(|(i, id)| Scored {
                id: EntityId::from_stored(*id),
                score: 1.0 - i as f32 * 0.1,
            })
            .collect()
    }

    #[test]
    fn a_document_ranked_by_both_lists_beats_one_ranked_first_by_either() {
        // The whole argument for fusing: agreement across two different notions
        // of relevance is stronger evidence than being top of one of them.
        let lexical = scored(&["a", "shared"]);
        let dense = scored(&["b", "shared"]);
        let fused = reciprocal_rank_fusion(
            &[
                RankedList {
                    results: &lexical,
                    weight: 1.0,
                },
                RankedList {
                    results: &dense,
                    weight: 1.0,
                },
            ],
            DEFAULT_RRF_K,
        );
        assert_eq!(fused[0].id.as_str(), "shared");
    }

    #[test]
    fn weights_shift_the_balance() {
        let lexical = scored(&["lex"]);
        let dense = scored(&["dense"]);
        let heavy_lexical = reciprocal_rank_fusion(
            &[
                RankedList {
                    results: &lexical,
                    weight: 3.0,
                },
                RankedList {
                    results: &dense,
                    weight: 1.0,
                },
            ],
            DEFAULT_RRF_K,
        );
        assert_eq!(heavy_lexical[0].id.as_str(), "lex");

        let heavy_dense = reciprocal_rank_fusion(
            &[
                RankedList {
                    results: &lexical,
                    weight: 1.0,
                },
                RankedList {
                    results: &dense,
                    weight: 3.0,
                },
            ],
            DEFAULT_RRF_K,
        );
        assert_eq!(heavy_dense[0].id.as_str(), "dense");
    }

    #[test]
    fn fusing_one_list_preserves_its_order() {
        let lexical = scored(&["a", "b", "c"]);
        let fused = reciprocal_rank_fusion(
            &[RankedList {
                results: &lexical,
                weight: 1.0,
            }],
            DEFAULT_RRF_K,
        );
        let ids: Vec<&str> = fused.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn ties_break_deterministically() {
        let one = scored(&["zeta"]);
        let two = scored(&["alpha"]);
        let a = reciprocal_rank_fusion(
            &[
                RankedList {
                    results: &one,
                    weight: 1.0,
                },
                RankedList {
                    results: &two,
                    weight: 1.0,
                },
            ],
            DEFAULT_RRF_K,
        );
        let b = reciprocal_rank_fusion(
            &[
                RankedList {
                    results: &two,
                    weight: 1.0,
                },
                RankedList {
                    results: &one,
                    weight: 1.0,
                },
            ],
            DEFAULT_RRF_K,
        );
        assert_eq!(a, b);
        assert_eq!(a[0].id.as_str(), "alpha");
    }
}
