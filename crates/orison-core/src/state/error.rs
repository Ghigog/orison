//! Typed errors for the state layer.
//!
//! `SaveManager.gd` reported failure by `printerr()` and an empty `Dictionary`:
//! a corrupt save and an empty campaign were the same value to every caller.
//! Nothing here can be mistaken for success.

/// Anything that can go wrong reading or writing campaign state.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("schema migration failed: {0}")]
    Migration(String),

    /// A row exists but does not decode into the type the caller asked for.
    /// Distinct from "no such row", which is `Ok(None)`.
    #[error("stored {what} for '{id}' is malformed: {detail}")]
    Malformed {
        what: &'static str,
        id: String,
        detail: String,
    },

    /// The caller referenced a campaign that is not in the database. Returned
    /// rather than silently creating one, which is what `initialize()` did.
    #[error("no campaign with id '{0}'")]
    UnknownCampaign(String),

    #[error("serialising {what}: {detail}")]
    Serialisation { what: &'static str, detail: String },
}

impl From<rusqlite_migration::Error> for StateError {
    fn from(e: rusqlite_migration::Error) -> Self {
        StateError::Migration(e.to_string())
    }
}
