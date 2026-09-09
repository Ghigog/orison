//! §3.6 exit criteria: a long note is chunked with overlap, retrieved chunks
//! carry their source note id, and that id is correct.
//!
//! One thing has to be said plainly before the numbers: **the fixture set
//! contains no long notes.** The longest file in `large` is 53 words. `large`
//! is a scale fixture — 207 files, built to break brute-force vector search —
//! not a length fixture, and at the default chunk size every note in every
//! fixture is a single chunk.
//!
//! So this suite does two things rather than pretending one covers both. It
//! exercises the mechanism against the real fixture at a chunk size that does
//! split its longest note, which keeps the provenance assertions anchored to
//! real vault text; and it exercises the default size against a note built long
//! enough to need it. Validating the default parameters against real multi-page
//! documents needs documents nobody has here, and is recorded as follow-up
//! rather than guessed at.

use std::path::{Path, PathBuf};

use orison_core::ingest::{chunk_entity, ingest_vault, Chunk, ChunkConfig, IngestOptions};
use orison_core::knowledge::{Entity, EntityId, EntityKind};
use orison_core::retrieval::{
    fold_chunk_hits, retrieve, LexicalIndex, MetadataFilter, NoRerank, RetrievalConfig, Scored,
};
use orison_core::state::{Campaign, CampaignStore, ChunkRow};

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

/// Small enough to split the fixture's longest note. See the module note.
fn fixture_chunking() -> ChunkConfig {
    ChunkConfig {
        target_words: 20,
        overlap_words: 6,
        min_words: 4,
    }
}

#[test]
fn the_longest_note_in_large_is_chunked_with_overlap_and_keeps_its_source_id() {
    let out = ingest_vault(
        &fixture_root("large"),
        &IngestOptions {
            chunking: fixture_chunking(),
            ..Default::default()
        },
    )
    .unwrap();

    // The longest note in the fixture, whichever it is: pinning a filename
    // would rot the moment the generator is re-run.
    let longest = out
        .graph
        .entities()
        .max_by_key(|e| e.body.split_whitespace().count())
        .unwrap();
    let words = longest.body.split_whitespace().count();
    println!("longest note in large: {} ({words} words)", longest.label);

    let chunks: Vec<&Chunk> = out
        .chunks
        .iter()
        .filter(|c| c.entity_id == longest.id)
        .collect();
    assert!(
        chunks.len() > 1,
        "{} produced {} chunk(s) at a 20-word target",
        longest.label,
        chunks.len()
    );

    for chunk in &chunks {
        // The exit criterion: the source note id is present and correct.
        assert_eq!(chunk.entity_id, longest.id);
        assert_eq!(Chunk::source_note(&chunk.id), Some(longest.id.clone()));
        // And the provenance range names exactly this text.
        assert_eq!(
            chunk.text,
            longest.body[chunk.char_start..chunk.char_end].trim()
        );
    }

    for pair in chunks.windows(2) {
        assert!(
            pair[1].char_start < pair[0].char_end,
            "consecutive chunks of {} do not overlap",
            longest.label
        );
    }
}

#[test]
fn every_note_in_every_fixture_is_chunked_and_nothing_is_orphaned() {
    for fixture in ["minimal", "messy", "large"] {
        let out = ingest_vault(
            &fixture_root(fixture),
            &IngestOptions {
                chunking: fixture_chunking(),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(out.report.chunks, out.chunks.len());
        assert!(
            out.chunks.len() >= out.graph.len(),
            "{fixture}: fewer chunks than notes"
        );

        for chunk in &out.chunks {
            assert!(
                out.graph.contains(&chunk.entity_id),
                "{fixture}: chunk {} points at an entity that is not in the graph",
                chunk.id
            );
        }
        // Every note with text has at least one chunk: a note that produces no
        // passage is a note that cannot be cited.
        for entity in out.graph.entities() {
            if entity.body.trim().is_empty() {
                continue;
            }
            assert!(
                out.chunks.iter().any(|c| c.entity_id == entity.id),
                "{fixture}: {} has body text but no chunks",
                entity.label
            );
        }
    }
}

#[test]
fn at_the_default_size_the_fixtures_are_one_chunk_per_note() {
    // Recorded rather than asserted as a virtue: this is what "no long notes in
    // the fixture set" looks like from the chunker's side, and it is why the
    // tests above pass an explicit config.
    let out = ingest_vault(&fixture_root("large"), &IngestOptions::default()).unwrap();
    assert_eq!(
        out.chunks.len(),
        out.graph.len(),
        "at the default 180-word target, large should be one chunk per note"
    );
}

#[test]
fn a_multi_page_note_splits_at_the_default_size() {
    // The default parameters, against a note long enough to need them, since
    // no fixture note is.
    let body: String = (0..8)
        .map(|p| {
            let sentence: String = (0..60)
                .map(|w| format!("para{p}word{w}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{sentence}.")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut note = Entity::new(
        EntityId::slug("A Long Lore Note"),
        "A Long Lore Note",
        EntityKind::Lore,
    );
    note.body = body.clone();

    let chunks = chunk_entity(&note, &ChunkConfig::default());
    assert!(
        chunks.len() >= 3,
        "480 words became {} chunks",
        chunks.len()
    );

    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.ordinal, i);
        assert_eq!(Chunk::source_note(&chunk.id).unwrap(), note.id);
        assert_eq!(chunk.text, body[chunk.char_start..chunk.char_end].trim());
    }
    for pair in chunks.windows(2) {
        assert!(pair[1].char_start < pair[0].char_end);
    }
}

#[test]
fn a_retrieved_chunk_resolves_to_the_note_it_came_from() {
    // The whole point of provenance, end to end: a chunk-level ranking folds
    // back to notes, and each note keeps the passage that earned it.
    let out = ingest_vault(
        &fixture_root("messy"),
        &IngestOptions {
            chunking: fixture_chunking(),
            ..Default::default()
        },
    )
    .unwrap();

    let landing = out.graph.resolve("Saltmarsh Landing").unwrap().clone();
    let chunk_hits: Vec<Scored> = out
        .chunks
        .iter()
        .filter(|c| c.entity_id == landing)
        .enumerate()
        .map(|(i, c)| Scored {
            id: EntityId::from_stored(c.id.clone()),
            score: 1.0 - i as f32 * 0.1,
        })
        .collect();
    assert!(chunk_hits.len() > 1, "need several chunks to fold");

    let folded = fold_chunk_hits(&chunk_hits);
    assert_eq!(
        folded.len(),
        1,
        "chunks of one note must collapse to one hit"
    );
    assert_eq!(folded[0].id, landing);
    let chunk_id = folded[0]
        .chunk_id
        .as_deref()
        .expect("provenance was dropped");
    assert_eq!(Chunk::source_note(chunk_id), Some(landing.clone()));

    // And the entity the citation points at is the one that has the text.
    let cited = out.chunks.iter().find(|c| c.id == chunk_id).unwrap();
    let entity = out.graph.get(&landing).unwrap();
    assert!(entity.body.contains(&cited.text));
}

#[test]
fn chunks_persist_with_their_provenance_intact() {
    let out = ingest_vault(
        &fixture_root("messy"),
        &IngestOptions {
            chunking: fixture_chunking(),
            ..Default::default()
        },
    )
    .unwrap();

    let mut store = CampaignStore::open_in_memory().unwrap();
    store
        .save_campaign(&Campaign::new("messy", "Messy", "2026-09-09T10:00:00Z"))
        .unwrap();
    out.graph.save(&mut store, "messy").unwrap();

    let rows: Vec<ChunkRow> = out
        .chunks
        .iter()
        .map(|c| ChunkRow {
            id: c.id.clone(),
            entity_id: c.entity_id.to_string(),
            ordinal: c.ordinal as i64,
            heading: c.heading.clone(),
            text: c.text.clone(),
            char_start: c.char_start as i64,
            char_end: c.char_end as i64,
        })
        .collect();
    store.replace_chunks("messy", &rows).unwrap();

    let loaded = store.chunks("messy").unwrap();
    assert_eq!(loaded.len(), out.chunks.len());

    // Offsets still name the right text after a round trip, which is what
    // makes a stored citation worth anything.
    for row in &loaded {
        let entity = out
            .graph
            .get(&EntityId::from_stored(row.entity_id.clone()))
            .unwrap();
        assert_eq!(
            row.text,
            entity.body[row.char_start as usize..row.char_end as usize].trim(),
            "{} no longer names its own text",
            row.id
        );
    }
}

#[test]
fn note_level_retrieval_is_unaffected_by_chunking() {
    // Chunking is additive. The §3.4 numbers were measured over entities and
    // must not move because chunks now exist alongside them.
    let out = ingest_vault(
        &fixture_root("messy"),
        &IngestOptions {
            chunking: fixture_chunking(),
            ..Default::default()
        },
    )
    .unwrap();
    let lexical = LexicalIndex::build(&out.graph).unwrap();
    assert_eq!(
        lexical.len(),
        out.graph.len(),
        "the lexical index indexes notes"
    );

    let hits = retrieve(
        "who knows about the hidden causeway",
        &out.graph,
        &lexical,
        None,
        &NoRerank,
        &RetrievalConfig {
            filter: MetadataFilter::everything(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        hits.hits[0].id,
        out.graph.resolve("Mira of the Fens").unwrap().clone()
    );
}
