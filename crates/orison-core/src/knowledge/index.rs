//! Name resolution: label, alias, id or slug to an [`EntityId`].
//!
//! This lives in `knowledge` rather than in `ingest` because deciding what
//! `[[The Landing]]` refers to is a question about entity identity, and entity
//! identity has exactly one owner (§3.3). `VaultCompiler` kept its own
//! `_node_name_to_id` and `_location_name_to_id` dictionaries and
//! `CampaignState` kept a third; that is the defect `orison_audit.md` §9
//! records, and the reason this is a shared component rather than a local one.

use std::collections::HashMap;

use super::types::EntityId;

/// A case-insensitive index from every name an entity answers to, to its id.
#[derive(Debug, Default, Clone)]
pub struct NameIndex {
    by_name: HashMap<String, EntityId>,
}

impl NameIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a label, its id, and any aliases. Later registrations do not
    /// displace earlier ones, so the first note to claim a name keeps it and
    /// resolution stays independent of directory iteration order.
    pub fn insert(&mut self, id: &EntityId, label: &str, aliases: &[String]) {
        self.insert_name(label, id);
        self.insert_name(id.as_str(), id);
        for alias in aliases {
            self.insert_name(alias, id);
        }
    }

    fn insert_name(&mut self, name: &str, id: &EntityId) {
        let key = normalise(name);
        if key.is_empty() {
            return;
        }
        self.by_name.entry(key).or_insert_with(|| id.clone());
    }

    /// Resolve a written reference. Tries the name as written, then its slug,
    /// so `[[Fen Marches]]`, `fen_marches` and `Fen marches` all land on the
    /// same entity.
    pub fn resolve(&self, name: &str) -> Option<&EntityId> {
        let key = normalise(name);
        if let Some(id) = self.by_name.get(&key) {
            return Some(id);
        }
        self.by_name.get(EntityId::slug(name).as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.resolve(name).is_some()
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

fn normalise(name: &str) -> String {
    name.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_labels_aliases_ids_and_slugs() {
        let mut index = NameIndex::new();
        let id = EntityId::slug("Saltmarsh Landing");
        index.insert(
            &id,
            "Saltmarsh Landing",
            &["The Landing".to_string(), "Saltmarsh".to_string()],
        );

        for name in [
            "Saltmarsh Landing",
            "saltmarsh landing",
            "The Landing",
            "saltmarsh",
            "saltmarsh_landing",
        ] {
            assert_eq!(index.resolve(name), Some(&id), "resolving {name:?}");
        }
        assert!(index.resolve("The Chancellor").is_none());
    }

    #[test]
    fn the_first_claim_on_a_name_wins() {
        let mut index = NameIndex::new();
        let first = EntityId::slug("a");
        let second = EntityId::slug("b");
        index.insert(&first, "Shared", &[]);
        index.insert(&second, "Shared", &[]);
        assert_eq!(index.resolve("Shared"), Some(&first));
    }
}
