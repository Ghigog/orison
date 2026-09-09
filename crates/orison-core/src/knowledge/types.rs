//! The typed entity vocabulary.
//!
//! The Godot build had none: a node was a `Dictionary` with `label`, `type`,
//! `desc` and `properties` keys, and `type` was whatever string happened to be
//! in the frontmatter. `"npc"` and `"character"` meant the same thing and were
//! compared separately at eleven call sites.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// A stable identifier for one entity, derived from its source file or its
/// frontmatter `id`. Slugged the same way `VaultCompiler` did — lowercased,
/// spaces to underscores — so ids from an existing Godot save still resolve.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityId(String);

impl EntityId {
    /// Slug a raw name. `"King Yuna"` and `"king yuna"` are the same entity;
    /// `"King_Yuna"` is too.
    pub fn slug(raw: &str) -> Self {
        let mut s = String::with_capacity(raw.len());
        for ch in raw.trim().chars() {
            match ch {
                ' ' | '\t' => s.push('_'),
                c => s.extend(c.to_lowercase()),
            }
        }
        EntityId(s)
    }

    /// Wrap an id that is already slugged (read back from the database).
    pub fn from_stored(s: impl Into<String>) -> Self {
        EntityId(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What kind of thing an entity is.
///
/// `Note` is new. The Godot build defaulted every unclassifiable file to
/// `"lore"`, which is a claim about the content rather than an admission that
/// nothing is known about it. `messy/notes/misc scratch.md` is the fixture for
/// exactly this: untyped, unstructured, and required to survive ingest without
/// being misclassified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Character,
    Location,
    Scene,
    Lore,
    Item,
    /// A file with no type, in a folder with no type hint. Retained and
    /// retrievable; claimed to be nothing in particular.
    Note,
    /// A RAPTOR summary node (§3.5). Carries `level` 1 or 2.
    Summary,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Character => "character",
            EntityKind::Location => "location",
            EntityKind::Scene => "scene",
            EntityKind::Lore => "lore",
            EntityKind::Item => "item",
            EntityKind::Note => "note",
            EntityKind::Summary => "summary",
        }
    }

    /// Parse a stored kind. Anything unrecognised becomes `Note`, never
    /// nothing: an entity with an unfamiliar type is still an entity.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "character" | "npc" => EntityKind::Character,
            "location" => EntityKind::Location,
            "scene" | "story" => EntityKind::Scene,
            "lore" => EntityKind::Lore,
            "item" => EntityKind::Item,
            "summary" => EntityKind::Summary,
            _ => EntityKind::Note,
        }
    }
}

/// The canonical fields ingest tries to fill from a note's sections.
///
/// Anything a heading does not map to goes to [`Entity::overflow`]. There is
/// deliberately no `_ =>` arm anywhere that drops content: that is the shape of
/// B-14 and of `rag_architecture.md` Bug 1, and it is the one thing this phase
/// must not rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalField {
    Biography,
    Personality,
    Appearance,
    Goals,
    Gender,
    Description,
    WritingStyle,
}

impl CanonicalField {
    pub fn as_str(self) -> &'static str {
        match self {
            CanonicalField::Biography => "biography",
            CanonicalField::Personality => "personality",
            CanonicalField::Appearance => "appearance",
            CanonicalField::Goals => "goals",
            CanonicalField::Gender => "gender",
            CanonicalField::Description => "description",
            CanonicalField::WritingStyle => "writing_style",
        }
    }
}

/// A section of a note that mapped to no canonical field.
///
/// `Saltmarsh Landing.md`'s `## Smells Like` is the fixture case: it matches
/// nothing, and it must still be retained and retrievable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverflowSection {
    /// `None` for text before the first heading.
    pub heading: Option<String>,
    pub content: String,
}

/// A relationship between two entities.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Frontmatter `connections:` between locations.
    ConnectedTo,
    /// A tag or wiki-link that lands on a location.
    AssociatedWith,
    /// A plain wiki-link.
    LinksTo,
    /// Frontmatter `relationships:`, carrying the author's own wording.
    Relationship(String),
    /// A RAPTOR summary to the nodes it summarises (§3.5).
    Summarises,
}

impl EdgeKind {
    pub fn as_str(&self) -> &str {
        match self {
            EdgeKind::ConnectedTo => "connected_to",
            EdgeKind::AssociatedWith => "associated_with",
            EdgeKind::LinksTo => "links_to",
            EdgeKind::Relationship(r) => r,
            EdgeKind::Summarises => "summarises",
        }
    }

    /// Read back a stored relation. Anything that is not one of the known
    /// structural relations is the author's own relationship wording, which is
    /// exactly how `VaultCompiler` stored it.
    pub fn from_stored(s: &str) -> Self {
        match s {
            "connected_to" => EdgeKind::ConnectedTo,
            "associated_with" => EdgeKind::AssociatedWith,
            "links_to" => EdgeKind::LinksTo,
            "summarises" => EdgeKind::Summarises,
            other => EdgeKind::Relationship(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: EntityId,
    pub to: EntityId,
    pub kind: EdgeKind,
    pub weight: f64,
}

/// One entity in the knowledge graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub label: String,
    pub kind: EntityKind,
    /// A short description, for prompt injection. Derived; never the only copy
    /// of anything.
    pub description: String,
    /// 0 for entities that came from a file, 1 or 2 for RAPTOR summaries.
    pub level: u8,
    /// Vault-relative path, when the entity came from a file.
    pub source_path: Option<String>,
    /// The complete original text of the note, always. This is B-14 as an
    /// invariant: extraction is additive to the source, never a replacement
    /// for it, so a heuristic that misfires costs quality and never data.
    pub body: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub fields: BTreeMap<CanonicalField, String>,
    /// Sections that mapped to no canonical field. Retained and indexed.
    pub overflow: Vec<OverflowSection>,
    /// Frontmatter and inferred properties, as JSON values.
    pub properties: BTreeMap<String, serde_json::Value>,
}

impl Entity {
    pub fn new(id: EntityId, label: impl Into<String>, kind: EntityKind) -> Self {
        Self {
            id,
            label: label.into(),
            kind,
            description: String::new(),
            level: 0,
            source_path: None,
            body: String::new(),
            aliases: Vec::new(),
            tags: Vec::new(),
            fields: BTreeMap::new(),
            overflow: Vec::new(),
            properties: BTreeMap::new(),
        }
    }

    pub fn field(&self, f: CanonicalField) -> Option<&str> {
        self.fields
            .get(&f)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    /// Every piece of text this entity can be retrieved by, in one string.
    ///
    /// Overflow sections are in here by construction, which is what "retained
    /// and made retrievable" has to mean to be worth anything.
    pub fn searchable_text(&self) -> String {
        let mut out = String::with_capacity(self.body.len() + 256);
        out.push_str(&self.label);
        for alias in &self.aliases {
            out.push('\n');
            out.push_str(alias);
        }
        for tag in &self.tags {
            out.push('\n');
            out.push_str(tag);
        }
        for value in self.fields.values() {
            out.push('\n');
            out.push_str(value);
        }
        for section in &self.overflow {
            out.push('\n');
            if let Some(h) = &section.heading {
                out.push_str(h);
                out.push('\n');
            }
            out.push_str(&section.content);
        }
        out.push('\n');
        out.push_str(&self.body);
        out
    }

    /// Fill empty canonical fields from an external extraction pass without
    /// overwriting anything already found in the source.
    ///
    /// This is where an LLM extraction pass belongs. The Godot build ran that
    /// pass as the *only* way to populate these fields, so a fenced JSON
    /// response (B-16) or an unrecognised heading (Bug 1) left a character with
    /// nothing at all. Here it can only add.
    pub fn merge_extracted_fields(
        &mut self,
        extracted: impl IntoIterator<Item = (CanonicalField, String)>,
    ) {
        for (field, value) in extracted {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let slot = self.fields.entry(field).or_default();
            if slot.trim().is_empty() {
                *slot = value.to_string();
            }
        }
    }
}
