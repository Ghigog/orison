//! Turn failures, typed.
//!
//! The handoff names this phase's version of the recurring defect: "the turn
//! loop swallowing a failed call and narrating around it". In
//! `GameLoopController._on_npc_stream_failed` every failure — a dead endpoint,
//! a cancelled request, a malformed response — becomes one string pushed at
//! the UI, and `_on_background_director_completed` discards its failures with
//! a `print()`. Here every one of them is a variant the loop must decide how
//! to present.

use serde::Serialize;

use crate::inference::InferenceError;
use crate::prompt::PromptError;
use crate::retrieval::RetrievalError;
use crate::state::StateError;

use super::state::TurnState;

/// Why an in-flight request stopped.
///
/// Cancellation is not a failure, and the Godot build already knew that — it
/// string-matched `error_msg == "Stream cancelled by user"` to handle it
/// silently. A string comparison is what this replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    /// The player sent new input while this turn was still generating. The
    /// case [orison_audit.md §16] is about: the Godot build cannot stop the
    /// request, so the model keeps generating output nobody will read.
    ///
    /// [orison_audit.md §16]: ../../../../docs/orison_audit.md
    PlayerActedAgain,
    /// A higher-priority request displaced this one. The background Director
    /// yielding to a player turn is the only case in practice.
    Preempted,
    /// The caller cancelled explicitly.
    Requested,
    /// The engine is shutting down.
    Shutdown,
}

impl std::fmt::Display for CancelReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CancelReason::PlayerActedAgain => "the player acted again",
            CancelReason::Preempted => "preempted by a higher-priority request",
            CancelReason::Requested => "cancelled by request",
            CancelReason::Shutdown => "engine shutting down",
        };
        f.write_str(s)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TurnError {
    #[error(transparent)]
    Inference(#[from] InferenceError),

    #[error(transparent)]
    State(#[from] StateError),

    #[error(transparent)]
    Retrieval(#[from] RetrievalError),

    #[error(transparent)]
    Prompt(#[from] PromptError),

    /// Not a failure. Carried as an error variant because the alternative is
    /// a success value that contains nothing, which is the shape every caller
    /// forgets to check.
    #[error("request stopped: {0}")]
    Cancelled(CancelReason),

    /// `send_player_input` with no conversation partner selected. A warning
    /// string in the Godot build; a value here.
    #[error("no character is selected to talk to")]
    NoActiveCharacter,

    /// Input that was empty, or that sanitising emptied. `send_player_input`
    /// returns early on this; a caller awaiting a turn deserves to be told
    /// its turn never ran.
    #[error("player input was empty")]
    EmptyInput,

    #[error("no campaign is loaded")]
    NoCampaign,

    /// A move to somewhere that is not a location in this campaign's graph,
    /// or that nothing connects to the place the player is standing.
    ///
    /// Typed rather than a printed warning, because the shell has to tell the
    /// two apart: "there is no such place" and "you cannot get there from
    /// here" are different answers to the player.
    #[error("{detail}")]
    CannotTravel { to: String, detail: String },

    /// A move the state machine does not permit. Unreachable through the
    /// public API; reaching it means the engine contradicted itself.
    #[error("illegal turn transition: {from} -> {to}")]
    IllegalTransition { from: TurnState, to: TurnState },

    /// The queue's worker is gone, so nothing will ever run this request.
    #[error("the request queue is closed")]
    QueueClosed,
}

/// A failure reduced to something a shell can render and a test can assert on,
/// without the shell matching on error text.
///
/// [`TurnError`] is not `Clone` (its sources are not), and turn events are
/// broadcast to every subscriber, so events carry this plus a detail string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FailureKind {
    /// The configured endpoint could not be reached. Actionable: start the
    /// server.
    ModelUnreachable,
    /// The endpoint answered but does not have the model. Actionable: pull it.
    ModelNotInstalled,
    /// The assembled prompt did not fit its budget. Never silently truncated
    /// (B-1, B-4).
    BudgetExceeded,
    /// The backend answered with something unusable.
    Backend,
    /// The campaign database refused a read or write.
    Storage,
    Retrieval,
    Cancelled,
    /// A misconfiguration the player can fix: no character selected, no
    /// campaign loaded.
    Configuration,
    Internal,
}

impl FailureKind {
    /// Whether this failure is worth showing the player at all.
    ///
    /// Cancellation is the one that is not: the player cancelled it by acting
    /// again, and their new turn is already on screen.
    pub fn is_reportable(self) -> bool {
        !matches!(self, FailureKind::Cancelled)
    }
}

impl From<&TurnError> for FailureKind {
    fn from(e: &TurnError) -> Self {
        match e {
            TurnError::Inference(InferenceError::Unreachable { .. }) => {
                FailureKind::ModelUnreachable
            }
            TurnError::Inference(InferenceError::ModelNotFound { .. }) => {
                FailureKind::ModelNotInstalled
            }
            TurnError::Inference(InferenceError::BudgetExceeded { .. }) | TurnError::Prompt(_) => {
                FailureKind::BudgetExceeded
            }
            TurnError::Inference(_) => FailureKind::Backend,
            TurnError::State(_) => FailureKind::Storage,
            TurnError::Retrieval(_) => FailureKind::Retrieval,
            TurnError::Cancelled(_) => FailureKind::Cancelled,
            TurnError::NoActiveCharacter
            | TurnError::NoCampaign
            | TurnError::EmptyInput
            | TurnError::CannotTravel { .. } => FailureKind::Configuration,
            TurnError::IllegalTransition { .. } | TurnError::QueueClosed => FailureKind::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreachable_endpoint_is_distinguishable_from_a_missing_model() {
        // The two have different fixes, and the Godot build presented both as
        // the same string. A shell must be able to tell them apart without
        // parsing a message.
        let missing = TurnError::Inference(InferenceError::ModelNotFound {
            model: "llama3.2:3b".into(),
        });
        assert_eq!(FailureKind::from(&missing), FailureKind::ModelNotInstalled);

        let budget = TurnError::Prompt(PromptError::Overflow {
            measured: 9000,
            limit: 8192,
        });
        assert_eq!(FailureKind::from(&budget), FailureKind::BudgetExceeded);
    }

    #[test]
    fn cancellation_is_the_only_failure_not_shown_to_the_player() {
        assert!(
            !FailureKind::from(&TurnError::Cancelled(CancelReason::PlayerActedAgain))
                .is_reportable()
        );
        assert!(FailureKind::from(&TurnError::NoActiveCharacter).is_reportable());
    }
}
