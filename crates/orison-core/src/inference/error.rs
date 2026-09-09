use std::time::Duration;

/// Every failure mode a caller of [`crate::inference::InferenceBackend`] must
/// handle. There is no success path that silently degrades: B-15 was an
/// unreachable model producing a 404 that never reached the caller, an
/// invisible `push_warning`, and a "generated successfully" message printed
/// anyway. In Rust that whole class of bug is a `Result` the caller cannot
/// ignore without an explicit `.unwrap()` or `#[allow]`.
#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    /// The backend's endpoint could not be reached at all (connection
    /// refused, DNS failure, TLS failure). This is the typed replacement for
    /// B-15: a dead model must surface here, not as an empty response.
    #[error("inference backend unreachable at {endpoint}: {source}")]
    Unreachable {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },

    /// The endpoint answered, but the requested model is not installed
    /// there. Distinct from `Unreachable` because the fix is different (pull
    /// the model, not check the server).
    #[error("model '{model}' is not available on this backend")]
    ModelNotFound { model: String },

    /// The backend returned a non-success status the caller can act on.
    #[error("backend returned {status}: {body}")]
    BackendError {
        status: reqwest::StatusCode,
        body: String,
    },

    /// The backend's reply could not be decoded as the expected shape. Only
    /// reachable when the backend does not honour schema-constrained
    /// decoding (§2.4); for `format: <schema>` requests this should be
    /// unreachable by construction.
    #[error("failed to deserialize backend response: {0}")]
    Decode(#[from] serde_json::Error),

    #[error("request timed out after {0:?}")]
    Timeout(Duration),

    /// A prompt exceeded its token budget. See `crate::prompt::budget`: this
    /// used to be a `push_warning` the model quietly absorbed by dropping the
    /// front of the context. Now it is a value the caller must handle.
    #[error("prompt of {measured} tokens exceeds the {limit} token budget")]
    BudgetExceeded { measured: usize, limit: usize },

    #[error("tokenizer error: {0}")]
    Tokenizer(String),

    /// A capability the caller asked for (e.g. native tool calling on
    /// `LlamaCppBackend`, which does not implement it yet) that this
    /// backend's `capabilities()` already reports as absent. Surfacing this
    /// as an error rather than silently ignoring `req.tools` is the same
    /// principle as B-15: a request that cannot be honoured must not look
    /// like one that succeeded.
    #[error("unsupported by this backend: {0}")]
    Unsupported(String),

    /// An internal `llama.cpp` failure (batch, decode, grammar compilation)
    /// with no more specific variant above.
    #[error("llama.cpp backend error: {0}")]
    Internal(String),

    #[error(transparent)]
    Http(#[from] reqwest::Error),
}
