//! Schema and migrations.
//!
//! `SaveManager._upgrade_save_state()` was the Godot build's migration
//! mechanism: a hand-written function that inspected a `schema_version` string,
//! mutated the loaded dictionary in place, and ran some of its work twice on
//! purpose ("ensure legacy character migration runs regardless of version")
//! because there was no record of what had already been applied. Here the
//! database records its own version and `rusqlite_migration` refuses to apply
//! a step twice.

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use std::sync::Once;

use super::error::StateError;

/// The schema version this build writes. Bumped by adding an `M::up` below,
/// never by editing one that has shipped.
pub const CURRENT_SCHEMA_VERSION: usize = 1;

static REGISTER_VEC: Once = Once::new();

/// Register `sqlite-vec` as an auto-extension, once per process.
///
/// Must happen before any connection is opened, which is why it lives here
/// rather than in `retrieval`: every connection this crate opens gets vector
/// support, and the dense index (§3.4) only has to create its table.
fn register_vector_extension() {
    REGISTER_VEC.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is the extension entry point exported by
        // the `sqlite-vec` C source compiled into this binary, and its real
        // signature is the auto-extension signature SQLite calls it with. The
        // crate declares it as `fn()` because C has no way to express the
        // dependency on SQLite's own header types.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut rusqlite::ffi::sqlite3,
                    *mut *mut std::os::raw::c_char,
                    *const rusqlite::ffi::sqlite3_api_routines,
                ) -> std::os::raw::c_int,
            >(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(V1)])
}

/// Version 1: the whole Godot save document, normalised.
///
/// Notes on choices that are not obvious:
///
/// - `knowledge_nodes.body` is `NOT NULL`. Character nodes losing their raw
///   source was B-14; making the column mandatory means a node that has thrown
///   its source away cannot be written at all.
/// - Everything hangs off `campaigns(id)` with `ON DELETE CASCADE`, so
///   deleting a campaign is one statement rather than
///   `SaveManager.delete_campaign()` plus a separate embeddings file that it
///   forgot to remove.
/// - `embedding_owners` maps a `sqlite-vec` rowid to whatever it embeds. The
///   `vec0` virtual table itself is created on demand by the dense index,
///   because its dimensionality depends on the configured embedding model and
///   model identifiers are configuration, never constants (B-10).
const V1: &str = r#"
CREATE TABLE campaigns (
    id                       TEXT PRIMARY KEY,
    title                    TEXT NOT NULL,
    created_at               TEXT NOT NULL,
    last_played              TEXT NOT NULL,
    active_scene             TEXT NOT NULL DEFAULT '',
    active_location          TEXT NOT NULL DEFAULT '',
    active_character         TEXT NOT NULL DEFAULT '',
    art_style                TEXT NOT NULL DEFAULT '',
    intro_narration          TEXT NOT NULL DEFAULT '',
    writing_style            TEXT NOT NULL DEFAULT '',
    playtime_seconds         REAL NOT NULL DEFAULT 0.0,
    engine_version           TEXT NOT NULL DEFAULT '',
    player_character         TEXT,
    memory_short_term        TEXT NOT NULL DEFAULT '',
    memory_medium_term       TEXT NOT NULL DEFAULT '',
    memory_long_term         TEXT NOT NULL DEFAULT '',
    pending_scene            TEXT,
    turns_since_last_director INTEGER NOT NULL DEFAULT 0,
    director_cooldown        INTEGER NOT NULL DEFAULT 0,
    last_director_beat       TEXT NOT NULL DEFAULT ''
);

CREATE TABLE knowledge_nodes (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    label       TEXT NOT NULL,
    kind        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    level       INTEGER NOT NULL DEFAULT 0,
    source_path TEXT,
    body        TEXT NOT NULL,
    properties  TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (campaign_id, id)
);
CREATE INDEX idx_knowledge_nodes_kind ON knowledge_nodes(campaign_id, kind);
CREATE INDEX idx_knowledge_nodes_level ON knowledge_nodes(campaign_id, level);

CREATE TABLE knowledge_edges (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    from_id     TEXT NOT NULL,
    to_id       TEXT NOT NULL,
    relation    TEXT NOT NULL,
    weight      REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (campaign_id, from_id, to_id, relation)
);
CREATE INDEX idx_knowledge_edges_from ON knowledge_edges(campaign_id, from_id);
CREATE INDEX idx_knowledge_edges_to ON knowledge_edges(campaign_id, to_id);

CREATE TABLE characters (
    campaign_id              TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    entity_id                TEXT NOT NULL,
    affinity                 REAL NOT NULL DEFAULT 0.0,
    base_emotion             TEXT NOT NULL DEFAULT '',
    base_intensity           REAL NOT NULL DEFAULT -1.0,
    long_term_memory         TEXT NOT NULL DEFAULT '',
    turns_since_last_summary INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (campaign_id, entity_id)
);

CREATE TABLE emotion_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    campaign_id   TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    entity_id     TEXT NOT NULL,
    timestamp     TEXT NOT NULL,
    emotion       TEXT NOT NULL,
    intensity     REAL NOT NULL,
    target        TEXT NOT NULL DEFAULT '',
    context       TEXT NOT NULL DEFAULT '',
    rapport_delta REAL NOT NULL DEFAULT 0.0
);
CREATE INDEX idx_emotion_events_entity ON emotion_events(campaign_id, entity_id, id);

CREATE TABLE inventory (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    entity_id   TEXT NOT NULL,
    item        TEXT NOT NULL,
    quantity    INTEGER NOT NULL,
    properties  TEXT,
    PRIMARY KEY (campaign_id, entity_id, item)
);

CREATE TABLE plot_flags (
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    PRIMARY KEY (campaign_id, key)
);

CREATE TABLE history_logs (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    campaign_id      TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    role             TEXT NOT NULL,
    content          TEXT NOT NULL,
    timestamp        TEXT NOT NULL,
    sender           TEXT,
    active_character TEXT
);
CREATE INDEX idx_history_campaign ON history_logs(campaign_id, id);

CREATE TABLE embedding_meta (
    id    INTEGER PRIMARY KEY CHECK (id = 1),
    model TEXT NOT NULL,
    dim   INTEGER NOT NULL
);

CREATE TABLE embedding_owners (
    rowid       INTEGER PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    ref_id      TEXT NOT NULL,
    UNIQUE (campaign_id, kind, ref_id)
);
"#;

/// Open (creating if needed) a campaign database at `path` and bring it to
/// [`CURRENT_SCHEMA_VERSION`].
pub fn open_connection(path: &Path) -> Result<Connection, StateError> {
    register_vector_extension();
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

/// An in-memory database, migrated. Used by tests and by any caller that wants
/// a scratch campaign that never touches disk.
pub fn open_in_memory() -> Result<Connection, StateError> {
    register_vector_extension();
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<(), StateError> {
    // WAL plus a busy timeout is what replaces `CampaignState._mutex`: readers
    // no longer block the writer, and a contended write waits rather than
    // failing.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

/// The schema version recorded in the database itself.
pub fn schema_version(conn: &Connection) -> Result<usize, StateError> {
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    Ok(v as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        // rusqlite_migration can prove the set applies forward from empty.
        assert!(migrations().validate().is_ok());
    }

    #[test]
    fn fresh_database_is_at_current_version() {
        let conn = open_in_memory().unwrap();
        assert_eq!(schema_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn vector_extension_is_registered() {
        // The dependency compiling is not the dependency working: without this
        // the first `vec0` table creation would fail at §3.4 instead of here.
        let conn = open_in_memory().unwrap();
        let version: String = conn
            .query_row("SELECT vec_version()", [], |r| r.get(0))
            .expect("sqlite-vec is not loaded");
        assert!(version.starts_with('v'), "unexpected version {version}");
    }
}
