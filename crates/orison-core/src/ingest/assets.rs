//! Resolving asset references to files that exist in the vault.
//!
//! Ports the reference-matching half of `_find_and_copy_asset()` and the
//! portrait search in `_process_nodes_first_pass()`: frontmatter keys first,
//! then body embeds, then a name match against the file list, in that order of
//! confidence.
//!
//! It deliberately does **not** port the copying. `VaultCompiler` copied every
//! matched asset into `user://assets/...` at compile time, which is a shell
//! concern and a platform assumption; `orison-core` has neither. What lands in
//! the graph is the vault-relative path of the file that was matched, and the
//! shell decides whether to copy, symlink or stream it.

use std::collections::BTreeMap;
use std::path::Path;

use super::markdown::Document;
use super::scan::relative_string;
use super::yaml::YamlValue;

/// Frontmatter keys that name a character portrait, in the order
/// `VaultCompiler` tried them.
pub const PORTRAIT_KEYS: [&str; 9] = [
    "image",
    "avatar",
    "portrait",
    "sprite",
    "picture",
    "cover image",
    "cover_image",
    "cover",
    "banner",
];

pub const SCENERY_KEYS: [&str; 7] = [
    "scenery",
    "background",
    "image",
    "cover",
    "scenery_image",
    "bg_image",
    "bg",
];

pub const AUDIO_KEYS: [&str; 6] = ["bgm", "music", "audio", "sound", "ambient", "soundtrack"];

/// Find the asset a note refers to, if any of it exists on disk.
pub fn resolve_asset(
    frontmatter: &BTreeMap<String, YamlValue>,
    doc: &Document,
    keys: &[&str],
    candidates: &[std::path::PathBuf],
    id: &str,
    label: &str,
    search_embeds: bool,
) -> Option<String> {
    for key in keys {
        if let Some(value) = frontmatter.get(*key).and_then(YamlValue::as_str) {
            if let Some(hit) = match_reference(value, candidates) {
                return Some(hit);
            }
        }
    }

    if search_embeds {
        for embed in &doc.embeds {
            if let Some(hit) = match_reference(embed, candidates) {
                return Some(hit);
            }
        }
    }

    // Last resort: a file named after the entity. Exact, then prefix, then
    // substring — the same three steps, in the same order, as the Godot build.
    let id_lower = id.to_ascii_lowercase();
    let label_lower = label.to_ascii_lowercase();
    for test in [
        |stem: &str, needle: &str| stem == needle,
        |stem: &str, needle: &str| stem.starts_with(needle),
        |stem: &str, needle: &str| stem.contains(needle),
    ] {
        for path in candidates {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if test(&stem, &id_lower) || test(&stem, &label_lower) {
                return Some(relative_string(path));
            }
        }
    }
    None
}

fn match_reference(raw: &str, candidates: &[std::path::PathBuf]) -> Option<String> {
    let mut value = raw.trim();
    if let Some(inner) = value.strip_prefix("[[").and_then(|v| v.strip_suffix("]]")) {
        value = inner.trim();
    }
    if let Some((head, _)) = value.split_once('|') {
        value = head.trim();
    }
    if value.is_empty() {
        return None;
    }
    let value_lower = value.to_ascii_lowercase();
    let file_name = Path::new(&value_lower)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| value_lower.clone());

    for path in candidates {
        let path_lower = relative_string(path).to_ascii_lowercase();
        let candidate_name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if candidate_name == file_name || path_lower.ends_with(&value_lower) {
            return Some(relative_string(path));
        }
    }

    let stem = Path::new(&file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if stem.is_empty() {
        return None;
    }
    for path in candidates {
        let candidate_stem = path
            .file_stem()
            .map(|f| f.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if candidate_stem == stem || candidate_stem.contains(&stem) {
            return Some(relative_string(path));
        }
    }
    None
}
