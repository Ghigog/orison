//! Turn configuration.
//!
//! Everything here is configuration rather than a constant, and model
//! identifiers in particular (B-10): the Godot build selected a context window
//! by string-comparing a model name, which is wrong the moment both roles are
//! configured to the same model — the exact configuration §4.2's experiment
//! tests.

use crate::inference::{KeepAlive, SamplingOptions};
use crate::prompt::BudgetFractions;
use crate::retrieval::{MetadataFilter, RetrievalConfig};

/// When the background Director runs.
///
/// The Godot build fires it when `turns_since_last_director >= 2` and no
/// cooldown is active, or immediately on a non-`none` escalation signal. That
/// is already the fix for [rag_architecture.md] Bug 7 (the original rule
/// needed *both*, and a 3B Actor almost never emitted an escalation, so the
/// Director fired only after six dead turns). Kept, and made configurable.
///
/// [rag_architecture.md]: ../../../../docs/rag_architecture.md
#[derive(Debug, Clone, Copy)]
pub struct DirectorPolicy {
    pub enabled: bool,
    /// Turns since the last beat before one is due.
    pub turn_threshold: i64,
    /// Turns to wait after a beat before another may be composed.
    pub cooldown_turns: i64,
    /// Whether an escalation signal from the Actor bypasses the cooldown.
    pub escalation_bypasses_cooldown: bool,
}

impl Default for DirectorPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            turn_threshold: 2,
            cooldown_turns: 3,
            escalation_bypasses_cooldown: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnConfig {
    /// Tokens held back from the context window for the response. Subtracted
    /// from `InferenceBackend::context_length()`, which is queried from the
    /// backend and never hardcoded (B-1).
    pub response_reserve: usize,
    /// Shares of the remaining budget. Checked to sum to at most 1.0 by
    /// `prompt::allocate`; the Godot build's summed to 1.05 (B-1).
    pub fractions: BudgetFractions,
    /// Transcript entries offered to the prompt before budgeting trims them.
    pub history_window: usize,
    /// Retrieval for the Actor: concrete facts, level 0 (§1.3).
    pub actor_retrieval: RetrievalConfig,
    /// Retrieval for the Director: campaign-level summaries, level 2 (§1.3).
    pub director_retrieval: RetrievalConfig,
    pub director: DirectorPolicy,
    pub actor_sampling: SamplingOptions,
    pub director_sampling: SamplingOptions,
    /// How long a model stays resident. The default is bounded; the Godot
    /// build sent `keep_alive: -1` on every request for both models (B-6).
    pub keep_alive: KeepAlive,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            response_reserve: 1024,
            fractions: BudgetFractions {
                system: 0.30,
                identity: 0.30,
                history: 0.25,
                lore: 0.15,
            },
            history_window: 50,
            actor_retrieval: RetrievalConfig {
                filter: MetadataFilter::level(0),
                limit: 6,
                // The configuration Phase 3 measured: recall 1.000 on every
                // fixture, at a documented cost in precision.
                expand_hops: 1,
                expand_from: 2,
                ..RetrievalConfig::default()
            },
            director_retrieval: RetrievalConfig {
                filter: MetadataFilter::level(2),
                limit: 4,
                expand_hops: 0,
                expand_from: 0,
                ..RetrievalConfig::default()
            },
            director: DirectorPolicy::default(),
            // Warmer than the Director: the Actor is writing dialogue.
            actor_sampling: SamplingOptions::new(0.8, 0.9),
            director_sampling: SamplingOptions::new(0.7, 0.9),
            keep_alive: KeepAlive::default(),
        }
    }
}
