//! Everything the CLI can fail at, as values.
//!
//! The recurring failure in this project's history is silent degradation: an
//! empty response narrated around, a dead model reported as success (B-15).
//! A shell is the last place that can still go wrong quietly, so every exit
//! path here is a typed error with something the player can act on.

use std::fmt;

use orison_core::inference::InferenceError;
use orison_core::ingest::IngestError;
use orison_core::state::StateError;
use orison_core::turn::{FailureKind, TurnError};

#[derive(Debug)]
pub enum CliError {
    Inference(InferenceError),
    State(StateError),
    Ingest(IngestError),
    Turn(TurnError),
    Tokenizer {
        path: String,
        detail: String,
    },
    /// A campaign id that is not in the database.
    NoSuchCampaign(String),
    /// `play` with nothing to play and nothing named.
    NoCampaigns,
    Io(std::io::Error),
}

impl std::error::Error for CliError {}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Inference(e) => write!(f, "{e}\n{}", advice(inference_kind(e))),
            CliError::State(e) => write!(f, "campaign database: {e}"),
            CliError::Ingest(e) => write!(f, "vault import: {e}"),
            CliError::Turn(e) => write!(f, "{e}"),
            CliError::Tokenizer { path, detail } => write!(
                f,
                "could not load the tokenizer at {path}: {detail}\n\
                 Point --tokenizer at the model's tokenizer.json, or omit it to count words."
            ),
            CliError::NoSuchCampaign(id) => write!(
                f,
                "no campaign with id {id:?}. Run `orison campaigns` to see what exists."
            ),
            CliError::NoCampaigns => write!(
                f,
                "there are no campaigns yet. Run `orison new --title \"…\" --vault <path>`."
            ),
            CliError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// The same classification `TurnError` makes, for a failure that happened
/// before there was a turn to fail — connecting at startup, most often.
fn inference_kind(e: &InferenceError) -> FailureKind {
    match e {
        InferenceError::Unreachable { .. } => FailureKind::ModelUnreachable,
        InferenceError::ModelNotFound { .. } => FailureKind::ModelNotInstalled,
        InferenceError::BudgetExceeded { .. } => FailureKind::BudgetExceeded,
        _ => FailureKind::Backend,
    }
}

/// What to do about a failure, in one line.
///
/// Every arm names an action. "Something went wrong" is the message this
/// function exists to never print.
pub fn advice(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::ModelUnreachable => {
            "The inference endpoint did not answer. Start it (`ollama serve`) and try again."
        }
        FailureKind::ModelNotInstalled => {
            "The endpoint is up but does not have that model. Pull it (`ollama pull <model>`)."
        }
        FailureKind::BudgetExceeded => {
            "The assembled prompt did not fit the context window. Raise --context-limit, or \
             let the transcript compact by playing on."
        }
        FailureKind::Backend => "The model answered with something unusable. Retry the turn.",
        FailureKind::Storage => "The campaign database refused the write. Check disk space.",
        FailureKind::Retrieval => "Retrieval failed. Re-import the vault (`orison import`).",
        FailureKind::Cancelled => "Stopped.",
        FailureKind::Configuration => "Nothing is selected to act on. Try /who, then /talk <name>.",
        FailureKind::Internal => "The engine contradicted itself. This is a bug worth reporting.",
    }
}

impl From<InferenceError> for CliError {
    fn from(e: InferenceError) -> Self {
        CliError::Inference(e)
    }
}
impl From<StateError> for CliError {
    fn from(e: StateError) -> Self {
        CliError::State(e)
    }
}
impl From<IngestError> for CliError {
    fn from(e: IngestError) -> Self {
        CliError::Ingest(e)
    }
}
impl From<TurnError> for CliError {
    fn from(e: TurnError) -> Self {
        CliError::Turn(e)
    }
}
impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Io(e)
    }
}
