//! Retrieval errors.

#[derive(Debug, thiserror::Error)]
pub enum RetrievalError {
    #[error("lexical index: {0}")]
    Lexical(String),

    #[error("dense index: {0}")]
    Dense(#[from] crate::state::StateError),

    /// The stored vectors were written by a different embedding model, or at a
    /// different dimensionality. Comparing them would silently produce
    /// nonsense, so it is refused. Model identifiers are configuration
    /// (B-10), which means they can change under a database and this has to be
    /// checked rather than assumed.
    #[error("embeddings were written by model '{stored}' at dim {stored_dim}, but this run uses '{current}' at dim {current_dim}")]
    EmbeddingModelMismatch {
        stored: String,
        stored_dim: usize,
        current: String,
        current_dim: usize,
    },

    #[error("a query vector of {got} dimensions cannot search an index of {expected}")]
    DimensionMismatch { expected: usize, got: usize },
}

impl From<tantivy::TantivyError> for RetrievalError {
    fn from(e: tantivy::TantivyError) -> Self {
        RetrievalError::Lexical(e.to_string())
    }
}

impl From<tantivy::query::QueryParserError> for RetrievalError {
    fn from(e: tantivy::query::QueryParserError) -> Self {
        RetrievalError::Lexical(e.to_string())
    }
}
