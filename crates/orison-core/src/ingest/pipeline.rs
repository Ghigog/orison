//! The ingest pipeline: a vault directory in, entities and edges out.
//!
//! Ports `VaultCompiler.run()`. Two structural differences are deliberate.
//!
//! **It is synchronous and takes no model.** `VaultCompiler` made one LLM call
//! per character file inside its first pass, so ingest could not run at all
//! without a live endpoint, and a fenced JSON response (B-16) left the
//! character with nothing. Here the deterministic section pass fills the
//! canonical fields, and a model — when one is available — can only top up what
//! the source did not say, via [`Entity::merge_extracted_fields`].
//!
//! **Nothing is discarded.** Every section either maps to a canonical field or
//! lands in [`Entity::overflow`], and the full source text is on every entity
//! regardless. [`IngestReport::sections_dropped`] exists to be asserted at
//! zero, and [`IngestReport::unaccounted_sections`] proves it by checking
//! coverage rather than by trusting a counter.

use std::collections::BTreeMap;
use std::path::Path;

use crate::knowledge::{
    CanonicalField, Edge, EdgeKind, Entity, EntityId, EntityKind, KnowledgeGraph, OverflowSection,
};

use super::assets::{self, AUDIO_KEYS, PORTRAIT_KEYS, SCENERY_KEYS};
use super::chunk::{chunk_entity, Chunk, ChunkConfig};
use super::classify::{self, GenderSource};
use super::error::IngestError;
use super::markdown::{self, Document};
use super::scan::{self, relative_string};
use super::sections;
use super::style;
use super::yaml::YamlValue;

/// Knobs the shell sets. Everything here has a defensible default; nothing here
/// is a model identifier (B-10).
#[derive(Debug, Clone)]
pub struct IngestOptions {
    /// Folder-to-type mappings the user chose during onboarding. Beats every
    /// heuristic.
    pub folder_types: BTreeMap<String, String>,
    /// Promote one note to a scene when the vault contains none.
    ///
    /// `_handle_scene_fallback()` did this unconditionally. It is a UI
    /// bootstrap requirement — something has to be the opening scene — not an
    /// ingest one, and applying it silently means a scratch note can end up
    /// labelled a scene. Off by default; Phase 4 owns scene selection and can
    /// ask for it.
    pub promote_scene_fallback: bool,
    /// Create `Mentions` edges where one note names another entity in prose
    /// without linking to it.
    ///
    /// On by default because it was measured, not assumed: without it, `large`
    /// recall sits exactly on the Godot baseline of 0.875 and cannot beat it,
    /// because `"who keeps the accord"` needs a hop from `The Quillion Accord`
    /// to `Pale Reach Chapel` and the Accord note names the chapel in a
    /// sentence rather than in a `[[wiki-link]]`. `tests/retrieval_quality.rs`
    /// measures both ways.
    pub link_mentions: bool,
    /// How long notes are split for retrieval (§3.6).
    pub chunking: ChunkConfig,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            folder_types: BTreeMap::new(),
            promote_scene_fallback: false,
            link_mentions: true,
            chunking: ChunkConfig::default(),
        }
    }
}

/// A wiki-link pointing at a note that does not exist.
///
/// `messy/30_Systems/lore/The Salt Tithe.md` links to `[[The Chancellor]]`,
/// which has no file. Recording it is the point: compilation must not abort and
/// must not invent an entity, and a vault author probably wants to know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanglingLink {
    pub from: EntityId,
    pub target: String,
}

/// What ingest did, in numbers that can be asserted on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestReport {
    pub notes_seen: usize,
    pub sections_seen: usize,
    pub sections_mapped: usize,
    pub sections_overflowed: usize,
    /// Always zero. Present so the exit criterion is a value and not a promise.
    pub sections_dropped: usize,
    /// Sections whose text could not be found anywhere on the resulting
    /// entity. Computed by comparison, not by counting, so it catches a loss
    /// that a counter would miss.
    pub unaccounted_sections: Vec<String>,
    pub dangling_links: Vec<DanglingLink>,
    /// Entities whose gender came from prose that contradicted their title.
    /// Worth a human read; see `rag_architecture.md` Bug 3.
    pub gender_conflicts: Vec<EntityId>,
    pub notes_without_a_type: usize,
    /// Edges created from a name written in prose rather than as a link.
    pub mention_edges: usize,
    /// Retrievable passages produced (§3.6).
    pub chunks: usize,
}

/// What a vault compiled to.
///
/// The entities live in the graph, not beside it. §3.3's rule is that the
/// graph is the only entity store, and an `IngestOutcome` that also carried a
/// `Vec<Entity>` would be the second one on the day it was written.
#[derive(Debug, Clone)]
pub struct IngestOutcome {
    pub graph: KnowledgeGraph,
    /// Retrievable passages, each carrying the id of the note it came from
    /// and its character range within that note (§3.6).
    pub chunks: Vec<Chunk>,
    pub writing_style: String,
    pub report: IngestReport,
}

/// Ingest a vault directory.
pub fn ingest_vault(
    vault_root: &Path,
    options: &IngestOptions,
) -> Result<IngestOutcome, IngestError> {
    let files = scan::scan(vault_root)?;
    let mut parsed: Vec<(String, Document)> = Vec::with_capacity(files.notes.len());
    for note in &files.notes {
        let full = vault_root.join(note);
        let source = std::fs::read_to_string(&full).map_err(|e| IngestError::Io {
            path: full.display().to_string(),
            detail: e.to_string(),
        })?;
        parsed.push((relative_string(note), markdown::parse(&source)));
    }
    Ok(ingest_documents(&parsed, &files, options))
}

/// The pure half, for tests and for callers that already have the text.
pub fn ingest_documents(
    parsed: &[(String, Document)],
    files: &scan::VaultFiles,
    options: &IngestOptions,
) -> IngestOutcome {
    let mut report = IngestReport {
        notes_seen: parsed.len(),
        ..Default::default()
    };

    let others: Vec<(&str, &Document)> = parsed.iter().map(|(p, d)| (p.as_str(), d)).collect();
    let mut built: Vec<Entity> = Vec::with_capacity(parsed.len());

    for (path, doc) in parsed {
        let kind = classify::classify(&doc.frontmatter, path, &options.folder_types);
        if kind == EntityKind::Note {
            report.notes_without_a_type += 1;
        }
        built.push(build_entity(path, doc, kind, files, &others, &mut report));
    }

    if options.promote_scene_fallback && !built.iter().any(|e| e.kind == EntityKind::Scene) {
        promote_scene_fallback(&mut built);
    }

    let is_character: Vec<bool> = built
        .iter()
        .map(|e| e.kind == EntityKind::Character)
        .collect();

    let mut graph = KnowledgeGraph::new();
    let mut ids: Vec<EntityId> = Vec::with_capacity(built.len());
    for entity in built {
        ids.push(graph.insert(entity));
    }

    // Edges resolve through the graph, which owns both name resolution and
    // entity kinds. There is no second map to keep in step with it.
    add_edges(parsed, &ids, &mut graph, &mut report);
    if options.link_mentions {
        report.mention_edges = link_mentions(&ids, &mut graph);
    }

    let writing_style = style::campaign_writing_style(
        &parsed
            .iter()
            .zip(&is_character)
            .map(|((p, d), is_char)| (p.as_str(), d, *is_char))
            .collect::<Vec<_>>(),
    );

    let chunks: Vec<Chunk> = graph
        .entities()
        .flat_map(|entity| chunk_entity(entity, &options.chunking))
        .collect();
    report.chunks = chunks.len();

    IngestOutcome {
        graph,
        chunks,
        writing_style,
        report,
    }
}

fn build_entity(
    path: &str,
    doc: &Document,
    kind: EntityKind,
    files: &scan::VaultFiles,
    others: &[(&str, &Document)],
    report: &mut IngestReport,
) -> Entity {
    let fm = &doc.frontmatter;
    let stem = classify::file_stem(path);
    let raw_id = fm
        .get("id")
        .map(YamlValue::as_display_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| stem.to_string());
    let id = EntityId::slug(&raw_id);

    let label = fm
        .get("name")
        .or_else(|| fm.get("title"))
        .map(YamlValue::as_display_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| stem.to_string());

    let mut entity = Entity::new(id.clone(), label.clone(), kind);
    entity.source_path = Some(path.to_string());
    // Unconditionally, for every kind. B-14: character nodes kept nothing raw,
    // so whatever the single extraction pass missed was gone for good.
    entity.body = doc.body.clone();

    entity.aliases = collect_aliases(fm);
    entity.tags = collect_tags(fm, doc);

    assign_sections(&mut entity, doc, stem, report);

    for (key, value) in fm {
        entity.properties.insert(key.clone(), value.to_json());
    }

    if kind == EntityKind::Character {
        let props = classify::creature_properties(fm, path);
        entity
            .properties
            .insert("is_creature".into(), props.is_creature.into());
        entity
            .properties
            .insert("can_speak".into(), props.can_speak.into());
        entity
            .properties
            .insert("humanoid".into(), props.humanoid.into());

        let gender = classify::infer_gender(fm, &label, &doc.body);
        if !gender.value.is_empty() {
            entity
                .fields
                .entry(CanonicalField::Gender)
                .or_insert(gender.value);
        }
        if gender.source == GenderSource::ProseOverTitle {
            report.gender_conflicts.push(id.clone());
        }

        let writing = style::character_writing_style(&label, id.as_str(), doc, others);
        if !writing.is_empty() {
            entity.fields.insert(CanonicalField::WritingStyle, writing);
        }

        if let Some(portrait) = assets::resolve_asset(
            fm,
            doc,
            &PORTRAIT_KEYS,
            &files.images,
            id.as_str(),
            &label,
            true,
        ) {
            entity.properties.insert("portrait".into(), portrait.into());
        }
        if let Some(voice) = assets::resolve_asset(
            fm,
            doc,
            &["voice", "voice_id", "speech", "voice_profile"],
            &files.audio,
            id.as_str(),
            &label,
            false,
        ) {
            entity.properties.insert("voice".into(), voice.into());
        }
    }

    if kind == EntityKind::Location {
        if let Some(bgm) = assets::resolve_asset(
            fm,
            doc,
            &AUDIO_KEYS,
            &files.audio,
            id.as_str(),
            &label,
            false,
        ) {
            entity.properties.insert("bgm".into(), bgm.into());
        }
        if let Some(bg) = assets::resolve_asset(
            fm,
            doc,
            &SCENERY_KEYS,
            &files.images,
            id.as_str(),
            &label,
            true,
        ) {
            entity.properties.insert("background".into(), bg.into());
        }
    }

    // Callouts and tables are content. The Godot parser extracted both and then
    // used neither, so a `> [!secret]` block reached the graph only by being
    // part of the body text it happened to sit in.
    if !doc.callouts.is_empty() {
        entity.properties.insert(
            "callouts".into(),
            serde_json::Value::Array(
                doc.callouts
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "kind": c.kind,
                            "title": c.title,
                            "content": c.content,
                        })
                    })
                    .collect(),
            ),
        );
    }
    if !doc.tables.is_empty() {
        entity.properties.insert(
            "tables".into(),
            serde_json::Value::Array(
                doc.tables
                    .iter()
                    .map(|t| serde_json::json!({ "headers": t.headers, "rows": t.rows }))
                    .collect(),
            ),
        );
    }
    if !doc.embeds.is_empty() {
        entity.properties.insert(
            "embeds".into(),
            serde_json::Value::Array(
                doc.embeds
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    entity.description = derive_description(&entity, fm, kind);
    entity
}

/// Map each section to a canonical field, or to overflow. There is no third
/// destination, and deliberately no `_ =>` arm that drops content.
fn assign_sections(entity: &mut Entity, doc: &Document, stem: &str, report: &mut IngestReport) {
    let primary = sections::primary_field(entity.kind);

    // A note that opens with its own title as an `# H1` — the default Obsidian
    // template — has no preamble, only a section headed with the entity's
    // name. That heading is the note's title, not a section label, so its body
    // is the note's opening prose. `Mira of the Fens.md` is exactly this shape
    // and has no other heading at all.
    let mut titles: Vec<String> = vec![
        sections::normalise_heading(&entity.label),
        sections::normalise_heading(stem),
        sections::normalise_heading(entity.id.as_str()),
    ];
    titles.extend(
        entity
            .aliases
            .iter()
            .map(|a| sections::normalise_heading(a)),
    );
    titles.retain(|t| !t.is_empty());

    for section in &doc.sections {
        if section.content.trim().is_empty() && section.heading.is_none() {
            continue;
        }
        report.sections_seen += 1;

        let field = match &section.heading {
            Some(heading) => {
                let normalised = sections::normalise_heading(heading);
                if titles.contains(&normalised) {
                    Some(primary)
                } else {
                    sections::canonical_field(&normalised, entity.kind)
                }
            }
            // Opening prose with no heading of its own belongs to the note's
            // primary field.
            None => Some(primary),
        };

        match field {
            Some(field) => {
                report.sections_mapped += 1;
                append_field(entity, field, &section.content);
            }
            None => {
                report.sections_overflowed += 1;
                entity.overflow.push(OverflowSection {
                    heading: section.heading.clone(),
                    content: section.content.clone(),
                });
            }
        }
    }

    // Coverage check, not a counter: every section's text must be findable on
    // the entity. A heuristic that silently ate one shows up here.
    let searchable = entity.searchable_text();
    for section in &doc.sections {
        let needle = section.content.trim();
        if needle.is_empty() {
            continue;
        }
        if !searchable.contains(needle) {
            report.sections_dropped += 1;
            report.unaccounted_sections.push(format!(
                "{}: {}",
                entity.id,
                section
                    .heading
                    .clone()
                    .unwrap_or_else(|| "(preamble)".into())
            ));
        }
    }
}

fn append_field(entity: &mut Entity, field: CanonicalField, content: &str) {
    let content = content.trim();
    if content.is_empty() {
        return;
    }
    let slot = entity.fields.entry(field).or_default();
    if slot.is_empty() {
        slot.push_str(content);
    } else {
        // Two sections mapping to the same field is normal — "History" and
        // "Background" both mean biography. Concatenating keeps both; the
        // Godot alias table would have kept whichever came last.
        slot.push_str("\n\n");
        slot.push_str(content);
    }
}

fn collect_aliases(fm: &BTreeMap<String, YamlValue>) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["aliases", "alias"] {
        match fm.get(key) {
            Some(YamlValue::List(items)) => out.extend(items.iter().cloned()),
            Some(other) => {
                let s = other.as_display_string();
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
            None => {}
        }
    }
    out.retain(|a| !a.trim().is_empty());
    out.dedup();
    out
}

fn collect_tags(fm: &BTreeMap<String, YamlValue>, doc: &Document) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |tag: &str| {
        let tag = tag.trim();
        if !tag.is_empty() && !out.iter().any(|t| t == tag) {
            out.push(tag.to_string());
        }
    };

    for key in ["tags", "tag"] {
        match fm.get(key) {
            Some(YamlValue::List(items)) => items.iter().for_each(|i| push(i)),
            Some(other) => push(&other.as_display_string()),
            None => {}
        }
    }
    for tag in &doc.tags {
        push(tag);
        // `#region/fens` is also a tag "fens", as in the Godot compiler: vault
        // authors nest tags and then refer to the leaf.
        if let Some(leaf) = tag.rsplit('/').next() {
            if leaf != tag {
                push(leaf);
            }
        }
    }
    out
}

/// The short text injected into prompts.
///
/// Ported preference order, with one addition: a canonical field found by the
/// section pass is preferred over a raw body prefix, which is what the Godot
/// build's LLM biography did when the call succeeded.
fn derive_description(
    entity: &Entity,
    fm: &BTreeMap<String, YamlValue>,
    kind: EntityKind,
) -> String {
    for key in ["description", "desc", "summary"] {
        if let Some(v) = fm.get(key) {
            let s = v.as_display_string().trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    for field in [CanonicalField::Description, CanonicalField::Biography] {
        if let Some(v) = entity.field(field) {
            return v.trim().to_string();
        }
    }
    // Same limits as `VaultCompiler`: characters and locations get a generous
    // prefix, everything else a short one.
    let limit = match kind {
        EntityKind::Character | EntityKind::Location => 4000,
        _ => 500,
    };
    entity
        .body
        .chars()
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
}

fn promote_scene_fallback(entities: &mut [Entity]) {
    const EXCLUDED_FOLDERS: [&str; 6] = [
        "/concepts/",
        "/systems/",
        "/gates/",
        "/rules/",
        "/templates/",
        "/meta/",
    ];
    for entity in entities.iter_mut() {
        if entity.body.trim().is_empty() {
            continue;
        }
        let path = entity
            .source_path
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let stem = classify::file_stem(&path);
        if matches!(stem, "writing_style" | "style" | "readme") {
            continue;
        }
        if EXCLUDED_FOLDERS.iter().any(|f| path.contains(f)) {
            continue;
        }
        if matches!(
            entity.kind,
            EntityKind::Character | EntityKind::Location | EntityKind::Lore | EntityKind::Item
        ) {
            continue;
        }
        entity.kind = EntityKind::Scene;
        return;
    }
}

fn add_edges(
    parsed: &[(String, Document)],
    ids: &[EntityId],
    graph: &mut KnowledgeGraph,
    report: &mut IngestReport,
) {
    // Collected first, then applied, because resolution borrows the graph.
    let mut pending: Vec<Edge> = Vec::new();

    for ((_, doc), from) in parsed.iter().zip(ids) {
        // Frontmatter `connections:`, between locations.
        if let Some(value) = doc.frontmatter.get("connections") {
            for target in list_of(value) {
                if let Some(to) = graph.resolve(&target) {
                    pending.push(Edge {
                        from: from.clone(),
                        to: to.clone(),
                        kind: EdgeKind::ConnectedTo,
                        weight: 1.0,
                    });
                }
            }
        }

        // Frontmatter `relationships:`, carrying the author's own wording.
        if let Some(YamlValue::Map(entries)) = doc.frontmatter.get("relationships") {
            for (target, relation) in entries {
                if let Some(to) = graph.resolve(target) {
                    pending.push(Edge {
                        from: from.clone(),
                        to: to.clone(),
                        kind: EdgeKind::Relationship(relation.clone()),
                        weight: 0.8,
                    });
                }
            }
        }

        // Tags that name a location associate with it, as in the Godot build.
        let tags = graph.get(from).map(|e| e.tags.clone()).unwrap_or_default();
        for tag in tags {
            if let Some(to) = graph.resolve(&tag) {
                if graph.kind_of(to) == Some(EntityKind::Location) {
                    pending.push(Edge {
                        from: from.clone(),
                        to: to.clone(),
                        kind: EdgeKind::AssociatedWith,
                        weight: 1.0,
                    });
                }
            }
        }

        // Wiki-links. The Godot build parsed these and used them only for
        // dangling-link detection; here they are the main source of edges.
        for link in &doc.wiki_links {
            match graph.resolve(&link.target) {
                Some(to) => {
                    let kind = if graph.kind_of(to) == Some(EntityKind::Location) {
                        EdgeKind::AssociatedWith
                    } else {
                        EdgeKind::LinksTo
                    };
                    pending.push(Edge {
                        from: from.clone(),
                        to: to.clone(),
                        kind,
                        weight: 1.0,
                    });
                }
                None => {
                    let dangling = DanglingLink {
                        from: from.clone(),
                        target: link.target.clone(),
                    };
                    if !report.dangling_links.contains(&dangling) {
                        report.dangling_links.push(dangling);
                    }
                }
            }
        }
    }

    for edge in pending {
        graph.connect(edge);
    }
}

/// Link entities that one note names in its prose without linking to.
///
/// Longest name first, whole-word only, and a matched span is consumed so
/// `"Aldous Vantareth the Younger"` never also counts as a mention of
/// `"Aldous Vantareth"`. `large` has 170 character files sharing a handful of
/// surnames, which is what makes that rule load-bearing rather than tidy.
///
/// A pair that any other edge already connects is left alone: an explicit link
/// is better evidence and there is no point recording the same relationship
/// twice with different strengths.
fn link_mentions(ids: &[EntityId], graph: &mut KnowledgeGraph) -> usize {
    /// Below this, a name is too short to be a confident match in prose.
    const MIN_NAME_LEN: usize = 4;

    let mut names: Vec<(String, EntityId)> = Vec::new();
    for id in ids {
        let Some(entity) = graph.get(id) else {
            continue;
        };
        for name in std::iter::once(&entity.label).chain(entity.aliases.iter()) {
            let name = name.trim().to_lowercase();
            if name.chars().count() < MIN_NAME_LEN {
                continue;
            }
            names.push((name, id.clone()));
        }
    }
    names.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

    let mut pending: Vec<Edge> = Vec::new();
    for id in ids {
        let Some(entity) = graph.get(id) else {
            continue;
        };
        let haystack = entity.searchable_text().to_lowercase();
        let mut claimed: Vec<(usize, usize)> = Vec::new();

        for (name, target) in &names {
            if target == id {
                continue;
            }
            let mut from = 0;
            while let Some(offset) = haystack[from..].find(name.as_str()) {
                let start = from + offset;
                let end = start + name.len();
                from = end;
                if !is_word_boundary(&haystack, start, end) {
                    continue;
                }
                if claimed.iter().any(|(s, e)| start < *e && end > *s) {
                    continue;
                }
                claimed.push((start, end));
                if !pending.iter().any(|e| &e.from == id && &e.to == target) {
                    pending.push(Edge {
                        from: id.clone(),
                        to: target.clone(),
                        kind: EdgeKind::Mentions,
                        weight: 0.5,
                    });
                }
                break;
            }
        }
    }

    let mut added = 0;
    for edge in pending {
        let already_connected = graph.edges_of(&edge.from).iter().any(|e| {
            (e.from == edge.from && e.to == edge.to) || (e.from == edge.to && e.to == edge.from)
        });
        if already_connected {
            continue;
        }
        if graph.connect(edge) {
            added += 1;
        }
    }
    added
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let before_ok = text[..start]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric());
    let after_ok = text[end..]
        .chars()
        .next()
        .is_none_or(|c| !c.is_alphanumeric());
    before_ok && after_ok
}

fn list_of(value: &YamlValue) -> Vec<String> {
    match value {
        YamlValue::List(items) => items.clone(),
        YamlValue::Map(entries) => entries.keys().cloned().collect(),
        other => vec![other.as_display_string()],
    }
}
