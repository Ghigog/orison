//! Dense ANN, via `sqlite-vec`.
//!
//! Replaces `EmbeddingStore.gd` (B-7): a `Dictionary` of every vector, cosined
//! against the query one at a time on every search, persisted as a single JSON
//! file rewritten in full. Fine at the few hundred nodes in `minimal` and
//! `messy`; `large` exists to break it, and a real personal vault is an order
//! of magnitude past `large`.
//!
//! Vectors come in already computed. This module deliberately does not know
//! how to make one: embedding is `InferenceBackend::embed`'s job (Phase 2), the
//! model behind it is configuration rather than a constant (B-10), and keeping
//! the index synchronous is what lets the rest of retrieval be tested without a
//! model running.

use rusqlite::{params, OptionalExtension};

use crate::knowledge::EntityId;
use crate::state::CampaignStore;

use super::error::RetrievalError;
use super::types::Scored;

/// What a stored vector belongs to. Chunks arrive in §3.6; entities are here
/// from the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorOwner {
    Entity,
    Chunk,
}

impl VectorOwner {
    fn as_str(self) -> &'static str {
        match self {
            VectorOwner::Entity => "entity",
            VectorOwner::Chunk => "chunk",
        }
    }
}

/// A `vec0` virtual table plus the mapping from its rowids back to entity ids.
pub struct DenseIndex<'a> {
    store: &'a CampaignStore,
    campaign_id: String,
    dim: usize,
}

impl std::fmt::Debug for DenseIndex<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DenseIndex")
            .field("campaign_id", &self.campaign_id)
            .field("dim", &self.dim)
            .finish_non_exhaustive()
    }
}

impl<'a> DenseIndex<'a> {
    /// Open, creating the vector table if this is the first time.
    ///
    /// `model` and `dim` are recorded and checked. Vectors from two different
    /// embedding models are not comparable, and comparing them anyway produces
    /// a plausible-looking ranking rather than an error, which is the worst
    /// available outcome.
    pub fn open(
        store: &'a CampaignStore,
        campaign_id: &str,
        model: &str,
        dim: usize,
    ) -> Result<Self, RetrievalError> {
        let conn = store.connection();
        let existing: Option<(String, i64)> = conn
            .query_row(
                "SELECT model, dim FROM embedding_meta WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| RetrievalError::Dense(e.into()))?;

        match existing {
            Some((stored_model, stored_dim)) => {
                if stored_model != model || stored_dim as usize != dim {
                    return Err(RetrievalError::EmbeddingModelMismatch {
                        stored: stored_model,
                        stored_dim: stored_dim as usize,
                        current: model.to_string(),
                        current_dim: dim,
                    });
                }
            }
            None => {
                conn.execute(
                    "INSERT INTO embedding_meta (id, model, dim) VALUES (1, ?1, ?2)",
                    params![model, dim as i64],
                )
                .map_err(|e| RetrievalError::Dense(e.into()))?;
                conn.execute_batch(&format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS embeddings USING vec0(embedding float[{dim}]);"
                ))
                .map_err(|e| RetrievalError::Dense(e.into()))?;
            }
        }

        Ok(Self {
            store,
            campaign_id: campaign_id.to_string(),
            dim,
        })
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Store one vector, replacing any vector already held for that owner.
    pub fn upsert(
        &self,
        owner: VectorOwner,
        ref_id: &str,
        vector: &[f32],
    ) -> Result<(), RetrievalError> {
        if vector.len() != self.dim {
            return Err(RetrievalError::DimensionMismatch {
                expected: self.dim,
                got: vector.len(),
            });
        }
        let conn = self.store.connection();
        conn.execute(
            "INSERT INTO embedding_owners (campaign_id, kind, ref_id) VALUES (?1, ?2, ?3)
             ON CONFLICT(campaign_id, kind, ref_id) DO NOTHING",
            params![self.campaign_id, owner.as_str(), ref_id],
        )
        .map_err(|e| RetrievalError::Dense(e.into()))?;
        let rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM embedding_owners
                 WHERE campaign_id = ?1 AND kind = ?2 AND ref_id = ?3",
                params![self.campaign_id, owner.as_str(), ref_id],
                |r| r.get(0),
            )
            .map_err(|e| RetrievalError::Dense(e.into()))?;

        conn.execute("DELETE FROM embeddings WHERE rowid = ?1", params![rowid])
            .map_err(|e| RetrievalError::Dense(e.into()))?;
        conn.execute(
            "INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)",
            params![rowid, to_bytes(vector)],
        )
        .map_err(|e| RetrievalError::Dense(e.into()))?;
        Ok(())
    }

    pub fn len(&self) -> Result<usize, RetrievalError> {
        let n: i64 = self
            .store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM embedding_owners WHERE campaign_id = ?1",
                params![self.campaign_id],
                |r| r.get(0),
            )
            .map_err(|e| RetrievalError::Dense(e.into()))?;
        Ok(n as usize)
    }

    pub fn is_empty(&self) -> Result<bool, RetrievalError> {
        Ok(self.len()? == 0)
    }

    /// k-nearest neighbours. Returns a similarity in [0, 1], converted from
    /// `sqlite-vec`'s L2 distance so callers never have to know which way round
    /// "better" is.
    pub fn search(
        &self,
        query: &[f32],
        k: usize,
        owner: VectorOwner,
    ) -> Result<Vec<Scored>, RetrievalError> {
        if query.len() != self.dim {
            return Err(RetrievalError::DimensionMismatch {
                expected: self.dim,
                got: query.len(),
            });
        }
        if k == 0 {
            return Ok(Vec::new());
        }
        let conn = self.store.connection();
        // Over-fetch: the `vec0` table is campaign-agnostic, so the join is
        // what scopes results, and it can discard some of them.
        let fetch = (k * 4).max(k + 16) as i64;
        let mut stmt = conn
            .prepare(
                "SELECT o.ref_id, e.distance
                 FROM embeddings e
                 JOIN embedding_owners o ON o.rowid = e.rowid
                 WHERE e.embedding MATCH ?1 AND k = ?2
                   AND o.campaign_id = ?3 AND o.kind = ?4
                 ORDER BY e.distance",
            )
            .map_err(|e| RetrievalError::Dense(e.into()))?;
        let rows = stmt
            .query_map(
                params![to_bytes(query), fetch, self.campaign_id, owner.as_str()],
                |r| {
                    let ref_id: String = r.get(0)?;
                    let distance: f64 = r.get(1)?;
                    Ok((ref_id, distance))
                },
            )
            .map_err(|e| RetrievalError::Dense(e.into()))?;

        let mut out = Vec::with_capacity(k);
        for row in rows {
            let (ref_id, distance) = row.map_err(|e| RetrievalError::Dense(e.into()))?;
            out.push(Scored {
                id: EntityId::from_stored(ref_id),
                score: 1.0 / (1.0 + distance as f32),
            });
            if out.len() >= k {
                break;
            }
        }
        Ok(out)
    }
}

fn to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Campaign;

    fn store() -> CampaignStore {
        let store = CampaignStore::open_in_memory().unwrap();
        store
            .save_campaign(&Campaign::new("c", "C", "2026-09-09T10:00:00Z"))
            .unwrap();
        store
    }

    #[test]
    fn nearest_neighbours_come_back_in_order() {
        // Fixed vectors, not embeddings: this proves the index works, not that
        // any particular model does. Quality needs a live model and is gated.
        let store = store();
        let index = DenseIndex::open(&store, "c", "test-model", 4).unwrap();
        index
            .upsert(VectorOwner::Entity, "near", &[1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .upsert(VectorOwner::Entity, "middle", &[0.8, 0.2, 0.0, 0.0])
            .unwrap();
        index
            .upsert(VectorOwner::Entity, "far", &[0.0, 1.0, 0.0, 0.0])
            .unwrap();

        let hits = index
            .search(&[1.0, 0.0, 0.0, 0.0], 3, VectorOwner::Entity)
            .unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["near", "middle", "far"]);
        assert!(hits[0].score > hits[1].score && hits[1].score > hits[2].score);
    }

    #[test]
    fn upserting_replaces_rather_than_duplicating() {
        let store = store();
        let index = DenseIndex::open(&store, "c", "test-model", 4).unwrap();
        index
            .upsert(VectorOwner::Entity, "a", &[1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .upsert(VectorOwner::Entity, "a", &[0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len().unwrap(), 1);

        let hits = index
            .search(&[0.0, 1.0, 0.0, 0.0], 5, VectorOwner::Entity)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].score > 0.99,
            "the newer vector should be the stored one"
        );
    }

    #[test]
    fn a_different_model_is_refused_rather_than_silently_compared() {
        let store = store();
        DenseIndex::open(&store, "c", "nomic-embed-text", 768).unwrap();
        let err = DenseIndex::open(&store, "c", "some-other-model", 768).unwrap_err();
        assert!(matches!(err, RetrievalError::EmbeddingModelMismatch { .. }));

        let err = DenseIndex::open(&store, "c", "nomic-embed-text", 384).unwrap_err();
        assert!(matches!(err, RetrievalError::EmbeddingModelMismatch { .. }));
    }

    #[test]
    fn a_wrong_sized_vector_is_an_error_not_a_silent_truncation() {
        let store = store();
        let index = DenseIndex::open(&store, "c", "test-model", 4).unwrap();
        let err = index
            .upsert(VectorOwner::Entity, "a", &[1.0, 0.0])
            .unwrap_err();
        assert!(matches!(
            err,
            RetrievalError::DimensionMismatch {
                expected: 4,
                got: 2
            }
        ));
    }

    #[test]
    fn entity_and_chunk_vectors_do_not_collide() {
        let store = store();
        let index = DenseIndex::open(&store, "c", "test-model", 4).unwrap();
        index
            .upsert(VectorOwner::Entity, "same_id", &[1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .upsert(VectorOwner::Chunk, "same_id", &[0.0, 1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len().unwrap(), 2);

        let hits = index
            .search(&[1.0, 0.0, 0.0, 0.0], 5, VectorOwner::Entity)
            .unwrap();
        assert_eq!(hits.len(), 1);
    }
}
