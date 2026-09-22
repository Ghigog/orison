//! Glue between [`orison_core::onboarding`] and a CLI/desktop campaign:
//! turning a [`Campaign`]'s player-character JSON into a [`PlayerCard`], and
//! persisting whichever starter gets picked. Shared by `orison new` and
//! `apps/desktop`'s `generate_starters`/`pick_starter` commands, so the two
//! shells cannot drift on what "picking a starter" commits to disk.

use std::sync::{Arc, Mutex};

use orison_core::inference::{InferenceBackend, KeepAlive, SamplingOptions};
use orison_core::knowledge::KnowledgeGraph;
use orison_core::onboarding::{
    generate_starters, Starter, StarterGenerationInput, StarterProgress,
};
use orison_core::prompt::PlayerCard;
use orison_core::state::{Campaign, CampaignStore, HistoryEntry, HistoryRole};
use orison_core::turn::{CancelReason, CancelToken};

use crate::campaign::lock;
use crate::error::CliError;

/// The player-character fields a starter prompt wants, parsed once from
/// `Campaign::player_character`'s free-form JSON so [`PlayerCard`] can borrow
/// from something that outlives the call.
///
/// Mirrors `orison_core::turn::engine`'s private `parse_player_card`: that
/// one is scoped to a live turn, and this pipeline runs before a turn (or a
/// `TurnEngine`) exists at all.
pub struct PlayerProfile {
    name: String,
    physical_description: Option<String>,
    personality: Option<String>,
    backstory: Option<String>,
}

impl PlayerProfile {
    pub fn from_campaign(campaign: &Campaign) -> Self {
        let value: Option<serde_json::Value> = campaign
            .player_character
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok());
        let text = |key: &str| -> Option<String> {
            value
                .as_ref()?
                .get(key)?
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Self {
            name: text("name").unwrap_or_else(|| "The player".to_string()),
            physical_description: text("physical_description"),
            personality: text("personality"),
            backstory: text("backstory"),
        }
    }

    pub fn card(&self) -> PlayerCard<'_> {
        PlayerCard {
            name: &self.name,
            physical_description: self.physical_description.as_deref(),
            personality: self.personality.as_deref(),
            backstory: self.backstory.as_deref(),
        }
    }
}

/// Run the two-pass pipeline for `campaign` and return up to 3 starters.
#[allow(clippy::too_many_arguments)]
pub async fn generate(
    graph: &KnowledgeGraph,
    campaign: &Campaign,
    director: &dyn InferenceBackend,
    actor: &dyn InferenceBackend,
    director_sampling: SamplingOptions,
    actor_sampling: SamplingOptions,
    keep_alive: KeepAlive,
    cancel: &CancelToken,
    on_progress: impl FnMut(StarterProgress),
) -> Result<Vec<Starter>, CancelReason> {
    let profile = PlayerProfile::from_campaign(campaign);
    let input = StarterGenerationInput {
        graph,
        campaign_title: &campaign.title,
        writing_style: &campaign.writing_style,
        player: profile.card(),
        director,
        actor,
        director_sampling,
        actor_sampling,
        keep_alive,
    };
    generate_starters(input, cancel, on_progress).await
}

/// Persist the player's pick: the starter's location and character become
/// active, its narration becomes the campaign's `intro_narration` *and* the
/// first line of the transcript, so it shows at the top of the first `play`
/// without every reader needing to know to check `intro_narration`
/// separately.
pub fn pick(
    store: &Arc<Mutex<CampaignStore>>,
    campaign: &mut Campaign,
    starter: &Starter,
    now: &str,
) -> Result<(), CliError> {
    campaign.active_location = starter.location_id.clone();
    campaign.active_character = starter.character_id.clone();
    campaign.intro_narration = starter.narration.clone();

    let guard = lock(store);
    guard.save_campaign(campaign)?;
    guard.append_history(
        &campaign.id,
        &HistoryEntry {
            role: HistoryRole::Narrator,
            content: starter.narration.clone(),
            timestamp: now.to_string(),
            sender: None,
            active_character: Some(starter.character_id.clone()),
        },
    )?;
    Ok(())
}
