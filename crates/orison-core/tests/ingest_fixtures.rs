//! §3.2 acceptance tests, against the fixture vaults.
//!
//! `fixtures/vaults/messy/` is not a smoke test. Every file in it breaks the
//! Godot ingest path in one specific, documented way, and `ground_truth.json`
//! names the way. Each case below is one of those, pinned.
//!
//! No model and no network: everything here runs under `cargo test` alone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use orison_core::ingest::{ingest_vault, IngestOptions, IngestOutcome};
use orison_core::knowledge::{CanonicalField, EdgeKind, Entity, EntityId, EntityKind};

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/vaults")
        .join(name)
}

fn ingest(name: &str) -> IngestOutcome {
    ingest_vault(&fixture_root(name), &IngestOptions::default())
        .unwrap_or_else(|e| panic!("ingesting {name}: {e}"))
}

fn entity<'a>(out: &'a IngestOutcome, label: &str) -> &'a Entity {
    out.graph
        .entities()
        .find(|e| e.label == label)
        .unwrap_or_else(|| {
            let known: Vec<&str> = out.graph.entities().map(|e| e.label.as_str()).collect();
            panic!("no entity labelled {label:?}; found {known:?}")
        })
}

fn has_edge(out: &IngestOutcome, from: &str, to: &str) -> bool {
    let from = EntityId::slug(from);
    let to = EntityId::slug(to);
    out.graph
        .edges()
        .iter()
        .any(|e| e.from == from && e.to == to)
}

// ----------------------------------------------------------------------
// The headline exit criterion
// ----------------------------------------------------------------------

#[test]
fn messy_drops_no_sections() {
    // "zero silently dropped sections", the metric the Godot build has no way
    // to report. `unaccounted_sections` is computed by checking that every
    // parsed section's text is findable on the entity it came from, so it
    // catches a loss that a counter incremented by hand would not.
    let out = ingest("messy");
    assert_eq!(
        out.report.unaccounted_sections,
        Vec::<String>::new(),
        "sections went missing"
    );
    assert_eq!(out.report.sections_dropped, 0);
    assert!(
        out.report.sections_overflowed > 0,
        "the overflow bucket must actually be used; `## Smells Like` alone should land in it"
    );
    assert_eq!(
        out.report.sections_seen,
        out.report.sections_mapped + out.report.sections_overflowed,
        "every section has exactly one destination"
    );
}

#[test]
fn messy_retains_all_six_required_source_substrings() {
    // `ground_truth.json` -> `ingest_must_retain`. The Godot baseline reached
    // 6 of 6 only after the B-14 fix; before it, three were gone for good.
    let out = ingest("messy");
    let cases: [(&str, &str); 6] = [
        ("King Yuna", "salt throne at nineteen"),
        ("King Yuna", "abolished before he dies"),
        ("Saltmarsh Landing", "Rot, tar, and woodsmoke"),
        (
            "Sergeant Adah",
            "loyal to individuals rather than institutions",
        ),
        ("Mira of the Fens", "where the old causeway surfaces"),
        ("misc scratch", "both were at the granary"),
    ];
    for (label, needle) in cases {
        let e = entity(&out, label);
        assert!(
            e.searchable_text().contains(needle),
            "{label}: lost {needle:?}"
        );
    }
}

// ----------------------------------------------------------------------
// One test per hostile case in ground_truth.json
// ----------------------------------------------------------------------

#[test]
fn bold_numeric_headings_populate_canonical_fields() {
    // `hostile_cases[0]`, and `rag_architecture.md` Bug 1. `**1. History**`
    // normalised to `"**1.history**"` in the Godot build and matched no alias,
    // so the section was discarded outright.
    let out = ingest("messy");
    let yuna = entity(&out, "King Yuna");
    assert_eq!(yuna.kind, EntityKind::Character);

    for field in [
        CanonicalField::Biography,
        CanonicalField::Personality,
        CanonicalField::Appearance,
        CanonicalField::Goals,
    ] {
        assert!(
            yuna.field(field).is_some(),
            "King Yuna has no {}",
            field.as_str()
        );
    }
    assert!(yuna
        .field(CanonicalField::Biography)
        .unwrap()
        .contains("salt throne at nineteen"));
    // "What He Wants" is the goals section under a heading named nothing like
    // "Goals".
    assert!(yuna
        .field(CanonicalField::Goals)
        .unwrap()
        .contains("abolished before he dies"));
}

#[test]
fn a_note_with_no_frontmatter_is_typed_by_its_folder() {
    // `hostile_cases[1]`. Also carries a callout and an inline hashtag that
    // must survive.
    let out = ingest("messy");
    let mira = entity(&out, "Mira of the Fens");
    assert_eq!(mira.kind, EntityKind::Character);
    assert!(mira.field(CanonicalField::Biography).is_some());
    assert!(
        mira.tags.iter().any(|t| t == "reclusive"),
        "tags: {:?}",
        mira.tags
    );

    let callouts = mira.properties.get("callouts").expect("callout not kept");
    let text = callouts.to_string();
    assert!(text.contains("secret"));
    assert!(text.contains("old causeway surfaces"));
}

#[test]
fn a_fenced_yaml_block_does_not_truncate_the_file() {
    // `hostile_cases[2]`. Naive frontmatter detection reads the fence's `---`
    // as a second frontmatter block and loses everything after it.
    let out = ingest("messy");
    let adah = entity(&out, "Sergeant Adah");

    assert!(
        adah.field(CanonicalField::Personality)
            .is_some_and(|p| p.contains("loyal to individuals rather than institutions")),
        "text after the code fence was lost"
    );
    assert!(adah
        .field(CanonicalField::Biography)
        .is_some_and(|b| b.contains("eleven years in the salt guard")));

    // The frontmatter that really is frontmatter still parsed.
    assert_eq!(
        adah.properties.get("power").and_then(|v| v.as_i64()),
        Some(3)
    );
    assert_eq!(
        adah.properties.get("courage").and_then(|v| v.as_i64()),
        Some(4)
    );
    assert_eq!(
        adah.properties.get("wisdom").and_then(|v| v.as_i64()),
        Some(1)
    );

    // The fenced sample is a code sample, not a second `post: granary` field.
    assert!(!adah.properties.contains_key("post"));
}

#[test]
fn gender_comes_from_title_and_prose_never_from_the_name() {
    // `hostile_cases[3]`, and the headline regression: Lord Anneke's name
    // carries a strong feminine prior while the title and body are
    // unambiguously masculine. `rag_architecture.md` Bug 3.
    let out = ingest("messy");
    for label in ["King Yuna", "Lord Anneke"] {
        let e = entity(&out, label);
        let gender = e
            .field(CanonicalField::Gender)
            .unwrap_or_else(|| panic!("{label} has no inferred gender"));
        assert!(
            gender.contains("he"),
            "{label}: expected masculine pronouns, got {gender:?}"
        );
        assert!(!gender.contains("she"), "{label}: got {gender:?}");
    }
    // Adah declares hers in frontmatter and must be taken at her word.
    assert_eq!(
        entity(&out, "Sergeant Adah").field(CanonicalField::Gender),
        Some("female, she/her")
    );
}

#[test]
fn a_non_canonical_section_lands_in_overflow_and_stays_retrievable() {
    // `hostile_cases[4]`, and `rag_architecture.md` §1.1. `## Smells Like`
    // matches no canonical field and must survive somewhere retrievable
    // rather than being discarded.
    let out = ingest("messy");
    let landing = entity(&out, "Saltmarsh Landing");
    assert_eq!(landing.kind, EntityKind::Location);

    let smells = landing
        .overflow
        .iter()
        .find(|s| s.heading.as_deref() == Some("Smells Like"))
        .expect("`## Smells Like` was not routed to the overflow bucket");
    assert!(smells.content.contains("Rot, tar, and woodsmoke"));

    // "Retained" is worth nothing without "retrievable".
    assert!(landing
        .searchable_text()
        .contains("Rot, tar, and woodsmoke"));

    // Its table is captured too, rather than being silently dropped markup.
    let tables = landing.properties.get("tables").expect("table not kept");
    assert!(tables
        .to_string()
        .contains("causeway surfaces on the lowest tide"));

    // Aliases from frontmatter resolve.
    assert!(landing.aliases.iter().any(|a| a == "Saltmarsh"));
}

#[test]
fn a_dangling_wiki_link_is_recorded_and_invents_nothing() {
    // `hostile_cases[5]` and `must_not_exist`. `[[The Chancellor]]` has no
    // file: compilation must tolerate it without creating a phantom character.
    let out = ingest("messy");
    assert!(
        !out.graph.entities().any(|e| e.label == "The Chancellor"),
        "a phantom entity was invented for a dangling link"
    );
    assert!(
        out.report
            .dangling_links
            .iter()
            .any(|d| d.target == "The Chancellor"),
        "the dangling link was not reported: {:?}",
        out.report.dangling_links
    );
}

#[test]
fn an_untyped_scratch_note_survives_and_is_not_called_a_character() {
    // `hostile_cases[6]`. No type, no name, no headings, in a folder with no
    // heuristic. The data-lake principle says it is still retrievable.
    let out = ingest("messy");
    let scratch = entity(&out, "misc scratch");
    assert_eq!(
        scratch.kind,
        EntityKind::Note,
        "an unclassifiable note must not be guessed at"
    );
    assert!(scratch
        .searchable_text()
        .contains("both were at the granary"));
    assert!(scratch.tags.iter().any(|t| t == "todo"));
}

// ----------------------------------------------------------------------
// Entities and edges
// ----------------------------------------------------------------------

#[test]
fn messy_produces_every_ground_truth_entity_with_its_required_fields() {
    let out = ingest("messy");
    let expected: [(&str, EntityKind, &[CanonicalField]); 7] = [
        (
            "King Yuna",
            EntityKind::Character,
            &[
                CanonicalField::Biography,
                CanonicalField::Personality,
                CanonicalField::Appearance,
                CanonicalField::Goals,
            ],
        ),
        (
            "Lord Anneke",
            EntityKind::Character,
            &[CanonicalField::Biography],
        ),
        (
            "Sergeant Adah",
            EntityKind::Character,
            &[CanonicalField::Biography, CanonicalField::Personality],
        ),
        (
            "Mira of the Fens",
            EntityKind::Character,
            &[CanonicalField::Biography],
        ),
        ("Saltmarsh Landing", EntityKind::Location, &[]),
        ("Fen Marches", EntityKind::Location, &[]),
        ("The Salt Tithe", EntityKind::Lore, &[]),
    ];

    for (label, kind, fields) in expected {
        let e = entity(&out, label);
        assert_eq!(e.kind, kind, "{label} classified wrongly");
        for field in fields {
            assert!(
                e.field(*field).is_some(),
                "{label} is missing required field {}",
                field.as_str()
            );
        }
        assert!(
            !e.body.trim().is_empty(),
            "{label} kept no raw source (B-14)"
        );
    }
}

#[test]
fn messy_edges_match_ground_truth() {
    let out = ingest("messy");
    // `ground_truth.json` -> `edges`.
    assert!(
        has_edge(&out, "Lord Anneke", "Fen Marches")
            || has_edge(&out, "Fen Marches", "Lord Anneke")
    );
    assert!(has_edge(&out, "Mira of the Fens", "Saltmarsh Landing"));
    assert!(has_edge(&out, "The Salt Tithe", "Saltmarsh Landing"));
}

#[test]
fn wiki_links_to_locations_are_association_edges() {
    let out = ingest("messy");
    let from = EntityId::slug("Mira of the Fens");
    let to = EntityId::slug("Saltmarsh Landing");
    let edges = out.graph.edges();
    let edge = edges
        .iter()
        .find(|e| e.from == from && e.to == to)
        .expect("edge missing");
    assert_eq!(edge.kind, EdgeKind::AssociatedWith);
}

// ----------------------------------------------------------------------
// minimal/ and large/
// ----------------------------------------------------------------------

#[test]
fn minimal_ingests_cleanly() {
    let out = ingest("minimal");
    assert_eq!(out.report.unaccounted_sections, Vec::<String>::new());

    let elara = entity(&out, "Elara Voss");
    assert_eq!(elara.field(CanonicalField::Gender), Some("female, she/her"));
    assert!(elara
        .field(CanonicalField::Appearance)
        .unwrap()
        .contains("braid"));

    let kettle = entity(&out, "The Kettle");
    assert_eq!(
        kettle
            .properties
            .get("is_creature")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        kettle.properties.get("can_speak").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        kettle.properties.get("humanoid").and_then(|v| v.as_bool()),
        Some(false)
    );

    // The two locations link to each other via the mill road.
    assert!(has_edge(&out, "Thornwick Archive", "Stonebridge"));
    assert!(has_edge(&out, "Stonebridge", "Thornwick Archive"));
}

#[test]
fn large_ingests_completely() {
    // 207 files. The point is coverage and the absence of loss, not quality;
    // retrieval quality against `large` is §3.4's business.
    let out = ingest("large");
    assert_eq!(out.report.notes_seen, 207);
    assert_eq!(out.graph.len(), 207);
    assert_eq!(out.report.unaccounted_sections, Vec::<String>::new());
    assert!(out.graph.entities().all(|e| !e.body.trim().is_empty()));

    // The needle the retrieval fixtures are built around must exist and keep
    // its rare proper noun.
    let accord = entity(&out, "The Quillion Accord");
    assert!(accord.searchable_text().contains("Quillion"));
}

// ----------------------------------------------------------------------
// Options
// ----------------------------------------------------------------------

#[test]
fn the_scene_fallback_is_opt_in() {
    // `_handle_scene_fallback()` ran unconditionally and would relabel
    // `misc scratch.md` a scene. That is a UI bootstrap requirement, not an
    // ingest one, so it is off unless asked for.
    let root = fixture_root("messy");
    let default = ingest_vault(&root, &IngestOptions::default()).unwrap();
    assert!(!default
        .graph
        .entities()
        .any(|e| e.kind == EntityKind::Scene));

    let promoted = ingest_vault(
        &root,
        &IngestOptions {
            promote_scene_fallback: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        promoted
            .graph
            .entities()
            .filter(|e| e.kind == EntityKind::Scene)
            .count(),
        1
    );
}

#[test]
fn explicit_folder_mappings_beat_every_heuristic() {
    let mut folder_types = BTreeMap::new();
    folder_types.insert("notes".to_string(), "lore".to_string());
    let out = ingest_vault(
        &fixture_root("messy"),
        &IngestOptions {
            folder_types,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(entity(&out, "misc scratch").kind, EntityKind::Lore);
}
