//! Chunking with provenance (§3.6).
//!
//! The current design embeds whole nodes, which is part of why lore context
//! gets truncated (`rag_architecture.md` Bug 5) and all of why a long note's
//! embedding is a blurry average of everything in it. A vector for a
//! three-page lore document points at the middle of three pages.
//!
//! Two things come out of chunking, and only one of them has a consumer yet.
//!
//! **Retrieval precision, now.** A chunk is about one thing, so its embedding
//! means one thing. This is the same argument
//! [`PassageReranker`](crate::retrieval::PassageReranker) makes on the lexical
//! side, applied at index time rather than at query time.
//!
//! **Provenance, later.** Every chunk carries the id of the note it came from
//! and the character range it occupies within it, so a retrieved passage can be
//! traced back to the sentence in the vault that produced it. Nothing consumes
//! that yet — showing the player *why* the story knows something is Phase 4's
//! turn loop and beyond — but retrofitting provenance after that exists is far
//! more expensive than carrying it from the start, which is why it is here now.
//!
//! ## On the chunk size
//!
//! Not prescribed by the migration plan, and deliberately so: the right answer
//! plausibly differs between a two-paragraph character bio and a multi-page
//! lore document. What was tried, and why the defaults are what they are:
//!
//! - **Fixed character windows** (600, 1200 characters). Rejected: they cut
//!   mid-sentence, and a chunk that begins halfway through a clause embeds
//!   badly and reads worse when shown to a player as a citation.
//! - **One chunk per section.** Rejected as the only rule: it is the right
//!   *boundary* but the wrong *size*. `messy`'s `## Rumour Table` is two lines
//!   and `## Description` is a paragraph, so section-sized chunks vary by more
//!   than an order of magnitude, and a note with no headings at all — every
//!   file in `large`, and `Mira of the Fens.md` — becomes one chunk again.
//! - **Paragraph-packed word windows, with overlap.** Chosen. Paragraphs are
//!   accumulated until the target is reached, so boundaries land where the
//!   author put them; a paragraph longer than the target is split on word
//!   boundaries rather than being allowed to dominate; and consecutive chunks
//!   overlap so a fact stated across a paragraph break is whole in at least one
//!   of them.
//!
//! `DEFAULT_TARGET_WORDS` of 180 is roughly 240 tokens for English prose, which
//! leaves room for several chunks inside any sensible lore budget. The 40-word
//! overlap is a little over two sentences: enough to carry an antecedent
//! ("He was there too") into the following chunk, which is the failure overlap
//! exists to prevent.
//!
//! **These numbers are not validated against a long real note**, because there
//! is no long note to validate against: the longest file in `large` is 53
//! words. `large` is a scale fixture — 207 files — not a length fixture, and
//! nothing in the fixture set exercises multi-chunk behaviour at the default
//! size. The tests below therefore cover the mechanism at a size that does
//! split the fixture's longest note, and separately at the default size against
//! a note built for the purpose. Tuning these against real documents is
//! follow-up work, recorded rather than guessed at.

use crate::knowledge::{Entity, EntityId};

/// Words per chunk, before overlap.
pub const DEFAULT_TARGET_WORDS: usize = 180;
/// Words of the previous chunk repeated at the start of the next.
pub const DEFAULT_OVERLAP_WORDS: usize = 40;
/// A trailing fragment shorter than this is folded into the previous chunk
/// rather than becoming a chunk of its own.
pub const DEFAULT_MIN_WORDS: usize = 25;

#[derive(Debug, Clone, Copy)]
pub struct ChunkConfig {
    pub target_words: usize,
    pub overlap_words: usize,
    pub min_words: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_words: DEFAULT_TARGET_WORDS,
            overlap_words: DEFAULT_OVERLAP_WORDS,
            min_words: DEFAULT_MIN_WORDS,
        }
    }
}

/// One retrievable passage, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// `<entity id>#<ordinal>`. Parseable back to its note by
    /// [`Chunk::source_note`], so a retrieval hit on a chunk resolves to an
    /// entity without a lookup table to keep in step.
    pub id: String,
    pub entity_id: EntityId,
    pub ordinal: usize,
    /// The heading in force where this chunk starts, when the note has one.
    pub heading: Option<String>,
    pub text: String,
    /// Character offsets into the entity's `body`. The provenance that lets a
    /// citation point at a sentence rather than at a file.
    pub char_start: usize,
    pub char_end: usize,
}

/// Storable form. Written here rather than at each call site because three
/// of them existed by Phase 5 and a fourth would have been the one that
/// transposed `char_start` and `char_end`.
impl From<&Chunk> for crate::state::ChunkRow {
    fn from(c: &Chunk) -> Self {
        Self {
            id: c.id.clone(),
            entity_id: c.entity_id.to_string(),
            ordinal: c.ordinal as i64,
            heading: c.heading.clone(),
            text: c.text.clone(),
            char_start: c.char_start as i64,
            char_end: c.char_end as i64,
        }
    }
}

impl Chunk {
    pub fn make_id(entity_id: &EntityId, ordinal: usize) -> String {
        format!("{entity_id}#{ordinal}")
    }

    /// The note a chunk id belongs to.
    pub fn source_note(chunk_id: &str) -> Option<EntityId> {
        let (entity, ordinal) = chunk_id.rsplit_once('#')?;
        ordinal.parse::<usize>().ok()?;
        Some(EntityId::from_stored(entity))
    }
}

/// Split an entity's source text into overlapping chunks.
///
/// Chunks the `body`, which is the complete original note — the canonical
/// fields and overflow sections are derived from it, so chunking those instead
/// would index some sentences twice and leave the offsets pointing at nothing.
pub fn chunk_entity(entity: &Entity, config: &ChunkConfig) -> Vec<Chunk> {
    let body = &entity.body;
    if body.trim().is_empty() {
        return Vec::new();
    }

    let blocks = paragraphs(body);
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut pending: Vec<&Block> = Vec::new();
    let mut pending_words = 0usize;

    let flush = |pending: &mut Vec<&Block>, chunks: &mut Vec<Chunk>| {
        if pending.is_empty() {
            return;
        }
        let start = pending[0].start;
        let end = pending[pending.len() - 1].end;
        let heading = pending
            .iter()
            .find_map(|b| b.heading.clone())
            .or_else(|| chunks.last().and_then(|c| c.heading.clone()));
        let ordinal = chunks.len();
        chunks.push(Chunk {
            id: Chunk::make_id(&entity.id, ordinal),
            entity_id: entity.id.clone(),
            ordinal,
            heading,
            text: body[start..end].trim().to_string(),
            char_start: start,
            char_end: end,
        });
        pending.clear();
    };

    for block in &blocks {
        if block.words == 0 {
            continue;
        }
        // A single block longer than the target is split rather than allowed to
        // become a chunk several times the size of every other one.
        if block.words > config.target_words {
            flush(&mut pending, &mut chunks);
            pending_words = 0;
            for piece in split_block(body, block, config) {
                let ordinal = chunks.len();
                chunks.push(Chunk {
                    id: Chunk::make_id(&entity.id, ordinal),
                    entity_id: entity.id.clone(),
                    ordinal,
                    heading: block
                        .heading
                        .clone()
                        .or_else(|| chunks.last().and_then(|c| c.heading.clone())),
                    text: body[piece.0..piece.1].trim().to_string(),
                    char_start: piece.0,
                    char_end: piece.1,
                });
            }
            continue;
        }

        if pending_words + block.words > config.target_words && !pending.is_empty() {
            flush(&mut pending, &mut chunks);
            pending_words = 0;
        }
        pending.push(block);
        pending_words += block.words;
    }
    flush(&mut pending, &mut chunks);

    // Fold a short tail into its predecessor: a two-line trailing chunk carries
    // no context and pollutes the ranking with a near-duplicate.
    if chunks.len() > 1 {
        let last_words = chunks[chunks.len() - 1].text.split_whitespace().count();
        if last_words < config.min_words {
            let tail = chunks.pop().expect("length checked");
            let previous = chunks.last_mut().expect("length checked");
            previous.char_end = tail.char_end;
            previous.text = body[previous.char_start..previous.char_end]
                .trim()
                .to_string();
        }
    }

    apply_overlap(&mut chunks, body, config);
    chunks
}

/// Prepend the tail of each chunk to the one after it.
///
/// Applied after the boundaries are decided rather than during, so the
/// character range still describes exactly where the chunk's own content sits.
/// The overlap widens `char_start` to cover the repeated words, which keeps the
/// range honest: every word in `text` is inside `[char_start, char_end)`.
fn apply_overlap(chunks: &mut [Chunk], body: &str, config: &ChunkConfig) {
    if config.overlap_words == 0 {
        return;
    }
    let starts: Vec<usize> = chunks.iter().map(|c| c.char_start).collect();
    for i in 1..chunks.len() {
        let previous_start = starts[i - 1];
        let own_start = starts[i];
        let overlap_start = back_up_words(body, previous_start, own_start, config.overlap_words);
        chunks[i].char_start = overlap_start;
        chunks[i].text = body[overlap_start..chunks[i].char_end].trim().to_string();
    }
}

/// The byte offset `words` words back from `to`, not going past `floor`.
fn back_up_words(body: &str, floor: usize, to: usize, words: usize) -> usize {
    let slice = &body[floor..to];
    let mut boundaries: Vec<usize> = Vec::new();
    let mut in_word = false;
    for (i, ch) in slice.char_indices() {
        if ch.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            boundaries.push(i);
        }
    }
    if boundaries.len() <= words {
        return floor;
    }
    floor + boundaries[boundaries.len() - words]
}

struct Block {
    start: usize,
    end: usize,
    words: usize,
    heading: Option<String>,
}

/// Split the body on blank lines, tracking the heading in force.
fn paragraphs(body: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut start: Option<usize> = None;
    let mut end = 0usize;
    let mut heading_for_block: Option<String> = None;

    let mut offset = 0usize;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim();
        let line_start = offset;
        offset += line.len();

        if trimmed.is_empty() {
            if let Some(s) = start.take() {
                blocks.push(Block {
                    start: s,
                    end,
                    words: body[s..end].split_whitespace().count(),
                    heading: heading_for_block.take(),
                });
            }
            continue;
        }

        if let Some(heading) = heading_text(trimmed) {
            if let Some(s) = start.take() {
                blocks.push(Block {
                    start: s,
                    end,
                    words: body[s..end].split_whitespace().count(),
                    heading: heading_for_block.take(),
                });
            }
            current_heading = Some(heading);
            continue;
        }

        if start.is_none() {
            start = Some(line_start);
            heading_for_block = current_heading.clone();
        }
        end = line_start + line.trim_end().len();
    }
    if let Some(s) = start {
        blocks.push(Block {
            start: s,
            end,
            words: body[s..end].split_whitespace().count(),
            heading: heading_for_block,
        });
    }
    blocks
}

fn heading_text(trimmed: &str) -> Option<String> {
    if let Some(rest) = trimmed.strip_prefix('#') {
        let extra = rest.chars().take_while(|c| *c == '#').count();
        let text = rest[extra..].trim();
        if !text.is_empty() && rest[extra..].starts_with(char::is_whitespace) {
            return Some(text.trim_end_matches('#').trim().to_string());
        }
    }
    for marker in ["**", "__"] {
        if let Some(inner) = trimmed
            .strip_prefix(marker)
            .and_then(|r| r.strip_suffix(marker))
        {
            let inner = inner.trim();
            if !inner.is_empty() && !inner.contains(marker) && inner.chars().count() <= 64 {
                return Some(inner.to_string());
            }
        }
    }
    None
}

/// Split one over-long block on word boundaries.
fn split_block(body: &str, block: &Block, config: &ChunkConfig) -> Vec<(usize, usize)> {
    let slice = &body[block.start..block.end];
    let word_starts: Vec<usize> = {
        let mut out = Vec::new();
        let mut in_word = false;
        for (i, ch) in slice.char_indices() {
            if ch.is_whitespace() {
                in_word = false;
            } else if !in_word {
                in_word = true;
                out.push(i);
            }
        }
        out
    };

    let mut pieces = Vec::new();
    let mut i = 0;
    while i < word_starts.len() {
        let end_word = (i + config.target_words).min(word_starts.len());
        let start = block.start + word_starts[i];
        let end = if end_word == word_starts.len() {
            block.end
        } else {
            block.start + word_starts[end_word]
        };
        pieces.push((start, end));
        if end_word == word_starts.len() {
            break;
        }
        i = end_word;
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::EntityKind;

    fn entity_with(body: &str) -> Entity {
        let mut e = Entity::new(EntityId::slug("note"), "Note", EntityKind::Lore);
        e.body = body.to_string();
        e
    }

    fn long_body(paragraphs: usize, words_each: usize) -> String {
        (0..paragraphs)
            .map(|p| {
                (0..words_each)
                    .map(|w| format!("p{p}w{w}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn a_short_note_is_one_chunk() {
        // Every note in every fixture is this shape at the default size; the
        // longest file in `large` is 53 words.
        let e = entity_with("A crooked jetty and eleven houses on stilts.");
        let chunks = chunk_entity(&e, &ChunkConfig::default());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].ordinal, 0);
        assert_eq!(chunks[0].char_start, 0);
    }

    #[test]
    fn an_empty_note_produces_no_chunks() {
        let e = entity_with("   \n\n  ");
        assert!(chunk_entity(&e, &ChunkConfig::default()).is_empty());
    }

    #[test]
    fn a_long_note_splits_with_overlap() {
        let body = long_body(6, 100);
        let e = entity_with(&body);
        let chunks = chunk_entity(&e, &ChunkConfig::default());
        assert!(chunks.len() > 1, "a 600-word note should not be one chunk");

        for pair in chunks.windows(2) {
            let (first, second) = (&pair[0], &pair[1]);
            assert!(
                second.char_start < first.char_end,
                "consecutive chunks do not overlap: {}..{} then {}..{}",
                first.char_start,
                first.char_end,
                second.char_start,
                second.char_end
            );
            let tail: Vec<&str> = first.text.split_whitespace().rev().take(5).collect();
            for word in tail {
                assert!(
                    second.text.contains(word),
                    "{word:?} from the end of one chunk is missing from the start of the next"
                );
            }
        }
    }

    #[test]
    fn every_chunk_is_exactly_the_text_its_range_names() {
        // The provenance guarantee: a citation that points at the wrong
        // characters is worse than no citation.
        let body = long_body(6, 100);
        let e = entity_with(&body);
        for chunk in chunk_entity(&e, &ChunkConfig::default()) {
            assert_eq!(
                chunk.text,
                body[chunk.char_start..chunk.char_end].trim(),
                "chunk {} does not match its own range",
                chunk.ordinal
            );
        }
    }

    #[test]
    fn no_content_is_lost_between_chunks() {
        let body = long_body(6, 100);
        let e = entity_with(&body);
        let chunks = chunk_entity(&e, &ChunkConfig::default());
        let covered: String = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for word in body.split_whitespace() {
            assert!(covered.contains(word), "{word:?} fell between chunks");
        }
    }

    #[test]
    fn chunk_ids_resolve_back_to_their_note() {
        let body = long_body(6, 100);
        let mut e = entity_with(&body);
        e.id = EntityId::slug("The Quillion Accord");
        for chunk in chunk_entity(&e, &ChunkConfig::default()) {
            assert_eq!(Chunk::source_note(&chunk.id), Some(e.id.clone()));
        }
        assert_eq!(Chunk::source_note("no_ordinal"), None);
        assert_eq!(Chunk::source_note("note#notanumber"), None);
    }

    #[test]
    fn headings_are_carried_onto_the_chunks_under_them() {
        let body = format!(
            "## Background\n\n{}\n\n## Temperament\n\n{}",
            long_body(1, 200),
            long_body(1, 200)
        );
        let e = entity_with(&body);
        let chunks = chunk_entity(&e, &ChunkConfig::default());
        assert!(chunks
            .iter()
            .any(|c| c.heading.as_deref() == Some("Background")));
        assert!(chunks
            .iter()
            .any(|c| c.heading.as_deref() == Some("Temperament")));
    }

    #[test]
    fn a_single_over_long_paragraph_is_split_rather_than_kept_whole() {
        let body = long_body(1, 500);
        let e = entity_with(&body);
        let chunks = chunk_entity(&e, &ChunkConfig::default());
        assert!(chunks.len() >= 3);
        for chunk in &chunks {
            let words = chunk.text.split_whitespace().count();
            assert!(
                words <= DEFAULT_TARGET_WORDS + DEFAULT_OVERLAP_WORDS + 5,
                "chunk of {words} words is over the target"
            );
        }
    }

    #[test]
    fn a_short_tail_is_folded_into_its_predecessor() {
        let body = format!("{}\n\n{}", long_body(1, 170), long_body(1, 5));
        let e = entity_with(&body);
        let chunks = chunk_entity(&e, &ChunkConfig::default());
        assert_eq!(
            chunks.len(),
            1,
            "a five-word tail should not be its own chunk"
        );
        assert!(chunks[0].text.contains("p0w4"));
    }

    #[test]
    fn overlap_can_be_switched_off() {
        let body = long_body(6, 100);
        let e = entity_with(&body);
        let chunks = chunk_entity(
            &e,
            &ChunkConfig {
                overlap_words: 0,
                ..Default::default()
            },
        );
        for pair in chunks.windows(2) {
            assert!(pair[1].char_start >= pair[0].char_end);
        }
    }
}
