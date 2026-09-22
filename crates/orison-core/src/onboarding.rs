//! Adventure-starter generation: the two-pass "start campaign" pipeline
//! that used to live in `OnboardingFlow.gd` + `SystemPrompts.gd`.
//!
//! Not a [`crate::turn::TurnEngine`] method, and deliberately so: no turn is
//! in progress when this runs — there is no active character or location yet
//! for a turn to advance. It is a standalone pipeline over a
//! [`KnowledgeGraph`] and the same two backends `TurnEngine` uses, callable
//! before a `TurnEngine`, or any session, exists at all.
//!
//! The pipeline:
//! 1. [`build_connected_starting_clusters`] — deterministic, no model call:
//!    rank locations by how much they connect to (speaking characters, then
//!    lore) and keep the top 3.
//! 2. Pass 1, on the Director backend: pick one candidate character per
//!    cluster and a hook concept.
//! 3. Pass 2, on the Actor backend, once per selected hook: write its opening
//!    narration.
//! 4. [`generate_fallback_starters`] stands in for a whole failed pass or one
//!    failed hook, per §Appendix C — a model failure degrades the scene
//!    rather than dead-ending onboarding.
//!
//! Ported per `docs/migration_plan.md` Appendix C: the persona rules survive,
//! the `JSON RESPONSE SCHEMA:` prose does not (the response is
//! schema-constrained instead, per §2.4), and every call is a role-separated
//! `ChatMessage` list rather than the Godot build's single concatenated
//! string (B-2).

use crate::inference::{
    ChatMessage, ChatRequest, InferenceBackend, KeepAlive, ResponseFormat, SamplingOptions,
};
use crate::knowledge::{CanonicalField, Entity, EntityId, EntityKind, KnowledgeGraph};
use crate::prompt::schemas::{StarterConcept, StarterNarrationResponse, StartersSelectionResponse};
use crate::prompt::templates::{starter_narration_instructions, starter_selection_instructions};
use crate::prompt::{CharacterCard, PlayerCard, Speech};
use crate::turn::{CancelReason, CancelToken};

/// One character who could plausibly be at a cluster's location.
#[derive(Debug, Clone)]
pub struct CandidateCharacter {
    pub id: EntityId,
    pub label: String,
    profile: String,
}

/// One candidate opening: a location, the speakable characters found there
/// (or, failing that, anywhere in the vault), and any nearby lore —
/// everything Pass 1 needs to invent a hook, before any model call happens.
#[derive(Debug, Clone)]
pub struct StartingCluster {
    pub location_id: EntityId,
    pub location_label: String,
    location_profile: String,
    /// Never empty: a cluster with nobody to populate it is not built.
    pub candidates: Vec<CandidateCharacter>,
    lore_profile: Option<String>,
}

impl StartingCluster {
    /// The character `_build_connected_starting_clusters` would place at
    /// this location absent a Pass 1 answer: the first candidate.
    pub fn primary_character_id(&self) -> &EntityId {
        &self.candidates[0].id
    }
}

/// One finished, pickable opening — what the player chooses between.
#[derive(Debug, Clone, PartialEq)]
pub struct Starter {
    pub title: String,
    pub description: String,
    pub location_id: String,
    pub character_id: String,
    pub narration: String,
}

/// Deterministic, no LLM: the port of `_build_connected_starting_clusters()`.
///
/// Locations are ranked by how much they connect to — speaking characters
/// first, then lore or scene notes — and the top 3 with at least one
/// reachable speaking character become candidate openings. A vault with no
/// such location returns an empty list, which the caller reads as "skip
/// straight to the fallback pipeline".
pub fn build_connected_starting_clusters(graph: &KnowledgeGraph) -> Vec<StartingCluster> {
    let anywhere: Vec<CandidateCharacter> = {
        let mut all: Vec<&Entity> = graph
            .by_kind(EntityKind::Character)
            .filter(|e| speaks(e))
            .collect();
        all.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        all.into_iter().map(candidate_of).collect()
    };

    struct Ranked<'g> {
        location: &'g Entity,
        // Every connected character or lore/scene note, speakable or not —
        // ranking is about how much a location connects to, same as
        // `_build_connected_starting_clusters`'s `characters.size() +
        // lore.size()`. Whether a connected character can actually speak is
        // decided separately, below, for the candidate list itself.
        connection_score: usize,
        speakable: Vec<CandidateCharacter>,
        lore: Option<&'g Entity>,
    }

    let mut ranked: Vec<Ranked> = graph
        .by_kind(EntityKind::Location)
        .map(|location| {
            let neighbours: Vec<&Entity> = graph
                .neighbours(&location.id)
                .iter()
                .filter_map(|id| graph.get(id))
                .collect();
            let characters: Vec<&Entity> = neighbours
                .iter()
                .copied()
                .filter(|e| e.kind == EntityKind::Character)
                .collect();
            let lore_notes: Vec<&Entity> = neighbours
                .iter()
                .copied()
                .filter(|e| matches!(e.kind, EntityKind::Scene | EntityKind::Lore))
                .collect();
            let mut speakable: Vec<CandidateCharacter> = characters
                .iter()
                .filter(|e| speaks(e))
                .map(|e| candidate_of(e))
                .collect();
            speakable.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
            let lore = lore_notes
                .iter()
                .min_by(|a, b| a.id.as_str().cmp(b.id.as_str()))
                .copied();
            Ranked {
                location,
                connection_score: characters.len() + lore_notes.len(),
                speakable,
                lore,
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.connection_score
            .cmp(&a.connection_score)
            .then_with(|| a.location.id.as_str().cmp(b.location.id.as_str()))
    });

    let mut clusters = Vec::new();
    for r in ranked {
        let mut candidates = r.speakable;
        if candidates.is_empty() {
            candidates = anywhere.clone();
        }
        if candidates.is_empty() {
            // Nobody in the whole vault can speak. No cluster is buildable
            // here; the whole-pipeline fallback (empty clusters) handles it.
            continue;
        }
        clusters.push(StartingCluster {
            location_id: r.location.id.clone(),
            location_label: r.location.label.clone(),
            location_profile: compact_profile(r.location),
            candidates,
            lore_profile: r.lore.map(compact_profile),
        });
        if clusters.len() == 3 {
            break;
        }
    }
    clusters
}

fn candidate_of(e: &Entity) -> CandidateCharacter {
    CandidateCharacter {
        id: e.id.clone(),
        label: e.label.clone(),
        profile: compact_profile(e),
    }
}

/// Whether this character can use words. Folds `is_creature` / `can_speak` /
/// `humanoid` the way `speech_of` does for a live turn; duplicated rather
/// than shared because that helper is private to the turn loop and this
/// pipeline runs before one exists.
fn speaks(e: &Entity) -> bool {
    let flag = |key: &str, default: bool| {
        e.properties
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(default)
    };
    Speech::from_flags(
        flag("is_creature", false),
        flag("can_speak", true),
        flag("humanoid", true),
    ) == Speech::Verbal
}

/// `Name — first sentence of its description, biography or body.`
fn compact_profile(e: &Entity) -> String {
    let text = e
        .field(CanonicalField::Description)
        .or_else(|| e.field(CanonicalField::Biography))
        .filter(|s| !s.is_empty())
        .unwrap_or(e.body.as_str());
    let sentence = first_sentence(text);
    if sentence.is_empty() {
        e.label.clone()
    } else {
        format!("{} — {sentence}", e.label)
    }
}

fn first_sentence(text: &str) -> String {
    let text = text.trim();
    let end = text
        .char_indices()
        .find(|(_, c)| matches!(c, '.' | '!' | '?'))
        .map(|(i, c)| i + c.len_utf8());
    match end {
        Some(end) => text[..end].trim().to_string(),
        None => text.chars().take(240).collect(),
    }
}

/// Deterministic starters, no model call: the port of
/// `_generate_fallback_starters()`. Always exactly 3, one per flavour, with a
/// generic stand-in for any index `clusters` does not reach.
///
/// Used both when there is nothing to send to a model (no locations at all)
/// and per-hook, when Pass 2 fails for that one hook without taking down the
/// other two.
pub fn generate_fallback_starters(clusters: &[StartingCluster]) -> Vec<Starter> {
    const FLAVORS: [(&str, &str); 3] = [
        (
            "A Whisper in the Shadows",
            "A hushed rumour has drawn {character} out to {location}, and something in the \
             dark does not want it followed.",
        ),
        (
            "The Burning Sigil",
            "A mark has appeared over {location} overnight, and {character} seems to be the \
             only one who understands what it means.",
        ),
        (
            "The Shattered Mirror",
            "Something at {location} has broken that should not have been able to, and \
             {character} is standing over the pieces, looking for someone to explain it to.",
        ),
    ];

    FLAVORS
        .iter()
        .enumerate()
        .map(|(index, (title, narration))| {
            let cluster = clusters.get(index);
            let location_label = cluster.map_or("an unknown place", |c| c.location_label.as_str());
            let character_label = cluster.map_or("a stranger", |c| c.candidates[0].label.as_str());
            Starter {
                title: (*title).to_string(),
                description: format!(
                    "A deterministic opening at {location_label}, involving {character_label}."
                ),
                location_id: cluster.map_or_else(String::new, |c| c.location_id.to_string()),
                character_id: cluster
                    .map_or_else(String::new, |c| c.primary_character_id().to_string()),
                narration: narration
                    .replace("{location}", location_label)
                    .replace("{character}", character_label),
            }
        })
        .collect()
}

fn selection_body(
    campaign_title: &str,
    player: &PlayerCard<'_>,
    clusters: &[StartingCluster],
) -> String {
    let mut out = player.render();
    out.push_str(&format!(
        "\nCAMPAIGN SETTING:\n- Title: {campaign_title}\n\n"
    ));
    out.push_str("STARTING CLUSTERS:\n");
    for (index, cluster) in clusters.iter().enumerate() {
        out.push_str(&format!("--- CLUSTER {} ---\n", index + 1));
        out.push_str(&format!(
            "- Location: ID: {} | Info: {}\n",
            cluster.location_id, cluster.location_profile
        ));
        out.push_str("- Candidate characters (pick exactly one):\n");
        for candidate in &cluster.candidates {
            out.push_str(&format!(
                "  * ID: {} | Info: {}\n",
                candidate.id, candidate.profile
            ));
        }
        if let Some(lore) = &cluster.lore_profile {
            out.push_str(&format!("- Associated lore/scene: {lore}\n"));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "Return exactly {} starters, one per cluster, in the same order the clusters were given.\n",
        clusters.len()
    ));
    out
}

#[allow(clippy::too_many_arguments)]
fn narration_body(
    campaign_title: &str,
    writing_style: &str,
    player: &PlayerCard<'_>,
    concept: &StarterConcept,
    location: &Entity,
    character: &Entity,
) -> String {
    let mut out = player.render();
    out.push_str(&format!(
        "\nCAMPAIGN SETTING:\n- Title: {campaign_title}\n\n"
    ));
    out.push_str(&format!(
        "ADVENTURE STARTER HOOK CONCEPT:\n- Title: {}\n- Concept: {}\n\n",
        concept.title, concept.concept
    ));
    out.push_str(&format!(
        "FEATURED LOCATION:\n- ID: {}\n- Name: {}\n- Description:\n{}\n\n",
        location.id,
        location.label,
        location
            .field(CanonicalField::Description)
            .unwrap_or(location.body.as_str())
    ));
    let card = CharacterCard {
        name: &character.label,
        gender: character
            .properties
            .get("gender")
            .and_then(serde_json::Value::as_str),
        biography: character.field(CanonicalField::Biography),
        personality: character.field(CanonicalField::Personality),
        appearance: character.field(CanonicalField::Appearance),
        goals: character.field(CanonicalField::Goals),
        writing_style: None,
    };
    out.push_str(&card.render());
    if !writing_style.trim().is_empty() {
        out.push_str(&format!(
            "\nCAMPAIGN WRITING STYLE REFERENCE:\nMatch the tone, pacing and voice of this \
             reference:\n\"\"\"\n{writing_style}\n\"\"\"\n"
        ));
    }
    out
}

/// Everything [`generate_starters`] needs, gathered in one place rather than
/// as a long argument list.
pub struct StarterGenerationInput<'a> {
    pub graph: &'a KnowledgeGraph,
    pub campaign_title: &'a str,
    pub writing_style: &'a str,
    pub player: PlayerCard<'a>,
    pub director: &'a dyn InferenceBackend,
    pub actor: &'a dyn InferenceBackend,
    pub director_sampling: SamplingOptions,
    pub actor_sampling: SamplingOptions,
    pub keep_alive: KeepAlive,
}

/// Where the pipeline is, for a caller that wants to show progress (the
/// issue's "Writing hook 2/3").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarterProgress {
    BuildingClusters,
    SelectingHooks,
    WritingNarration { index: usize, total: usize },
}

/// Run the full two-pass pipeline and return up to 3 starters, always at
/// least the deterministic fallback set.
///
/// Never fails on a model error: Pass 1 failing (a bad connection, a
/// malformed response) falls back to [`generate_fallback_starters`] for
/// every hook; Pass 2 failing falls back for that one hook only, exactly as
/// `_generate_adventure_hooks` / `_handle_narration_failure` did. The only
/// error this returns is cancellation, so a caller who cancelled mid-pass
/// can tell that apart from "the model failed and we fell back".
pub async fn generate_starters(
    input: StarterGenerationInput<'_>,
    cancel: &CancelToken,
    mut on_progress: impl FnMut(StarterProgress),
) -> Result<Vec<Starter>, CancelReason> {
    on_progress(StarterProgress::BuildingClusters);
    let clusters = build_connected_starting_clusters(input.graph);
    let fallback = generate_fallback_starters(&clusters);
    if clusters.is_empty() {
        return Ok(fallback);
    }

    on_progress(StarterProgress::SelectingHooks);
    let selection_request = ChatRequest {
        sampling: input.director_sampling.clone(),
        keep_alive: input.keep_alive,
        ..ChatRequest::new(vec![
            ChatMessage::system(starter_selection_instructions()),
            ChatMessage::user(selection_body(
                input.campaign_title,
                &input.player,
                &clusters,
            )),
        ])
        .with_response_format(ResponseFormat::for_type::<StartersSelectionResponse>())
    };
    let selection: Option<StartersSelectionResponse> = tokio::select! {
        biased;
        reason = cancel.cancelled() => return Err(reason),
        result = input.director.chat(selection_request) => {
            result.ok().and_then(|r| r.parse().ok())
        }
    };
    let Some(selection) = selection.filter(|s| !s.starters.is_empty()) else {
        return Ok(fallback);
    };

    let total = selection.starters.len().min(clusters.len()).min(3);
    let mut starters = Vec::with_capacity(fallback.len());
    for (index, concept) in selection.starters.into_iter().take(total).enumerate() {
        on_progress(StarterProgress::WritingNarration { index, total });
        let cluster = &clusters[index];

        let location_id = EntityId::slug(&concept.location_id);
        let location_id = if input.graph.contains(&location_id) {
            location_id
        } else {
            cluster.location_id.clone()
        };
        let character_id = EntityId::slug(&concept.character_id);
        let character_id = if cluster.candidates.iter().any(|c| c.id == character_id) {
            character_id
        } else {
            cluster.primary_character_id().clone()
        };

        let (Some(location), Some(character)) = (
            input.graph.get(&location_id),
            input.graph.get(&character_id),
        ) else {
            starters.push(fallback[index].clone());
            continue;
        };

        let narration_request = ChatRequest {
            sampling: input.actor_sampling.clone(),
            keep_alive: input.keep_alive,
            ..ChatRequest::new(vec![
                ChatMessage::system(starter_narration_instructions()),
                ChatMessage::user(narration_body(
                    input.campaign_title,
                    input.writing_style,
                    &input.player,
                    &concept,
                    location,
                    character,
                )),
            ])
            .with_response_format(ResponseFormat::for_type::<StarterNarrationResponse>())
        };
        let narrated: Option<StarterNarrationResponse> = tokio::select! {
            biased;
            reason = cancel.cancelled() => return Err(reason),
            result = input.actor.chat(narration_request) => {
                result.ok().and_then(|r| r.parse().ok())
            }
        };
        starters.push(match narrated {
            Some(n) => Starter {
                title: n.title,
                description: n.description,
                location_id: location_id.to_string(),
                character_id: character_id.to_string(),
                narration: n.narration,
            },
            None => fallback[index].clone(),
        });
    }
    while starters.len() < fallback.len() {
        let index = starters.len();
        starters.push(fallback[index].clone());
    }
    Ok(starters)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{Edge, EdgeKind};

    fn entity(name: &str, kind: EntityKind) -> Entity {
        let mut e = Entity::new(EntityId::slug(name), name, kind);
        e.body = format!("{name} is a fine and ordinary place or person.");
        e
    }

    fn connect_vault() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        g.insert(entity("Stonebridge", EntityKind::Location));
        g.insert(entity("Bram Holt", EntityKind::Character));
        g.insert(entity("Thornwick Archive", EntityKind::Location));
        g.insert(entity("Elara Voss", EntityKind::Character));
        let mut kettle = entity("The Kettle", EntityKind::Character);
        kettle.properties.insert("is_creature".into(), true.into());
        kettle.properties.insert("can_speak".into(), false.into());
        g.insert(kettle);
        g.connect(Edge {
            from: EntityId::slug("Bram Holt"),
            to: EntityId::slug("Stonebridge"),
            kind: EdgeKind::AssociatedWith,
            weight: 1.0,
        });
        g.connect(Edge {
            from: EntityId::slug("Elara Voss"),
            to: EntityId::slug("Thornwick Archive"),
            kind: EdgeKind::AssociatedWith,
            weight: 1.0,
        });
        g.connect(Edge {
            from: EntityId::slug("The Kettle"),
            to: EntityId::slug("Thornwick Archive"),
            kind: EdgeKind::AssociatedWith,
            weight: 1.0,
        });
        g
    }

    #[test]
    fn clusters_rank_by_connection_count_and_exclude_non_speakers() {
        let g = connect_vault();
        let clusters = build_connected_starting_clusters(&g);
        assert_eq!(clusters.len(), 2);
        // Thornwick has two connections (Elara + the mute Kettle); Stonebridge
        // has one. Only Elara is a speakable candidate for Thornwick.
        assert_eq!(clusters[0].location_id, EntityId::slug("Thornwick Archive"));
        assert_eq!(clusters[0].candidates.len(), 1);
        assert_eq!(clusters[0].candidates[0].id, EntityId::slug("Elara Voss"));
        assert_eq!(clusters[1].location_id, EntityId::slug("Stonebridge"));
    }

    #[test]
    fn a_location_with_no_speaking_neighbour_falls_back_to_the_whole_vault() {
        let mut g = KnowledgeGraph::new();
        g.insert(entity("Empty Hall", EntityKind::Location));
        g.insert(entity("Wandering Bard", EntityKind::Character));
        // No edge at all between them.
        let clusters = build_connected_starting_clusters(&g);
        assert_eq!(clusters.len(), 1);
        assert_eq!(
            clusters[0].candidates[0].id,
            EntityId::slug("Wandering Bard")
        );
    }

    #[test]
    fn no_locations_at_all_yields_no_clusters() {
        let mut g = KnowledgeGraph::new();
        g.insert(entity("Someone", EntityKind::Character));
        assert!(build_connected_starting_clusters(&g).is_empty());
    }

    #[test]
    fn no_speaking_character_anywhere_yields_no_clusters() {
        let mut g = KnowledgeGraph::new();
        g.insert(entity("A Place", EntityKind::Location));
        let mut mute = entity("A Beast", EntityKind::Character);
        mute.properties.insert("can_speak".into(), false.into());
        g.insert(mute);
        assert!(build_connected_starting_clusters(&g).is_empty());
    }

    #[test]
    fn fallback_starters_are_always_exactly_three_and_deterministic() {
        let g = connect_vault();
        let clusters = build_connected_starting_clusters(&g);
        let a = generate_fallback_starters(&clusters);
        let b = generate_fallback_starters(&clusters);
        assert_eq!(a.len(), 3);
        assert_eq!(a, b);
        // The third flavour has no cluster behind it (only 2 exist) and gets
        // the generic stand-in rather than panicking on an out-of-bounds index.
        assert_eq!(a[2].location_id, "");
        assert!(a[2].narration.contains("an unknown place"));
    }

    #[test]
    fn fallback_starters_on_no_clusters_still_returns_three() {
        assert_eq!(generate_fallback_starters(&[]).len(), 3);
    }
}
