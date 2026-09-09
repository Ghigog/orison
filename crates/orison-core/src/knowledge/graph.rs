//! `KnowledgeGraph`: typed nodes, typed edges, `petgraph` underneath, and the
//! only place in `orison-core` that holds entities.
//!
//! `orison_audit.md` §9 records the shape of the problem this replaces:
//! `VaultCompiler` kept `_node_name_to_id` and `_location_name_to_id`,
//! `CampaignState` kept its own view of every character inside node
//! `properties`, and `KnowledgeGraphManager` kept the graph — three stores that
//! could and did disagree. Large parts of both callers bypassed the graph
//! entirely.
//!
//! So: entity data is read from here or it is not read. If a caller wants an
//! entity and reaches for something else, that is the defect coming back.
//! `tests/knowledge_graph.rs` greps the crate for it.

use std::collections::HashMap;

use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use serde::{Deserialize, Serialize};

use crate::state::{CampaignStore, EdgeRow, NodeRow, StateError};

use super::index::NameIndex;
use super::types::{CanonicalField, Edge, EdgeKind, Entity, EntityId, EntityKind, OverflowSection};

#[derive(Debug, Clone, PartialEq)]
struct EdgeData {
    kind: EdgeKind,
    weight: f64,
}

/// The campaign's entities and their relationships.
#[derive(Debug, Clone, Default)]
pub struct KnowledgeGraph {
    graph: StableDiGraph<Entity, EdgeData>,
    /// The one legitimate `HashMap<EntityId, _>` in the crate: it maps an id to
    /// its position *inside* this graph, so it cannot drift out of agreement
    /// with the thing it indexes.
    index: HashMap<EntityId, NodeIndex>,
    names: NameIndex,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    // ------------------------------------------------------------------
    // Building
    // ------------------------------------------------------------------

    /// Insert an entity, replacing any entity already stored under its id.
    pub fn insert(&mut self, entity: Entity) -> EntityId {
        let id = entity.id.clone();
        self.names.insert(&id, &entity.label, &entity.aliases);
        match self.index.get(&id) {
            Some(idx) => {
                self.graph[*idx] = entity;
            }
            None => {
                let idx = self.graph.add_node(entity);
                self.index.insert(id.clone(), idx);
            }
        }
        id
    }

    /// Add an edge. Returns `false` when either endpoint is unknown — a
    /// dangling link is not an error and must not create a phantom node.
    pub fn connect(&mut self, edge: Edge) -> bool {
        let (Some(&from), Some(&to)) = (self.index.get(&edge.from), self.index.get(&edge.to))
        else {
            return false;
        };
        if from == to {
            return false;
        }
        let data = EdgeData {
            kind: edge.kind,
            weight: edge.weight.clamp(0.0, 1.0),
        };
        // Updating in place rather than appending keeps re-ingest idempotent;
        // `_add_edge_internal` did the same scan for the same reason.
        if let Some(existing) = self
            .graph
            .edges_connecting(from, to)
            .find(|e| e.weight().kind == data.kind)
            .map(|e| e.id())
        {
            self.graph[existing] = data;
        } else {
            self.graph.add_edge(from, to, data);
        }
        true
    }

    pub fn remove(&mut self, id: &EntityId) -> Option<Entity> {
        let idx = self.index.remove(id)?;
        // petgraph removes the incident edges with the node, so there is no
        // orphaned-edge cleanup pass to forget.
        self.graph.remove_node(idx)
    }

    // ------------------------------------------------------------------
    // Reading
    // ------------------------------------------------------------------

    pub fn get(&self, id: &EntityId) -> Option<&Entity> {
        self.index.get(id).map(|idx| &self.graph[*idx])
    }

    pub fn get_mut(&mut self, id: &EntityId) -> Option<&mut Entity> {
        let idx = *self.index.get(id)?;
        Some(&mut self.graph[idx])
    }

    pub fn contains(&self, id: &EntityId) -> bool {
        self.index.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Every entity, in insertion order.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.graph.node_weights()
    }

    pub fn ids(&self) -> impl Iterator<Item = &EntityId> + '_ {
        self.graph.node_weights().map(|e| &e.id)
    }

    pub fn by_kind(&self, kind: EntityKind) -> impl Iterator<Item = &Entity> {
        self.entities().filter(move |e| e.kind == kind)
    }

    pub fn by_level(&self, level: u8) -> impl Iterator<Item = &Entity> {
        self.entities().filter(move |e| e.level == level)
    }

    pub fn kind_of(&self, id: &EntityId) -> Option<EntityKind> {
        self.get(id).map(|e| e.kind)
    }

    /// Resolve a written name, alias or slug. The replacement for
    /// `_node_name_to_id`.
    pub fn resolve(&self, name: &str) -> Option<&EntityId> {
        self.names.resolve(name)
    }

    pub fn edges(&self) -> Vec<Edge> {
        IntoEdgeReferences::edge_references(&self.graph)
            .map(|e| Edge {
                from: self.graph[e.source()].id.clone(),
                to: self.graph[e.target()].id.clone(),
                kind: e.weight().kind.clone(),
                weight: e.weight().weight,
            })
            .collect()
    }

    /// Every edge touching `id`, in either direction.
    pub fn edges_of(&self, id: &EntityId) -> Vec<Edge> {
        let Some(&idx) = self.index.get(id) else {
            return Vec::new();
        };
        let mut out: Vec<Edge> = Vec::new();
        for direction in [petgraph::Direction::Outgoing, petgraph::Direction::Incoming] {
            for e in self.graph.edges_directed(idx, direction) {
                out.push(Edge {
                    from: self.graph[e.source()].id.clone(),
                    to: self.graph[e.target()].id.clone(),
                    kind: e.weight().kind.clone(),
                    weight: e.weight().weight,
                });
            }
        }
        out
    }

    /// Direct neighbours, ignoring edge direction. A relationship is a
    /// relationship whichever note happened to write it down.
    pub fn neighbours(&self, id: &EntityId) -> Vec<EntityId> {
        let Some(&idx) = self.index.get(id) else {
            return Vec::new();
        };
        let mut out: Vec<EntityId> = Vec::new();
        for n in self.graph.neighbors_undirected(idx) {
            let neighbour = self.graph[n].id.clone();
            if !out.contains(&neighbour) {
                out.push(neighbour);
            }
        }
        out
    }

    /// Breadth-first expansion out to `depth` hops, excluding the start.
    ///
    /// The replacement for `get_entities_connected_to()`'s hand-written
    /// recursion, which re-walked the entire edge array at every step and
    /// revisited nodes it had already seen at a greater depth.
    pub fn within(&self, id: &EntityId, depth: usize) -> Vec<EntityId> {
        self.within_hops(id, depth)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// The same expansion, keeping how far away each entity was.
    ///
    /// Retrieval needs the hop count to decay a neighbour's score with
    /// distance. Without it, "two hops away" and "adjacent" score alike, and
    /// the expansion floods the result set — which is half of why the Godot
    /// build returns 74 of 207 nodes for one query.
    pub fn within_hops(&self, id: &EntityId, depth: usize) -> Vec<(EntityId, usize)> {
        let Some(&start) = self.index.get(id) else {
            return Vec::new();
        };
        let mut seen: HashMap<NodeIndex, usize> = HashMap::from([(start, 0)]);
        let mut frontier = vec![start];
        let mut out: Vec<(EntityId, usize)> = Vec::new();

        for hop in 1..=depth {
            let mut next = Vec::new();
            for node in frontier.drain(..) {
                for neighbour in self.graph.neighbors_undirected(node) {
                    if seen.contains_key(&neighbour) {
                        continue;
                    }
                    seen.insert(neighbour, hop);
                    out.push((self.graph[neighbour].id.clone(), hop));
                    next.push(neighbour);
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // Persistence
    // ------------------------------------------------------------------

    pub fn to_rows(&self) -> (Vec<NodeRow>, Vec<EdgeRow>) {
        let nodes = self
            .entities()
            .map(|e| NodeRow {
                id: e.id.to_string(),
                label: e.label.clone(),
                kind: e.kind.as_str().to_string(),
                description: e.description.clone(),
                level: e.level as i64,
                source_path: e.source_path.clone(),
                body: e.body.clone(),
                properties: serde_json::to_string(&StoredProperties::from(e))
                    .unwrap_or_else(|_| "{}".to_string()),
            })
            .collect();
        let edges = self
            .edges()
            .into_iter()
            .map(|e| EdgeRow {
                from_id: e.from.to_string(),
                to_id: e.to.to_string(),
                relation: e.kind.as_str().to_string(),
                weight: e.weight,
            })
            .collect();
        (nodes, edges)
    }

    pub fn from_rows(nodes: &[NodeRow], edges: &[EdgeRow]) -> Result<Self, StateError> {
        let mut graph = KnowledgeGraph::new();
        for row in nodes {
            let stored: StoredProperties =
                serde_json::from_str(&row.properties).map_err(|e| StateError::Malformed {
                    what: "node properties",
                    id: row.id.clone(),
                    detail: e.to_string(),
                })?;
            let id = EntityId::from_stored(row.id.clone());
            let mut entity =
                Entity::new(id, row.label.clone(), EntityKind::from_str_lossy(&row.kind));
            entity.description = row.description.clone();
            entity.level = row.level.clamp(0, u8::MAX as i64) as u8;
            entity.source_path = row.source_path.clone();
            entity.body = row.body.clone();
            entity.aliases = stored.aliases;
            entity.tags = stored.tags;
            entity.fields = stored.fields;
            entity.overflow = stored.overflow;
            entity.properties = stored.properties;
            graph.insert(entity);
        }
        for row in edges {
            graph.connect(Edge {
                from: EntityId::from_stored(row.from_id.clone()),
                to: EntityId::from_stored(row.to_id.clone()),
                kind: EdgeKind::from_stored(&row.relation),
                weight: row.weight,
            });
        }
        Ok(graph)
    }

    pub fn save(&self, store: &mut CampaignStore, campaign_id: &str) -> Result<(), StateError> {
        let (nodes, edges) = self.to_rows();
        store.replace_graph(campaign_id, &nodes, &edges)
    }

    pub fn load(store: &CampaignStore, campaign_id: &str) -> Result<Self, StateError> {
        let nodes = store.graph_nodes(campaign_id)?;
        let edges = store.graph_edges(campaign_id)?;
        Self::from_rows(&nodes, &edges)
    }
}

/// Everything about an entity that is not its own column.
#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredProperties {
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    fields: std::collections::BTreeMap<CanonicalField, String>,
    #[serde(default)]
    overflow: Vec<OverflowSection>,
    #[serde(default)]
    properties: std::collections::BTreeMap<String, serde_json::Value>,
}

impl From<&Entity> for StoredProperties {
    fn from(e: &Entity) -> Self {
        Self {
            aliases: e.aliases.clone(),
            tags: e.tags.clone(),
            fields: e.fields.clone(),
            overflow: e.overflow.clone(),
            properties: e.properties.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(name: &str, kind: EntityKind) -> Entity {
        let mut e = Entity::new(EntityId::slug(name), name, kind);
        e.body = format!("Body of {name}.");
        e
    }

    fn sample() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        g.insert(entity("Mira of the Fens", EntityKind::Character));
        g.insert(entity("Saltmarsh Landing", EntityKind::Location));
        g.insert(entity("Fen Marches", EntityKind::Location));
        g.insert(entity("The Salt Tithe", EntityKind::Lore));
        g.connect(Edge {
            from: EntityId::slug("Mira of the Fens"),
            to: EntityId::slug("Saltmarsh Landing"),
            kind: EdgeKind::AssociatedWith,
            weight: 1.0,
        });
        g.connect(Edge {
            from: EntityId::slug("Saltmarsh Landing"),
            to: EntityId::slug("Fen Marches"),
            kind: EdgeKind::ConnectedTo,
            weight: 1.0,
        });
        g
    }

    #[test]
    fn insert_is_idempotent_on_id() {
        let mut g = sample();
        let before = g.len();
        g.insert(entity("Mira of the Fens", EntityKind::Character));
        assert_eq!(g.len(), before);
    }

    #[test]
    fn connecting_an_unknown_endpoint_creates_nothing() {
        let mut g = sample();
        let before = (g.len(), g.edge_count());
        let added = g.connect(Edge {
            from: EntityId::slug("The Salt Tithe"),
            to: EntityId::slug("The Chancellor"),
            kind: EdgeKind::LinksTo,
            weight: 1.0,
        });
        assert!(!added);
        assert_eq!((g.len(), g.edge_count()), before);
    }

    #[test]
    fn reconnecting_updates_rather_than_duplicates() {
        let mut g = sample();
        let before = g.edge_count();
        g.connect(Edge {
            from: EntityId::slug("Mira of the Fens"),
            to: EntityId::slug("Saltmarsh Landing"),
            kind: EdgeKind::AssociatedWith,
            weight: 0.5,
        });
        assert_eq!(g.edge_count(), before);
        assert_eq!(
            g.edges_of(&EntityId::slug("Mira of the Fens"))[0].weight,
            0.5
        );
    }

    #[test]
    fn neighbours_ignore_edge_direction() {
        let g = sample();
        let landing = EntityId::slug("Saltmarsh Landing");
        let mut names: Vec<String> = g
            .neighbours(&landing)
            .iter()
            .map(|i| i.to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["fen_marches", "mira_of_the_fens"]);
    }

    #[test]
    fn within_expands_by_hops_and_excludes_the_start() {
        let g = sample();
        let mira = EntityId::slug("Mira of the Fens");
        assert_eq!(
            g.within(&mira, 1),
            vec![EntityId::slug("Saltmarsh Landing")]
        );
        let two = g.within(&mira, 2);
        assert!(two.contains(&EntityId::slug("Fen Marches")));
        assert!(!two.contains(&mira));
    }

    #[test]
    fn removing_an_entity_takes_its_edges_with_it() {
        let mut g = sample();
        g.remove(&EntityId::slug("Saltmarsh Landing"));
        assert!(!g.contains(&EntityId::slug("Saltmarsh Landing")));
        assert!(g.edges_of(&EntityId::slug("Mira of the Fens")).is_empty());
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn resolution_goes_through_the_graph() {
        let mut g = KnowledgeGraph::new();
        let mut landing = entity("Saltmarsh Landing", EntityKind::Location);
        landing.aliases = vec!["The Landing".into(), "Saltmarsh".into()];
        g.insert(landing);
        assert_eq!(
            g.resolve("the landing"),
            Some(&EntityId::slug("Saltmarsh Landing"))
        );
        assert!(g.resolve("The Chancellor").is_none());
    }
}
