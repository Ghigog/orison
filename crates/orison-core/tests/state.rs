//! §3.1 exit criteria: campaign state round-trips through SQLite, and the
//! schema migration mechanism applies cleanly to a database created by an
//! earlier run.
//!
//! No model, no network, no fixtures. Everything here is `cargo test` alone,
//! per the Phase 1 lesson that a metric which never fires is not a metric.

use orison_core::state::{
    Campaign, CampaignStore, CharacterState, ChunkRow, EdgeRow, EmotionEvent, HistoryEntry,
    HistoryRole, InventoryItem, NodeRow, StateError, CURRENT_SCHEMA_VERSION,
};

fn campaign() -> Campaign {
    Campaign::new("saltmarsh", "The Salt Tithe", "2026-09-09T10:00:00Z")
}

fn store_with_campaign() -> CampaignStore {
    let store = CampaignStore::open_in_memory().unwrap();
    store.save_campaign(&campaign()).unwrap();
    store
}

#[test]
fn campaign_round_trips() {
    let store = CampaignStore::open_in_memory().unwrap();
    let mut c = campaign();
    c.active_location = "saltmarsh_landing".into();
    c.writing_style = "Terse, salt-stained.".into();
    c.playtime_seconds = 1234.5;
    c.player_character = Some("mira_of_the_fens".into());
    c.turns_since_last_director = 3;

    store.save_campaign(&c).unwrap();
    let loaded = store.load_campaign("saltmarsh").unwrap().unwrap();
    assert_eq!(loaded, c);
}

#[test]
fn missing_campaign_is_none_not_empty() {
    // `SaveManager.load_campaign()` returned `{}` both for "no such file" and
    // for "the file is corrupt". Those are different answers.
    let store = CampaignStore::open_in_memory().unwrap();
    assert!(store.load_campaign("nope").unwrap().is_none());
}

#[test]
fn writing_child_rows_for_an_unknown_campaign_is_an_error() {
    let store = CampaignStore::open_in_memory().unwrap();
    let err = store
        .set_plot_flag("ghost", "tithe_abolished", "false")
        .unwrap_err();
    assert!(matches!(err, StateError::UnknownCampaign(id) if id == "ghost"));
}

#[test]
fn campaign_list_is_a_query_not_a_directory_walk() {
    let store = CampaignStore::open_in_memory().unwrap();
    store.save_campaign(&campaign()).unwrap();
    store
        .save_campaign(&Campaign::new(
            "fens",
            "Fen Marches",
            "2026-09-10T10:00:00Z",
        ))
        .unwrap();

    let list = store.list_campaigns().unwrap();
    assert_eq!(list.len(), 2);
    // Ordered by last_played, most recent first.
    assert_eq!(list[0].id, "fens");
    assert_eq!(list[1].title, "The Salt Tithe");
}

#[test]
fn character_state_and_affinity_clamp() {
    let store = store_with_campaign();
    let mut s = CharacterState::new("lord_anneke");
    s.base_emotion = "serenity".into();
    s.base_intensity = 0.6;
    store.save_character_state("saltmarsh", &s).unwrap();

    assert_eq!(
        store.character_state("saltmarsh", "lord_anneke").unwrap(),
        Some(s)
    );

    let v = store
        .adjust_affinity("saltmarsh", "lord_anneke", 0.4)
        .unwrap();
    assert_eq!(v, Some(0.4));
    // Clamped, exactly as `CampaignState.adjust_affinity` did.
    let v = store
        .adjust_affinity("saltmarsh", "lord_anneke", 5.0)
        .unwrap();
    assert_eq!(v, Some(1.0));
    let v = store
        .adjust_affinity("saltmarsh", "lord_anneke", -50.0)
        .unwrap();
    assert_eq!(v, Some(-1.0));

    // No row is `None`, not a silently created one at 0.0.
    assert_eq!(
        store.adjust_affinity("saltmarsh", "nobody", 0.1).unwrap(),
        None
    );
}

#[test]
fn emotion_events_are_not_capped_at_twenty() {
    // The Godot build dropped the oldest past 20 because the whole history
    // lived inside a JSON document rewritten on every save. Rows are cheap.
    let store = store_with_campaign();
    for i in 0..25 {
        store
            .add_emotion_event(
                "saltmarsh",
                &EmotionEvent {
                    entity_id: "king_yuna".into(),
                    timestamp: format!("2026-09-09T10:{i:02}:00Z"),
                    emotion: "anger".into(),
                    intensity: 0.5,
                    target: "player".into(),
                    context: format!("turn {i}"),
                    rapport_delta: 0.05,
                },
            )
            .unwrap();
    }

    let all = store.emotion_events("saltmarsh", "king_yuna", 100).unwrap();
    assert_eq!(all.len(), 25);
    // Most recent first.
    assert_eq!(all[0].context, "turn 24");

    let recent = store.emotion_events("saltmarsh", "king_yuna", 5).unwrap();
    assert_eq!(recent.len(), 5);
}

#[test]
fn emotion_intensity_and_rapport_are_clamped_on_write() {
    let store = store_with_campaign();
    store
        .add_emotion_event(
            "saltmarsh",
            &EmotionEvent {
                entity_id: "king_yuna".into(),
                timestamp: "2026-09-09T10:00:00Z".into(),
                emotion: "joy".into(),
                intensity: 9.0,
                target: String::new(),
                context: String::new(),
                rapport_delta: 9.0,
            },
        )
        .unwrap();
    let e = &store.emotion_events("saltmarsh", "king_yuna", 1).unwrap()[0];
    assert_eq!(e.intensity, 1.0);
    assert_eq!(e.rapport_delta, 0.2);
}

#[test]
fn inventory_stacks_and_removes() {
    let store = store_with_campaign();
    let item = |q| InventoryItem {
        entity_id: "sergeant_adah".into(),
        item: "salt measure".into(),
        quantity: q,
        properties: None,
    };
    store.add_to_inventory("saltmarsh", &item(2)).unwrap();
    store.add_to_inventory("saltmarsh", &item(3)).unwrap();

    let inv = store.inventory("saltmarsh", "sergeant_adah").unwrap();
    assert_eq!(inv.len(), 1);
    assert_eq!(inv[0].quantity, 5);

    // Removing more than is held changes nothing and says so.
    assert!(!store
        .remove_from_inventory("saltmarsh", "sergeant_adah", "salt measure", 6)
        .unwrap());
    assert_eq!(
        store.inventory("saltmarsh", "sergeant_adah").unwrap()[0].quantity,
        5
    );

    assert!(store
        .remove_from_inventory("saltmarsh", "sergeant_adah", "salt measure", 5)
        .unwrap());
    assert!(store
        .inventory("saltmarsh", "sergeant_adah")
        .unwrap()
        .is_empty());
}

#[test]
fn plot_flags_round_trip() {
    let store = store_with_campaign();
    store
        .set_plot_flag("saltmarsh", "tithe_abolished", "false")
        .unwrap();
    store
        .set_plot_flag("saltmarsh", "tithe_abolished", "true")
        .unwrap();
    assert_eq!(
        store.plot_flag("saltmarsh", "tithe_abolished").unwrap(),
        Some("true".into())
    );
    assert_eq!(store.plot_flags("saltmarsh").unwrap().len(), 1);
    assert_eq!(store.plot_flag("saltmarsh", "absent").unwrap(), None);
}

#[test]
fn history_keeps_everything_and_reads_back_in_prompt_order() {
    let store = store_with_campaign();
    for i in 0..150 {
        store
            .append_history(
                "saltmarsh",
                &HistoryEntry {
                    role: if i % 2 == 0 {
                        HistoryRole::Player
                    } else {
                        HistoryRole::Character
                    },
                    content: format!("line {i}"),
                    timestamp: "2026-09-09T10:00:00Z".into(),
                    sender: None,
                    active_character: Some("lord_anneke".into()),
                },
            )
            .unwrap();
    }
    // The Godot build truncated at 100 entries. Nothing is dropped here.
    assert_eq!(store.history_len("saltmarsh").unwrap(), 150);

    let recent = store.recent_history("saltmarsh", 4).unwrap();
    assert_eq!(recent.len(), 4);
    // Oldest first within the window: the order a prompt wants.
    assert_eq!(recent[0].content, "line 146");
    assert_eq!(recent[3].content, "line 149");
    assert_eq!(recent[3].role, HistoryRole::Character);
}

#[test]
fn graph_rows_round_trip_and_cascade() {
    let mut store = store_with_campaign();
    let nodes = vec![NodeRow {
        id: "king_yuna".into(),
        label: "King Yuna".into(),
        kind: "character".into(),
        description: "Took the salt throne at nineteen.".into(),
        level: 0,
        source_path: Some("10_Entities/characters/King Yuna.md".into()),
        body: "**1. History**\n\nYuna took the salt throne at nineteen.".into(),
        properties: r#"{"tags":["coast"]}"#.into(),
    }];
    let edges = vec![EdgeRow {
        from_id: "king_yuna".into(),
        to_id: "the_salt_tithe".into(),
        relation: "links_to".into(),
        weight: 1.0,
    }];
    store.replace_graph("saltmarsh", &nodes, &edges).unwrap();

    assert_eq!(store.graph_nodes("saltmarsh").unwrap(), nodes);
    assert_eq!(store.graph_edges("saltmarsh").unwrap(), edges);

    // Replacing is a replacement, not an append.
    store.replace_graph("saltmarsh", &nodes, &[]).unwrap();
    assert_eq!(store.graph_nodes("saltmarsh").unwrap().len(), 1);
    assert!(store.graph_edges("saltmarsh").unwrap().is_empty());

    // Deleting the campaign takes its graph with it. The Godot build left
    // `<campaign>_embeddings.json` orphaned on disk.
    assert!(store.delete_campaign("saltmarsh").unwrap());
    assert!(store.graph_nodes("saltmarsh").unwrap().is_empty());
}

#[test]
fn migrations_apply_to_a_database_written_by_an_earlier_run() {
    // A genuine forward upgrade, not a reopen: the database is created at
    // schema version 1, exactly as a build predating §3.6 would have written
    // it, then opened normally and carried to version 2 with its campaign
    // intact.
    //
    // This is what `SaveManager._upgrade_save_state()` could not do reliably.
    // It inspected a version string, mutated the loaded dictionary in place,
    // and deliberately re-ran part of its own work every load because nothing
    // recorded what had already been applied.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("campaign.sqlite3");

    {
        let conn = orison_core::state::open_at_version(&path, 1).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
        // Chunking does not exist yet at this version.
        assert!(conn
            .prepare("SELECT 1 FROM chunks")
            .is_err_and(|e| e.to_string().contains("chunks")));

        conn.execute(
            "INSERT INTO campaigns (id, title, created_at, last_played)
             VALUES ('saltmarsh', 'The Salt Tithe', '2026-09-09T10:00:00Z', '2026-09-09T10:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO plot_flags (campaign_id, key, value)
             VALUES ('saltmarsh', 'tithe_abolished', 'false')",
            [],
        )
        .unwrap();
    }

    let upgraded = CampaignStore::open(&path).unwrap();
    assert_eq!(upgraded.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert_eq!(CURRENT_SCHEMA_VERSION, 2);

    // The campaign survived the upgrade.
    let campaign = upgraded.load_campaign("saltmarsh").unwrap().unwrap();
    assert_eq!(campaign.title, "The Salt Tithe");
    assert_eq!(
        upgraded.plot_flag("saltmarsh", "tithe_abolished").unwrap(),
        Some("false".into())
    );

    // And what version 2 added now works on it.
    let mut upgraded = upgraded;
    upgraded
        .replace_chunks(
            "saltmarsh",
            &[ChunkRow {
                id: "king_yuna#0".into(),
                entity_id: "king_yuna".into(),
                ordinal: 0,
                heading: Some("History".into()),
                text: "Yuna took the salt throne at nineteen.".into(),
                char_start: 0,
                char_end: 37,
            }],
        )
        .unwrap();
    assert_eq!(upgraded.chunks("saltmarsh").unwrap().len(), 1);
}

#[test]
fn chunks_round_trip_and_cascade_with_their_campaign() {
    let mut store = store_with_campaign();
    let rows = vec![
        ChunkRow {
            id: "king_yuna#0".into(),
            entity_id: "king_yuna".into(),
            ordinal: 0,
            heading: Some("History".into()),
            text: "Yuna took the salt throne at nineteen.".into(),
            char_start: 0,
            char_end: 37,
        },
        ChunkRow {
            id: "king_yuna#1".into(),
            entity_id: "king_yuna".into(),
            ordinal: 1,
            heading: Some("Manner".into()),
            text: "Blunt to the point of rudeness.".into(),
            char_start: 30,
            char_end: 61,
        },
    ];
    store.replace_chunks("saltmarsh", &rows).unwrap();

    assert_eq!(store.chunks("saltmarsh").unwrap(), rows);
    assert_eq!(store.chunks_of("saltmarsh", "king_yuna").unwrap().len(), 2);
    assert_eq!(
        store
            .chunk("saltmarsh", "king_yuna#1")
            .unwrap()
            .unwrap()
            .heading,
        Some("Manner".into())
    );
    assert_eq!(store.chunk("saltmarsh", "absent#0").unwrap(), None);

    // Replacing is a replacement.
    store.replace_chunks("saltmarsh", &rows[..1]).unwrap();
    assert_eq!(store.chunks("saltmarsh").unwrap().len(), 1);

    store.delete_campaign("saltmarsh").unwrap();
    assert!(store.chunks("saltmarsh").unwrap().is_empty());
}
