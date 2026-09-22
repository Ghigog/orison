//! Types crossing the Tauri IPC boundary that `orison-core` and `orison-cli`
//! don't already serialize themselves. `TurnEvent`, `TurnState`,
//! `DirectorState`, `FailureKind` and `CampaignSummary` derive `Serialize` in
//! `orison-core` directly, because they cross this boundary on every turn.
//! Everything here is a one-shot query/response value instead, so it gets a
//! thin DTO rather than pulling `serde` derives onto engine-internal types
//! like `Entity` and `IngestReport` that have no other reason to know they
//! are ever serialized.

use serde::{Deserialize, Serialize};

use orison_core::emotion::{emotion_str, EmotionState, Rapport};
use orison_core::inference::{HealthStatus, ModelHealth};
use orison_core::ingest::IngestReport;
use orison_core::knowledge::{Entity, EntityKind};
use orison_core::turn::GraphEdge;

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

/// One edge of the knowledge graph, both endpoints resolved to a label, for
/// the Map screen's drawn graph (#34). `kind` is `EdgeKind::as_str()` — the
/// same stored spelling ingest writes (`"connected_to"`, `"mentions"`, …), or
/// the author's own relationship wording when it's neither of those.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeDto {
    pub from_id: String,
    pub from_label: String,
    pub to_id: String,
    pub to_label: String,
    pub kind: String,
}

impl From<&GraphEdge> for EdgeDto {
    fn from(e: &GraphEdge) -> Self {
        Self {
            from_id: e.from_id.clone(),
            from_label: e.from_label.clone(),
            to_id: e.to_id.clone(),
            to_label: e.to_label.clone(),
            kind: e.kind.as_str().to_string(),
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

/// What the connect screen's per-field reachability badge needs. Kept as a
/// tagged enum rather than collapsed into one string so the frontend can
/// render "not pulled" (with the `ollama pull <model>` fix) distinctly from
/// "unreachable" — `HealthStatus` makes the same distinction on the Rust
/// side and it must survive the IPC boundary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ModelHealthStatusDto {
    Available,
    ModelNotInstalled,
    Unreachable { detail: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelHealthDto {
    pub model: String,
    pub status: ModelHealthStatusDto,
    pub context_length: Option<usize>,
}

impl From<ModelHealth> for ModelHealthDto {
    fn from(h: ModelHealth) -> Self {
        Self {
            model: h.model,
            status: match h.status {
                HealthStatus::Available => ModelHealthStatusDto::Available,
                HealthStatus::ModelNotInstalled => ModelHealthStatusDto::ModelNotInstalled,
                HealthStatus::Unreachable { detail } => {
                    ModelHealthStatusDto::Unreachable { detail }
                }
            },
            context_length: h.context_length,
        }
    }
}

/// One line of a campaign's stored transcript, for repainting the play
/// screen after the shell has navigated away from it.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryLineDto {
    /// `"player"`, `"character"`, `"narrator"` or `"system"`.
    pub role: String,
    pub text: String,
    pub sender: Option<String>,
}

/// A character's current emotional state and rapport, for the character
/// screen (#33) — one snapshot off `EmotionEngine`, no per-event history.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterEmotionDto {
    pub emotion: String,
    pub intensity: f32,
    pub target: String,
    pub reason: String,
    /// `-1.0..=1.0`.
    pub affinity: f64,
    /// The band `affinity` falls in, e.g. "Best Friend" — kept off the
    /// frontend so the [emotions.md] bands live in exactly one place.
    ///
    /// [emotions.md]: ../../../../../docs/emotions.md
    pub rapport_label: String,
    pub rapport_behaviour: String,
}

impl CharacterEmotionDto {
    pub fn new(state: &EmotionState, affinity: f64) -> Self {
        let rapport = Rapport::of(affinity);
        Self {
            emotion: emotion_str(state.emotion).to_string(),
            intensity: state.intensity,
            target: state.target.clone(),
            reason: state.reason.clone(),
            affinity,
            rapport_label: rapport.label().to_string(),
            rapport_behaviour: rapport.behaviour().to_string(),
        }
    }
}

/// One adventure-starter hook: a pickable opening the starters screen shows
/// between compile and play. Round-trips both ways — `generate_starters`
/// returns these, and the one the player picks comes back as the argument to
/// `pick_starter` — so it derives both `Serialize` and `Deserialize` rather
/// than needing a second shape for the pick.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarterDto {
    pub title: String,
    pub description: String,
    pub location_id: String,
    pub character_id: String,
    pub narration: String,
}

impl From<&orison_core::onboarding::Starter> for StarterDto {
    fn from(s: &orison_core::onboarding::Starter) -> Self {
        Self {
            title: s.title.clone(),
            description: s.description.clone(),
            location_id: s.location_id.clone(),
            character_id: s.character_id.clone(),
            narration: s.narration.clone(),
        }
    }
}

impl From<StarterDto> for orison_core::onboarding::Starter {
    fn from(s: StarterDto) -> Self {
        Self {
            title: s.title,
            description: s.description,
            location_id: s.location_id,
            character_id: s.character_id,
            narration: s.narration,
        }
    }
}

/// One folder of a vault that holds notes, with what ingest would call them
/// if the player said nothing.
#[derive(Debug, Clone, Serialize)]
pub struct VaultFolderDto {
    /// Vault-relative, `/`-separated; empty for notes at the vault root.
    pub folder: String,
    pub notes: usize,
    /// The folder-name heuristic's answer. A note's own `type:` frontmatter
    /// can still differ, so this is a guess, not a promise.
    pub guess: String,
}
