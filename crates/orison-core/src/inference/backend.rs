use async_trait::async_trait;
use futures_core::stream::BoxStream;
use tokenizers::Tokenizer;

use super::error::InferenceError;
use super::types::{Capabilities, ChatDelta, ChatRequest, ChatResponse, ModelHealth};

/// The seam between `orison-core` and every model runtime it can talk to.
/// Two implementations exist from Phase 2: [`super::ollama::OllamaBackend`]
/// (desktop default, `/api/chat`) and [`super::llamacpp::LlamaCppBackend`]
/// (in-process, the only backend that could ever run on a phone). Both must
/// pass the same conformance suite in `tests/conformance.rs`; that shared
/// suite is as much the deliverable as either implementation.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// A single, non-streamed chat completion.
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, InferenceError>;

    /// The same request, streamed. Implementations must not couple
    /// throughput to any external polling loop (B-5: the Godot build drives
    /// HTTP streaming from `_process(delta)`, so throughput tracks frame
    /// rate and timeouts accumulate in frame deltas).
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatDelta, InferenceError>>, InferenceError>;

    /// Embeddings for a batch of texts, in the order given.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, InferenceError>;

    /// The model's real tokenizer. Backing `crate::prompt::budget` token
    /// counts, replacing the `length / 4` heuristic (B-4).
    fn tokenizer(&self) -> &Tokenizer;

    /// The context window actually served, queried from the backend. Never
    /// hardcoded: the Godot build hardcoded `4096 if character_model else
    /// 8192`, selected by string-comparing model names, and that mismatch
    /// against the window actually served is B-1.
    fn context_length(&self) -> usize;

    fn capabilities(&self) -> Capabilities;

    /// Typed, mandatory health check. An unreachable model or one that is
    /// not installed must come back as [`super::types::HealthStatus`], not
    /// as a silently empty response a caller can mistake for success
    /// (B-15).
    async fn health(&self) -> Result<ModelHealth, InferenceError>;
}
