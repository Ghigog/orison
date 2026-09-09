//! Hybrid retrieval (§3.4).
//!
//! Metadata filter → BM25 (`tantivy`) and dense ANN (`sqlite-vec`) in parallel
//! → reciprocal rank fusion → graph expansion → rerank → budget-aware
//! truncation.
//!
//! **Why hybrid, for this corpus specifically.** A personal worldbuilding vault
//! is almost entirely rare proper nouns: invented character, place and faction
//! names. Dense embeddings are structurally weak on exactly that (B-9) — a name
//! the model has never seen does not cluster near anything useful — while BM25
//! handles rare-token exact match natively. The `large` fixture makes the
//! argument in two queries against the same note: `"Quillion"` (one occurrence
//! in 207 files, trivial for BM25) and `"what stopped the boundary war"`
//! (paraphrase, dense-favourable).
//!
//! Nothing in this module holds entities. Every stage passes [`EntityId`]s and
//! resolves them against the graph, which is the only entity store (§3.3).
//!
//! [`EntityId`]: crate::knowledge::EntityId

pub mod bm25;
pub mod dense;
pub mod error;
pub mod eval;
pub mod fusion;
pub mod pipeline;
pub mod rerank;
pub mod types;

pub use bm25::LexicalIndex;
pub use dense::{DenseIndex, VectorOwner};
pub use error::RetrievalError;
pub use eval::{precision_at_k, recall_at_k, reciprocal_rank, QualityReport};
pub use fusion::{reciprocal_rank_fusion, RankedList, DEFAULT_RRF_K};
pub use pipeline::{format_context, retrieve, RetrievalConfig, RetrievalResult, StageCounts};
pub use rerank::{NoRerank, PassageReranker, Reranker};
pub use types::{MetadataFilter, Scored};
