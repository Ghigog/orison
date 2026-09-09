//! Ingest errors.
//!
//! Note what is *not* in here: there is no error for "this section did not
//! match a canonical field" and no error for "this wiki-link points at nothing".
//! Neither is a failure. The first goes to the overflow bucket and the second
//! is recorded in the report as a dangling link, because a vault that mentions
//! a character it has not written a note for yet is a normal vault, and
//! `messy/30_Systems/lore/The Salt Tithe.md` is the fixture that says so.

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("vault directory not found: {0}")]
    VaultNotFound(String),

    #[error("reading {path}: {detail}")]
    Io { path: String, detail: String },
}
