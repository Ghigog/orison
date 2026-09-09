//! The SQLite state layer (§3.1).
//!
//! Replaces `SaveManager.gd` (whole-document JSON load and rewrite on every
//! save) and `CampaignState.gd` (one `Dictionary` per concern, no queries, and
//! a hand-rolled `Mutex` guarding every read and write because there was no
//! other concurrency story).
//!
//! Three things change, and each removes a workaround rather than adding a
//! feature:
//!
//! - **Saves are incremental.** `SaveManager.save_campaign()` serialised the
//!   entire campaign document — knowledge graph included — to a temporary file
//!   and renamed it over the old one, for every mutation. Here a mutation is a
//!   statement.
//! - **State is queryable.** `CampaignState` had no way to ask a question of
//!   its own data, so every reader kept a dictionary of its own. That is the
//!   direct cause of the parallel-entity-store defect §3.3 exists to close.
//! - **Concurrency is the database's problem.** `CampaignState._mutex`
//!   serialised all state access in-process. WAL plus a busy timeout does the
//!   same job across processes and without the deadlock risk that made every
//!   signal in `apply_state_change()` fire outside the lock.
//!
//! ## What lives here and what does not
//!
//! This module stores *campaign* state: what a playthrough did. What the vault
//! *says* — entities, their fields, their relationships — belongs to
//! [`crate::knowledge`], which is the only entity store (§3.3). The
//! `knowledge_nodes` / `knowledge_edges` tables are that graph's persistence,
//! written and read through `knowledge`, never queried for entity data from
//! anywhere else.
//!
//! The `characters` table is the one that invites confusion, so to be explicit:
//! it holds *play state* keyed by a graph entity id (affinity, base emotion,
//! memory tiers). It is not a second copy of the character. The Godot build
//! kept both in one `properties` dictionary on the graph node, which is why
//! "recompile the vault" and "keep my save" were in tension there.

pub mod error;
pub mod schema;
pub mod store;
pub mod types;

pub use error::StateError;
pub use schema::{open_at_version, open_connection, schema_version, CURRENT_SCHEMA_VERSION};
pub use store::CampaignStore;
pub use types::{
    Campaign, CampaignSummary, CharacterState, ChunkRow, EdgeRow, EmotionEvent, HistoryEntry,
    HistoryRole, InventoryItem, NodeRow, SessionSummary,
};
