//! Writing-style extraction.
//!
//! Ported from `_extract_character_writing_style()` and
//! `_extract_campaign_writing_style()`. This is a genuinely valuable heuristic
//! and it is preserved as-is in behaviour: a traits prefix from frontmatter, an
//! explicit style field, a style-named section, the character's own
//! blockquotes, and finally lines of their dialogue mined from every other note
//! in the vault.

use std::collections::BTreeMap;

use super::markdown::Document;
use super::yaml::YamlValue;

/// A character's voice, in the format `PromptBuilder` expects.
pub fn character_writing_style(
    label: &str,
    id: &str,
    doc: &Document,
    other_notes: &[(&str, &Document)],
) -> String {
    let fm = &doc.frontmatter;
    let traits_prefix = traits_prefix(fm);

    if let Some(style) = first_non_empty(fm, &["writing_style", "dialogue_style", "style"]) {
        return format!("{traits_prefix}{style}");
    }

    // A section explicitly about how this character speaks.
    for section in &doc.sections {
        let Some(heading) = &section.heading else {
            continue;
        };
        let normalised = super::sections::normalise_heading(heading);
        if matches!(
            normalised.as_str(),
            "writingstyle" | "dialoguestyle" | "dialogue" | "quotes" | "personality" | "voice"
        ) && !section.content.trim().is_empty()
        {
            return format!("{traits_prefix}{}", section.content.trim());
        }
    }

    let mut snippets: Vec<String> = Vec::new();
    for line in doc.body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('>') {
            // A callout header is markup, not a line the character said.
            if rest.trim_start().starts_with("[!") {
                continue;
            }
            let quote = unquote(rest.trim());
            if !quote.is_empty() && !snippets.contains(&quote) {
                snippets.push(quote);
            }
        }
    }

    let id_lower = id.to_ascii_lowercase();
    let label_lower = label.to_ascii_lowercase();
    'outer: for (path, other) in other_notes {
        let stem = super::classify::file_stem(path).to_ascii_lowercase();
        if stem == id_lower || stem.contains(&id_lower) || stem.contains(&label_lower) {
            continue;
        }
        for line in other.body.lines() {
            if let Some(said) = spoken_line(line.trim(), label, id) {
                if !snippets.contains(&said) {
                    snippets.push(said);
                    if snippets.len() >= 5 {
                        break 'outer;
                    }
                }
            }
        }
    }

    if snippets.is_empty() {
        return traits_prefix.trim().to_string();
    }
    let joined: String = snippets.iter().map(|s| format!("- \"{s}\"\n")).collect();
    format!("{traits_prefix}{}", joined.trim())
}

fn traits_prefix(fm: &BTreeMap<String, YamlValue>) -> String {
    let Some(value) = ["traits", "personality", "voice", "tone"]
        .iter()
        .find_map(|k| fm.get(*k))
    else {
        return String::new();
    };
    let text = value.as_display_string();
    let text = text.trim();
    if text.is_empty() {
        String::new()
    } else {
        format!("Personality Traits/Tone: {text}\n")
    }
}

fn first_non_empty(fm: &BTreeMap<String, YamlValue>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        fm.get(*k)
            .and_then(YamlValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
}

/// `Name: line`, `**Name**: line`, `Name says, "line"`.
fn spoken_line(trimmed: &str, label: &str, id: &str) -> Option<String> {
    let id_capitalised = capitalise(id);
    let prefixes = [
        format!("{label}:"),
        format!("**{label}**:"),
        format!("*{label}*:"),
        format!("{id_capitalised}:"),
        format!("**{id_capitalised}**:"),
    ];
    for prefix in &prefixes {
        if let Some(rest) = trimmed.strip_prefix(prefix.as_str()) {
            let said = unquote(rest.trim());
            if !said.is_empty() {
                return Some(said);
            }
        }
    }

    for pattern in [
        format!("{label} says, \""),
        format!("{label} said, \""),
        format!("{label} says \""),
        format!("{label} said \""),
    ] {
        if let Some(idx) = trimmed
            .to_ascii_lowercase()
            .find(&pattern.to_ascii_lowercase())
        {
            let after = &trimmed[idx + pattern.len()..];
            let said = after.split('"').next().unwrap_or("").trim();
            if !said.is_empty() {
                return Some(said.to_string());
            }
        }
    }
    None
}

fn capitalise(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// The campaign's prose voice: a dedicated style note, then any non-character
/// note's `writing_style` frontmatter, then a sample of scene prose.
pub fn campaign_writing_style(notes: &[(&str, &Document, bool)]) -> String {
    for (path, doc, _) in notes {
        let stem = super::classify::file_stem(path).to_ascii_lowercase();
        if stem == "writing_style" || stem == "style" {
            let body = doc.body.trim();
            if !body.is_empty() {
                return body.to_string();
            }
        }
    }

    for (_, doc, is_character) in notes {
        if *is_character {
            continue;
        }
        if let Some(style) = first_non_empty(&doc.frontmatter, &["writing_style", "style"]) {
            return style;
        }
    }

    let mut snippets: Vec<String> = Vec::new();
    for (_, doc, _) in notes {
        let declared = doc
            .frontmatter
            .get("type")
            .map(|v| v.first_string().to_ascii_lowercase())
            .unwrap_or_default();
        if declared != "scene" && declared != "story" {
            continue;
        }
        let body = doc.body.trim();
        if body.is_empty() {
            continue;
        }
        let mut snippet: String = body.chars().take(250).collect();
        if body.chars().count() > 250 {
            snippet.push_str("...");
        }
        snippets.push(snippet);
        if snippets.len() >= 2 {
            break;
        }
    }
    if snippets.is_empty() {
        String::new()
    } else {
        format!("Representative Prose Style:\n{}", snippets.join("\n---\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::markdown::parse;

    #[test]
    fn an_explicit_style_field_wins_and_keeps_its_traits_prefix() {
        let doc =
            parse("---\ntraits: dry, precise\nwriting_style: Clipped sentences.\n---\n\nBody.\n");
        let style = character_writing_style("Elara Voss", "elara_voss", &doc, &[]);
        assert!(style.starts_with("Personality Traits/Tone: dry, precise\n"));
        assert!(style.ends_with("Clipped sentences."));
    }

    #[test]
    fn blockquotes_become_voice_samples_but_callout_markup_does_not() {
        let doc = parse("> [!secret]\n> A hidden fact.\n\n> \"I have not the time.\"\n");
        let style = character_writing_style("Mira", "mira", &doc, &[]);
        assert!(style.contains("I have not the time."));
        assert!(!style.contains("[!secret]"));
    }

    #[test]
    fn dialogue_is_mined_from_other_notes() {
        let doc = parse("A quiet man.\n");
        let scene = parse("Bram Holt: \"Two coppers, same as always.\"\n");
        let style = character_writing_style(
            "Bram Holt",
            "bram_holt",
            &doc,
            &[("scenes/toll.md", &scene)],
        );
        assert!(style.contains("Two coppers, same as always."));
    }

    #[test]
    fn a_characters_own_note_is_not_mined_for_its_own_dialogue() {
        let doc = parse("A quiet man.\n");
        let own = parse("Bram Holt: \"Should not appear.\"\n");
        let style = character_writing_style(
            "Bram Holt",
            "bram_holt",
            &doc,
            &[("characters/bram_holt.md", &own)],
        );
        assert!(!style.contains("Should not appear."));
    }
}
