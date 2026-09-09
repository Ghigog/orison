//! Schema-constrained response types (§2.4).
//!
//! Every model response Orison parses becomes a plain
//! `#[derive(Deserialize, JsonSchema)]` struct here. The schema sent to the
//! backend's structured-output facility (via
//! `crate::inference::ResponseFormat::for_type`) is generated from the same
//! type that later deserialises the response, so drift between what was
//! asked for and what gets parsed cannot happen.
//!
//! **`JsonRepair.gd` (152 lines) is not ported.** It existed only to clean
//! up the legacy `format: "json"` flag (B-3), which guarantees syntactic
//! JSON but not fields, types, or enum values — and to work around B-16,
//! where the extraction path used a stricter parser with no repair at all
//! and every character response failed because the model wrapped its JSON
//! in ` ```json ` fences. Constrained decoding makes both failure modes
//! impossible: every `parsing_failed` branch downstream of these types
//! disappears.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Emotion {
    Serenity,
    Joy,
    Sadness,
    Anger,
    Fear,
    Trust,
    Disgust,
    Surprise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EscalationSignal {
    SceneChange,
    Combat,
    Revelation,
    None,
}

/// A character or Director's emotional reaction to an event. Bounds mirror
/// `SystemPrompts.gd`: intensity 0.0-1.0, rapport delta -0.2..0.2. The
/// schema constrains the *shape*; a caller still clamps the numeric ranges,
/// since JSON Schema `minimum`/`maximum` are advisory for most decoders.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmotionalUpdate {
    pub emotion: Emotion,
    pub intensity: f32,
    pub reason: String,
    pub rapport_delta: f32,
}

/// The Character Agent's response. Ported from the JSON schema embedded in
/// `SystemPrompts.get_character_agent_prompt()`, minus the prose asking for
/// it: the sampler enforces the shape now, so the prompt no longer needs to
/// beg for it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CharacterResponse {
    /// Mandatory reasoning step: what the player did, what's happening, and
    /// what the logical next step is.
    pub thinking: String,
    /// Objective third-person narration of any environmental event or
    /// action outcome. May be empty.
    pub narration: String,
    /// The character's first-person spoken response and minor parenthetical
    /// actions. Empty, or gesture-only, for a non-speaking creature.
    pub dialogue: String,
    pub emotional_update: EmotionalUpdate,
    pub escalation_signal: EscalationSignal,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryUpdates {
    pub short_term: String,
    pub medium_term: String,
    pub long_term: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InventoryAction {
    Add,
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InventoryUpdate {
    pub item_id: String,
    pub action: InventoryAction,
    pub quantity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceType {
    Say,
    Do,
    Story,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Choice {
    pub text: String,
    #[serde(rename = "type")]
    pub kind: ChoiceType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Ability {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiceRoll {
    pub required: bool,
    #[serde(default)]
    pub ability: Option<Ability>,
    #[serde(default)]
    pub dc: Option<u8>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// The Director/DM's response. Ported from the JSON schema embedded in
/// `SystemPrompts.get_world_builder_prompt()`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DirectorResponse {
    pub narration: String,
    pub memory_updates: MemoryUpdates,
    #[serde(default)]
    pub plot_updates: HashMap<String, bool>,
    #[serde(default)]
    pub inventory_updates: Vec<InventoryUpdate>,
    pub choices: Vec<Choice>,
    pub dice_roll: DiceRoll,
}

/// A medium-term summary of a stretch of conversation (§4.4).
///
/// A struct with one field rather than a bare string, because the request is
/// schema-constrained: `MemoryManager` asked for a paragraph in prose and then
/// had to check the answer against the literal string `"null"`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionSummaryResponse {
    /// Objective third-person, a few sentences.
    pub summary: String,
}

/// The character's long-term memory, rewritten to absorb new summaries.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DistilledMemoryResponse {
    /// One or two paragraphs, integrating the existing memory with what is
    /// new rather than replacing it.
    pub memory: String,
}

/// A character's resting disposition, deduced from their biography.
///
/// The Godot build asked for this in prose and parsed the answer with
/// `JsonRepair`; here the sampler is constrained to the shape. Its purpose is
/// `characters.base_emotion`, which is what a character decays back toward
/// between scenes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BaselineDisposition {
    pub base_emotion: Emotion,
    pub base_intensity: f32,
    pub reason: String,
}

/// One response that does both jobs (§4.2, arm C).
///
/// The Director/Actor split exists because a 3B Actor could not also do
/// Director work. This type is what "one 8B model, one call per turn"
/// requires: the union of both schemas, so a single constrained response
/// carries the character's line *and* the world state it changed.
///
/// It is deliberately the union rather than a redesign. The arms have to be
/// comparable, and a combined schema that also improved the fields would
/// measure two changes at once. `narration` appears once because in this arm
/// there is one narrator; splitting it would be the redesign.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CombinedTurnResponse {
    pub thinking: String,
    pub narration: String,
    pub dialogue: String,
    pub emotional_update: EmotionalUpdate,
    pub escalation_signal: EscalationSignal,
    pub memory_updates: MemoryUpdates,
    #[serde(default)]
    pub plot_updates: HashMap<String, bool>,
    #[serde(default)]
    pub inventory_updates: Vec<InventoryUpdate>,
    pub choices: Vec<Choice>,
    pub dice_roll: DiceRoll,
}

impl CombinedTurnResponse {
    /// The Actor's half, so everything downstream of a two-call turn works
    /// unchanged.
    pub fn as_character(&self) -> CharacterResponse {
        CharacterResponse {
            thinking: self.thinking.clone(),
            narration: self.narration.clone(),
            dialogue: self.dialogue.clone(),
            emotional_update: self.emotional_update.clone(),
            escalation_signal: self.escalation_signal,
        }
    }

    /// The Director's half. `narration` is left empty: this arm has already
    /// narrated in the character response, and returning it twice would log
    /// the same beat to the transcript twice.
    pub fn as_director(&self) -> DirectorResponse {
        DirectorResponse {
            narration: String::new(),
            memory_updates: self.memory_updates.clone(),
            plot_updates: self.plot_updates.clone(),
            inventory_updates: self.inventory_updates.clone(),
            choices: self.choices.clone(),
            dice_roll: self.dice_roll.clone(),
        }
    }
}

/// The Director ReAct loop's per-step response, ported from
/// `SystemPrompts.get_director_react_system_prompt()` for completeness
/// against §2.4's exit bar. **Superseded by native tool calling (§2.6):**
/// once tool calls are typed and validated through
/// `crate::inference::tools`, a model no longer emits this shape at all —
/// `action`/`args` become a real `ToolCall`, and "conclude research" is
/// simply a response with no tool calls. Kept here to document what the
/// interim schema-constrained (pre-native-tools) step looked like.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ReactStep {
    ToolCall {
        thought: String,
        action: String,
        args: serde_json::Value,
    },
    Final {
        thought: String,
        #[serde(rename = "final")]
        is_final: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::ResponseFormat;

    /// The exit bar for §2.4 is schema validity 100% *by construction*.
    /// This is the part of that claim a unit test can pin: every response
    /// type here must actually produce a JSON Schema, and must round-trip
    /// example payloads shaped like the ones `SystemPrompts.gd` asked for
    /// in prose.
    #[test]
    fn character_response_schema_generates_and_round_trips() {
        let _format = ResponseFormat::for_type::<CharacterResponse>();
        let payload = serde_json::json!({
            "thinking": "The player greeted the guard.",
            "narration": "The guard straightens up.",
            "dialogue": "Move along, traveler.",
            "emotional_update": {
                "emotion": "serenity",
                "intensity": 0.4,
                "reason": "A routine greeting.",
                "rapport_delta": 0.0
            },
            "escalation_signal": "none"
        });
        let parsed: CharacterResponse = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.escalation_signal, EscalationSignal::None);
    }

    #[test]
    fn director_response_schema_generates_and_round_trips() {
        let _format = ResponseFormat::for_type::<DirectorResponse>();
        let payload = serde_json::json!({
            "narration": "The tavern door creaks open.",
            "memory_updates": {
                "short_term": "Entered the tavern.",
                "medium_term": "Looking for the informant.",
                "long_term": "The party seeks the missing heir."
            },
            "plot_updates": { "met_informant": false },
            "inventory_updates": [],
            "choices": [
                { "text": "Approach the bar.", "type": "do" },
                { "text": "\"Anyone seen a stranger in a grey cloak?\"", "type": "say" }
            ],
            "dice_roll": { "required": false }
        });
        let parsed: DirectorResponse = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.choices.len(), 2);
        assert_eq!(parsed.choices[0].kind, ChoiceType::Do);
    }

    #[test]
    fn react_step_final_and_tool_call_both_round_trip() {
        let final_step: ReactStep = serde_json::from_value(serde_json::json!({
            "thought": "I have enough information.",
            "final": true
        }))
        .unwrap();
        assert!(matches!(
            final_step,
            ReactStep::Final { is_final: true, .. }
        ));

        let call_step: ReactStep = serde_json::from_value(serde_json::json!({
            "thought": "I need the location's description.",
            "action": "get_location_detail",
            "args": { "location_name": "The Guttering Lamp" }
        }))
        .unwrap();
        assert!(matches!(call_step, ReactStep::ToolCall { .. }));
    }
}
