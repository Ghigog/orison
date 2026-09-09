//! The turn state machine (§4.1).
//!
//! `GameLoopController.gd` coordinates a turn with a three-value enum
//! (`TurnState`) plus a scatter of booleans that callbacks mutate from
//! wherever they happen to run: `is_initializing`, `_is_generating_beginning`,
//! `_is_director_running`, and the `input_disabled_changed` signal that every
//! exit path has to remember to emit. [orison_audit.md §12] records the
//! consequence: nothing states which orderings are legal, so nothing can
//! detect an illegal one.
//!
//! Two machines live here rather than one, because a turn and the background
//! Director genuinely do run at the same time. Making that concurrency two
//! explicit machines with their own legal transitions is the point; folding it
//! into one enum would need a state per pair, and folding it into a `bool` is
//! what is being replaced.
//!
//! [orison_audit.md §12]: ../../../../docs/orison_audit.md

use std::fmt;

/// Where a single player turn is.
///
/// `Preparing` covers everything before the model is called (sanitising input,
/// writing the player's line to history, retrieval); it is separate from
/// `Streaming` because cancellation during it must not leave a half-written
/// turn behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnState {
    /// No turn in flight. The only state in which player input is accepted
    /// without cancelling something first.
    Idle,
    /// Input accepted; gathering state and retrieving context. No model call
    /// has been issued yet.
    Preparing,
    /// The Actor call is in flight; deltas are reaching the player.
    Streaming,
    /// The response arrived and is being applied to state: history, emotion,
    /// memory, plot flags.
    Applying,
}

impl TurnState {
    /// Whether `self -> next` is a legal move.
    ///
    /// Every transition a turn can make is listed here and nowhere else. A
    /// transition that is not in this table is a bug in the caller, not a
    /// condition to be worked around at the call site.
    pub fn can_advance_to(self, next: TurnState) -> bool {
        use TurnState::*;
        matches!(
            (self, next),
            (Idle, Preparing)
                | (Preparing, Streaming)
                | (Streaming, Applying)
                | (Applying, Idle)
                // Cancellation and failure both return to Idle from wherever
                // the turn had reached.
                | (Preparing, Idle)
                | (Streaming, Idle)
        )
    }

    /// Whether a turn is in flight at all. The replacement for
    /// `input_disabled_changed`: the UI derives the flag from the state
    /// rather than each exit path remembering to emit it.
    pub fn is_busy(self) -> bool {
        !matches!(self, TurnState::Idle)
    }
}

impl fmt::Display for TurnState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            TurnState::Idle => "idle",
            TurnState::Preparing => "preparing",
            TurnState::Streaming => "streaming",
            TurnState::Applying => "applying",
        };
        f.write_str(name)
    }
}

/// Where the background Director is.
///
/// `Ready` is the state `pending_scene` represented in the save document: a
/// beat has been composed and is waiting for a moment to be shown. It is a
/// state rather than a nullable field so that "composed but not yet consumed"
/// cannot be confused with "not running".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirectorState {
    Idle,
    /// Running the deterministic retrieval pre-pass and, under the profiles
    /// that keep it, any research calls.
    Researching,
    /// The Director model call is in flight.
    Composing,
    /// A beat is composed and queued for the next quiet moment.
    Ready,
}

impl DirectorState {
    pub fn can_advance_to(self, next: DirectorState) -> bool {
        use DirectorState::*;
        matches!(
            (self, next),
            (Idle, Researching)
                | (Researching, Composing)
                | (Composing, Ready)
                | (Ready, Idle)
                // Failure or cancellation from either working state.
                | (Researching, Idle)
                | (Composing, Idle)
        )
    }

    pub fn is_running(self) -> bool {
        matches!(self, DirectorState::Researching | DirectorState::Composing)
    }
}

impl fmt::Display for DirectorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            DirectorState::Idle => "idle",
            DirectorState::Researching => "researching",
            DirectorState::Composing => "composing",
            DirectorState::Ready => "ready",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_cannot_skip_the_model_call() {
        // Preparing -> Applying would mean applying a response that was never
        // requested. The Godot build has no way to say this is illegal.
        assert!(!TurnState::Preparing.can_advance_to(TurnState::Applying));
        assert!(TurnState::Preparing.can_advance_to(TurnState::Streaming));
    }

    #[test]
    fn every_working_state_can_be_abandoned_back_to_idle() {
        // Cancellation must be expressible from anywhere a turn can be
        // interrupted, or the machine forces a cancelled turn to finish.
        assert!(TurnState::Preparing.can_advance_to(TurnState::Idle));
        assert!(TurnState::Streaming.can_advance_to(TurnState::Idle));
    }

    #[test]
    fn applying_must_complete() {
        // State has already been written by the time a turn is Applying;
        // there is no correct way to abandon it half-done, so the only exit
        // is forward.
        assert!(TurnState::Applying.can_advance_to(TurnState::Idle));
        assert!(!TurnState::Applying.can_advance_to(TurnState::Streaming));
    }

    #[test]
    fn director_ready_is_distinct_from_idle() {
        assert!(!DirectorState::Ready.is_running());
        assert_ne!(DirectorState::Ready, DirectorState::Idle);
        // A composed beat is consumed, not abandoned: Ready -> Idle is the
        // only way out, and it cannot go straight back to work.
        assert!(DirectorState::Ready.can_advance_to(DirectorState::Idle));
        assert!(!DirectorState::Ready.can_advance_to(DirectorState::Composing));
    }
}
