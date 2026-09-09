//! The memory tiers (§4.4).
//!
//! `MemoryManager.gd`'s three tiers, per [rag_architecture.md §1.5]:
//!
//! | Tier | What it is | Where it lives |
//! |---|---|---|
//! | Short-term | The last N turns, verbatim | `history_logs`, uncompacted |
//! | Medium-term | Summaries of stretches of conversation | `session_summaries` |
//! | Long-term | One distilled narrative per character | `characters.long_term_memory` |
//!
//! **Biography is not a tier.** §1.5 is emphatic and the handoff repeats it,
//! so it is worth being concrete about what the separation means here: a
//! character's biography is a `CanonicalField` on a graph [`Entity`], written
//! by ingest, and nothing in this module reads or writes it. What the
//! character did *before* the adventure and what they did *in your game* are
//! different questions with different answers, and conflating them is how an
//! NPC ends up remembering things that never happened to them. The two never
//! meet in a struct, and they reach the prompt as separate blocks.
//!
//! **Compaction is by tokens, not turns.** `COMPACTION_THRESHOLD = 30` was a
//! turn count standing in for a size, from a build whose only measure of size
//! was `length / 4` (B-4). Thirty turns of terse exchanges and thirty turns of
//! paragraphs are not the same amount of context, and the thing that actually
//! overflows is the budget. Phase 2 made real counts available, so the trigger
//! is a real count.
//!
//! [rag_architecture.md §1.5]: ../../../../docs/rag_architecture.md
//! [`Entity`]: crate::knowledge::Entity

pub mod manager;
pub mod policy;

pub use manager::{CompactionPlan, DistillationPlan, MemoryManager, SessionMemory};
pub use policy::MemoryPolicy;
