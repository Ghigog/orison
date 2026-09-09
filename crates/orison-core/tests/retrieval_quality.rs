//! §3.4 exit criteria, measured against `fixtures/vaults/*/ground_truth.json`.
//!
//! What this asserts, and why each one:
//!
//! - Recall at the fixture's own `k` beats the post-fix Godot baseline in
//!   `eval_baseline.md` on every fixture. Those are the numbers the port has to
//!   beat; the as-found numbers were broken and beating those would prove
//!   nothing.
//! - Every stage's contribution is reported separately, so "add a stage only
//!   where the number actually moves" is a decision made from numbers.
//! - Precision is reported too. The Godot baseline does not have it, and
//!   `eval_baseline.md` is explicit that it is the weak point: `"what stopped
//!   the boundary war"` retrieves the right note and drags in 74 of 207.
//! - The positive control from `minimal` is kept and checked, because it is
//!   what makes any zero elsewhere trustworthy rather than a suspected harness
//!   fault. Phase 1's lesson: prove the negative before believing it.
//!
//! Everything here runs with no model. The dense half of the pipeline needs
//! real embeddings and is gated separately in `retrieval_dense.rs`.
//!
//! Run with `cargo test --test retrieval_quality -- --nocapture` to see the
//! per-stage table.

use std::path::{Path, PathBuf};

use orison_core::ingest::{ingest_vault, IngestOptions};
use orison_core::knowledge::{EntityId, KnowledgeGraph};
use orison_core::retrieval::{
    retrieve, LexicalIndex, MetadataFilter, NoRerank, PassageReranker, QualityReport, Reranker,
    RetrievalConfig,
};

struct Case {
    query: String,
    relevant: Vec<EntityId>,
    k: usize,
}

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

fn load(name: &str) -> (KnowledgeGraph, Vec<Case>) {
    load_with(name, &IngestOptions::default())
}

fn load_with(name: &str, options: &IngestOptions) -> (KnowledgeGraph, Vec<Case>) {
    let graph = ingest_vault(&fixture_root(name), options)
        .unwrap_or_else(|e| panic!("ingesting {name}: {e}"))
        .graph;

    let raw = std::fs::read_to_string(fixture_root(name).join("ground_truth.json")).unwrap();
    let truth: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let cases = truth["retrieval"]
        .as_array()
        .expect("no retrieval cases")
        .iter()
        .map(|case| {
            let relevant = case["relevant"]
                .as_array()
                .unwrap()
                .iter()
                .map(|label| {
                    let label = label.as_str().unwrap();
                    // Ground truth names entities by label; resolution is the
                    // graph's job, which is also a check that labels resolve.
                    graph
                        .resolve(label)
                        .unwrap_or_else(|| {
                            panic!("{name}: ground truth names unknown entity {label:?}")
                        })
                        .clone()
                })
                .collect();
            Case {
                query: case["query"].as_str().unwrap().to_string(),
                relevant,
                k: case["k"].as_u64().unwrap_or(5) as usize,
            }
        })
        .collect();
    (graph, cases)
}

/// One pipeline configuration, over one fixture.
fn measure(
    label: &str,
    graph: &KnowledgeGraph,
    lexical: &LexicalIndex,
    cases: &[Case],
    reranker: &dyn Reranker,
    expand_hops: usize,
    expand_from: usize,
) -> QualityReport {
    let mut report = QualityReport {
        label: label.to_string(),
        ..Default::default()
    };
    for case in cases {
        let config = RetrievalConfig {
            filter: MetadataFilter::everything(),
            limit: case.k,
            expand_hops,
            expand_from,
            ..Default::default()
        };
        let out = retrieve(&case.query, graph, lexical, None, reranker, &config).unwrap();
        report.accumulate(&case.query, &out.hits, &case.relevant, case.k);
    }
    report.finish()
}

fn stages(fixture: &str) -> (Vec<QualityReport>, KnowledgeGraph) {
    let (graph, cases) = load(fixture);
    let lexical = LexicalIndex::build(&graph).unwrap();
    let passage = PassageReranker::default();

    let reports = vec![
        measure("bm25", &graph, &lexical, &cases, &NoRerank, 0, 0),
        measure(
            "bm25 + graph expansion",
            &graph,
            &lexical,
            &cases,
            &NoRerank,
            1,
            2,
        ),
        measure("bm25 + rerank", &graph, &lexical, &cases, &passage, 0, 0),
        measure(
            "bm25 + graph expansion + rerank",
            &graph,
            &lexical,
            &cases,
            &passage,
            1,
            2,
        ),
    ];
    (reports, graph)
}

fn print_table(fixture: &str, reports: &[QualityReport]) {
    println!("\n=== {fixture} ===");
    println!(
        "{:<34} {:>8} {:>10} {:>6} {:>6}",
        "stage", "recall", "precision", "mrr", "empty"
    );
    for r in reports {
        println!(
            "{:<34} {:>8.3} {:>10.3} {:>6.3} {:>6}",
            r.label, r.recall, r.precision, r.mrr, r.empty
        );
    }
    for (query, recall, precision) in &reports[reports.len() - 1].per_query {
        println!("    r={recall:.2} p={precision:.2}  {query}");
    }
}

/// The post-fix Godot baselines from `eval_baseline.md`. Not the as-found
/// numbers: those were the broken ones, and Phase 1 existed to fix them before
/// anything was measured against them.
const GODOT_BASELINE_MINIMAL: f32 = 1.000;
const GODOT_BASELINE_MESSY: f32 = 0.933;
const GODOT_BASELINE_LARGE: f32 = 0.875;

fn best(reports: &[QualityReport]) -> &QualityReport {
    reports
        .iter()
        .max_by(|a, b| a.recall.partial_cmp(&b.recall).unwrap())
        .unwrap()
}

#[test]
fn minimal_meets_or_beats_the_godot_baseline() {
    let (reports, _) = stages("minimal");
    print_table("minimal", &reports);
    let best = best(&reports);
    assert!(
        best.recall >= GODOT_BASELINE_MINIMAL,
        "recall {:.3} below the Godot baseline {GODOT_BASELINE_MINIMAL:.3} ({})",
        best.recall,
        best.label
    );
    assert_eq!(best.empty, 0);
}

#[test]
fn messy_beats_the_godot_baseline() {
    let (reports, _) = stages("messy");
    print_table("messy", &reports);
    let best = best(&reports);
    assert!(
        best.recall > GODOT_BASELINE_MESSY,
        "recall {:.3} does not beat the Godot baseline {GODOT_BASELINE_MESSY:.3} ({})",
        best.recall,
        best.label
    );
    assert_eq!(best.empty, 0);
}

#[test]
fn large_beats_the_godot_baseline() {
    let (reports, _) = stages("large");
    print_table("large", &reports);
    let best = best(&reports);
    assert!(
        best.recall > GODOT_BASELINE_LARGE,
        "recall {:.3} does not beat the Godot baseline {GODOT_BASELINE_LARGE:.3} ({})",
        best.recall,
        best.label
    );
    assert_eq!(best.empty, 0);
}

#[test]
fn the_positive_control_still_passes() {
    // `minimal`'s control query embeds a node's full label verbatim. It is the
    // only shape the Godot build's inverted containment could match, and it is
    // what makes every other number in that fixture trustworthy. Deleting it
    // would make a future zero indistinguishable from a wiring fault.
    let (graph, cases) = load("minimal");
    let lexical = LexicalIndex::build(&graph).unwrap();
    let control = cases
        .iter()
        .find(|c| c.query == "tell me about thornwick archive")
        .expect("the positive control is missing from ground_truth.json");

    let out = retrieve(
        &control.query,
        &graph,
        &lexical,
        None,
        &NoRerank,
        &RetrievalConfig {
            filter: MetadataFilter::everything(),
            limit: control.k,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(out.hits[0].id, EntityId::slug("Thornwick Archive"));
}

#[test]
fn the_rare_proper_noun_case_is_what_bm25_is_here_for() {
    // B-9 in one query: "Quillion" appears in exactly one note out of 207 and
    // is out of vocabulary for any embedding model. Dense retrieval cannot do
    // this; BM25 does it without help.
    let (graph, _) = load("large");
    let lexical = LexicalIndex::build(&graph).unwrap();
    let out = retrieve(
        "Quillion",
        &graph,
        &lexical,
        None,
        &NoRerank,
        &RetrievalConfig {
            filter: MetadataFilter::everything(),
            limit: 10,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(out.hits[0].id, EntityId::slug("The Quillion Accord"));
}

#[test]
fn the_two_documented_hard_queries_are_re_tested_and_reported() {
    // `eval_baseline.md` keeps these as targets, not blockers: messy's granary
    // query scored 0.67 and large's accord query 0.50, both needing genuine
    // multi-hop reasoning. The exit criterion is that their outcome is
    // recorded plainly either way, so this test prints and asserts the
    // direction of travel rather than a number pulled from nowhere.
    let (messy_reports, _) = stages("messy");
    let granary = best(&messy_reports)
        .recall_of("who was at the granary")
        .expect("query missing from the fixture");
    println!("\nmessy / 'who was at the granary': {granary:.2} (Godot lexical-only: 0.67)");

    let (large_reports, _) = stages("large");
    let accord = best(&large_reports)
        .recall_of("who keeps the accord")
        .expect("query missing from the fixture");
    println!("large / 'who keeps the accord': {accord:.2} (Godot lexical-only: 0.50)");

    assert!(
        granary >= 0.67,
        "regression on the granary case: {granary:.2} < 0.67"
    );
    assert!(
        accord >= 0.50,
        "regression on the accord case: {accord:.2} < 0.50"
    );
}

#[test]
fn precision_is_measured_and_not_catastrophic() {
    // The Godot build has no precision figure at all, and the one anecdote in
    // `eval_baseline.md` is 1 relevant note in 74 returned. Any number here is
    // new information; this asserts the floor rather than a target, since
    // there is nothing to compare against yet.
    let (reports, _) = stages("large");
    let best = best(&reports);
    println!(
        "\nlarge precision at the fixture's own k: {:.3} ({})",
        best.precision, best.label
    );
    assert!(
        best.precision > 0.10,
        "precision {:.3} is worse than one relevant result in ten",
        best.precision
    );
}

#[test]
fn mention_edges_are_what_carry_the_accord_query() {
    // The measurement that decided `IngestOptions::link_mentions`. Without
    // prose-name edges, `large` sits exactly on the Godot baseline of 0.875 and
    // cannot beat it: `"who keeps the accord"` needs the hop from
    // `The Quillion Accord` to `Pale Reach Chapel`, and the Accord note names
    // the chapel in a sentence rather than in a `[[wiki-link]]`. Recorded here
    // so the option's default is a measured decision rather than a preference.
    let without = IngestOptions {
        link_mentions: false,
        ..Default::default()
    };
    let (graph, cases) = load_with("large", &without);
    let lexical = LexicalIndex::build(&graph).unwrap();
    let report = measure(
        "no mention edges",
        &graph,
        &lexical,
        &cases,
        &NoRerank,
        1,
        2,
    );
    println!(
        "\nlarge without mention edges: recall {:.3} (accord {:.2})",
        report.recall,
        report.recall_of("who keeps the accord").unwrap()
    );
    assert_eq!(
        report.recall_of("who keeps the accord"),
        Some(0.5),
        "without mention edges the accord query should still be the documented 0.50"
    );

    let (graph, cases) = load("large");
    let lexical = LexicalIndex::build(&graph).unwrap();
    let with = measure("mention edges", &graph, &lexical, &cases, &NoRerank, 1, 2);
    println!(
        "large with mention edges:    recall {:.3} (accord {:.2})",
        with.recall,
        with.recall_of("who keeps the accord").unwrap()
    );
    assert!(
        with.recall > report.recall,
        "mention edges did not move the number they were added for"
    );
}

#[test]
fn reranking_moves_mrr_and_not_recall_on_these_fixtures() {
    // Recorded plainly, as the handoff asks: on the fixtures available, the
    // reranker does not move recall at all. Every relevant result these
    // queries can reach is already inside the top k before reranking, so a
    // set measure at k cannot see it. It does move mean reciprocal rank on
    // `messy`, which is the measure that can see a reordering.
    //
    // That is a weak result and it is stated as one. The stage is kept because
    // its cost is a single pass over candidates already in hand, and because
    // the precision problem it targets is real at a scale no fixture here
    // reaches — `eval_baseline.md` records 74 of 207 nodes returned for one
    // query. It is not kept because the plan mentions it.
    let (graph, cases) = load("messy");
    let lexical = LexicalIndex::build(&graph).unwrap();

    let plain = measure("no rerank", &graph, &lexical, &cases, &NoRerank, 0, 0);
    let reranked = measure(
        "passage rerank",
        &graph,
        &lexical,
        &cases,
        &PassageReranker::default(),
        0,
        0,
    );

    println!(
        "\nmessy rerank effect: recall {:.3} -> {:.3}, mrr {:.3} -> {:.3}",
        plain.recall, reranked.recall, plain.mrr, reranked.mrr
    );
    assert_eq!(
        plain.recall, reranked.recall,
        "recall is unchanged by reranking on this fixture"
    );
    assert!(
        reranked.mrr >= plain.mrr,
        "reranking must not push relevant results down: {:.3} -> {:.3}",
        plain.mrr,
        reranked.mrr
    );
}
