//! Turning chunk hits back into notes, without losing which passage matched.
//!
//! Dense retrieval over chunks (§3.6) returns chunk ids. Two things need to
//! happen to them: the ranking has to collapse to one entry per note, because
//! three chunks of the same note are one answer rather than three, and the
//! chunk that actually matched has to survive that collapse, because it is the
//! provenance — the thing that eventually lets the game show a player *why* the
//! story knows something.

use crate::ingest::Chunk;
use crate::knowledge::EntityId;

use super::types::Scored;

/// A note, the score it earned, and the passage that earned it.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcedHit {
    pub id: EntityId,
    pub score: f32,
    /// The best-scoring chunk of this note, when the hit came from one.
    pub chunk_id: Option<String>,
}

/// Collapse chunk-level hits to note-level, keeping the best chunk of each.
///
/// Input order is preserved for equal scores, so this is stable against the
/// retriever's own ranking rather than reordering it.
pub fn fold_chunk_hits(hits: &[Scored]) -> Vec<SourcedHit> {
    let mut out: Vec<SourcedHit> = Vec::new();
    for hit in hits {
        let chunk_id = hit.id.as_str().to_string();
        let (entity, chunk) = match Chunk::source_note(&chunk_id) {
            Some(entity) => (entity, Some(chunk_id)),
            // Not a chunk id: an entity-level hit, passed through unchanged so
            // a mixed ranking does not have to be split before it gets here.
            None => (hit.id.clone(), None),
        };

        match out.iter_mut().find(|existing| existing.id == entity) {
            Some(existing) => {
                if hit.score > existing.score {
                    existing.score = hit.score;
                    existing.chunk_id = chunk;
                }
            }
            None => out.push(SourcedHit {
                id: entity,
                score: hit.score,
                chunk_id: chunk,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, score: f32) -> Scored {
        Scored {
            id: EntityId::from_stored(id),
            score,
        }
    }

    #[test]
    fn chunks_of_one_note_collapse_to_one_hit() {
        let folded = fold_chunk_hits(&[
            hit("the_quillion_accord#0", 0.9),
            hit("the_quillion_accord#2", 0.7),
            hit("on_debt_bondage#0", 0.5),
        ]);
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0].id, EntityId::from_stored("the_quillion_accord"));
        assert_eq!(folded[0].chunk_id.as_deref(), Some("the_quillion_accord#0"));
    }

    #[test]
    fn the_best_chunk_is_the_one_kept() {
        let folded =
            fold_chunk_hits(&[hit("note#0", 0.4), hit("note#3", 0.95), hit("note#1", 0.5)]);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].chunk_id.as_deref(), Some("note#3"));
        assert_eq!(folded[0].score, 0.95);
    }

    #[test]
    fn entity_level_hits_pass_through_with_no_provenance_claimed() {
        let folded = fold_chunk_hits(&[hit("saltmarsh_landing", 0.8)]);
        assert_eq!(folded[0].id, EntityId::from_stored("saltmarsh_landing"));
        assert_eq!(folded[0].chunk_id, None);
    }

    #[test]
    fn an_id_that_merely_contains_a_hash_is_not_a_chunk() {
        let folded = fold_chunk_hits(&[hit("weird#name", 0.8)]);
        assert_eq!(folded[0].id, EntityId::from_stored("weird#name"));
        assert_eq!(folded[0].chunk_id, None);
    }
}
