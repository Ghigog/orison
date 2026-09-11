//! Types crossing the Tauri IPC boundary that `orison-core` and `orison-cli`
//! don't already serialize themselves. `TurnEvent`, `TurnState`,
//! `DirectorState`, `FailureKind` and `CampaignSummary` derive `Serialize` in
//! `orison-core` directly, because they cross this boundary on every turn.
//! Everything here is a one-shot query/response value instead, so it gets a
//! thin DTO rather than pulling `serde` derives onto engine-internal types
//! like `Entity` and `IngestReport` that have no other reason to know they
//! are ever serialized.

use serde::{Deserialize, Serialize};

use orison_core::ingest::IngestReport;
use orison_core::knowledge::{Entity, EntityKind};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityDto {
    pub id: String,
    pub label: String,
    pub kind: EntityKind,
    pub description: String,
}

impl From<&Entity> for EntityDto {
    fn from(e: &Entity) -> Self {
        Self {
            id: e.id.as_str().to_string(),
            label: e.label.clone(),
            kind: e.kind,
            description: e.description.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestReportDto {
    pub notes_seen: usize,
    pub sections_mapped: usize,
    pub sections_overflowed: usize,
    pub chunks: usize,
    pub notes_without_a_type: usize,
    pub unaccounted_sections: Vec<String>,
    pub dangling_link_count: usize,
    pub gender_conflict_count: usize,
}

impl From<&IngestReport> for IngestReportDto {
    fn from(r: &IngestReport) -> Self {
        Self {
            notes_seen: r.notes_seen,
            sections_mapped: r.sections_mapped,
            sections_overflowed: r.sections_overflowed,
            chunks: r.chunks,
            notes_without_a_type: r.notes_without_a_type,
            unaccounted_sections: r.unaccounted_sections.clone(),
            dangling_link_count: r.dangling_links.len(),
            gender_conflict_count: r.gender_conflicts.len(),
        }
    }
}

/// What the Models screen needs to fill in and submit. Mirrors
/// `orison_cli::config::ModelArgs` field-for-field; kept separate because
/// that struct derives `clap::Args`, not `Deserialize`, and its shape is
/// CLI-flag-shaped (a bare `--profile` value) rather than IPC-shaped.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArgsDto {
    pub url: String,
    pub actor_model: String,
    pub director_model: Option<String>,
    /// `true` for the Director+Actor split, `false` for a single call.
    /// See `orison_cli::config::Profile` — the two-arm choice the Models
    /// screen's profile toggle presents.
    pub two_calls: bool,
    pub context_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsSummaryDto {
    pub summary: String,
    pub context_length: usize,
    pub tokenizer_is_approximate: bool,
}
