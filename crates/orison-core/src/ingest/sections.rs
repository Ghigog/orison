//! Heading normalisation and the canonical-field alias table.
//!
//! This is the direct fix for `rag_architecture.md` Bug 1.
//! `VaultCompiler._normalize_section_name()` stripped spaces, underscores and
//! hyphens and nothing else, so `**1. History**` normalised to
//! `"**1.history**"`, matched no alias, and the section was thrown away. The
//! Godot build's answer was to delete section parsing entirely and hand the
//! whole note to a model. That trades one silent loss for another: with no
//! model reachable, or with a model that fences its JSON (B-16), the character
//! ends up with no fields at all.
//!
//! Here, normalisation removes everything that is formatting rather than
//! meaning, and anything that still fails to match goes to the overflow bucket
//! rather than to the bin.

use crate::knowledge::CanonicalField;
use crate::knowledge::EntityKind;

/// Reduce a heading to its comparable form.
///
/// Strips, in order: leading `#` markers, wrapping and inline emphasis (`**`,
/// `__`, `*`, `_`), list bullets, ordinal prefixes (`1.`, `2)`, `iii.`),
/// trailing colons, and finally every character that is not a letter or digit.
pub fn normalise_heading(raw: &str) -> String {
    let mut s = raw.trim().trim_start_matches('#').trim().to_string();

    // Emphasis markers, wrapping or inline. Removing them wholesale is safe
    // here because the result is only ever used as a lookup key.
    for marker in ["***", "**", "__", "*", "_"] {
        s = s.replace(marker, " ");
    }
    let mut s = s.trim().to_string();

    // Bullets and ordinal prefixes: "1.", "2)", "iv.", "- ".
    loop {
        let before = s.clone();
        s = s
            .trim_start_matches(['-', '+', '\u{2022}'])
            .trim()
            .to_string();
        s = strip_ordinal_prefix(&s).to_string();
        if s == before {
            break;
        }
    }

    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn strip_ordinal_prefix(s: &str) -> &str {
    let mut chars = s.char_indices();
    let mut end = 0;
    let mut saw_digit_or_roman = false;
    for (i, c) in chars.by_ref() {
        if c.is_ascii_digit() || matches!(c.to_ascii_lowercase(), 'i' | 'v' | 'x') {
            saw_digit_or_roman = true;
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if !saw_digit_or_roman {
        return s;
    }
    let rest = &s[end..];
    match rest.strip_prefix(['.', ')', ':']) {
        Some(tail) => tail.trim_start(),
        // "iv" or "3" alone is not an ordinal prefix, it is the whole heading.
        None => s,
    }
}

/// Map a normalised heading to a canonical field, if it maps at all.
///
/// The table is deliberately modest. Every entry earns its place by appearing
/// in a real vault or in the `messy/` fixture; guessing wider costs nothing in
/// data (unmapped sections are retained) and costs precision when a guess is
/// wrong.
pub fn canonical_field(normalised: &str, kind: EntityKind) -> Option<CanonicalField> {
    use CanonicalField::*;

    // Character-specific readings first, where the same heading means
    // different things depending on what the note is about.
    if kind == EntityKind::Character {
        match normalised {
            "appearance"
            | "physicaldescription"
            | "physicalappearance"
            | "physical"
            | "looks"
            | "features"
            | "portrait"
            | "description" => return Some(Appearance),
            // "Who he is" / "Who she is" / "Who they are": the Lord Anneke
            // fixture's opening heading.
            "whoheis" | "whosheis" | "whotheyare" | "whoitis" | "whois" | "whotheyre" => {
                return Some(Biography)
            }
            _ => {}
        }
    }

    match normalised {
        "biography" | "bio" | "history" | "background" | "backstory" | "past" | "origins"
        | "origin" | "life" | "story" => Some(Biography),

        "personality" | "temperament" | "manner" | "manners" | "traits" | "demeanour"
        | "demeanor" | "disposition" | "behaviour" | "behavior" | "bearing" | "attitude"
        | "mannerisms" | "character" => Some(Personality),

        "appearance" | "physicaldescription" | "physicalappearance" | "looks" | "features" => {
            Some(Appearance)
        }

        "goals" | "goal" | "motivation" | "motivations" | "aims" | "aim" | "wants"
        | "whathewants" | "whatshewants" | "whattheywant" | "objectives" | "objective"
        | "desires" | "ambitions" | "agenda" => Some(Goals),

        "gender" | "pronouns" | "sex" => Some(Gender),

        "description" | "overview" | "summary" | "details" | "about" => Some(Description),

        "writingstyle" | "dialoguestyle" | "dialogue" | "voice" | "quotes" | "speech" | "style"
        | "sayings" => Some(WritingStyle),

        _ => None,
    }
}

/// The field a note's opening prose belongs in when it has no heading of its
/// own. `messy/10_Entities/characters/Mira of the Fens.md` is entirely this.
pub fn primary_field(kind: EntityKind) -> CanonicalField {
    match kind {
        EntityKind::Character => CanonicalField::Biography,
        _ => CanonicalField::Description,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_numeric_headings_normalise_to_their_alias() {
        // The exact string from `rag_architecture.md` Bug 1.
        assert_eq!(normalise_heading("**1. History**"), "history");
        assert_eq!(
            canonical_field("history", EntityKind::Character),
            Some(CanonicalField::Biography)
        );
    }

    #[test]
    fn normalisation_handles_the_full_messy_fixture_set() {
        let cases = [
            ("**1. History**", "history"),
            ("**2. Manner**", "manner"),
            ("**3. Appearance**", "appearance"),
            ("**4. What He Wants**", "whathewants"),
            ("## Who he is", "whoheis"),
            ("## Bearing", "bearing"),
            ("## Background", "background"),
            ("## Temperament", "temperament"),
            ("## Smells Like", "smellslike"),
            ("## Rumour Table", "rumourtable"),
            (
                "### 2) Notes on her ledger format",
                "notesonherledgerformat",
            ),
            ("Goals:", "goals"),
            ("- History", "history"),
        ];
        for (raw, expected) in cases {
            assert_eq!(normalise_heading(raw), expected, "normalising {raw:?}");
        }
    }

    #[test]
    fn a_heading_that_is_only_a_numeral_survives_normalisation() {
        // "3" is the whole heading, not an ordinal prefix on an empty one.
        assert_eq!(normalise_heading("## 3"), "3");
        assert_eq!(normalise_heading("## IV"), "iv");
    }

    #[test]
    fn unmapped_headings_return_none_so_they_reach_overflow() {
        assert_eq!(canonical_field("smellslike", EntityKind::Location), None);
        assert_eq!(canonical_field("rumourtable", EntityKind::Location), None);
    }

    #[test]
    fn description_reads_as_appearance_on_a_character_only() {
        assert_eq!(
            canonical_field("description", EntityKind::Character),
            Some(CanonicalField::Appearance)
        );
        assert_eq!(
            canonical_field("description", EntityKind::Location),
            Some(CanonicalField::Description)
        );
    }
}
