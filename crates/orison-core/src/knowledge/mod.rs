//! The knowledge graph: the only entity store (§3.3).
//!
//! This module owns what an entity *is*. Ingest (§3.2) produces these types,
//! retrieval (§3.4) returns ids that resolve here, and nothing anywhere else in
//! `orison-core` keeps its own map of entities. `orison_audit.md` §9 records
//! what happens when it does: `VaultCompiler` and `CampaignState` each kept
//! their own dictionaries and bypassed the graph, giving three sources of truth
//! that could and did disagree.

pub mod index;
pub mod types;

pub use index::NameIndex;
pub use types::{CanonicalField, Edge, EdgeKind, Entity, EntityId, EntityKind, OverflowSection};
