//! What a turn tells the outside world.
//!
//! `GameLoopController` declares fourteen signals and an `EventBus` autoload,
//! and a shell binds to them individually. One typed event stream replaces
//! both: `orison-core` never knows what is rendering it (§3.1), and a shell
//! that matches on an enum cannot silently miss a case the way a shell that
//! forgets to connect a signal does.
//!
//! Broadcast rather than a channel per subscriber: a sidebar, a transcript and
//! a portrait all want the same turn.

use crate::prompt::schemas::{Choice, DiceRoll, Emotion, EscalationSignal};

use super::error::FailureKind;
use super::state::{DirectorState, TurnState};

/// Who a line of transcript belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Speaker {
    Player,
    /// A character, by graph entity id.
    Character(String),
    Narrator,
    System,
}

#[derive(Debug, Clone)]
pub enum TurnEvent {
    /// The turn machine moved. A shell derives "is input enabled" from
    /// `TurnState::is_busy` rather than from a separate signal that every
    /// exit path has to remember to emit.
    StateChanged(TurnState),
    DirectorStateChanged(DirectorState),

    /// The player's line, after sanitising. What is logged is what was sent.
    PlayerMessage {
        text: String,
    },

    /// A watched field of the Actor's response opened. `field` is
    /// `"narration"` or `"dialogue"`.
    StreamStarted {
        speaker: Speaker,
        field: &'static str,
    },
    /// Decoded display text, mid-response.
    StreamDelta {
        text: String,
    },
    StreamEnded {
        field: &'static str,
    },

    /// A complete line, emitted after the response parsed. A shell that
    /// rendered the stream replaces its provisional text with this; one that
    /// ignored the stream still gets everything.
    Message {
        speaker: Speaker,
        text: String,
    },

    /// A character's emotional state changed. Suppressed when nothing moved
    /// (RAG006).
    EmotionChanged {
        entity_id: String,
        emotion: Emotion,
        intensity: f32,
        affinity: f64,
    },
    /// A new portrait or physical reaction should be generated. At most once
    /// per turn (RAG003).
    ReactionRequested {
        entity_id: String,
        emotion: Emotion,
    },

    /// The Actor asked for the scene to escalate.
    Escalation(EscalationSignal),
    /// The Director's beat was shown and its state applied.
    SceneBeatApplied {
        choices: Vec<Choice>,
        dice_roll: DiceRoll,
    },

    /// Campaign or character memory was rewritten.
    MemoryUpdated {
        entity_id: Option<String>,
    },

    /// Something worth telling the player that is not a story beat.
    SystemMessage {
        text: String,
    },

    /// A typed failure the loop decided to surface. Never an empty response
    /// narrated around: `kind` says what to do about it and `detail` says
    /// what happened.
    Failed {
        kind: FailureKind,
        detail: String,
    },

    TurnCompleted,
}
