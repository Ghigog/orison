//! The frontmatter subset of YAML that vaults actually use.
//!
//! A port of `MarkdownParser._parse_yaml()`, which is not a YAML parser and
//! does not pretend to be: scalars, inline lists, and block lists, which is
//! what Obsidian frontmatter contains. Pulling in a full YAML implementation
//! would change behaviour on malformed frontmatter from "keep what parsed" to
//! "reject the file", and rejecting a file is the one thing ingest must not do.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum YamlValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<String>),
    /// A one-level nested mapping: `relationships:` with indented
    /// `Target: relation` lines under it. `MarkdownParser._parse_yaml()` read
    /// those indented lines as top-level keys, which quietly turned every
    /// relationship target into a stray frontmatter field.
    Map(BTreeMap<String, String>),
}

impl YamlValue {
    /// The scalar as a string. Lists join with ", " because every consumer of a
    /// list-valued frontmatter field in the Godot build did exactly that.
    pub fn as_display_string(&self) -> String {
        match self {
            YamlValue::Bool(b) => b.to_string(),
            YamlValue::Int(i) => i.to_string(),
            YamlValue::Float(f) => f.to_string(),
            YamlValue::Str(s) => s.clone(),
            YamlValue::List(items) => items.join(", "),
            YamlValue::Map(entries) => entries
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            YamlValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            YamlValue::Bool(b) => Some(*b),
            YamlValue::Str(s) => match s.to_ascii_lowercase().as_str() {
                "true" | "yes" => Some(true),
                "false" | "no" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    /// The first element of a list, or the scalar itself. `type: [character,
    /// noble]` means "character"; `VaultCompiler._get_type_safe()` did the same.
    pub fn first_string(&self) -> String {
        match self {
            YamlValue::List(items) => items.first().cloned().unwrap_or_default(),
            other => other.as_display_string(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            YamlValue::Bool(b) => serde_json::Value::Bool(*b),
            YamlValue::Int(i) => serde_json::Value::from(*i),
            YamlValue::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            YamlValue::Str(s) => serde_json::Value::String(s.clone()),
            YamlValue::List(items) => serde_json::Value::Array(
                items
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
            YamlValue::Map(entries) => serde_json::Value::Object(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect(),
            ),
        }
    }
}

pub fn parse_frontmatter(text: &str) -> BTreeMap<String, YamlValue> {
    let mut out: BTreeMap<String, YamlValue> = BTreeMap::new();
    // The key an indented block belongs to, and the indent of the key line.
    let mut pending: Option<(String, usize)> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if let Some((key, key_indent)) = pending.clone() {
            if indent > key_indent {
                if let Some(item) = trimmed
                    .strip_prefix("- ")
                    .or_else(|| trimmed.strip_prefix('-').filter(|r| !r.starts_with('-')))
                {
                    push_list_item(&mut out, &key, strip_quotes(item.trim()));
                    continue;
                }
                if let Some((sub_key, sub_value)) = trimmed.split_once(':') {
                    let sub_value = strip_quotes(sub_value.trim()).to_string();
                    match out
                        .entry(key)
                        .or_insert_with(|| YamlValue::Map(BTreeMap::new()))
                    {
                        YamlValue::Map(entries) => {
                            entries.insert(sub_key.trim().to_string(), sub_value);
                        }
                        slot => {
                            let mut entries = BTreeMap::new();
                            entries.insert(sub_key.trim().to_string(), sub_value);
                            *slot = YamlValue::Map(entries);
                        }
                    }
                    continue;
                }
                continue;
            }
            // A list written flush with its key, which is legal YAML and
            // common in hand-written frontmatter.
            if indent == key_indent {
                if let Some(item) = trimmed
                    .strip_prefix("- ")
                    .or_else(|| trimmed.strip_prefix('-').filter(|r| !r.starts_with('-')))
                {
                    push_list_item(&mut out, &key, strip_quotes(item.trim()));
                    continue;
                }
            }
            pending = None;
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim();

        if value.is_empty() {
            pending = Some((key.clone(), indent));
            out.insert(key, YamlValue::List(Vec::new()));
            continue;
        }

        if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            let items = inner
                .split(',')
                .map(|i| strip_quotes(i.trim()).to_string())
                .filter(|i| !i.is_empty())
                .collect();
            out.insert(key, YamlValue::List(items));
            continue;
        }

        out.insert(key, cast_scalar(strip_quotes(value)));
    }

    // A key introducing a block that turned out to be empty stays an empty
    // list rather than vanishing: the author wrote the key.
    out
}

fn push_list_item(out: &mut BTreeMap<String, YamlValue>, key: &str, value: &str) {
    match out
        .entry(key.to_string())
        .or_insert_with(|| YamlValue::List(Vec::new()))
    {
        YamlValue::List(items) => items.push(value.to_string()),
        slot => *slot = YamlValue::List(vec![value.to_string()]),
    }
}

fn strip_quotes(s: &str) -> &str {
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return &s[1..s.len() - 1];
        }
    }
    s
}

fn cast_scalar(s: &str) -> YamlValue {
    match s.to_ascii_lowercase().as_str() {
        "true" => return YamlValue::Bool(true),
        "false" => return YamlValue::Bool(false),
        _ => {}
    }
    if let Ok(i) = s.parse::<i64>() {
        return YamlValue::Int(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return YamlValue::Float(f);
    }
    YamlValue::Str(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_are_typed() {
        let fm = parse_frontmatter("type: character\npower: 3\ncan_speak: false\nweight: 1.5\n");
        assert_eq!(fm["type"], YamlValue::Str("character".into()));
        assert_eq!(fm["power"], YamlValue::Int(3));
        assert_eq!(fm["can_speak"], YamlValue::Bool(false));
        assert_eq!(fm["weight"], YamlValue::Float(1.5));
    }

    #[test]
    fn inline_and_block_lists() {
        let fm = parse_frontmatter(
            "aliases: [The Landing, Saltmarsh]\ntags:\n  - border\n  - contested\n",
        );
        assert_eq!(
            fm["aliases"],
            YamlValue::List(vec!["The Landing".into(), "Saltmarsh".into()])
        );
        assert_eq!(
            fm["tags"],
            YamlValue::List(vec!["border".into(), "contested".into()])
        );
    }

    #[test]
    fn nested_maps_are_a_map_not_stray_top_level_keys() {
        let fm = parse_frontmatter(
            "relationships:\n  Bram Holt: rival\n  Elara Voss: mentor\nname: Mira\n",
        );
        let YamlValue::Map(entries) = &fm["relationships"] else {
            panic!("expected a map, got {:?}", fm["relationships"]);
        };
        assert_eq!(entries["Bram Holt"], "rival");
        assert_eq!(entries["Elara Voss"], "mentor");
        assert!(
            !fm.contains_key("Bram Holt"),
            "indented keys leaked to the top level"
        );
        assert_eq!(fm["name"], YamlValue::Str("Mira".into()));
    }

    #[test]
    fn a_value_containing_a_colon_keeps_its_tail() {
        let fm = parse_frontmatter("gender: female, she/her\nnote: see this: here\n");
        assert_eq!(fm["gender"], YamlValue::Str("female, she/her".into()));
        assert_eq!(fm["note"], YamlValue::Str("see this: here".into()));
    }
}
