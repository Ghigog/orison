//! RAPTOR hierarchical summaries (§3.5).
//!
//! Levels 0/1/2 as designed in `rag_architecture.md` §1.3 and §3.2, ported
//! rather than redesigned: level 0 is the vault's own notes, level 1 groups
//! them into themes, level 2 groups the themes into arcs, and retrieval targets
//! a level depending on how wide a question is. Summary nodes are graph nodes
//! with a `level`, not a separate store.
//!
//! Two replacements the handoff calls for, and what each one fixes:
//!
//! **`linfa-clustering` replaces the hand-rolled k-means**
//! (`VaultCompiler._kmeans_cluster`, 76 lines). That implementation ran a fixed
//! ten iterations with no convergence test, seeded centroids by taking every
//! `n/k`-th vector in dictionary order, and re-seeded an emptied cluster from an
//! arbitrary point each round.
//!
//! **A real fallback replaces round-robin partitioning.** The Godot version fell
//! back to `clusters[i % k].append(node)` whenever embeddings were unavailable
//! or scarce — dealing nodes into buckets by iteration order and then asking a
//! model to name the theme connecting them, which produced a confident summary
//! of an arbitrary group. Here, a node with no embedding is
//! [`RaptorError::MissingEmbedding`], and too few nodes for the requested
//! cluster count reduces the count rather than inventing structure. Phase 2
//! made embeddings available in-process, so "we could not embed" is now a real
//! failure rather than a routine condition to paper over.

use std::collections::BTreeMap;

use linfa::prelude::Predict;
use linfa::traits::Fit;
use linfa::DatasetBase;
use linfa_clustering::KMeans;
use ndarray::Array2;

use super::graph::KnowledgeGraph;
use super::types::{Edge, EdgeKind, Entity, EntityId, EntityKind};

#[derive(Debug, thiserror::Error)]
pub enum RaptorError {
    /// A candidate node has no embedding.
    ///
    /// Deliberately an error. The Godot build treated this as normal and
    /// silently switched to round-robin, so a vault compiled without
    /// `nomic-embed-text` installed still produced a full, plausible-looking
    /// summary hierarchy over groups that meant nothing.
    #[error("no embedding for '{0}'; clustering it would be guesswork")]
    MissingEmbedding(EntityId),

    #[error("embedding for '{id}' has {got} dimensions, expected {expected}")]
    InconsistentDimensions {
        id: EntityId,
        expected: usize,
        got: usize,
    },

    #[error("clustering failed: {0}")]
    Clustering(String),
}

/// The shape of the hierarchy.
///
/// The divisors are ported from the Godot build unchanged: `max(3, ceil(n/5))`
/// level-1 clusters and `max(1, ceil(l1/5))` level-2 clusters. They were never
/// validated against anything larger than the `large` fixture, which the
/// migration plan records as an open question for a later phase rather than
/// something to change here on a guess.
#[derive(Debug, Clone, Copy)]
pub struct RaptorConfig {
    pub l1_min_clusters: usize,
    pub l2_min_clusters: usize,
    pub cluster_divisor: usize,
    pub max_iterations: u64,
    pub tolerance: f64,
}

impl Default for RaptorConfig {
    fn default() -> Self {
        Self {
            l1_min_clusters: 3,
            l2_min_clusters: 1,
            cluster_divisor: 5,
            max_iterations: 100,
            tolerance: 1e-4,
        }
    }
}

/// `max(min, ceil(n / divisor))`, capped at `n`.
///
/// The cap is the whole of the fallback. k-means cannot produce more clusters
/// than it has points, and the Godot build's answer — round-robin into `k`
/// buckets, some of them empty — produced summary nodes describing nothing.
/// Asking for fewer clusters is an honest answer to having fewer notes.
pub fn cluster_count(n: usize, min: usize, divisor: usize) -> usize {
    if n == 0 {
        return 0;
    }
    min.max(n.div_ceil(divisor.max(1))).min(n)
}

/// A cluster's generated title and summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub title: String,
    pub text: String,
}

/// Turns a group of entities into a theme.
///
/// Synchronous on purpose. A real implementation calls a model, which is async,
/// and a caller with a runtime can bridge that in a few lines; making the trait
/// async instead would make the whole hierarchy untestable without a live
/// endpoint, which is how the Godot build ended up unable to compile a vault at
/// all without one.
pub trait Summariser {
    fn summarise(&self, level: u8, index: usize, members: &[&Entity]) -> Summary;
}

/// The summariser used when no model is available.
///
/// This is the Godot build's *fallback* text — "Narrative connection between
/// X, Y, Z." — and it is deliberately dull. A cluster named by listing its
/// members is obviously a placeholder; a cluster named by a model that was
/// handed an arbitrary round-robin group is not, and that was the problem.
#[derive(Debug, Default, Clone, Copy)]
pub struct MemberListSummariser;

impl Summariser for MemberListSummariser {
    fn summarise(&self, level: u8, index: usize, members: &[&Entity]) -> Summary {
        let labels: Vec<&str> = members.iter().map(|e| e.label.as_str()).collect();
        match level {
            1 => Summary {
                title: format!("Narrative Theme {}", index + 1),
                text: format!("Narrative connection between {}.", labels.join(", ")),
            },
            _ => Summary {
                title: format!("Campaign Arc {}", index + 1),
                text: format!("Overarching arc connecting: {}.", labels.join(", ")),
            },
        }
    }
}

/// What the build produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RaptorReport {
    pub candidates: usize,
    pub l1_clusters: usize,
    pub l2_clusters: usize,
    /// Level-1 summary ids, in order.
    pub l1_ids: Vec<EntityId>,
    pub l2_ids: Vec<EntityId>,
}

/// The entity kinds that get summarised.
///
/// Characters, locations and scenes, as in `_generate_raptor_summaries()`. Lore
/// and untyped notes are retrievable at level 0 and are not what the Director's
/// campaign-level view is made of.
pub const CANDIDATE_KINDS: [EntityKind; 3] = [
    EntityKind::Character,
    EntityKind::Location,
    EntityKind::Scene,
];

/// Build levels 1 and 2 over the graph's level-0 entities.
///
/// Any summary nodes already present are removed first, so rebuilding is
/// idempotent rather than additive.
pub fn build(
    graph: &mut KnowledgeGraph,
    embeddings: &BTreeMap<EntityId, Vec<f32>>,
    summariser: &dyn Summariser,
    config: &RaptorConfig,
) -> Result<RaptorReport, RaptorError> {
    let existing: Vec<EntityId> = graph
        .entities()
        .filter(|e| e.kind == EntityKind::Summary)
        .map(|e| e.id.clone())
        .collect();
    for id in existing {
        graph.remove(&id);
    }

    let candidates: Vec<EntityId> = graph
        .entities()
        .filter(|e| e.level == 0 && CANDIDATE_KINDS.contains(&e.kind))
        .map(|e| e.id.clone())
        .collect();

    let mut report = RaptorReport {
        candidates: candidates.len(),
        ..Default::default()
    };
    if candidates.is_empty() {
        return Ok(report);
    }

    let vectors = gather(&candidates, embeddings)?;
    let k1 = cluster_count(
        candidates.len(),
        config.l1_min_clusters,
        config.cluster_divisor,
    );
    let l1_groups = cluster(&vectors, k1, config)?;

    let mut l1_centroids: Vec<Vec<f32>> = Vec::with_capacity(l1_groups.len());
    for (index, group) in l1_groups.iter().enumerate() {
        let members: Vec<&Entity> = group
            .iter()
            .filter_map(|i| graph.get(&candidates[*i]))
            .collect();
        let summary = summariser.summarise(1, index, &members);

        let id = EntityId::from_stored(format!("summary_l1_{index}"));
        let mut entity = Entity::new(id.clone(), summary.title, EntityKind::Summary);
        entity.level = 1;
        entity.description = summary.text.clone();
        entity.body = summary.text;
        graph.insert(entity);

        for i in group {
            graph.connect(Edge {
                from: id.clone(),
                to: candidates[*i].clone(),
                kind: EdgeKind::Summarises,
                weight: 1.0,
            });
        }

        // A level-1 node's position is the mean of what it summarises.
        //
        // The Godot build embedded each generated summary *string* and
        // clustered those, so level 2 grouped the model's phrasing rather than
        // the notes underneath. The centroid is what the cluster actually is,
        // and it needs no second round-trip to a model.
        l1_centroids.push(centroid(&vectors, group));
        report.l1_ids.push(id);
    }
    report.l1_clusters = report.l1_ids.len();

    if !l1_centroids.is_empty() {
        let k2 = cluster_count(
            l1_centroids.len(),
            config.l2_min_clusters,
            config.cluster_divisor,
        );
        let l2_groups = cluster(&l1_centroids, k2, config)?;
        for (index, group) in l2_groups.iter().enumerate() {
            let members: Vec<&Entity> = group
                .iter()
                .filter_map(|i| graph.get(&report.l1_ids[*i]))
                .collect();
            let summary = summariser.summarise(2, index, &members);

            let id = EntityId::from_stored(format!("summary_l2_{index}"));
            let mut entity = Entity::new(id.clone(), summary.title, EntityKind::Summary);
            entity.level = 2;
            entity.description = summary.text.clone();
            entity.body = summary.text;
            graph.insert(entity);

            for i in group {
                graph.connect(Edge {
                    from: id.clone(),
                    to: report.l1_ids[*i].clone(),
                    kind: EdgeKind::Summarises,
                    weight: 1.0,
                });
            }
            report.l2_ids.push(id);
        }
        report.l2_clusters = report.l2_ids.len();
    }

    Ok(report)
}

fn gather(
    ids: &[EntityId],
    embeddings: &BTreeMap<EntityId, Vec<f32>>,
) -> Result<Vec<Vec<f32>>, RaptorError> {
    let mut out = Vec::with_capacity(ids.len());
    let mut dim = None;
    for id in ids {
        let vector = embeddings
            .get(id)
            .ok_or_else(|| RaptorError::MissingEmbedding(id.clone()))?;
        let expected = *dim.get_or_insert(vector.len());
        if vector.len() != expected {
            return Err(RaptorError::InconsistentDimensions {
                id: id.clone(),
                expected,
                got: vector.len(),
            });
        }
        out.push(vector.clone());
    }
    Ok(out)
}

/// k-means over the vectors, returning the member indices of each cluster.
///
/// Empty clusters are dropped rather than kept as summary nodes describing
/// nothing.
fn cluster(
    vectors: &[Vec<f32>],
    k: usize,
    config: &RaptorConfig,
) -> Result<Vec<Vec<usize>>, RaptorError> {
    if k <= 1 || vectors.len() <= 1 {
        return Ok(vec![(0..vectors.len()).collect()]);
    }

    let rows = vectors.len();
    let cols = vectors[0].len();
    let flat: Vec<f64> = vectors
        .iter()
        .flat_map(|v| v.iter().map(|f| *f as f64))
        .collect();
    let records = Array2::from_shape_vec((rows, cols), flat)
        .map_err(|e| RaptorError::Clustering(e.to_string()))?;

    let dataset = DatasetBase::from(records.clone());
    let model = KMeans::params(k)
        .max_n_iterations(config.max_iterations)
        .tolerance(config.tolerance)
        .fit(&dataset)
        .map_err(|e| RaptorError::Clustering(e.to_string()))?;
    let assignments = model.predict(&records);

    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); k];
    for (i, cluster) in assignments.iter().enumerate() {
        if let Some(group) = groups.get_mut(*cluster) {
            group.push(i);
        }
    }
    groups.retain(|g| !g.is_empty());
    Ok(groups)
}

fn centroid(vectors: &[Vec<f32>], group: &[usize]) -> Vec<f32> {
    let dim = vectors.first().map(Vec::len).unwrap_or(0);
    let mut sum = vec![0.0f32; dim];
    for i in group {
        for (d, value) in vectors[*i].iter().enumerate() {
            sum[d] += value;
        }
    }
    let n = group.len().max(1) as f32;
    for value in &mut sum {
        *value /= n;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(name: &str, kind: EntityKind) -> Entity {
        let mut e = Entity::new(EntityId::slug(name), name, kind);
        e.body = format!("Body of {name}.");
        e
    }

    /// Two clearly separated groups in two dimensions, so the clustering can be
    /// checked against an answer that is known rather than plausible.
    fn separated_graph(per_group: usize) -> (KnowledgeGraph, BTreeMap<EntityId, Vec<f32>>) {
        let mut graph = KnowledgeGraph::new();
        let mut embeddings = BTreeMap::new();
        for group in 0..2 {
            for i in 0..per_group {
                let name = format!("g{group}_n{i}");
                let id = EntityId::slug(&name);
                graph.insert(entity(&name, EntityKind::Character));
                let offset = i as f32 * 0.01;
                embeddings.insert(
                    id,
                    vec![group as f32 * 10.0 + offset, group as f32 * -10.0 + offset],
                );
            }
        }
        (graph, embeddings)
    }

    #[test]
    fn cluster_counts_match_the_godot_shape() {
        // max(3, ceil(n/5)) for L1.
        assert_eq!(cluster_count(6, 3, 5), 3);
        assert_eq!(cluster_count(20, 3, 5), 4);
        assert_eq!(cluster_count(200, 3, 5), 40);
        // max(1, ceil(l1/5)) for L2.
        assert_eq!(cluster_count(3, 1, 5), 1);
        assert_eq!(cluster_count(40, 1, 5), 8);
    }

    #[test]
    fn asking_for_more_clusters_than_notes_reduces_the_count() {
        // The replacement for round-robin. Two notes cannot make three themes.
        assert_eq!(cluster_count(2, 3, 5), 2);
        assert_eq!(cluster_count(1, 3, 5), 1);
        assert_eq!(cluster_count(0, 3, 5), 0);
    }

    #[test]
    fn well_separated_notes_cluster_by_their_separation() {
        let (mut graph, embeddings) = separated_graph(5);
        let report = build(
            &mut graph,
            &embeddings,
            &MemberListSummariser,
            &RaptorConfig {
                l1_min_clusters: 2,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.candidates, 10);
        assert_eq!(report.l1_clusters, 2);

        // Each summary should cover one group and not straddle both.
        for id in &report.l1_ids {
            // Outgoing only: `edges_of` is undirected, and the level-2 node
            // above this one points back down at it with the same relation.
            let members: Vec<String> = graph
                .edges_of(id)
                .iter()
                .filter(|e| e.kind == EdgeKind::Summarises && &e.from == id)
                .map(|e| e.to.to_string())
                .collect();
            assert_eq!(members.len(), 5);
            let first = members[0].chars().nth(1).unwrap();
            assert!(
                members.iter().all(|m| m.chars().nth(1) == Some(first)),
                "a cluster straddled both groups: {members:?}"
            );
        }
    }

    #[test]
    fn a_missing_embedding_is_an_error_not_a_round_robin() {
        let (mut graph, mut embeddings) = separated_graph(3);
        let dropped = EntityId::slug("g0_n0");
        embeddings.remove(&dropped);

        let err = build(
            &mut graph,
            &embeddings,
            &MemberListSummariser,
            &RaptorConfig::default(),
        )
        .unwrap_err();
        assert!(matches!(err, RaptorError::MissingEmbedding(id) if id == dropped));
    }

    #[test]
    fn summaries_are_graph_nodes_with_a_level() {
        let (mut graph, embeddings) = separated_graph(5);
        let report = build(
            &mut graph,
            &embeddings,
            &MemberListSummariser,
            &RaptorConfig::default(),
        )
        .unwrap();

        for id in &report.l1_ids {
            let e = graph.get(id).unwrap();
            assert_eq!(e.kind, EntityKind::Summary);
            assert_eq!(e.level, 1);
        }
        for id in &report.l2_ids {
            assert_eq!(graph.get(id).unwrap().level, 2);
        }
        assert_eq!(graph.by_level(1).count(), report.l1_clusters);
        assert_eq!(graph.by_level(2).count(), report.l2_clusters);
    }

    #[test]
    fn rebuilding_replaces_the_hierarchy_rather_than_stacking_another() {
        let (mut graph, embeddings) = separated_graph(5);
        let first = build(
            &mut graph,
            &embeddings,
            &MemberListSummariser,
            &RaptorConfig::default(),
        )
        .unwrap();
        let second = build(
            &mut graph,
            &embeddings,
            &MemberListSummariser,
            &RaptorConfig::default(),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(
            graph.by_kind(EntityKind::Summary).count(),
            second.l1_clusters + second.l2_clusters
        );
    }

    #[test]
    fn lore_and_untyped_notes_are_not_candidates() {
        let (mut graph, mut embeddings) = separated_graph(3);
        for (name, kind) in [("A Tithe", EntityKind::Lore), ("scratch", EntityKind::Note)] {
            let id = EntityId::slug(name);
            graph.insert(entity(name, kind));
            embeddings.insert(id, vec![0.0, 0.0]);
        }
        let report = build(
            &mut graph,
            &embeddings,
            &MemberListSummariser,
            &RaptorConfig::default(),
        )
        .unwrap();
        assert_eq!(
            report.candidates, 6,
            "only characters, locations and scenes"
        );
    }
}
