//! §3.3 exit criteria: the knowledge graph is the only entity store, it
//! round-trips through SQLite, and no parallel entity dictionaries exist.

use std::path::{Path, PathBuf};

use orison_core::ingest::{ingest_vault, IngestOptions};
use orison_core::knowledge::{Edge, EdgeKind, Entity, EntityId, EntityKind, KnowledgeGraph};
use orison_core::state::{Campaign, CampaignStore};

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

fn messy_graph() -> KnowledgeGraph {
    ingest_vault(&fixture_root("messy"), &IngestOptions::default())
        .unwrap()
        .graph
}

#[test]
fn the_graph_round_trips_through_sqlite_without_losing_anything() {
    let graph = messy_graph();
    let mut store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("saltmarsh", "Messy", "2026-09-09T10:00:00Z"))
        .unwrap();

    graph.save(&mut store, "saltmarsh").unwrap();
    let loaded = KnowledgeGraph::load(&store, "saltmarsh").unwrap();

    assert_eq!(loaded.len(), graph.len());
    assert_eq!(loaded.edge_count(), graph.edge_count());

    for original in graph.entities() {
        let round_tripped = loaded
            .get(&original.id)
            .unwrap_or_else(|| panic!("{} did not survive the round trip", original.id));
        assert_eq!(
            round_tripped, original,
            "{} changed in storage",
            original.id
        );
    }

    // Aliases resolve after a reload, so name resolution is a property of the
    // graph rather than of the ingest run that built it.
    assert_eq!(
        loaded.resolve("The Landing"),
        Some(&EntityId::slug("Saltmarsh Landing"))
    );
}

#[test]
fn overflow_sections_survive_persistence() {
    // The `## Smells Like` payload has to be there after a save and load, or
    // "retained and retrievable" only holds until the game is closed.
    let graph = messy_graph();
    let mut store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("saltmarsh", "Messy", "2026-09-09T10:00:00Z"))
        .unwrap();
    graph.save(&mut store, "saltmarsh").unwrap();

    let loaded = KnowledgeGraph::load(&store, "saltmarsh").unwrap();
    let landing = loaded.get(&EntityId::slug("Saltmarsh Landing")).unwrap();
    assert!(landing
        .overflow
        .iter()
        .any(|s| s.content.contains("Rot, tar, and woodsmoke")));
    assert!(landing
        .searchable_text()
        .contains("Rot, tar, and woodsmoke"));
}

#[test]
fn saving_twice_replaces_rather_than_accumulates() {
    let graph = messy_graph();
    let mut store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("saltmarsh", "Messy", "2026-09-09T10:00:00Z"))
        .unwrap();
    graph.save(&mut store, "saltmarsh").unwrap();
    graph.save(&mut store, "saltmarsh").unwrap();

    let loaded = KnowledgeGraph::load(&store, "saltmarsh").unwrap();
    assert_eq!(loaded.len(), graph.len());
    assert_eq!(loaded.edge_count(), graph.edge_count());
}

#[test]
fn multi_hop_traversal_reaches_the_two_hop_neighbour() {
    // Mira links to the Landing, which connects to the Fen Marches. The Godot
    // retrieval path expanded exactly one degree, which is why the documented
    // two-hop queries fail there.
    let graph = messy_graph();
    let mira = EntityId::slug("Mira of the Fens");

    let one = graph.within(&mira, 1);
    assert!(one.contains(&EntityId::slug("Saltmarsh Landing")));
    assert!(!one.contains(&EntityId::slug("Fen Marches")));

    let two = graph.within(&mira, 2);
    assert!(two.contains(&EntityId::slug("Fen Marches")));
}

#[test]
fn entities_are_queryable_by_kind_without_a_side_index() {
    let graph = messy_graph();
    assert_eq!(graph.by_kind(EntityKind::Character).count(), 4);
    assert_eq!(graph.by_kind(EntityKind::Location).count(), 2);
    assert_eq!(graph.by_kind(EntityKind::Lore).count(), 1);
    assert_eq!(graph.by_kind(EntityKind::Note).count(), 1);
}

#[test]
fn a_malformed_properties_blob_is_an_error_not_a_silent_empty_entity() {
    use orison_core::state::{EdgeRow, NodeRow};
    let nodes = vec![NodeRow {
        id: "broken".into(),
        label: "Broken".into(),
        kind: "character".into(),
        description: String::new(),
        level: 0,
        source_path: None,
        body: "Body.".into(),
        properties: "{not json".into(),
    }];
    let err = KnowledgeGraph::from_rows(&nodes, &[] as &[EdgeRow]).unwrap_err();
    assert!(
        err.to_string().contains("broken"),
        "the error should name the row: {err}"
    );
}

#[test]
fn graph_edits_are_visible_immediately_because_there_is_one_copy() {
    let mut graph = KnowledgeGraph::new();
    let mut e = Entity::new(
        EntityId::slug("Bram Holt"),
        "Bram Holt",
        EntityKind::Character,
    );
    e.body = "Holds the toll post.".into();
    graph.insert(e);

    graph
        .get_mut(&EntityId::slug("Bram Holt"))
        .unwrap()
        .description = "Toll keeper.".into();

    assert_eq!(
        graph.get(&EntityId::slug("Bram Holt")).unwrap().description,
        "Toll keeper."
    );
}

#[test]
fn edges_survive_with_their_authored_relationship_wording() {
    let mut graph = KnowledgeGraph::new();
    for name in ["Elara Voss", "Bram Holt"] {
        graph.insert(Entity::new(
            EntityId::slug(name),
            name,
            EntityKind::Character,
        ));
    }
    graph.connect(Edge {
        from: EntityId::slug("Elara Voss"),
        to: EntityId::slug("Bram Holt"),
        kind: EdgeKind::Relationship("distrusts".into()),
        weight: 0.8,
    });

    let mut store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("c", "C", "2026-09-09T10:00:00Z"))
        .unwrap();
    graph.save(&mut store, "c").unwrap();

    let loaded = KnowledgeGraph::load(&store, "c").unwrap();
    assert_eq!(
        loaded.edges()[0].kind,
        EdgeKind::Relationship("distrusts".into())
    );
}

// ----------------------------------------------------------------------
// The structural exit criterion
// ----------------------------------------------------------------------

#[test]
fn no_parallel_entity_dictionaries_exist_outside_knowledge() {
    // The grep-able test the handoff asks for. `orison_audit.md` §9 records
    // three disagreeing entity stores in the Godot build; this is what stops
    // the fourth being added by accident.
    //
    // A map keyed by `EntityId` outside `knowledge/` is a cache of graph state
    // by definition: the graph is the only thing that mints those ids.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();

    for file in rust_files(&src) {
        if file.starts_with(src.join("knowledge")) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for (n, line) in text.lines().enumerate() {
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            let holds_entities = compact.contains("Map<EntityId")
                || compact.contains("Map<String,Entity>")
                || compact.contains("Map<EntityId,Entity>");
            if holds_entities {
                offenders.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "entity dictionaries outside knowledge/:\n{}",
        offenders.join("\n")
    );
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
