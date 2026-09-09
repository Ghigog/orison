//! The emotion model, in one place.
//!
//! [emotions.md] defines it: an emotional tag, an intensity in `0.0..=1.0`,
//! a target, and a persistent rapport score in `-1.0..=1.0` with named bands.
//! The audit found that model implemented across five files with the one
//! *named* `EmotionEngine.gd` nearly empty ([orison_audit.md §7]); every rule
//! in it now lives here and nowhere else.
//!
//! [emotions.md]: ../../../../docs/emotions.md
//! [orison_audit.md §7]: ../../../../docs/orison_audit.md

use crate::prompt::schemas::Emotion;

/// The neutral tag. Reached by decay when a character has no recorded
/// baseline of their own.
pub const NEUTRAL: Emotion = Emotion::Serenity;

/// Intensity below which two states count as the same, for the RAG006 no-op
/// check. Ported from the Godot threshold.
pub const NOOP_EPSILON: f32 = 0.05;

/// How much intensity one turn of decay removes.
pub const DEFAULT_DECAY_RATE: f32 = 0.05;

/// Parse an emotion tag written by a model or read back from a row.
///
/// Unknown tags become [`NEUTRAL`] rather than being dropped, matching the
/// Godot validation. Under schema-constrained decoding (§2.4) an unknown tag
/// is not reachable from a model at all; this exists for rows written by
/// older builds.
pub fn parse_emotion(raw: &str) -> Emotion {
    match raw.trim().to_lowercase().as_str() {
        "joy" => Emotion::Joy,
        "sadness" => Emotion::Sadness,
        "anger" => Emotion::Anger,
        "fear" => Emotion::Fear,
        "trust" => Emotion::Trust,
        "disgust" => Emotion::Disgust,
        "surprise" => Emotion::Surprise,
        _ => NEUTRAL,
    }
}

pub fn emotion_str(emotion: Emotion) -> &'static str {
    match emotion {
        Emotion::Serenity => "serenity",
        Emotion::Joy => "joy",
        Emotion::Sadness => "sadness",
        Emotion::Anger => "anger",
        Emotion::Fear => "fear",
        Emotion::Trust => "trust",
        Emotion::Disgust => "disgust",
        Emotion::Surprise => "surprise",
    }
}

/// The default tone guidance from [emotions.md] §2.1, which is what the
/// emotional profile block in a prompt is for.
///
/// [emotions.md]: ../../../../docs/emotions.md
pub fn tone_guidance(emotion: Emotion) -> &'static str {
    match emotion {
        Emotion::Serenity => "Calm, warm, clear, confident. Balanced responses.",
        Emotion::Joy => "Upbeat, helpful, cooperative, and enthusiastic.",
        Emotion::Sadness => "Gentle, quiet, melancholic, or distant.",
        Emotion::Anger => "Clipped, blunt, impatient, or tense.",
        Emotion::Fear => "Careful, tentative, defensive, or guarded.",
        Emotion::Trust => "Open, vulnerable, supportive.",
        Emotion::Disgust => "Cold, dismissive, revolted.",
        Emotion::Surprise => "Expressive, unsettled, highly reactive.",
    }
}

/// One character's emotional state at a moment.
#[derive(Debug, Clone, PartialEq)]
pub struct EmotionState {
    pub emotion: Emotion,
    /// Always in `0.0..=1.0`.
    pub intensity: f32,
    /// Who or what the feeling is about. `"player"` by default.
    pub target: String,
    pub reason: String,
}

impl EmotionState {
    pub fn new(emotion: Emotion, intensity: f32) -> Self {
        Self {
            emotion,
            intensity: intensity.clamp(0.0, 1.0),
            target: "player".to_string(),
            reason: String::new(),
        }
    }

    /// The starting state for a character with nothing recorded.
    pub fn neutral() -> Self {
        Self::new(NEUTRAL, 0.5)
    }

    /// Whether moving to `next` is a real change.
    ///
    /// **RAG006.** A no-op delta — same tag, intensity within
    /// [`NOOP_EPSILON`] — must not emit a visual update, because in the Godot
    /// build every such update queued another physical-reaction generation.
    /// Bug 6 is what that looks like from the outside: the same emotion line
    /// every turn, and three model calls behind it.
    pub fn differs_from(&self, next: &EmotionState) -> bool {
        self.emotion != next.emotion || (self.intensity - next.intensity).abs() >= NOOP_EPSILON
    }
}

/// The rapport bands from [emotions.md] §3.
///
/// [emotions.md]: ../../../../docs/emotions.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rapport {
    Nemesis,
    Enemy,
    Acquaintance,
    Friend,
    BestFriend,
}

impl Rapport {
    pub fn of(affinity: f64) -> Self {
        match affinity {
            a if a <= -0.6 => Rapport::Nemesis,
            a if a <= -0.2 => Rapport::Enemy,
            a if a < 0.2 => Rapport::Acquaintance,
            a if a < 0.6 => Rapport::Friend,
            _ => Rapport::BestFriend,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Rapport::Nemesis => "Nemesis",
            Rapport::Enemy => "Enemy",
            Rapport::Acquaintance => "Acquaintance",
            Rapport::Friend => "Friend",
            Rapport::BestFriend => "Best Friend",
        }
    }

    /// The behaviour profile the band implies, for the prompt.
    pub fn behaviour(self) -> &'static str {
        match self {
            Rapport::Nemesis => "Openly adversarial or hostile. Tries to hinder the player.",
            Rapport::Enemy => "Guarded, cold and dismissive. Reluctant to share information.",
            Rapport::Acquaintance => "Neutral and formal. Transactional interactions.",
            Rapport::Friend => "Warm, familiar, cooperative and helpful.",
            Rapport::BestFriend => "Deeply loyal. Offers hidden paths and protection.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_state_is_not_a_change() {
        // RAG006: the exact case that made the Godot build queue a reaction
        // generation every turn while the emotion line never moved.
        let now = EmotionState::new(Emotion::Serenity, 0.60);
        assert!(!now.differs_from(&EmotionState::new(Emotion::Serenity, 0.60)));
        assert!(!now.differs_from(&EmotionState::new(Emotion::Serenity, 0.62)));
        assert!(now.differs_from(&EmotionState::new(Emotion::Serenity, 0.70)));
        assert!(now.differs_from(&EmotionState::new(Emotion::Anger, 0.60)));
    }

    #[test]
    fn rapport_bands_match_the_documented_ranges() {
        assert_eq!(Rapport::of(-1.0), Rapport::Nemesis);
        assert_eq!(Rapport::of(-0.6), Rapport::Nemesis);
        assert_eq!(Rapport::of(-0.59), Rapport::Enemy);
        assert_eq!(Rapport::of(-0.19), Rapport::Acquaintance);
        assert_eq!(Rapport::of(0.19), Rapport::Acquaintance);
        assert_eq!(Rapport::of(0.2), Rapport::Friend);
        assert_eq!(Rapport::of(0.6), Rapport::BestFriend);
    }

    #[test]
    fn intensity_cannot_leave_its_range() {
        assert_eq!(EmotionState::new(Emotion::Joy, 5.0).intensity, 1.0);
        assert_eq!(EmotionState::new(Emotion::Joy, -3.0).intensity, 0.0);
    }
}
