//! Fixtures a turn runs against, and the stand-in re-exported from the crate.
//!
//! The stand-in itself lives in `orison_core::testing`, behind the
//! `test-support` feature, because `orison-cli` needs the same one. See the
//! note there on why there is exactly one.

// Each test binary compiles this file and uses a different subset of it.
#![allow(dead_code, unused_imports)]

use std::sync::Arc;

use orison_core::knowledge::{
    CanonicalField, Edge, EdgeKind, Entity, EntityId, EntityKind, KnowledgeGraph,
};
use orison_core::retrieval::LexicalIndex;
use orison_core::state::{Campaign, CampaignStore};
use orison_core::turn::{FixedClock, Session};

pub use orison_core::testing::{
    character_response_json, combined_response_json, director_response_json, fixture_root,
    fixture_script, test_tokenizer, FakeOllama, Observed, ADVERTISED_CONTEXT_LENGTH,
};

/// A two-character, two-location campaign with enough text for BM25 to have
/// something to rank.
pub fn test_session() -> (Session, Arc<std::sync::Mutex<CampaignStore>>) {
    let store = CampaignStore::open_in_memory().expect("in-memory campaign store");
    let campaign_id = "test-campaign";

    let mut campaign = Campaign::new(campaign_id, "The Guttering Lamp", "2026-01-01T00:00:00Z");
    campaign.active_character = "quillion".to_string();
    campaign.active_location = "counting-house".to_string();
    store.save_campaign(&campaign).expect("save campaign");

    let mut graph = KnowledgeGraph::new();

    let mut quillion = Entity::new(
        EntityId::from_stored("quillion"),
        "Quillion",
        EntityKind::Character,
    );
    quillion.description = "The ledger-keeper of the counting house.".to_string();
    quillion.body = "Quillion keeps the ledger of the boundary accord.".to_string();
    quillion.fields.insert(
        CanonicalField::Biography,
        "Quillion has kept the accord's ledger for nineteen years.".to_string(),
    );
    quillion
        .fields
        .insert(CanonicalField::Gender, "he/him".to_string());
    quillion.fields.insert(
        CanonicalField::Personality,
        "Precise, unhurried, allergic to flattery.".to_string(),
    );
    graph.insert(quillion);

    let mut house = Entity::new(
        EntityId::from_stored("counting-house"),
        "The Counting House",
        EntityKind::Location,
    );
    house.description = "A cold stone room of ledgers and tallies.".to_string();
    house.body = "The counting house holds every tally the accord ever needed.".to_string();
    graph.insert(house);

    let mut accord = Entity::new(
        EntityId::from_stored("boundary-accord"),
        "The Boundary Accord",
        EntityKind::Lore,
    );
    accord.description = "The treaty that stopped the boundary war.".to_string();
    accord.body =
        "The boundary accord ended the war and is kept in the counting house.".to_string();
    graph.insert(accord);

    graph.connect(Edge {
        from: EntityId::from_stored("quillion"),
        to: EntityId::from_stored("boundary-accord"),
        kind: EdgeKind::Relationship("keeps".to_string()),
        weight: 1.0,
    });

    let mut store = store;
    graph.save(&mut store, campaign_id).expect("save graph");
    let lexical = LexicalIndex::build(&graph).expect("build lexical index");

    let store = Arc::new(std::sync::Mutex::new(store));
    let session = Session::new(
        campaign_id,
        Arc::clone(&store),
        Arc::new(graph),
        Arc::new(lexical),
        Arc::new(FixedClock::default()),
    );
    (session, store)
}

/// A campaign built from one of the repository's fixture vaults, with the
/// transcript script that fixture defines.
///
/// `ground_truth.json` already carries `transcript_character`,
/// `transcript_script` and `transcript_forbidden_pronouns` — the same fields
/// `eval/EvalRunnerNode.gd` reads — so the §4.2 arms are scored on exactly
/// the transcripts the Godot narrative baseline was recorded on.
pub fn fixture_session(
    fixture: &str,
) -> (
    Session,
    Arc<std::sync::Mutex<CampaignStore>>,
    orison_core::turn::TranscriptScript,
) {
    let root = fixture_root(fixture);
    let script = fixture_script(fixture);

    let outcome =
        orison_core::ingest::ingest_vault(&root, &orison_core::ingest::IngestOptions::default())
            .expect("ingest the fixture vault");
    let graph = outcome.graph;

    let character_id = graph
        .resolve(&script.character)
        .unwrap_or_else(|| panic!("{} is not in the compiled graph", script.character))
        .clone();

    let mut store = CampaignStore::open_in_memory().expect("in-memory campaign store");
    let campaign_id = format!("eval-{fixture}");
    let mut campaign = Campaign::new(
        &campaign_id,
        format!("Eval {fixture}"),
        "2026-01-01T00:00:00Z",
    );
    campaign.active_character = character_id.as_str().to_string();
    campaign.writing_style = outcome.writing_style.clone();
    store.save_campaign(&campaign).expect("save campaign");
    graph.save(&mut store, &campaign_id).expect("save graph");

    let lexical = LexicalIndex::build(&graph).expect("build lexical index");
    let store = Arc::new(std::sync::Mutex::new(store));
    let session = Session::new(
        &campaign_id,
        Arc::clone(&store),
        Arc::new(graph),
        Arc::new(lexical),
        Arc::new(FixedClock::default()),
    );
    (session, store, script)
}
