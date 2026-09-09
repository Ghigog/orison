//! Markdown parsing: frontmatter, sections, wiki-links, tags, callouts,
//! tables and embeds.
//!
//! Ports `MarkdownParser.gd`, with four defects fixed:
//!
//! 1. **Frontmatter is only frontmatter at the top of the file.**
//!    `MarkdownParser.parse_file()` counted every `---` line in the document
//!    and treated the first two as delimiters wherever they appeared, so a
//!    fenced YAML sample truncated the file at its opening `---` and swallowed
//!    everything up to its closing one. `messy/10_Entities/characters/Sergeant
//!    Adah.md` exists to catch precisely that.
//! 2. **Fenced code is not content.** Hashtags, wiki-links and headings inside
//!    a fence are code, not markup. The Godot parser extracted all three.
//! 3. **Sections are parsed at all.** The Godot compiler abandoned section
//!    parsing entirely (`rag_architecture.md` §4.1) in favour of a single LLM
//!    call, which means its ingest cannot populate a character field without a
//!    live model. Sections come back here as the deterministic base layer;
//!    model extraction becomes a top-up that can only fill gaps
//!    (`Entity::merge_extracted_fields`).
//! 4. **Bold and numbered headings are headings.** `**1. History**` is the
//!    original Bug 1, and normalising it is handled in
//!    [`super::sections::normalise_heading`].

use std::collections::BTreeMap;

use super::yaml::{self, YamlValue};

/// A parsed Markdown document. Nothing here is lossy: `body` is the complete
/// text after the frontmatter, and every section's content is a slice of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub frontmatter: BTreeMap<String, YamlValue>,
    /// Everything after the frontmatter block, verbatim.
    pub body: String,
    pub sections: Vec<Section>,
    pub wiki_links: Vec<WikiLink>,
    pub tags: Vec<String>,
    pub callouts: Vec<Callout>,
    pub tables: Vec<Table>,
    pub embeds: Vec<String>,
}

/// A run of body text under one heading. `heading == None` is the text before
/// the first heading, which most notes have and some notes are entirely made
/// of (`messy/10_Entities/characters/Mira of the Fens.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub heading: Option<String>,
    pub level: u8,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub target: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callout {
    pub kind: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Parse one Markdown document.
pub fn parse(source: &str) -> Document {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let (frontmatter_text, body) = split_frontmatter(source);
    let frontmatter = yaml::parse_frontmatter(frontmatter_text);

    let lines: Vec<&str> = body.lines().collect();
    let fenced = fence_mask(&lines);

    let sections = split_sections(&lines, &fenced);
    let wiki_links = extract_wiki_links(&lines, &fenced);
    let tags = extract_tags(&lines, &fenced);
    let callouts = extract_callouts(&lines, &fenced);
    let tables = extract_tables(&lines, &fenced);
    let embeds = extract_embeds(&lines, &fenced);

    Document {
        frontmatter,
        body: body.to_string(),
        sections,
        wiki_links,
        tags,
        callouts,
        tables,
        embeds,
    }
}

/// Split leading YAML frontmatter from the body.
///
/// A `---` is a frontmatter delimiter only as the document's first non-empty
/// line, and the block ends at the next `---` at the start of a line. Any other
/// `---` in the file is a horizontal rule or, as in the Adah fixture, part of a
/// code sample.
fn split_frontmatter(source: &str) -> (&str, &str) {
    let trimmed_start = source.trim_start_matches(['\n', '\r']);
    let leading = source.len() - trimmed_start.len();

    let mut lines = trimmed_start.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return ("", source);
    };
    if first.trim_end() != "---" {
        return ("", source);
    }

    let mut offset = leading + first.len();
    let start = offset;
    for line in lines {
        if line.trim_end() == "---" {
            let fm = &source[start..offset];
            let rest = &source[offset + line.len()..];
            return (fm, rest.trim_start_matches(['\n', '\r']));
        }
        offset += line.len();
    }
    // An unterminated opening `---` is a rule, not frontmatter. Godot's parser
    // treated the whole rest of the file as YAML in this case and lost it.
    ("", source)
}

/// `true` for every line that sits inside a fenced code block, fence markers
/// included.
fn fence_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = Vec::with_capacity(lines.len());
    let mut fence: Option<(char, usize)> = None;
    for line in lines {
        let trimmed = line.trim_start();
        let opener = fence_marker(trimmed);
        match (&fence, opener) {
            (None, Some((ch, len))) => {
                fence = Some((ch, len));
                mask.push(true);
            }
            (Some((open_ch, open_len)), Some((ch, len)))
                if ch == *open_ch && len >= *open_len && trimmed[len..].trim().is_empty() =>
            {
                fence = None;
                mask.push(true);
            }
            _ => mask.push(fence.is_some()),
        }
    }
    mask
}

fn fence_marker(trimmed: &str) -> Option<(char, usize)> {
    for ch in ['`', '~'] {
        let len = trimmed.chars().take_while(|c| *c == ch).count();
        if len >= 3 {
            return Some((ch, len));
        }
    }
    None
}

/// Is this line a heading, and at what level?
///
/// ATX headings (`## Manner`) plus whole-line bold headings (`**1. History**`,
/// `__Manner__`). The bold form is what `messy/10_Entities/characters/King
/// Yuna.md` uses and what Bug 1 was about: not recognising it discarded every
/// section of that file.
fn heading_of(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix('#') {
        let extra = rest.chars().take_while(|c| *c == '#').count();
        let level = 1 + extra;
        if level > 6 {
            return None;
        }
        let text = rest[extra..].trim();
        // `#tag` on its own line is a tag, not a heading: an ATX heading needs
        // whitespace after the hashes.
        if !rest[extra..].starts_with(char::is_whitespace) || text.is_empty() {
            return None;
        }
        return Some((level as u8, text.trim_end_matches('#').trim().to_string()));
    }

    for marker in ["**", "__"] {
        if let Some(inner) = trimmed
            .strip_prefix(marker)
            .and_then(|r| r.strip_suffix(marker))
        {
            let inner = inner.trim();
            // A whole-line bold run is a heading only if it is short and has no
            // internal marker pair; a bold sentence in the middle of prose is
            // not a heading.
            if !inner.is_empty() && !inner.contains(marker) && inner.chars().count() <= 64 {
                return Some((2, inner.to_string()));
            }
        }
    }
    None
}

fn split_sections(lines: &[&str], fenced: &[bool]) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut current = Section {
        heading: None,
        level: 0,
        content: String::new(),
    };

    for (i, line) in lines.iter().enumerate() {
        if !fenced[i] {
            if let Some((level, text)) = heading_of(line) {
                if current.heading.is_some() || !current.content.trim().is_empty() {
                    current.content = current.content.trim().to_string();
                    sections.push(current);
                }
                current = Section {
                    heading: Some(text),
                    level,
                    content: String::new(),
                };
                continue;
            }
        }
        current.content.push_str(line);
        current.content.push('\n');
    }

    if current.heading.is_some() || !current.content.trim().is_empty() {
        current.content = current.content.trim().to_string();
        sections.push(current);
    }
    sections
}

fn extract_wiki_links(lines: &[&str], fenced: &[bool]) -> Vec<WikiLink> {
    let mut out: Vec<WikiLink> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if fenced[i] {
            continue;
        }
        let bytes: Vec<char> = line.chars().collect();
        let mut idx = 0;
        while idx + 1 < bytes.len() {
            if bytes[idx] == '[' && bytes[idx + 1] == '[' {
                // `![[...]]` is an embed, handled separately.
                let is_embed = idx > 0 && bytes[idx - 1] == '!';
                if let Some(end) = find_close(&bytes, idx + 2) {
                    let inner: String = bytes[idx + 2..end].iter().collect();
                    if !is_embed {
                        if let Some(link) = parse_wiki_link(&inner) {
                            if !out.contains(&link) {
                                out.push(link);
                            }
                        }
                    }
                    idx = end + 2;
                    continue;
                }
            }
            idx += 1;
        }
    }
    out
}

fn find_close(chars: &[char], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == ']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_wiki_link(inner: &str) -> Option<WikiLink> {
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }
    let (target_part, label) = match inner.split_once('|') {
        Some((t, l)) => (t.trim(), l.trim().to_string()),
        None => (inner, inner.to_string()),
    };
    // `[[Note#Heading]]` and `[[Note^block]]` point at the note.
    let target = target_part
        .split(['#', '^'])
        .next()
        .unwrap_or(target_part)
        .trim();
    if target.is_empty() {
        return None;
    }
    Some(WikiLink {
        target: target.to_string(),
        label,
    })
}

/// `#tag`, `#tag/subtag`. Hex colours are excluded, as in the Godot parser,
/// and fenced lines are skipped, which it did not do.
fn extract_tags(lines: &[&str], fenced: &[bool]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if fenced[i] {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut idx = 0;
        while idx < chars.len() {
            if chars[idx] == '#' && (idx == 0 || chars[idx - 1].is_whitespace()) {
                let start = idx + 1;
                let mut end = start;
                while end < chars.len()
                    && (chars[end].is_ascii_alphanumeric() || matches!(chars[end], '_' | '-' | '/'))
                {
                    end += 1;
                }
                let tag: String = chars[start..end].iter().collect();
                if !tag.is_empty()
                    && !is_hex_colour(&tag)
                    && !tag.chars().all(|c| c.is_ascii_digit())
                    && !out.contains(&tag)
                {
                    out.push(tag);
                }
                idx = end.max(idx + 1);
                continue;
            }
            idx += 1;
        }
    }
    out
}

fn is_hex_colour(tag: &str) -> bool {
    matches!(tag.len(), 3 | 6) && tag.chars().all(|c| c.is_ascii_hexdigit())
}

/// `> [!secret]` blocks. The payload of `messy`'s hardest retrieval case lives
/// inside one of these, so dropping them fails a documented query outright.
fn extract_callouts(lines: &[&str], fenced: &[bool]) -> Vec<Callout> {
    let mut out = Vec::new();
    let mut current: Option<Callout> = None;

    for (i, line) in lines.iter().enumerate() {
        if fenced[i] {
            continue;
        }
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('>') {
            let rest = rest.trim_start();
            if let Some(header) = parse_callout_header(rest) {
                if let Some(done) = current.take() {
                    out.push(done);
                }
                current = Some(header);
                continue;
            }
            if let Some(c) = current.as_mut() {
                if !c.content.is_empty() {
                    c.content.push('\n');
                }
                c.content.push_str(rest.trim());
                continue;
            }
        } else if let Some(done) = current.take() {
            out.push(done);
        }
    }
    if let Some(done) = current {
        out.push(done);
    }
    out
}

fn parse_callout_header(rest: &str) -> Option<Callout> {
    let rest = rest.strip_prefix("[!")?;
    let (kind, after) = rest.split_once(']')?;
    if kind.is_empty()
        || !kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(Callout {
        kind: kind.to_ascii_lowercase(),
        title: after.trim_start_matches(['+', '-']).trim().to_string(),
        content: String::new(),
    })
}

fn extract_tables(lines: &[&str], fenced: &[bool]) -> Vec<Table> {
    let mut out = Vec::new();
    let mut pending_headers: Option<Vec<String>> = None;
    let mut current: Option<Table> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let is_row = !fenced[i] && trimmed.starts_with('|') && trimmed.ends_with('|');
        if !is_row {
            if let Some(t) = current.take() {
                out.push(t);
            }
            pending_headers = None;
            continue;
        }
        let cells = parse_table_row(trimmed);
        if let Some(t) = current.as_mut() {
            t.rows.push(cells);
        } else if let Some(headers) = pending_headers.take() {
            if is_separator_row(&cells) {
                current = Some(Table {
                    headers,
                    rows: Vec::new(),
                });
            } else {
                pending_headers = Some(cells);
            }
        } else {
            pending_headers = Some(cells);
        }
    }
    if let Some(t) = current {
        out.push(t);
    }
    out
}

fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
}

fn parse_table_row(row: &str) -> Vec<String> {
    row.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// `![[image.png]]` and `![alt](image.png)`.
fn extract_embeds(lines: &[&str], fenced: &[bool]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if fenced[i] {
            continue;
        }
        let mut rest = *line;
        while let Some(pos) = rest.find("![[") {
            let after = &rest[pos + 3..];
            match after.find("]]") {
                Some(end) => {
                    let target = after[..end].split('|').next().unwrap_or("").trim();
                    if !target.is_empty() && !out.iter().any(|e| e == target) {
                        out.push(target.to_string());
                    }
                    rest = &after[end + 2..];
                }
                None => break,
            }
        }

        let mut rest = *line;
        while let Some(pos) = rest.find("![") {
            let after = &rest[pos + 2..];
            let Some(close) = after.find(']') else { break };
            let tail = &after[close + 1..];
            if let Some(stripped) = tail.strip_prefix('(') {
                if let Some(end) = stripped.find(')') {
                    let target = stripped[..end]
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !target.is_empty() && !out.iter().any(|e| e == target) {
                        out.push(target.to_string());
                    }
                    rest = &stripped[end + 1..];
                    continue;
                }
            }
            rest = tail;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_stops_at_the_first_closing_delimiter() {
        let doc = parse("---\ntype: character\nname: Adah\n---\n\nBody text.\n");
        assert_eq!(
            doc.frontmatter.get("name"),
            Some(&YamlValue::Str("Adah".into()))
        );
        assert_eq!(doc.body.trim(), "Body text.");
    }

    #[test]
    fn a_fenced_yaml_sample_is_not_frontmatter() {
        // The Sergeant Adah case. Godot's parser counted the fence's `---`
        // lines as delimiters and truncated everything after them.
        let src = "---\ntype: character\n---\n\n## Ledger\n\n```yaml\n---\npost: granary\n---\n```\n\n## Temperament\n\nLoyal to individuals.\n";
        let doc = parse(src);
        assert_eq!(
            doc.frontmatter.get("type"),
            Some(&YamlValue::Str("character".into()))
        );
        assert!(
            doc.body.contains("Loyal to individuals"),
            "text after the fence was lost"
        );
        let headings: Vec<_> = doc
            .sections
            .iter()
            .filter_map(|s| s.heading.clone())
            .collect();
        assert_eq!(headings, vec!["Ledger", "Temperament"]);
    }

    #[test]
    fn an_unterminated_opening_rule_is_not_frontmatter() {
        let doc = parse("---\njust a rule, no closing delimiter\n");
        assert!(doc.frontmatter.is_empty());
        assert!(doc.body.contains("just a rule"));
    }

    #[test]
    fn bold_numeric_headings_are_headings() {
        // Bug 1, at the parsing end.
        let doc = parse("---\ntype: character\n---\n\n**1. History**\n\nTook the throne.\n\n**2. Manner**\n\nBlunt.\n");
        let headings: Vec<_> = doc
            .sections
            .iter()
            .filter_map(|s| s.heading.clone())
            .collect();
        assert_eq!(headings, vec!["1. History", "2. Manner"]);
        assert_eq!(doc.sections[0].content, "Took the throne.");
    }

    #[test]
    fn a_bold_run_inside_prose_is_not_a_heading() {
        let doc = parse("He is **absolutely** certain of it.\n");
        assert!(doc.sections.iter().all(|s| s.heading.is_none()));
    }

    #[test]
    fn preamble_before_the_first_heading_is_kept_as_a_section() {
        let doc = parse("Loose opening line.\n\n## Bearing\n\nQuiet.\n");
        assert_eq!(doc.sections.len(), 2);
        assert_eq!(doc.sections[0].heading, None);
        assert_eq!(doc.sections[0].content, "Loose opening line.");
    }

    #[test]
    fn wiki_links_handle_pipes_and_anchors() {
        let doc =
            parse("See [[Fen Marches]], [[Saltmarsh Landing|the Landing]] and [[Lore#Tithe]].\n");
        assert_eq!(
            doc.wiki_links,
            vec![
                WikiLink {
                    target: "Fen Marches".into(),
                    label: "Fen Marches".into()
                },
                WikiLink {
                    target: "Saltmarsh Landing".into(),
                    label: "the Landing".into()
                },
                WikiLink {
                    target: "Lore".into(),
                    label: "Lore#Tithe".into()
                },
            ]
        );
    }

    #[test]
    fn embeds_are_not_wiki_links() {
        let doc = parse("![[portrait.png]] and [[Fen Marches]]\n");
        assert_eq!(doc.wiki_links.len(), 1);
        assert_eq!(doc.embeds, vec!["portrait.png"]);
    }

    #[test]
    fn markdown_image_embeds_are_found() {
        let doc = parse("![a king](assets/yuna.png)\n");
        assert_eq!(doc.embeds, vec!["assets/yuna.png"]);
    }

    #[test]
    fn tags_skip_headings_hex_colours_and_fences() {
        let doc = parse(
            "# Heading\n\n#border #contested #ff00aa\n\n```\n#!/bin/sh\n#notacode-tag\n```\n",
        );
        assert_eq!(doc.tags, vec!["border", "contested"]);
    }

    #[test]
    fn callouts_are_captured() {
        let doc = parse("> [!secret]\n> Mira knows where the causeway surfaces.\n> She has told no one.\n\nAfter.\n");
        assert_eq!(doc.callouts.len(), 1);
        assert_eq!(doc.callouts[0].kind, "secret");
        assert!(doc.callouts[0].content.contains("causeway surfaces"));
        assert!(doc.callouts[0].content.contains("told no one"));
    }

    #[test]
    fn tables_are_captured() {
        let doc = parse(
            "| d6 | Rumour |\n|---|---|\n| 1 | The causeway surfaces. |\n| 2 | A fourth son. |\n",
        );
        assert_eq!(doc.tables.len(), 1);
        assert_eq!(doc.tables[0].headers, vec!["d6", "Rumour"]);
        assert_eq!(doc.tables[0].rows.len(), 2);
    }
}
