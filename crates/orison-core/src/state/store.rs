//! `CampaignStore`: every read and write of campaign state, as queries.
//!
//! The Godot equivalent is `CampaignState.apply_state_change()`, a 200-line
//! `match` over stringly-typed change dictionaries behind a mutex. Each arm of
//! that match is a method here, with its arguments named and typed, and the
//! transaction boundary is the statement rather than "the whole document, on
//! the next autosave".

use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

use super::error::StateError;
use super::schema;
use super::types::{
    Campaign, CampaignSummary, CharacterState, EdgeRow, EmotionEvent, HistoryEntry, HistoryRole,
    InventoryItem, NodeRow,
};

/// A handle on one campaign database file. Cheap to clone conceptually — open
/// a second one rather than sharing across threads; WAL is what makes that
/// safe, and it is the reason `CampaignState._mutex` is gone.
pub struct CampaignStore {
    conn: Connection,
}

impl CampaignStore {
    pub fn open(path: &Path) -> Result<Self, StateError> {
        Ok(Self {
            conn: schema::open_connection(path)?,
        })
    }

    pub fn open_in_memory() -> Result<Self, StateError> {
        Ok(Self {
            conn: schema::open_in_memory()?,
        })
    }

    /// Escape hatch for the modules that own their own tables: `knowledge`
    /// persistence and the dense index. Campaign state itself is only ever
    /// touched through the methods below.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn schema_version(&self) -> Result<usize, StateError> {
        schema::schema_version(&self.conn)
    }

    // ------------------------------------------------------------------
    // Campaigns
    // ------------------------------------------------------------------

    /// Insert or replace the whole campaign row. The one place a "write the
    /// document" shape survives, because a campaign row *is* one document's
    /// worth of scalars.
    pub fn save_campaign(&self, c: &Campaign) -> Result<(), StateError> {
        self.conn.execute(
            "INSERT INTO campaigns (
                 id, title, created_at, last_played, active_scene, active_location,
                 active_character, art_style, intro_narration, writing_style,
                 playtime_seconds, engine_version, player_character,
                 memory_short_term, memory_medium_term, memory_long_term,
                 pending_scene, turns_since_last_director, director_cooldown,
                 last_director_beat
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                       ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(id) DO UPDATE SET
                 title = excluded.title,
                 last_played = excluded.last_played,
                 active_scene = excluded.active_scene,
                 active_location = excluded.active_location,
                 active_character = excluded.active_character,
                 art_style = excluded.art_style,
                 intro_narration = excluded.intro_narration,
                 writing_style = excluded.writing_style,
                 playtime_seconds = excluded.playtime_seconds,
                 engine_version = excluded.engine_version,
                 player_character = excluded.player_character,
                 memory_short_term = excluded.memory_short_term,
                 memory_medium_term = excluded.memory_medium_term,
                 memory_long_term = excluded.memory_long_term,
                 pending_scene = excluded.pending_scene,
                 turns_since_last_director = excluded.turns_since_last_director,
                 director_cooldown = excluded.director_cooldown,
                 last_director_beat = excluded.last_director_beat",
            params![
                c.id,
                c.title,
                c.created_at,
                c.last_played,
                c.active_scene,
                c.active_location,
                c.active_character,
                c.art_style,
                c.intro_narration,
                c.writing_style,
                c.playtime_seconds,
                c.engine_version,
                c.player_character,
                c.memory_short_term,
                c.memory_medium_term,
                c.memory_long_term,
                c.pending_scene,
                c.turns_since_last_director,
                c.director_cooldown,
                c.last_director_beat,
            ],
        )?;
        Ok(())
    }

    /// `Ok(None)` means no such campaign. It never means "a campaign that
    /// failed to parse", which is what `SaveManager.load_campaign()`'s empty
    /// dictionary meant for both.
    pub fn load_campaign(&self, id: &str) -> Result<Option<Campaign>, StateError> {
        let c = self
            .conn
            .query_row(
                "SELECT id, title, created_at, last_played, active_scene, active_location,
                        active_character, art_style, intro_narration, writing_style,
                        playtime_seconds, engine_version, player_character,
                        memory_short_term, memory_medium_term, memory_long_term,
                        pending_scene, turns_since_last_director, director_cooldown,
                        last_director_beat
                 FROM campaigns WHERE id = ?1",
                params![id],
                campaign_from_row,
            )
            .optional()?;
        Ok(c)
    }

    pub fn list_campaigns(&self) -> Result<Vec<CampaignSummary>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, last_played, playtime_seconds, active_scene
             FROM campaigns ORDER BY last_played DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(CampaignSummary {
                id: r.get(0)?,
                title: r.get(1)?,
                last_played: r.get(2)?,
                playtime_seconds: r.get(3)?,
                active_scene: r.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Cascades to every table keyed on the campaign, embeddings included.
    /// The Godot build deleted the save file and left
    /// `<campaign>_embeddings.json` behind.
    pub fn delete_campaign(&self, id: &str) -> Result<bool, StateError> {
        let n = self
            .conn
            .execute("DELETE FROM campaigns WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    fn require_campaign(&self, id: &str) -> Result<(), StateError> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM campaigns WHERE id = ?1)",
            params![id],
            |r| r.get(0),
        )?;
        if exists {
            Ok(())
        } else {
            Err(StateError::UnknownCampaign(id.to_string()))
        }
    }

    // ------------------------------------------------------------------
    // Character play state
    // ------------------------------------------------------------------

    pub fn save_character_state(
        &self,
        campaign_id: &str,
        state: &CharacterState,
    ) -> Result<(), StateError> {
        self.require_campaign(campaign_id)?;
        self.conn.execute(
            "INSERT INTO characters (campaign_id, entity_id, affinity, base_emotion,
                                     base_intensity, long_term_memory, turns_since_last_summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(campaign_id, entity_id) DO UPDATE SET
                 affinity = excluded.affinity,
                 base_emotion = excluded.base_emotion,
                 base_intensity = excluded.base_intensity,
                 long_term_memory = excluded.long_term_memory,
                 turns_since_last_summary = excluded.turns_since_last_summary",
            params![
                campaign_id,
                state.entity_id,
                state.affinity,
                state.base_emotion,
                state.base_intensity,
                state.long_term_memory,
                state.turns_since_last_summary,
            ],
        )?;
        Ok(())
    }

    pub fn character_state(
        &self,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<Option<CharacterState>, StateError> {
        let s = self
            .conn
            .query_row(
                "SELECT entity_id, affinity, base_emotion, base_intensity,
                        long_term_memory, turns_since_last_summary
                 FROM characters WHERE campaign_id = ?1 AND entity_id = ?2",
                params![campaign_id, entity_id],
                |r| {
                    Ok(CharacterState {
                        entity_id: r.get(0)?,
                        affinity: r.get(1)?,
                        base_emotion: r.get(2)?,
                        base_intensity: r.get(3)?,
                        long_term_memory: r.get(4)?,
                        turns_since_last_summary: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(s)
    }

    /// Clamped to [-1.0, 1.0] in SQL, as `CampaignState` did in GDScript.
    /// Returns the new value, or `None` if the character has no state row.
    pub fn adjust_affinity(
        &self,
        campaign_id: &str,
        entity_id: &str,
        delta: f64,
    ) -> Result<Option<f64>, StateError> {
        let updated = self.conn.execute(
            "UPDATE characters
                SET affinity = MAX(-1.0, MIN(1.0, affinity + ?3))
              WHERE campaign_id = ?1 AND entity_id = ?2",
            params![campaign_id, entity_id, delta],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        let v: f64 = self.conn.query_row(
            "SELECT affinity FROM characters WHERE campaign_id = ?1 AND entity_id = ?2",
            params![campaign_id, entity_id],
            |r| r.get(0),
        )?;
        Ok(Some(v))
    }

    // ------------------------------------------------------------------
    // Emotion events
    // ------------------------------------------------------------------

    pub fn add_emotion_event(
        &self,
        campaign_id: &str,
        event: &EmotionEvent,
    ) -> Result<(), StateError> {
        self.require_campaign(campaign_id)?;
        self.conn.execute(
            "INSERT INTO emotion_events (campaign_id, entity_id, timestamp, emotion,
                                         intensity, target, context, rapport_delta)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                campaign_id,
                event.entity_id,
                event.timestamp,
                event.emotion,
                event.intensity.clamp(0.0, 1.0),
                event.target,
                event.context,
                event.rapport_delta.clamp(-0.2, 0.2),
            ],
        )?;
        Ok(())
    }

    /// Most recent first. The Godot build could only ever see the last 20
    /// because it discarded the rest at write time; here the limit is the
    /// reader's choice.
    pub fn emotion_events(
        &self,
        campaign_id: &str,
        entity_id: &str,
        limit: usize,
    ) -> Result<Vec<EmotionEvent>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT entity_id, timestamp, emotion, intensity, target, context, rapport_delta
             FROM emotion_events
             WHERE campaign_id = ?1 AND entity_id = ?2
             ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![campaign_id, entity_id, limit as i64], |r| {
            Ok(EmotionEvent {
                entity_id: r.get(0)?,
                timestamp: r.get(1)?,
                emotion: r.get(2)?,
                intensity: r.get(3)?,
                target: r.get(4)?,
                context: r.get(5)?,
                rapport_delta: r.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    // ------------------------------------------------------------------
    // Inventory
    // ------------------------------------------------------------------

    pub fn add_to_inventory(
        &self,
        campaign_id: &str,
        item: &InventoryItem,
    ) -> Result<(), StateError> {
        self.require_campaign(campaign_id)?;
        self.conn.execute(
            "INSERT INTO inventory (campaign_id, entity_id, item, quantity, properties)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(campaign_id, entity_id, item) DO UPDATE SET
                 quantity = quantity + excluded.quantity,
                 properties = COALESCE(excluded.properties, inventory.properties)",
            params![
                campaign_id,
                item.entity_id,
                item.item,
                item.quantity,
                item.properties,
            ],
        )?;
        Ok(())
    }

    /// Removes `quantity` of `item`, deleting the row when it reaches zero.
    /// Returns `false` when the holder does not have that many, leaving the
    /// row untouched — the same contract as the GDScript version, which is
    /// worth preserving because callers branch on it.
    pub fn remove_from_inventory(
        &self,
        campaign_id: &str,
        entity_id: &str,
        item: &str,
        quantity: i64,
    ) -> Result<bool, StateError> {
        let current: Option<i64> = self
            .conn
            .query_row(
                "SELECT quantity FROM inventory
                 WHERE campaign_id = ?1 AND entity_id = ?2 AND item = ?3",
                params![campaign_id, entity_id, item],
                |r| r.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            return Ok(false);
        };
        if current < quantity {
            return Ok(false);
        }
        if current == quantity {
            self.conn.execute(
                "DELETE FROM inventory
                 WHERE campaign_id = ?1 AND entity_id = ?2 AND item = ?3",
                params![campaign_id, entity_id, item],
            )?;
        } else {
            self.conn.execute(
                "UPDATE inventory SET quantity = quantity - ?4
                 WHERE campaign_id = ?1 AND entity_id = ?2 AND item = ?3",
                params![campaign_id, entity_id, item, quantity],
            )?;
        }
        Ok(true)
    }

    pub fn inventory(
        &self,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<Vec<InventoryItem>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT entity_id, item, quantity, properties FROM inventory
             WHERE campaign_id = ?1 AND entity_id = ?2 ORDER BY item",
        )?;
        let rows = stmt.query_map(params![campaign_id, entity_id], |r| {
            Ok(InventoryItem {
                entity_id: r.get(0)?,
                item: r.get(1)?,
                quantity: r.get(2)?,
                properties: r.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    // ------------------------------------------------------------------
    // Plot flags
    // ------------------------------------------------------------------

    pub fn set_plot_flag(
        &self,
        campaign_id: &str,
        key: &str,
        value: &str,
    ) -> Result<(), StateError> {
        self.require_campaign(campaign_id)?;
        self.conn.execute(
            "INSERT INTO plot_flags (campaign_id, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(campaign_id, key) DO UPDATE SET value = excluded.value",
            params![campaign_id, key, value],
        )?;
        Ok(())
    }

    pub fn plot_flag(&self, campaign_id: &str, key: &str) -> Result<Option<String>, StateError> {
        let v = self
            .conn
            .query_row(
                "SELECT value FROM plot_flags WHERE campaign_id = ?1 AND key = ?2",
                params![campaign_id, key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    pub fn plot_flags(&self, campaign_id: &str) -> Result<Vec<(String, String)>, StateError> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM plot_flags WHERE campaign_id = ?1 ORDER BY key")?;
        let rows = stmt.query_map(params![campaign_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    // ------------------------------------------------------------------
    // History
    // ------------------------------------------------------------------

    pub fn append_history(
        &self,
        campaign_id: &str,
        entry: &HistoryEntry,
    ) -> Result<(), StateError> {
        self.require_campaign(campaign_id)?;
        self.conn.execute(
            "INSERT INTO history_logs (campaign_id, role, content, timestamp, sender, active_character)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                campaign_id,
                entry.role.as_str(),
                entry.content,
                entry.timestamp,
                entry.sender,
                entry.active_character,
            ],
        )?;
        Ok(())
    }

    /// The most recent `limit` entries, oldest first — the order a prompt
    /// wants them in.
    pub fn recent_history(
        &self,
        campaign_id: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, timestamp, sender, active_character FROM (
                 SELECT id, role, content, timestamp, sender, active_character
                 FROM history_logs WHERE campaign_id = ?1
                 ORDER BY id DESC LIMIT ?2
             ) ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![campaign_id, limit as i64], |r| {
            let role: String = r.get(0)?;
            Ok(HistoryEntry {
                role: HistoryRole::from_str_lossy(&role),
                content: r.get(1)?,
                timestamp: r.get(2)?,
                sender: r.get(3)?,
                active_character: r.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn history_len(&self, campaign_id: &str) -> Result<usize, StateError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM history_logs WHERE campaign_id = ?1",
            params![campaign_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    // ------------------------------------------------------------------
    // Knowledge graph persistence
    //
    // Row-level access only. The graph's *meaning* lives in `knowledge`; this
    // is where it lands on disk.
    // ------------------------------------------------------------------

    pub fn replace_graph(
        &mut self,
        campaign_id: &str,
        nodes: &[NodeRow],
        edges: &[EdgeRow],
    ) -> Result<(), StateError> {
        self.require_campaign(campaign_id)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM knowledge_edges WHERE campaign_id = ?1",
            params![campaign_id],
        )?;
        tx.execute(
            "DELETE FROM knowledge_nodes WHERE campaign_id = ?1",
            params![campaign_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO knowledge_nodes
                     (campaign_id, id, label, kind, description, level, source_path, body, properties)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for n in nodes {
                stmt.execute(params![
                    campaign_id,
                    n.id,
                    n.label,
                    n.kind,
                    n.description,
                    n.level,
                    n.source_path,
                    n.body,
                    n.properties,
                ])?;
            }
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO knowledge_edges
                     (campaign_id, from_id, to_id, relation, weight)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for e in edges {
                stmt.execute(params![
                    campaign_id,
                    e.from_id,
                    e.to_id,
                    e.relation,
                    e.weight
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn graph_nodes(&self, campaign_id: &str) -> Result<Vec<NodeRow>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, kind, description, level, source_path, body, properties
             FROM knowledge_nodes WHERE campaign_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![campaign_id], |r| {
            Ok(NodeRow {
                id: r.get(0)?,
                label: r.get(1)?,
                kind: r.get(2)?,
                description: r.get(3)?,
                level: r.get(4)?,
                source_path: r.get(5)?,
                body: r.get(6)?,
                properties: r.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn graph_edges(&self, campaign_id: &str) -> Result<Vec<EdgeRow>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, relation, weight FROM knowledge_edges
             WHERE campaign_id = ?1 ORDER BY from_id, to_id, relation",
        )?;
        let rows = stmt.query_map(params![campaign_id], |r| {
            Ok(EdgeRow {
                from_id: r.get(0)?,
                to_id: r.get(1)?,
                relation: r.get(2)?,
                weight: r.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn campaign_from_row(r: &Row<'_>) -> rusqlite::Result<Campaign> {
    Ok(Campaign {
        id: r.get(0)?,
        title: r.get(1)?,
        created_at: r.get(2)?,
        last_played: r.get(3)?,
        active_scene: r.get(4)?,
        active_location: r.get(5)?,
        active_character: r.get(6)?,
        art_style: r.get(7)?,
        intro_narration: r.get(8)?,
        writing_style: r.get(9)?,
        playtime_seconds: r.get(10)?,
        engine_version: r.get(11)?,
        player_character: r.get(12)?,
        memory_short_term: r.get(13)?,
        memory_medium_term: r.get(14)?,
        memory_long_term: r.get(15)?,
        pending_scene: r.get(16)?,
        turns_since_last_director: r.get(17)?,
        director_cooldown: r.get(18)?,
        last_director_beat: r.get(19)?,
    })
}
