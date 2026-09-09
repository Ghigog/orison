//! The inference layer: a typed, role-separated replacement for the Godot
//! build's `/api/generate` + concatenated-string calls. See
//! `docs/migration_plan.md` §3.4 and the Phase 2 handoff for the defects
//! this closes (B-1 through B-6, B-8, B-10, B-15).

pub mod backend;
pub mod error;
pub mod ollama;
pub mod schema;
pub mod types;

pub use backend::InferenceBackend;
pub use error::InferenceError;
pub use ollama::OllamaBackend;
pub use types::{
    Capabilities, ChatDelta, ChatMessage, ChatRequest, ChatResponse, DoneReason, HealthStatus,
    KeepAlive, ModelHealth, ResponseFormat, Role, SamplingOptions, ToolCall, ToolDefinition,
};
