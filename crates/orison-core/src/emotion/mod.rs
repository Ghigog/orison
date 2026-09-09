//! The emotion engine (§4.3).
//!
//! [emotions.md]'s tri-dimensional model — a tag, an intensity, a target, and
//! a persistent rapport score — implemented in exactly one module, which is
//! the whole point of the task: [orison_audit.md §7] found the logic spread
//! across five files with the one named `EmotionEngine.gd` nearly empty.
//!
//! [emotions.md]: ../../../../docs/emotions.md
//! [orison_audit.md §7]: ../../../../docs/orison_audit.md

pub mod engine;
pub mod types;

pub use engine::{EmotionConfig, EmotionEngine, EmotionOutcome};
pub use types::{
    emotion_str, parse_emotion, tone_guidance, EmotionState, Rapport, DEFAULT_DECAY_RATE,
    NOOP_EPSILON,
};
