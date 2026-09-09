//! Shared retrieval types.

use crate::knowledge::{EntityId, EntityKind};

/// One retrieved entity and the score that put it there.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub id: EntityId,
    pub score: f32,
}

/// Which slice of the graph a query is allowed to see.
///
/// Ports the `target_level` argument of `KnowledgeGraphManager.retrieve_context`,
/// which is how the Director gets campaign-level summaries and the Actor gets
/// concrete facts (`rag_architecture.md` §1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataFilter {
    /// `None` means every level. `Some(0)` is source notes only, which is what
    /// the character agent wants; `Some(1)` and `Some(2)` are RAPTOR summaries.
    pub level: Option<u8>,
    /// Empty means every kind.
    pub kinds: Vec<EntityKind>,
}

impl Default for MetadataFilter {
    fn default() -> Self {
        Self {
            level: Some(0),
            kinds: Vec::new(),
        }
    }
}

impl MetadataFilter {
    pub fn everything() -> Self {
        Self {
            level: None,
            kinds: Vec::new(),
        }
    }

    pub fn level(level: u8) -> Self {
        Self {
            level: Some(level),
            kinds: Vec::new(),
        }
    }

    pub fn with_kinds(mut self, kinds: impl IntoIterator<Item = EntityKind>) -> Self {
        self.kinds = kinds.into_iter().collect();
        self
    }

    pub fn admits(&self, level: u8, kind: EntityKind) -> bool {
        if let Some(want) = self.level {
            if level != want {
                return false;
            }
        }
        self.kinds.is_empty() || self.kinds.contains(&kind)
    }
}
