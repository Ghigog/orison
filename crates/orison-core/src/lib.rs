//! orison-core: the Orison engine. No UI, no platform assumptions.
//!
//! `orison-core` never knows what is rendering it: it exposes typed APIs and
//! emits events, and never imports a shell toolkit. See
//! `docs/migration_plan.md` §3.1 in the repository root.

pub mod inference;
pub mod prompt;

// Phase 3: ingest, knowledge, retrieval, state.
// Phase 4: turn, memory.
pub mod emotion;
pub mod ingest;
pub mod knowledge;
pub mod memory;
pub mod retrieval;
pub mod state;
pub mod turn;

/// A stand-in Ollama and the fixture response bodies, for tests in this crate
/// and in `orison-cli`. Off by default; nothing here reaches a release build.
#[cfg(feature = "test-support")]
pub mod testing;
