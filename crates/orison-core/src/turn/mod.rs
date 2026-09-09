//! Orchestration: the turn loop (§4.1).
//!
//! Everything below this module exists — a backend that can be called, state
//! that can be queried, a graph that can be retrieved from. This is what runs
//! a turn.
//!
//! The port of `GameLoopController.gd` (1,077 lines) is split by concern:
//!
//! | Module | What it owns |
//! |---|---|
//! | [`state`] | Which transitions are legal, for the turn and the Director |
//! | [`queue`] | Sequential execution, priority, and real cancellation |
//! | [`stream`] | Display text out of a response that is still arriving |
//! | [`session`] | The per-campaign store, graph and index a turn runs against |
//! | [`config`] | Budgets, retrieval settings and the Director's trigger rule |
//! | [`event`] | What a turn tells a shell, as one typed stream |
//! | [`engine`] | The turn itself |

pub mod clock;
pub mod config;
pub mod engine;
pub mod error;
pub mod event;
pub mod experiment;
pub mod queue;
pub mod session;
pub mod state;
pub mod stream;

pub use clock::{Clock, FixedClock, SystemClock};
pub use config::{DirectorPolicy, TurnConfig, TurnProfile};
pub use engine::{TurnEngine, TurnOutcome};
pub use error::{CancelReason, FailureKind, TurnError};
pub use event::{Speaker, TurnEvent};
pub use experiment::{ArmReport, Finding, ScoredTurn, TranscriptScript};
pub use queue::{CancelToken, Priority, RequestQueue, Ticket};
pub use session::Session;
pub use state::{DirectorState, TurnState};
pub use stream::{FieldStreamer, StreamPiece};
