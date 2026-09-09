//! Typed rows. Every one of these replaces an untyped `Dictionary` that the
//! Godot build read with `.get(key, default)`, where a missing key and a
//! present-but-wrong-typed key were indistinguishable at the call site.

use serde::{Deserialize, Serialize};

/// Campaign-level state: one row per playthrough.
///
/// The fields after `writing_style` are carried across from the Godot save
/// schema unchanged. Phase 3 stores them; Phase 4 owns the logic that reads
/// them. They are here rather than deferred because they are save state that
/// already exists, and a save that silently loses them is a regression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Campaign {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub last_played: String,
    pub active_scene: String,
    pub active_location: String,
    pub active_character: String,
    pub art_style: String,
    pub intro_narration: String,
    pub writing_style: String,
    pub playtime_seconds: f64,
    pub engine_version: String,
    pub player_character: Option<String>,
    pub memory_short_term: String,
    pub memory_medium_term: String,
    pub memory_long_term: String,
    /// Free-form JSON, as in the Godot save. Opaque to this layer.
    pub pending_scene: Option<String>,
    pub turns_since_last_director: i64,
    pub director_cooldown: i64,
    pub last_director_beat: String,
}

impl Campaign {
    /// A new campaign with the Godot build's defaults. `now` is passed in
    /// rather than read from the clock so tests are deterministic and so
    /// nothing in `orison-core` depends on a wall clock it cannot control.
    pub fn new(id: impl Into<String>, title: impl Into<String>, now: impl Into<String>) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            title: title.into(),
            created_at: now.clone(),
            last_played: now,
            active_scene: String::new(),
            active_location: String::new(),
            active_character: String::new(),
            art_style: "Digital Anime Art".to_string(),
            intro_narration: String::new(),
            writing_style: String::new(),
            playtime_seconds: 0.0,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            player_character: None,
            memory_short_term: String::new(),
            memory_medium_term: String::new(),
            memory_long_term: String::new(),
            pending_scene: None,
            turns_since_last_director: 0,
            director_cooldown: 0,
            last_director_beat: String::new(),
        }
    }
}

/// What `SaveManager.get_campaign_list()` needed, without reading and parsing
/// every save file on disk to get it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CampaignSummary {
    pub id: String,
    pub title: String,
    pub last_played: String,
    pub playtime_seconds: f64,
    pub active_scene: String,
}

/// Per-playthrough state for one character, keyed by its graph entity id.
///
/// Deliberately holds nothing the vault supplies: no name, no biography, no
/// appearance. Those come from the graph. If a field belongs in both, it
/// belongs in the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterState {
    pub entity_id: String,
    pub affinity: f64,
    pub base_emotion: String,
    pub base_intensity: f64,
    pub long_term_memory: String,
    pub turns_since_last_summary: i64,
}

impl CharacterState {
    pub fn new(entity_id: impl Into<String>) -> Self {
        Self {
            entity_id: entity_id.into(),
            affinity: 0.0,
            base_emotion: String::new(),
            base_intensity: -1.0,
            long_term_memory: String::new(),
            turns_since_last_summary: 0,
        }
    }
}

/// One entry in a character's emotional history.
///
/// The Godot build kept the last 20 per character and discarded the rest,
/// because they lived inside a JSON document that was rewritten whole on every
/// save. Rows are cheap; nothing is discarded here. Readers ask for what they
/// need with a limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmotionEvent {
    pub entity_id: String,
    pub timestamp: String,
    pub emotion: String,
    pub intensity: f64,
    pub target: String,
    pub context: String,
    pub rapport_delta: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub entity_id: String,
    pub item: String,
    pub quantity: i64,
    /// Free-form JSON object, as in the Godot save.
    pub properties: Option<String>,
}

/// Who said a line of transcript. `SaveManager` stored this as a bare string
/// and every reader compared it against a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRole {
    Player,
    Character,
    Narrator,
    System,
}

impl HistoryRole {
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryRole::Player => "player",
            HistoryRole::Character => "character",
            HistoryRole::Narrator => "narrator",
            HistoryRole::System => "system",
        }
    }

    /// Unknown roles map to `System` rather than being dropped: a transcript
    /// line with an unrecognised role is still a transcript line. Silent
    /// discard is the failure mode this whole phase is built against.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "player" => HistoryRole::Player,
            "character" => HistoryRole::Character,
            "narrator" => HistoryRole::Narrator,
            _ => HistoryRole::System,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub role: HistoryRole,
    pub content: String,
    pub timestamp: String,
    pub sender: Option<String>,
    pub active_character: Option<String>,
}

/// A knowledge-graph node as stored. The graph owns the meaning of these
/// fields; this layer only reads and writes the row. See [`crate::knowledge`].
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRow {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub description: String,
    pub level: i64,
    pub source_path: Option<String>,
    /// The full original text of the note. Never derived, never truncated:
    /// this is the B-14 fix as a schema constraint (`NOT NULL`) rather than a
    /// convention.
    pub body: String,
    /// JSON object: aliases, tags, canonical fields, overflow sections and
    /// frontmatter. Structured by `knowledge`, opaque here.
    pub properties: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeRow {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
    pub weight: f64,
}

/// A chunk as stored (§3.6).
///
/// `char_start` and `char_end` index into the source node's `body`. They are
/// the provenance: a retrieved passage can point at the sentence it came from,
/// not merely at the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkRow {
    pub id: String,
    pub entity_id: String,
    pub ordinal: i64,
    pub heading: Option<String>,
    pub text: String,
    pub char_start: i64,
    pub char_end: i64,
}

/// One medium-term summary of a stretch of conversation with a character
/// (§4.4).
///
/// `covers_from` and `covers_to` are `history_logs.id` bounds: the lines this
/// summary stands in for. They are the provenance, and they are why the
/// transcript can keep the originals rather than being rewritten without them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: i64,
    pub entity_id: String,
    pub summary: String,
    pub created_at: String,
    pub covers_from: i64,
    pub covers_to: i64,
    /// Whether this summary has already been folded into long-term memory.
    pub distilled: bool,
}
