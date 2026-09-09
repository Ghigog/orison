//! Contact with a real vault.
//!
//! The handoff's closing advice: "The fastest way to find out whether any of
//! this is right is to ingest a real vault. The fixtures are small and
//! deliberately hostile; a real Obsidian vault is neither, and it will surface
//! heuristics that are wrong far faster than any test written against
//! `messy/`."
//!
//! ```bash
//! ORISON_TEST_VAULT=/path/to/vault \
//!   cargo test -p orison-core --test real_vault -- --nocapture
//! ```
//!
//! It skips loudly without that variable. **No vault content is printed** —
//! only counts, and note *paths* where a specific file needs looking at. The
//! one inviolable constraint in `AGENTS.md` applies to test output too.
//!
//! What each number means, and what to do about it, is in the report this
//! prints. Only one thing here is a hard failure: `unaccounted_sections` must
//! be empty, because "nothing is silently discarded" (§1.1) is an invariant
//! rather than a target.

use std::path::PathBuf;
use std::time::Instant;

use orison_core::ingest::{ingest_vault, IngestOptions};
use orison_core::knowledge::EntityKind;
use orison_core::retrieval::{
    format_context, retrieve, LexicalIndex, MetadataFilter, PassageReranker, RetrievalConfig,
};

fn vault() -> Option<PathBuf> {
    std::env::var("ORISON_TEST_VAULT").ok().map(PathBuf::from)
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

#[test]
fn ingest_a_real_vault_and_report() {
    let Some(root) = vault() else {
        eprintln!(
            "SKIPPED ingest_a_real_vault_and_report: set ORISON_TEST_VAULT to an Obsidian \
             vault directory. Nothing from it is printed except note paths."
        );
        return;
    };

    let started = Instant::now();
    let outcome = ingest_vault(&root, &IngestOptions::default()).expect("ingest the vault");
    let ingest_time = started.elapsed();
    let report = &outcome.report;
    let graph = &outcome.graph;

    println!("\n=== INGEST ({:?}) ===", ingest_time);
    println!("notes                {}", report.notes_seen);
    println!("entities             {}", graph.len());
    println!("edges                {}", graph.edge_count());
    println!("  of which mentions  {}", report.mention_edges);
    println!("chunks               {}", report.chunks);
    println!(
        "sections             {} seen, {} mapped ({:.1}%), {} overflowed ({:.1}%)",
        report.sections_seen,
        report.sections_mapped,
        percent(report.sections_mapped, report.sections_seen),
        report.sections_overflowed,
        percent(report.sections_overflowed, report.sections_seen),
    );

    println!("\n=== WHAT TO LOOK AT ===");

    // The classification heuristic is the thing most likely to be wrong at
    // real scale. A high untyped share means folder conventions this vault
    // uses are not in `IngestOptions::folder_types`.
    println!(
        "untyped notes        {} ({:.1}%)  <- high means folder_types needs this vault's \
         conventions",
        report.notes_without_a_type,
        percent(report.notes_without_a_type, report.notes_seen),
    );

    // A dangling link is a wiki-link to a note that does not exist. Some are
    // genuine (the author has not written that note yet); a lot of them
    // usually means link resolution is missing an alias convention.
    println!(
        "dangling links       {}  <- a few is normal; a lot means alias resolution is missing \
         something",
        report.dangling_links.len(),
    );
    for link in report.dangling_links.iter().take(10) {
        println!("    {} -> [[{}]]", link.from.as_str(), link.target);
    }

    println!(
        "gender conflicts     {}  <- frontmatter disagreeing with a heading; each one is a \
         wrong pronoun waiting to happen",
        report.gender_conflicts.len(),
    );
    for id in report.gender_conflicts.iter().take(10) {
        println!("    {}", id.as_str());
    }

    println!("\n=== ENTITY MIX ===");
    for kind in [
        EntityKind::Character,
        EntityKind::Location,
        EntityKind::Lore,
        EntityKind::Scene,
        EntityKind::Item,
        EntityKind::Note,
        EntityKind::Summary,
    ] {
        let count = graph.by_kind(kind).count();
        println!("{:<12} {count}", kind.as_str());
    }

    // Chunking was mechanism-verified but never corpus-tuned: the longest
    // note in any fixture is 53 words, so nothing in the fixture set
    // exercises multi-chunk behaviour at the default size. This is the first
    // corpus that can.
    let longest = graph
        .entities()
        .map(|e| e.body.split_whitespace().count())
        .max()
        .unwrap_or(0);
    let multi_chunk = {
        let mut counts = std::collections::HashMap::new();
        for chunk in &outcome.chunks {
            *counts.entry(chunk.entity_id.clone()).or_insert(0usize) += 1;
        }
        counts.values().filter(|n| **n > 1).count()
    };
    println!("\n=== CHUNKING (never corpus-tuned before now) ===");
    println!("longest note         {longest} words");
    println!(
        "notes over one chunk {multi_chunk} of {} ({:.1}%)",
        graph.len(),
        percent(multi_chunk, graph.len()),
    );

    let started = Instant::now();
    let lexical = LexicalIndex::build(graph).expect("build the lexical index");
    println!("\nindex build          {:?}", started.elapsed());

    // Retrieval at real scale, which is where the open questions live:
    // precision after graph expansion, and whether the reranker earns its
    // place. Queries come from the vault's own entity labels so nothing has
    // to be invented — and nothing from the vault is printed.
    let probes: Vec<String> = graph
        .by_kind(EntityKind::Character)
        .take(5)
        .map(|e| e.label.clone())
        .collect();
    if probes.is_empty() {
        println!("\nno characters in this vault; skipping the retrieval probes");
        return;
    }

    println!("\n=== RETRIEVAL AT THIS SCALE ===");
    println!("(query labels are this vault's, so they are not printed)");
    let reranker = PassageReranker::default();
    for expand in [0usize, 1] {
        let config = RetrievalConfig {
            filter: MetadataFilter::level(0),
            limit: 6,
            expand_hops: expand,
            expand_from: if expand > 0 { 2 } else { 0 },
            ..RetrievalConfig::default()
        };
        let mut total = std::time::Duration::ZERO;
        let mut context_chars = 0;
        for probe in &probes {
            let started = Instant::now();
            let result = retrieve(probe, graph, &lexical, None, &reranker, &config)
                .expect("retrieval succeeds");
            total += started.elapsed();
            // Word count stands in for tokens: no model is configured here,
            // and `length / 4` is exactly what this codebase does not do.
            context_chars +=
                format_context(&result.hits, graph, 1024, |s| s.split_whitespace().count()).len();
        }
        println!(
            "expand_hops={expand}  mean query {:?}, mean context {} chars",
            total / probes.len() as u32,
            context_chars / probes.len(),
        );
    }
    println!(
        "\nIf context feels padded in play, `RetrievalConfig::expand_from` and `expand_hops` \
         are the dials."
    );

    // The one invariant. §1.1: no information is ever silently discarded.
    assert!(
        report.unaccounted_sections.is_empty(),
        "{} sections were neither mapped nor retained, which breaks §1.1: {:?}",
        report.unaccounted_sections.len(),
        report
            .unaccounted_sections
            .iter()
            .take(20)
            .collect::<Vec<_>>(),
    );
    assert_eq!(report.sections_dropped, 0, "sections were dropped outright");
}
