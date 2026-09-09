//! Type classification, character-property inference, and gender inference.
//!
//! The folder and frontmatter heuristics are ported from
//! `VaultCompiler._process_nodes_first_pass()` and
//! `_determine_character_properties()` and behave the same way, including the
//! order they are applied in: a deepest-folder match beats a match anywhere
//! else in the path, which is what keeps `10_Entities/places/` a location
//! rather than a character even though `10_Entities` contains "entit".
//!
//! Gender inference is new, and is the deterministic answer to
//! `rag_architecture.md` Bug 3.

use std::collections::BTreeMap;

use crate::knowledge::EntityKind;

use super::yaml::YamlValue;

/// Decide what a file is.
///
/// `explicit_folders` are the user's own folder-to-type mappings from
/// onboarding, which win outright.
pub fn classify(
    frontmatter: &BTreeMap<String, YamlValue>,
    relative_path: &str,
    explicit_folders: &BTreeMap<String, String>,
) -> EntityKind {
    let relative_folder = parent_folder(relative_path);
    if let Some(mapped) = explicit_folders.get(relative_folder) {
        return match mapped.to_ascii_lowercase().as_str() {
            "scene" | "story" | "event" | "quest" => EntityKind::Scene,
            other => EntityKind::from_str_lossy(other),
        };
    }

    let declared = declared_type(frontmatter);
    // Leading separator for the same reason as in `creature_properties`.
    let path_lower = format!("/{}", relative_path.to_ascii_lowercase());
    let folder_lower = relative_folder.to_ascii_lowercase();
    let last_folder = folder_lower.rsplit('/').next().unwrap_or("").to_string();
    let stem_lower = file_stem(relative_path).to_ascii_lowercase();

    let is_scene_path = ["scene", "story", "chapter", "event", "quest", "plot"]
        .iter()
        .any(|k| path_lower.contains(&format!("/{k}")));
    let is_scene_name = matches!(
        stem_lower.as_str(),
        "start"
            | "beginning"
            | "intro"
            | "introduction"
            | "scene_1"
            | "scene1"
            | "chapter_1"
            | "chapter1"
    );

    const LOCATION_WORDS: [&str; 7] = [
        "location",
        "environment",
        "world",
        "env",
        "map",
        "place",
        "setting",
    ];
    const CHARACTER_WORDS: [&str; 5] = ["character", "npc", "entity", "person", "people"];

    // Deepest folder component first, then anywhere in the path.
    let (mut is_location_path, mut is_character_path) = (false, false);
    if LOCATION_WORDS.iter().any(|w| last_folder.contains(w)) {
        is_location_path = true;
    } else if CHARACTER_WORDS.iter().any(|w| last_folder.contains(w)) {
        is_character_path = true;
    } else if LOCATION_WORDS
        .iter()
        .any(|w| folder_lower.contains(w) || path_lower.contains(&format!("/{w}")))
    {
        is_location_path = true;
    } else if CHARACTER_WORDS
        .iter()
        .any(|w| folder_lower.contains(w) || path_lower.contains(&format!("/{w}")))
    {
        is_character_path = true;
    }

    match declared.as_deref() {
        Some("scene" | "story" | "event" | "quest") => return EntityKind::Scene,
        Some("location") => return EntityKind::Location,
        Some("character" | "npc" | "fauna" | "flora" | "creature" | "monster" | "bestiary") => {
            return EntityKind::Character
        }
        _ => {}
    }

    if is_scene_path || is_scene_name {
        return EntityKind::Scene;
    }
    if is_location_path {
        return EntityKind::Location;
    }
    if is_character_path {
        return EntityKind::Character;
    }

    match declared.as_deref() {
        Some("lore") => EntityKind::Lore,
        Some("item") => EntityKind::Item,
        Some(other) if !other.is_empty() => EntityKind::from_str_lossy(other),
        // No type, no folder hint. `VaultCompiler._get_type_safe()` called this
        // "lore", which asserts something about content nobody knows anything
        // about. `messy/notes/misc scratch.md` is the fixture for it.
        _ => EntityKind::Note,
    }
}

fn declared_type(frontmatter: &BTreeMap<String, YamlValue>) -> Option<String> {
    frontmatter
        .get("type")
        .or_else(|| frontmatter.get("orison_type"))
        .map(|v| v.first_string().trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
}

pub fn parent_folder(relative_path: &str) -> &str {
    match relative_path.rfind('/') {
        Some(i) => &relative_path[..i],
        None => "",
    }
}

pub fn file_stem(relative_path: &str) -> &str {
    let name = relative_path.rsplit('/').next().unwrap_or(relative_path);
    name.strip_suffix(".md").unwrap_or(name)
}

/// Whether a character speaks, and whether it is humanoid.
///
/// A direct port of `_determine_character_properties()`: an explicit
/// frontmatter value always wins, and path or type hints only fill in what
/// frontmatter did not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatureProperties {
    pub is_creature: bool,
    pub can_speak: bool,
    pub humanoid: bool,
}

pub fn creature_properties(
    frontmatter: &BTreeMap<String, YamlValue>,
    relative_path: &str,
) -> CreatureProperties {
    let bool_of = |key: &str| frontmatter.get(key).and_then(YamlValue::as_bool);

    let mut props = CreatureProperties {
        is_creature: bool_of("is_creature")
            .or_else(|| bool_of("creature"))
            .unwrap_or(false),
        can_speak: bool_of("can_speak").unwrap_or(true),
        humanoid: bool_of("humanoid").unwrap_or(true),
    };

    // Paths here are vault-relative, so a top-level `bestiary/` folder has no
    // leading separator. The Godot checks were written against absolute paths
    // and all required one; prefixing restores the same matches.
    let path_lower = format!("/{}", relative_path.to_ascii_lowercase());
    let stem = file_stem(&path_lower).to_string();
    let path_says_creature = [
        "/fauna/",
        "/flora/",
        "/creature",
        "/monster",
        "/bestiary",
        "/animal",
        "/beast",
    ]
    .iter()
    .any(|k| path_lower.contains(k))
        || stem.starts_with("fauna_")
        || stem.starts_with("flora_");

    let declared = declared_type(frontmatter).unwrap_or_default();
    let type_says_creature = ["creature", "monster", "flora", "fauna", "animal", "beast"]
        .iter()
        .any(|k| declared.contains(k));

    if path_says_creature || type_says_creature {
        if !frontmatter.contains_key("is_creature") && !frontmatter.contains_key("creature") {
            props.is_creature = true;
        }
        if !frontmatter.contains_key("can_speak") {
            props.can_speak = false;
        }
        if !frontmatter.contains_key("humanoid") {
            props.humanoid = false;
        }
    }

    props
}

/// Where an inferred gender came from. Recorded so a wrong answer is
/// debuggable rather than mysterious, which is what Bug 3 was for six months.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenderSource {
    Frontmatter,
    /// An honorific in the label: "King", "Lady".
    Title,
    /// Third-person pronouns counted in the note's own prose.
    Prose,
    /// Title and prose disagreed; the prose won. Worth surfacing.
    ProseOverTitle,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredGender {
    /// In the Godot build's format: `"male, he/him"`.
    pub value: String,
    pub source: GenderSource,
}

/// Infer gender and pronouns, **never from the given name**.
///
/// That prohibition is the whole point. `llama3.2:3b` narrated King Yuna as
/// "she" because the name carries a feminine statistical prior, and
/// `messy/10_Entities/characters/Lord Anneke.md` was built to be the same trap
/// with nothing else to lean on. The signals used here, in order:
///
/// 1. A frontmatter `gender`, `pronouns` or `sex` field, or a boolean
///    `he/him`-style key.
/// 2. An honorific in the label.
/// 3. The pronouns the author actually used in the prose.
///
/// When the title and the prose disagree, the prose wins: the author's own
/// usage across a whole note is better evidence than one word of address.
pub fn infer_gender(
    frontmatter: &BTreeMap<String, YamlValue>,
    label: &str,
    body: &str,
) -> InferredGender {
    for key in ["gender", "pronouns", "sex"] {
        if let Some(v) = frontmatter.get(key) {
            let s = v.as_display_string().trim().to_string();
            if !s.is_empty() {
                return InferredGender {
                    value: s,
                    source: GenderSource::Frontmatter,
                };
            }
        }
    }
    for (key, phrase) in [
        ("he/him", "male, he/him"),
        ("she/her", "female, she/her"),
        ("they/them", "non-binary, they/them"),
    ] {
        if let Some(v) = frontmatter.get(key) {
            match v.as_bool() {
                Some(true) => {
                    return InferredGender {
                        value: phrase.to_string(),
                        source: GenderSource::Frontmatter,
                    }
                }
                Some(false) => continue,
                None => {
                    let s = v.as_display_string().trim().to_string();
                    if !s.is_empty() {
                        return InferredGender {
                            value: s,
                            source: GenderSource::Frontmatter,
                        };
                    }
                }
            }
        }
    }

    let title = gender_from_title(label);
    let prose = gender_from_prose(body);

    match (title, prose) {
        (Some(t), Some(p)) if t == p => InferredGender {
            value: phrase_for(t),
            source: GenderSource::Title,
        },
        (Some(_), Some(p)) => InferredGender {
            value: phrase_for(p),
            source: GenderSource::ProseOverTitle,
        },
        (Some(t), None) => InferredGender {
            value: phrase_for(t),
            source: GenderSource::Title,
        },
        (None, Some(p)) => InferredGender {
            value: phrase_for(p),
            source: GenderSource::Prose,
        },
        (None, None) => InferredGender {
            value: String::new(),
            source: GenderSource::None,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gendered {
    Masculine,
    Feminine,
    Neutral,
}

fn phrase_for(g: Gendered) -> String {
    match g {
        Gendered::Masculine => "male, he/him".to_string(),
        Gendered::Feminine => "female, she/her".to_string(),
        Gendered::Neutral => "non-binary, they/them".to_string(),
    }
}

/// Honorifics only. Given names are never consulted, at any point.
fn gender_from_title(label: &str) -> Option<Gendered> {
    const MASCULINE: [&str; 17] = [
        "king", "lord", "sir", "duke", "baron", "count", "emperor", "prince", "father", "brother",
        "mr", "mister", "master", "earl", "marquis", "abbot", "friar",
    ];
    const FEMININE: [&str; 17] = [
        "queen", "lady", "dame", "duchess", "baroness", "countess", "empress", "princess",
        "mother", "sister", "mrs", "ms", "miss", "mistress", "madam", "abbess", "matron",
    ];

    for word in label.split_whitespace() {
        let w: String = word
            .chars()
            .filter(|c| c.is_alphabetic())
            .flat_map(|c| c.to_lowercase())
            .collect();
        if MASCULINE.contains(&w.as_str()) {
            return Some(Gendered::Masculine);
        }
        if FEMININE.contains(&w.as_str()) {
            return Some(Gendered::Feminine);
        }
    }
    None
}

/// Count third-person pronouns as whole words. A clear majority decides;
/// anything close is treated as no evidence rather than a coin toss.
fn gender_from_prose(body: &str) -> Option<Gendered> {
    let (mut masc, mut fem, mut neut) = (0usize, 0usize, 0usize);
    for word in body.split(|c: char| !c.is_alphabetic() && c != '\'') {
        match word.to_ascii_lowercase().as_str() {
            "he" | "him" | "his" | "himself" => masc += 1,
            "she" | "her" | "hers" | "herself" => fem += 1,
            "they" | "them" | "their" | "theirs" | "themselves" => neut += 1,
            _ => {}
        }
    }

    let mut ranked = [
        (Gendered::Masculine, masc),
        (Gendered::Feminine, fem),
        (Gendered::Neutral, neut),
    ];
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let (top, top_n) = ranked[0];
    let (_, second_n) = ranked[1];

    if top_n >= 2 && top_n >= second_n * 2 {
        Some(top)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::yaml::parse_frontmatter;

    fn folders() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn deepest_folder_beats_an_ancestor() {
        // `10_Entities` contains "entit"; `places` must still win.
        let fm = parse_frontmatter("");
        assert_eq!(
            classify(&fm, "10_Entities/places/Fen Marches.md", &folders()),
            EntityKind::Location
        );
        assert_eq!(
            classify(
                &fm,
                "10_Entities/characters/Mira of the Fens.md",
                &folders()
            ),
            EntityKind::Character
        );
    }

    #[test]
    fn an_untyped_note_in_an_unhinted_folder_is_a_note() {
        let fm = parse_frontmatter("");
        assert_eq!(
            classify(&fm, "notes/misc scratch.md", &folders()),
            EntityKind::Note
        );
    }

    #[test]
    fn frontmatter_lore_survives_a_lore_folder() {
        let fm = parse_frontmatter("type: lore\n");
        assert_eq!(
            classify(&fm, "30_Systems/lore/The Salt Tithe.md", &folders()),
            EntityKind::Lore
        );
    }

    #[test]
    fn explicit_folder_mapping_wins() {
        let mut folders = BTreeMap::new();
        folders.insert("notes".to_string(), "lore".to_string());
        let fm = parse_frontmatter("");
        assert_eq!(
            classify(&fm, "notes/misc scratch.md", &folders),
            EntityKind::Lore
        );
    }

    #[test]
    fn creature_defaults_follow_the_path_but_never_override_frontmatter() {
        let fm = parse_frontmatter("");
        let p = creature_properties(&fm, "bestiary/fen adder.md");
        assert_eq!(
            p,
            CreatureProperties {
                is_creature: true,
                can_speak: false,
                humanoid: false
            }
        );

        let fm = parse_frontmatter("can_speak: true\n");
        let p = creature_properties(&fm, "bestiary/fen adder.md");
        assert!(p.can_speak, "an explicit frontmatter value must win");
    }

    #[test]
    fn gender_comes_from_frontmatter_when_present() {
        let fm = parse_frontmatter("gender: female, she/her\n");
        let g = infer_gender(&fm, "Sergeant Adah", "She served eleven years.");
        assert_eq!(g.value, "female, she/her");
        assert_eq!(g.source, GenderSource::Frontmatter);
    }

    #[test]
    fn gender_comes_from_the_title_and_prose_never_the_name() {
        // The headline case. "Yuna" and "Anneke" both carry feminine priors;
        // neither is consulted.
        let fm = parse_frontmatter("");
        let yuna = infer_gender(
            &fm,
            "King Yuna",
            "Yuna took the salt throne after his three older brothers drowned. He has ruled for thirty-one years.",
        );
        assert_eq!(yuna.value, "male, he/him");

        let anneke = infer_gender(
            &fm,
            "Lord Anneke",
            "He is a widower, twice, and has raised four sons alone. The men of his household call him \"the old man\".",
        );
        assert_eq!(anneke.value, "male, he/him");
    }

    #[test]
    fn prose_wins_when_it_contradicts_the_title() {
        let fm = parse_frontmatter("");
        let g = infer_gender(
            &fm,
            "Lord Elenwe",
            "She holds the marches. Her banner flies over the causeway, and her word is law there.",
        );
        assert_eq!(g.value, "female, she/her");
        assert_eq!(g.source, GenderSource::ProseOverTitle);
    }

    #[test]
    fn no_evidence_yields_no_claim() {
        let fm = parse_frontmatter("");
        let g = infer_gender(&fm, "Mira", "Runs the eel traps south of the Landing.");
        assert!(g.value.is_empty());
        assert_eq!(g.source, GenderSource::None);
    }
}
