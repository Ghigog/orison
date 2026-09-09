//! Vault ingest (§3.2, §3.6).
//!
//! Ports `VaultCompiler.gd` (1,358 lines) and `MarkdownParser.gd`, behaviour
//! first. The heading heuristics, character-property inference, asset discovery
//! and writing-style extraction are all here; the structure is not, because a
//! line-by-line translation of that file would carry across the reason it was
//! hard to reason about.
//!
//! The governing rule, from `rag_architecture.md` §1.1 and from what B-14 cost:
//! **everything must be ingested**. A section that maps to no canonical field
//! goes to the overflow bucket. A wiki-link with no target is recorded, not
//! fatal. The complete source text is on every entity, always. If a heuristic
//! in this module misfires, the cost is retrieval quality, never data.

pub mod assets;
pub mod chunk;
pub mod classify;
pub mod error;
pub mod markdown;
pub mod pipeline;
pub mod scan;
pub mod sections;
pub mod style;
pub mod yaml;

pub use chunk::{chunk_entity, Chunk, ChunkConfig};
pub use error::IngestError;
pub use pipeline::{
    ingest_documents, ingest_vault, DanglingLink, IngestOptions, IngestOutcome, IngestReport,
};
pub use scan::VaultFiles;
