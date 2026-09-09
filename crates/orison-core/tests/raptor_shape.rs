//! §3.5 exit criteria: L1/L2 generation runs on the `messy` and `large`
//! fixtures and produces the same *shape* of hierarchy as the Godot build.
//!
//! Shape, not membership. `linfa-clustering`'s k-means will not reproduce a
//! hand-rolled ten-iteration implementation's clusters and is not expected to;
//! what has to hold is `max(3, ceil(n/5))` level-1 clusters and
//! `max(1, ceil(l1/5))` level-2 clusters over the same candidate set.
//!
//! Real embeddings are not needed to check that, and using them would make this
//! a live test for no gain. The vectors here are deterministic and derived from
//! each entity's own text, which is enough to exercise clustering, the level
//! structure and the edges. Whether the *summaries* are any good needs a model
//! and is gated in `retrieval_dense.rs`'s style; that is a separate question
//! from whether the hierarchy is built correctly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use orison_core::ingest::{ingest_vault, IngestOptions};
use orison_core::knowledge::raptor::{build, cluster_count, MemberListSummariser, CANDIDATE_KINDS};
use orison_core::knowledge::{EdgeKind, EntityId, EntityKind, KnowledgeGraph, RaptorConfig};
use orison_core::state::{Campaign, CampaignStore};

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

fn graph_of(name: &str) -> KnowledgeGraph {
    ingest_vault(&fixture_root(name), &IngestOptions::default())
        .unwrap()
        .graph
}

/// A stable, low-dimensional vector per entity, derived from its text.
///
/// Not an embedding and not pretending to be: it is a deterministic stand-in
/// so the hierarchy's structure can be asserted without a model. Nothing here
/// asserts anything about semantic quality.
fn stub_embeddings(graph: &KnowledgeGraph) -> BTreeMap<EntityId, Vec<f32>> {
    const DIM: usize = 16;
    graph
        .entities()
        .map(|entity| {
            let mut vector = vec![0.0f32; DIM];
            for (i, word) in entity.searchable_text().split_whitespace().enumerate() {
                let hash = word
                    .bytes()
                    .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
                vector[hash as usize % DIM] += 1.0 / (1.0 + i as f32);
            }
            let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for v in &mut vector {
                    *v /= norm;
                }
            }
            (entity.id.clone(), vector)
        })
        .collect()
}

fn check_shape(fixture: &str) {
    let mut graph = graph_of(fixture);
    let embeddings = stub_embeddings(&graph);

    let expected_candidates = graph
        .entities()
        .filter(|e| e.level == 0 && CANDIDATE_KINDS.contains(&e.kind))
        .count();

    let config = RaptorConfig::default();
    let report = build(&mut graph, &embeddings, &MemberListSummariser, &config).unwrap();

    assert_eq!(report.candidates, expected_candidates);
    assert_eq!(
        report.l1_clusters,
        cluster_count(expected_candidates, 3, 5),
        "{fixture}: L1 count is not max(3, ceil(n/5))"
    );
    assert_eq!(
        report.l2_clusters,
        cluster_count(report.l1_clusters, 1, 5),
        "{fixture}: L2 count is not max(1, ceil(l1/5))"
    );

    // Every candidate is summarised by exactly one level-1 node: a hierarchy
    // that drops notes is the same defect as an ingest that drops sections.
    let mut covered = 0;
    for entity in graph.entities() {
        if entity.level != 0 || !CANDIDATE_KINDS.contains(&entity.kind) {
            continue;
        }
        let parents = graph
            .edges_of(&entity.id)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Summarises && e.to == entity.id)
            .count();
        assert_eq!(
            parents, 1,
            "{fixture}: {} is summarised by {parents} level-1 nodes",
            entity.id
        );
        covered += 1;
    }
    assert_eq!(covered, expected_candidates);

    // And every level-1 node has a level-2 parent.
    for id in &report.l1_ids {
        let parents = graph
            .edges_of(id)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Summarises && &e.to == id)
            .count();
        assert_eq!(parents, 1, "{fixture}: {id} has {parents} level-2 parents");
    }

    println!(
        "{fixture}: {} candidates -> {} L1 -> {} L2",
        report.candidates, report.l1_clusters, report.l2_clusters
    );
}

#[test]
fn messy_produces_the_godot_hierarchy_shape() {
    // 4 characters + 2 locations = 6 candidates -> max(3, 2) = 3 L1 -> 1 L2.
    check_shape("messy");
}

#[test]
fn large_produces_the_godot_hierarchy_shape() {
    // 170 characters + 30 locations = 200 candidates -> 40 L1 -> 8 L2.
    check_shape("large");
}

#[test]
fn summaries_are_retrievable_at_their_own_level() {
    // The point of the hierarchy: `rag_architecture.md` §1.3 has the Director
    // reading level 2 and the character agent reading level 0. If the levels
    // are not separable at retrieval time the hierarchy is decoration.
    use orison_core::retrieval::{
        retrieve, LexicalIndex, MetadataFilter, NoRerank, RetrievalConfig,
    };

    let mut graph = graph_of("messy");
    let embeddings = stub_embeddings(&graph);
    let report = build(
        &mut graph,
        &embeddings,
        &MemberListSummariser,
        &RaptorConfig::default(),
    )
    .unwrap();

    let lexical = LexicalIndex::build(&graph).unwrap();
    let query = "Mira of the Fens";

    let level0 = retrieve(
        query,
        &graph,
        &lexical,
        None,
        &NoRerank,
        &RetrievalConfig {
            filter: MetadataFilter::level(0),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(level0
        .hits
        .iter()
        .all(|h| graph.get(&h.id).unwrap().level == 0));

    let level1 = retrieve(
        query,
        &graph,
        &lexical,
        None,
        &NoRerank,
        &RetrievalConfig {
            filter: MetadataFilter::level(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!level1.hits.is_empty(), "no level-1 summary was reachable");
    assert!(level1
        .hits
        .iter()
        .all(|h| graph.get(&h.id).unwrap().level == 1));
    assert_eq!(report.l1_clusters, graph.by_level(1).count());
}

#[test]
fn the_hierarchy_survives_a_round_trip_through_sqlite() {
    let mut graph = graph_of("messy");
    let embeddings = stub_embeddings(&graph);
    let report = build(
        &mut graph,
        &embeddings,
        &MemberListSummariser,
        &RaptorConfig::default(),
    )
    .unwrap();

    let mut store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("messy", "Messy", "2026-09-09T10:00:00Z"))
        .unwrap();
    graph.save(&mut store, "messy").unwrap();

    let loaded = KnowledgeGraph::load(&store, "messy").unwrap();
    assert_eq!(loaded.by_level(1).count(), report.l1_clusters);
    assert_eq!(loaded.by_level(2).count(), report.l2_clusters);
    assert_eq!(
        loaded.by_kind(EntityKind::Summary).count(),
        report.l1_clusters + report.l2_clusters
    );
    for id in &report.l1_ids {
        assert!(loaded.contains(id), "{id} did not survive storage");
    }
}
