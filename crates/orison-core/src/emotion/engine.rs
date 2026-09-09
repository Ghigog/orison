//! Applying and decaying emotional state (§4.3).
//!
//! Every write to `characters` and `emotion_events` that represents a feeling
//! goes through here. That is the consolidation the audit asked for: the
//! Godot build spread this across `EmotionEngine.gd`,
//! `EmotionPromptBuilder.gd`, `GameLoopController.gd`, `CampaignState.gd` and
//! `CharacterVisuals.gd`, and Bug 6 — the same emotion line every turn — was
//! the result of three of them writing over each other.
//!
//! **What is deliberately not ported.**
//!
//! - *The extract-and-repair path.* An emotional update arrives as a typed
//!   field on a schema-constrained response (§2.4), so there is nothing to
//!   extract and nothing to repair. `JsonRepair.gd` has no equivalent here.
//! - *`trigger_emotion_reflection`.* A second model call that re-derived the
//!   character's emotion from the Director's narration, and one of the three
//!   paths that fought over the same state. It is also Bug 6's second root
//!   cause outright: it overwrote the turn's real emotion with a boilerplate
//!   reflection. The beat's narration reaches the next turn as history, and
//!   the Actor's own typed field is the one place an emotion now comes from.
//! - *The three trigger paths.* `apply_emotion` was reachable from stream
//!   completion, from reflection, and from `select_character`, which is
//!   RAG003: three physical-reaction generations per player turn. There is
//!   one entry point now, so the debounce is structural rather than a guard.

use crate::prompt::schemas::{BaselineDisposition, EmotionalUpdate};
use crate::state::{CampaignStore, CharacterState, EmotionEvent, StateError};

use super::types::{
    emotion_str, parse_emotion, EmotionState, Rapport, DEFAULT_DECAY_RATE, NEUTRAL,
};

/// What applying an update changed.
#[derive(Debug, Clone)]
pub struct EmotionOutcome {
    pub entity_id: String,
    pub state: EmotionState,
    pub affinity: f64,
    /// `false` when the state did not actually move (RAG006). A caller must
    /// not emit a visual update or request a reaction on a `false`.
    pub changed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct EmotionConfig {
    /// Intensity removed per turn from characters who were not addressed.
    pub decay_rate: f32,
    /// How many past events to consider. The Godot save could hold 20 per
    /// character because it discarded the rest at write time; rows are cheap
    /// now, so this is a reader's choice rather than a storage limit.
    pub history_window: usize,
}

impl Default for EmotionConfig {
    fn default() -> Self {
        Self {
            decay_rate: DEFAULT_DECAY_RATE,
            history_window: 50,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmotionEngine {
    config: EmotionConfig,
}

impl EmotionEngine {
    pub fn new(config: EmotionConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> EmotionConfig {
        self.config
    }

    /// The character's current state: their most recent event, or their
    /// baseline, or neutral.
    pub fn current(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<EmotionState, StateError> {
        if let Some(latest) = store
            .emotion_events(campaign_id, entity_id, 1)?
            .into_iter()
            .next()
        {
            return Ok(EmotionState {
                emotion: parse_emotion(&latest.emotion),
                intensity: latest.intensity as f32,
                target: latest.target,
                reason: latest.context,
            });
        }
        Ok(self
            .baseline(store, campaign_id, entity_id)?
            .unwrap_or_else(EmotionState::neutral))
    }

    /// The character's *recorded* resting state, from
    /// `characters.base_emotion`, or `None` when it has not been deduced.
    ///
    /// A `base_intensity` below zero means "not deduced yet", which is how the
    /// Godot save encoded it. The distinction from [`Self::resting_state`]
    /// matters: an undeduced character decays to nothing, while a deduced one
    /// decays to what they are like when nothing is happening.
    pub fn baseline(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<Option<EmotionState>, StateError> {
        let Some(state) = store.character_state(campaign_id, entity_id)? else {
            return Ok(None);
        };
        if state.base_emotion.trim().is_empty() || state.base_intensity < 0.0 {
            return Ok(None);
        }
        Ok(Some(EmotionState {
            emotion: parse_emotion(&state.base_emotion),
            intensity: state.base_intensity as f32,
            target: "player".to_string(),
            reason: "Baseline disposition.".to_string(),
        }))
    }

    /// Where decay is heading: the recorded baseline, or fully faded neutral.
    ///
    /// The fallback is the Godot behaviour exactly — serenity at intensity
    /// zero — which is the right answer for a character nothing is known
    /// about. It is only wrong for a character whose baseline *was* deduced,
    /// and that is the case this distinction fixes.
    pub fn resting_state(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<EmotionState, StateError> {
        Ok(self
            .baseline(store, campaign_id, entity_id)?
            .unwrap_or_else(|| EmotionState::new(NEUTRAL, 0.0)))
    }

    /// Apply the typed emotional update from a response.
    ///
    /// Always records the event and always moves rapport — the record is the
    /// history, and losing a turn's affinity change because the tag happened
    /// to repeat would be wrong. What the no-op check gates is the *visual*
    /// update, which is what RAG006 is about.
    pub fn apply(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
        update: &EmotionalUpdate,
        timestamp: &str,
    ) -> Result<EmotionOutcome, StateError> {
        let previous = self.current(store, campaign_id, entity_id)?;
        let next = EmotionState {
            emotion: update.emotion,
            intensity: update.intensity.clamp(0.0, 1.0),
            target: "player".to_string(),
            reason: update.reason.clone(),
        };
        let rapport_delta = (update.rapport_delta as f64).clamp(-0.2, 0.2);

        self.ensure_state_row(store, campaign_id, entity_id)?;
        let affinity = store
            .adjust_affinity(campaign_id, entity_id, rapport_delta)?
            .unwrap_or(0.0);
        store.add_emotion_event(
            campaign_id,
            &EmotionEvent {
                entity_id: entity_id.to_string(),
                timestamp: timestamp.to_string(),
                emotion: emotion_str(next.emotion).to_string(),
                intensity: next.intensity as f64,
                target: next.target.clone(),
                context: next.reason.clone(),
                rapport_delta,
            },
        )?;

        Ok(EmotionOutcome {
            entity_id: entity_id.to_string(),
            changed: previous.differs_from(&next),
            state: next,
            affinity,
        })
    }

    /// Move every character except `reinforced` one step toward their
    /// baseline.
    ///
    /// **One change from the Godot behaviour, and it is a fix.** There, decay
    /// always ran toward serenity at intensity 0, so `base_emotion` — the
    /// field a whole model call exists to deduce — became inert after the
    /// first decay. Here a character with a recorded baseline decays toward
    /// *that*, which is what a resting disposition means. A character without
    /// one still decays to neutral, exactly as before.
    pub fn decay(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_ids: &[String],
        reinforced: Option<&str>,
        context: &str,
        timestamp: &str,
    ) -> Result<Vec<EmotionOutcome>, StateError> {
        let mut outcomes = Vec::new();
        for entity_id in entity_ids {
            if Some(entity_id.as_str()) == reinforced || entity_id == "player" {
                continue;
            }
            let current = self.current(store, campaign_id, entity_id)?;
            let baseline = self.resting_state(store, campaign_id, entity_id)?;
            let Some(next) = self.decayed(&current, &baseline) else {
                continue;
            };

            store.add_emotion_event(
                campaign_id,
                &EmotionEvent {
                    entity_id: entity_id.clone(),
                    timestamp: timestamp.to_string(),
                    emotion: emotion_str(next.emotion).to_string(),
                    intensity: next.intensity as f64,
                    target: next.target.clone(),
                    context: context.to_string(),
                    rapport_delta: 0.0,
                },
            )?;
            let affinity = store
                .character_state(campaign_id, entity_id)?
                .map(|s| s.affinity)
                .unwrap_or(0.0);
            outcomes.push(EmotionOutcome {
                entity_id: entity_id.clone(),
                changed: current.differs_from(&next),
                state: next,
                affinity,
            });
        }
        Ok(outcomes)
    }

    /// One step of decay, or `None` when the character is already at rest.
    fn decayed(&self, current: &EmotionState, baseline: &EmotionState) -> Option<EmotionState> {
        let at_rest = current.emotion == baseline.emotion
            && (current.intensity - baseline.intensity).abs() < f32::EPSILON;
        if at_rest {
            return None;
        }
        let faded = (current.intensity - self.config.decay_rate).max(0.0);
        // Once the feeling has faded out, the character returns to their
        // resting disposition rather than to a hardcoded tag.
        if faded <= 0.0 && current.emotion != baseline.emotion {
            return Some(EmotionState {
                emotion: baseline.emotion,
                intensity: baseline.intensity,
                target: current.target.clone(),
                reason: "Returned to baseline.".to_string(),
            });
        }
        Some(EmotionState {
            emotion: if faded <= 0.0 {
                NEUTRAL
            } else {
                current.emotion
            },
            intensity: faded,
            target: current.target.clone(),
            reason: current.reason.clone(),
        })
    }

    /// Record a deduced baseline. The result of the model call that reads a
    /// character's biography and answers with their resting disposition.
    pub fn set_baseline(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
        deduced: &BaselineDisposition,
        timestamp: &str,
    ) -> Result<(), StateError> {
        let intensity = deduced.base_intensity.clamp(0.0, 1.0);
        let mut state = self.ensure_state_row(store, campaign_id, entity_id)?;
        state.base_emotion = emotion_str(deduced.base_emotion).to_string();
        state.base_intensity = intensity as f64;
        store.save_character_state(campaign_id, &state)?;
        store.add_emotion_event(
            campaign_id,
            &EmotionEvent {
                entity_id: entity_id.to_string(),
                timestamp: timestamp.to_string(),
                emotion: emotion_str(deduced.base_emotion).to_string(),
                intensity: intensity as f64,
                target: "player".to_string(),
                context: deduced.reason.clone(),
                rapport_delta: 0.0,
            },
        )
    }

    /// Whether this character still needs their baseline deduced.
    pub fn needs_baseline(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<bool, StateError> {
        Ok(match store.character_state(campaign_id, entity_id)? {
            Some(state) => state.base_emotion.trim().is_empty() || state.base_intensity < 0.0,
            None => true,
        })
    }

    /// The emotional profile block, per [emotions.md] §4.1.
    ///
    /// Built here rather than in `prompt` because every rule it states — the
    /// bands, the tone guidance, the "do not reveal your parameters"
    /// instruction — is this module's, and splitting them is how they came to
    /// be spread across five files in the first place.
    ///
    /// [emotions.md]: ../../../../docs/emotions.md
    pub fn profile_block(&self, name: &str, state: &EmotionState, affinity: f64) -> String {
        let rapport = Rapport::of(affinity);
        let mut out = format!("EMOTIONAL PROFILE:\n- You are {name}.\n");
        out.push_str(&format!(
            "- Active feeling: {} (intensity {:.1}/1.0) towards {}\n",
            emotion_str(state.emotion),
            state.intensity,
            state.target,
        ));
        if !state.reason.trim().is_empty() {
            out.push_str(&format!("- Reason: {}\n", state.reason.trim()));
        }
        out.push_str(&format!(
            "- Tone this implies: {}\n",
            super::types::tone_guidance(state.emotion)
        ));
        out.push_str(&format!(
            "- Relationship with the player: {} ({:+.2}). {}\n",
            rapport.label(),
            affinity,
            rapport.behaviour(),
        ));
        out.push_str(
            "- Let these shape your tone and choices naturally. Never state your raw affinity \
             score or emotion parameters.\n",
        );
        out
    }

    fn ensure_state_row(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<CharacterState, StateError> {
        if let Some(state) = store.character_state(campaign_id, entity_id)? {
            return Ok(state);
        }
        let state = CharacterState::new(entity_id);
        store.save_character_state(campaign_id, &state)?;
        Ok(state)
    }
}
