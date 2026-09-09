//! Real BM25, via `tantivy`.
//!
//! The Godot build advertised "Hybrid BM25 + KNN semantic retrieval" and never
//! had BM25 at all. What it had was, first, an inverted containment test
//! (B-13: it asked whether the *query* contained the node's *label*, so 13 of
//! 14 fixture queries retrieved nothing), and after the Phase 1 fix,
//! term-overlap scoring with no inverse document frequency and no length
//! normalisation. Both of those consequences matter here:
//!
//! - **No IDF** means a query term that appears in every note counts as much as
//!   one that appears in a single note. On `large`, where 170 of 207 files are
//!   characters sharing a handful of surnames, that is most of the corpus.
//! - **No length normalisation** means a long note is easier to match than a
//!   short one, purely for being long. `The Quillion Accord` is three
//!   sentences.
//!
//! BM25 fixes both by construction, which is the entire argument for using an
//! index rather than a scoring loop.

use std::collections::HashMap;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::{Index, IndexReader, TantivyDocument};

use crate::knowledge::{Entity, EntityId, KnowledgeGraph};

use super::error::RetrievalError;
use super::types::{MetadataFilter, Scored};

/// Field weights.
///
/// An entity named in the query is almost always its subject, so the label and
/// its aliases carry the most weight; tags are the vault author's own grouping
/// mechanism and carry nearly as much. The ordering is inherited from the
/// Phase 1 lexical fix, which measured well; the values are multipliers into
/// BM25 rather than the flat additive bonuses that scoring loop used.
const LABEL_BOOST: f32 = 4.0;
const ALIAS_BOOST: f32 = 3.0;
const TAG_BOOST: f32 = 2.5;
const FIELD_BOOST: f32 = 1.5;
const OVERFLOW_BOOST: f32 = 1.2;
const BODY_BOOST: f32 = 1.0;

struct Fields {
    id: Field,
    label: Field,
    aliases: Field,
    tags: Field,
    canonical: Field,
    overflow: Field,
    body: Field,
}

/// An in-memory BM25 index over the graph's entities.
///
/// Held separately from the graph on purpose: it is an index of entity text,
/// not a store of entities. Everything it returns is an [`EntityId`] to look up
/// in the graph, never a copy of the entity (§3.3).
pub struct LexicalIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
    parser: QueryParser,
    /// Level and kind per indexed id, so the metadata filter can run without a
    /// second pass over the graph.
    metadata: HashMap<String, (u8, crate::knowledge::EntityKind)>,
}

impl LexicalIndex {
    /// Build an index over every entity in the graph.
    pub fn build(graph: &KnowledgeGraph) -> Result<Self, RetrievalError> {
        let mut builder = Schema::builder();
        let fields = Fields {
            id: builder.add_text_field("id", STRING | STORED),
            label: builder.add_text_field("label", TEXT),
            aliases: builder.add_text_field("aliases", TEXT),
            tags: builder.add_text_field("tags", TEXT),
            canonical: builder.add_text_field("canonical", TEXT),
            // Indexed with the same analyser as everything else: an overflow
            // section is content, and content that cannot be searched has not
            // really been retained.
            overflow: builder.add_text_field("overflow", TEXT),
            body: builder.add_text_field("body", TEXT),
        };
        let schema = builder.build();
        let index = Index::create_in_ram(schema);

        let mut writer = index.writer(15_000_000)?;
        let mut metadata = HashMap::new();
        for entity in graph.entities() {
            writer.add_document(document_for(&fields, entity))?;
            metadata.insert(entity.id.to_string(), (entity.level, entity.kind));
        }
        writer.commit()?;

        let reader = index.reader()?;
        let mut parser = QueryParser::for_index(
            &index,
            vec![
                fields.label,
                fields.aliases,
                fields.tags,
                fields.canonical,
                fields.overflow,
                fields.body,
            ],
        );
        parser.set_field_boost(fields.label, LABEL_BOOST);
        parser.set_field_boost(fields.aliases, ALIAS_BOOST);
        parser.set_field_boost(fields.tags, TAG_BOOST);
        parser.set_field_boost(fields.canonical, FIELD_BOOST);
        parser.set_field_boost(fields.overflow, OVERFLOW_BOOST);
        parser.set_field_boost(fields.body, BODY_BOOST);
        // Left disjunctive on purpose: any term may match. Requiring all of
        // them would drop every natural-language question carrying a word the
        // vault happens not to use, which is most of them.

        Ok(Self {
            index,
            reader,
            fields,
            parser,
            metadata,
        })
    }

    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    /// Search, most relevant first.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        filter: &MetadataFilter,
    ) -> Result<Vec<Scored>, RetrievalError> {
        let cleaned = sanitise(query);
        if cleaned.is_empty() {
            return Ok(Vec::new());
        }
        let parsed = self.parser.parse_query(&cleaned)?;
        let searcher = self.reader.searcher();
        // Over-fetch, because the metadata filter runs after scoring and could
        // otherwise leave fewer than `limit` results.
        let fetch = (limit * 4).max(limit + 16);
        let hits = searcher.search(&parsed, &TopDocs::with_limit(fetch).order_by_score())?;

        let mut out = Vec::with_capacity(limit);
        for (score, address) in hits {
            let doc: TantivyDocument = searcher.doc(address)?;
            let Some(id) = doc
                .get_first(self.fields.id)
                .and_then(|v| v.as_str())
                .map(str::to_string)
            else {
                continue;
            };
            if let Some((level, kind)) = self.metadata.get(&id) {
                if !filter.admits(*level, *kind) {
                    continue;
                }
            }
            out.push(Scored {
                id: EntityId::from_stored(id),
                score,
            });
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    /// The underlying index, for callers that want to run their own query.
    pub fn index(&self) -> &Index {
        &self.index
    }
}

fn document_for(fields: &Fields, entity: &Entity) -> TantivyDocument {
    let mut doc = TantivyDocument::default();
    doc.add_text(fields.id, entity.id.as_str());
    doc.add_text(fields.label, &entity.label);
    doc.add_text(fields.aliases, entity.aliases.join(" "));
    // Tags are written `#region/fens`; splitting on the separator lets a query
    // for "fens" match without the author having written the leaf tag too.
    doc.add_text(
        fields.tags,
        entity.tags.join(" ").replace(['/', '-', '_'], " "),
    );
    doc.add_text(
        fields.canonical,
        entity
            .fields
            .values()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
    );
    doc.add_text(
        fields.overflow,
        entity
            .overflow
            .iter()
            .map(|s| {
                let heading = s.heading.clone().unwrap_or_default();
                format!("{heading}\n{}", s.content)
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    doc.add_text(fields.body, &entity.body);
    doc
}

/// Reduce a natural-language question to bare terms.
///
/// Query-parser syntax (`:`, `^`, `+`, `-`, `"`, `(`) is meaningful to tantivy
/// and meaningless in "who keeps the accord?", so it is stripped rather than
/// escaped: a player's apostrophe should never be a parse error.
pub fn sanitise(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for ch in query.chars() {
        if ch.is_alphanumeric() || ch.is_whitespace() {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{Entity, EntityId, EntityKind, OverflowSection};

    fn graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();

        let mut accord = Entity::new(
            EntityId::slug("The Quillion Accord"),
            "The Quillion Accord",
            EntityKind::Lore,
        );
        accord.body =
            "The Quillion Accord ended the boundary war between Vantareth and Oskorby.".into();
        g.insert(accord);

        let mut chapel = Entity::new(
            EntityId::slug("Pale Reach Chapel"),
            "Pale Reach Chapel",
            EntityKind::Location,
        );
        chapel.body = "A half-abandoned place in the Pale Reach. It floods in spring.".into();
        g.insert(chapel);

        let mut landing = Entity::new(
            EntityId::slug("Saltmarsh Landing"),
            "Saltmarsh Landing",
            EntityKind::Location,
        );
        landing.aliases = vec!["The Landing".into()];
        landing.body = "A crooked jetty and eleven houses on stilts.".into();
        landing.overflow = vec![OverflowSection {
            heading: Some("Smells Like".into()),
            content: "Rot, tar, and woodsmoke.".into(),
        }];
        g.insert(landing);

        let mut summary = Entity::new(
            EntityId::slug("summary_l1_0"),
            "Coastal Politics",
            EntityKind::Summary,
        );
        summary.level = 1;
        summary.body = "The Accord and the houses that signed it.".into();
        g.insert(summary);

        g
    }

    #[test]
    fn a_rare_proper_noun_is_found_by_itself() {
        // The `large` fixture's argument for BM25: "Quillion" appears in one
        // note out of 207 and is out of vocabulary for any embedding model.
        let index = LexicalIndex::build(&graph()).unwrap();
        let hits = index
            .search("Quillion", 5, &MetadataFilter::everything())
            .unwrap();
        assert_eq!(hits[0].id, EntityId::slug("The Quillion Accord"));
    }

    #[test]
    fn overflow_content_is_searchable() {
        // "Retained" without "searchable" is just a slower way of losing it.
        let index = LexicalIndex::build(&graph()).unwrap();
        let hits = index
            .search("woodsmoke", 5, &MetadataFilter::everything())
            .unwrap();
        assert_eq!(hits[0].id, EntityId::slug("Saltmarsh Landing"));
    }

    #[test]
    fn aliases_are_searchable() {
        let index = LexicalIndex::build(&graph()).unwrap();
        let hits = index
            .search("the landing", 5, &MetadataFilter::everything())
            .unwrap();
        assert!(hits
            .iter()
            .any(|h| h.id == EntityId::slug("Saltmarsh Landing")));
    }

    #[test]
    fn the_level_filter_separates_summaries_from_source_notes() {
        let index = LexicalIndex::build(&graph()).unwrap();
        let raw = index
            .search("accord", 10, &MetadataFilter::level(0))
            .unwrap();
        assert!(raw.iter().all(|h| h.id.as_str() != "summary_l1_0"));

        let summaries = index
            .search("accord", 10, &MetadataFilter::level(1))
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, EntityId::slug("summary_l1_0"));
    }

    #[test]
    fn punctuation_in_a_question_is_not_a_parse_error() {
        let index = LexicalIndex::build(&graph()).unwrap();
        let hits = index
            .search(
                "what's the accord? (the one at Pale Reach)",
                5,
                &MetadataFilter::everything(),
            )
            .unwrap();
        assert!(!hits.is_empty());
    }

    #[test]
    fn an_empty_query_returns_nothing_rather_than_everything() {
        let index = LexicalIndex::build(&graph()).unwrap();
        assert!(index
            .search("   ?!  ", 5, &MetadataFilter::everything())
            .unwrap()
            .is_empty());
    }
}
