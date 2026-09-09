//! The half of §3.4 that needs a real embedding model.
//!
//! BM25, fusion, expansion and reranking are all measurable with `cargo test`
//! alone and live in `retrieval_quality.rs`. Dense retrieval cannot be: a
//! synthetic vector proves the index works, not that retrieval works, and a
//! hand-rolled stand-in for an embedding would produce numbers that look like
//! evidence and are not.
//!
//! So this is gated, and it **skips loudly**, per the Phase 1 lesson that a
//! metric which never fires is not a metric. It reuses
//! `ORISON_TEST_OLLAMA_URL` from the Phase 2 conformance suite and adds
//! `ORISON_TEST_OLLAMA_EMBED_MODEL`, because a chat model identifier is not an
//! embedding model identifier and reusing `ORISON_TEST_OLLAMA_MODEL` for both
//! would ask Ollama to embed with a model that cannot.
//!
//! ```text
//! ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
//! ORISON_TEST_OLLAMA_EMBED_MODEL=nomic-embed-text \
//!   cargo test --test retrieval_dense -- --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::str::FromStr;

use orison_core::inference::{InferenceBackend, OllamaBackend};
use orison_core::ingest::{ingest_vault, IngestOptions};
use orison_core::knowledge::{EntityId, KnowledgeGraph};
use orison_core::retrieval::{
    retrieve, DenseIndex, LexicalIndex, MetadataFilter, NoRerank, QualityReport, RetrievalConfig,
    VectorOwner,
};
use orison_core::state::{Campaign, CampaignStore};

fn live_embedding_config() -> Option<(String, String)> {
    let url = std::env::var("ORISON_TEST_OLLAMA_URL").ok()?;
    let model = std::env::var("ORISON_TEST_OLLAMA_EMBED_MODEL").ok()?;
    Some((url, model))
}

fn skip_notice() {
    eprintln!(
        "SKIPPED: set ORISON_TEST_OLLAMA_URL and ORISON_TEST_OLLAMA_EMBED_MODEL \
         to run the dense half of the retrieval suite against a live Ollama"
    );
}

fn minimal_tokenizer() -> tokenizers::Tokenizer {
    tokenizers::Tokenizer::from_str(
        r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":{"type":"Whitespace"},"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0},"unk_token":"[UNK]"}}"#,
    )
    .expect("minimal tokenizer json is valid")
}

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

struct Case {
    query: String,
    relevant: Vec<EntityId>,
    k: usize,
}

fn load(name: &str) -> (KnowledgeGraph, Vec<Case>) {
    let graph = ingest_vault(&fixture_root(name), &IngestOptions::default())
        .unwrap()
        .graph;
    let raw = std::fs::read_to_string(fixture_root(name).join("ground_truth.json")).unwrap();
    let truth: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let cases = truth["retrieval"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| Case {
            query: case["query"].as_str().unwrap().to_string(),
            relevant: case["relevant"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| graph.resolve(l.as_str().unwrap()).unwrap().clone())
                .collect(),
            k: case["k"].as_u64().unwrap_or(5) as usize,
        })
        .collect();
    (graph, cases)
}

/// Ingest `messy`, embed every entity, and compare BM25 alone against the full
/// hybrid pipeline on the same queries.
#[tokio::test]
async fn dense_retrieval_and_fusion_against_a_live_embedding_model() {
    let Some((url, model)) = live_embedding_config() else {
        skip_notice();
        return;
    };

    let backend = OllamaBackend::connect(&url, &model, minimal_tokenizer())
        .await
        .expect("connect to the configured Ollama");

    let (graph, cases) = load("messy");
    let lexical = LexicalIndex::build(&graph).unwrap();

    let store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("messy", "Messy", "2026-09-09T10:00:00Z"))
        .unwrap();

    // One embed call to learn the model's dimensionality: it is configuration,
    // not a constant (B-10), and hardcoding 768 for `nomic-embed-text` is the
    // exact shape of assumption this codebase keeps paying for.
    let probe = backend
        .embed(&["dimension probe".to_string()])
        .await
        .expect("embed");
    let dim = probe[0].len();
    println!("embedding model {model} reports {dim} dimensions");

    let index = DenseIndex::open(&store, "messy", &model, dim).unwrap();
    for entity in graph.entities() {
        let text = entity.searchable_text();
        let vectors = backend.embed(&[text]).await.expect("embed entity");
        index
            .upsert(VectorOwner::Entity, entity.id.as_str(), &vectors[0])
            .unwrap();
    }
    assert_eq!(index.len().unwrap(), graph.len());

    let mut lexical_only = QualityReport {
        label: "bm25 only".into(),
        ..Default::default()
    };
    let mut hybrid = QualityReport {
        label: "bm25 + dense + rrf".into(),
        ..Default::default()
    };

    for case in &cases {
        let config = RetrievalConfig {
            filter: MetadataFilter::everything(),
            limit: case.k,
            ..Default::default()
        };
        let bm25 = retrieve(&case.query, &graph, &lexical, None, &NoRerank, &config).unwrap();
        lexical_only.accumulate(&case.query, &bm25.hits, &case.relevant, case.k);

        let query_vector = backend
            .embed(std::slice::from_ref(&case.query))
            .await
            .expect("embed query");
        let fused = retrieve(
            &case.query,
            &graph,
            &lexical,
            Some((&index, &query_vector[0])),
            &NoRerank,
            &config,
        )
        .unwrap();
        assert!(
            fused.stages.dense > 0,
            "the dense stage contributed nothing for {:?}, which means it is not wired, \
             not that it disagreed",
            case.query
        );
        hybrid.accumulate(&case.query, &fused.hits, &case.relevant, case.k);
    }

    let lexical_only = lexical_only.finish();
    let hybrid = hybrid.finish();
    for report in [&lexical_only, &hybrid] {
        println!(
            "{:<22} recall {:.3}  precision {:.3}  mrr {:.3}",
            report.label, report.recall, report.precision, report.mrr
        );
    }

    // The assertion is that adding dense retrieval does not make recall worse.
    // Whether it makes it *better* on these fixtures is the measurement, and
    // asserting a specific improvement would be asserting a number nobody has
    // observed yet.
    assert!(
        hybrid.recall >= lexical_only.recall,
        "fusing dense retrieval in cost recall: {:.3} -> {:.3}",
        lexical_only.recall,
        hybrid.recall
    );
}

/// B-9, live: dense retrieval is structurally weak on invented proper nouns,
/// and BM25 is not. Reported rather than asserted, because it is a claim about
/// the embedding model rather than about this code.
#[tokio::test]
async fn the_rare_proper_noun_case_reported_against_a_live_model() {
    let Some((url, model)) = live_embedding_config() else {
        skip_notice();
        return;
    };
    let backend = OllamaBackend::connect(&url, &model, minimal_tokenizer())
        .await
        .expect("connect");

    let (graph, _) = load("large");
    let store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("large", "Large", "2026-09-09T10:00:00Z"))
        .unwrap();

    let probe = backend.embed(&["probe".to_string()]).await.expect("embed");
    let index = DenseIndex::open(&store, "large", &model, probe[0].len()).unwrap();
    for entity in graph.entities() {
        let vectors = backend
            .embed(&[entity.searchable_text()])
            .await
            .expect("embed");
        index
            .upsert(VectorOwner::Entity, entity.id.as_str(), &vectors[0])
            .unwrap();
    }

    let target = EntityId::slug("The Quillion Accord");
    for query in ["Quillion", "what stopped the boundary war"] {
        let vector = backend.embed(&[query.to_string()]).await.expect("embed");
        let dense = index.search(&vector[0], 10, VectorOwner::Entity).unwrap();
        let rank = dense.iter().position(|h| h.id == target);
        println!(
            "dense rank of The Quillion Accord for {query:?}: {}",
            rank.map(|r| (r + 1).to_string())
                .unwrap_or_else(|| "not in top 10".into())
        );
    }
}
