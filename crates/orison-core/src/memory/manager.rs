//! Deciding what to compact, and applying the result.
//!
//! Split deliberately into *plan* and *apply*: the model call sits between
//! them, and everything on either side is synchronous and testable without
//! one. `MemoryManager.gd` interleaves them inside a callback, which is why it
//! needs `_active_compactions` and `_active_distillations` — two dictionaries
//! of booleans guarding against a second call for the same character starting
//! while the first is in flight. Here the plan is a value; a caller that runs
//! two of them concurrently gets two summaries covering disjoint ranges rather
//! than a corrupted transcript.

use crate::state::{CampaignStore, HistoryEntry, SessionSummary, StateError};

use super::policy::MemoryPolicy;

/// A stretch of transcript that should become one summary.
#[derive(Debug, Clone)]
pub struct CompactionPlan {
    pub entity_id: String,
    /// Inclusive `history_logs.id` bounds of what this will summarise.
    pub covers_from: i64,
    pub covers_to: i64,
    /// The lines themselves, oldest first, ready to be formatted.
    pub entries: Vec<HistoryEntry>,
    /// What those lines cost the prompt, by the real tokenizer.
    pub tokens: usize,
}

/// The summaries that should be folded into long-term memory.
#[derive(Debug, Clone)]
pub struct DistillationPlan {
    pub entity_id: String,
    /// The exact summaries included, by id, so a summary written while this
    /// was in flight is not marked as covered by it.
    pub summary_ids: Vec<i64>,
    pub summaries: Vec<String>,
    /// What long-term memory already says, to be integrated rather than
    /// replaced.
    pub existing: String,
}

/// One character's session memory, ready for a prompt.
#[derive(Debug, Clone, Default)]
pub struct SessionMemory {
    pub long_term: String,
    pub summaries: Vec<String>,
}

impl SessionMemory {
    pub fn is_empty(&self) -> bool {
        self.long_term.trim().is_empty() && self.summaries.is_empty()
    }

    /// The prompt block.
    ///
    /// Labelled "in this adventure" on purpose. The model receives the
    /// character's biography in a different block, and the one thing that must
    /// not happen is the two blurring into a single undifferentiated past.
    pub fn block(&self, character_name: &str) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut out = format!("WHAT {character_name} REMEMBERS OF THIS ADVENTURE:\n");
        out.push_str(
            "(Distinct from their biography above, which is who they were before it began.)\n",
        );
        if !self.long_term.trim().is_empty() {
            out.push_str(&format!("- Overall: {}\n", self.long_term.trim()));
        }
        for summary in &self.summaries {
            out.push_str(&format!("- {}\n", summary.trim()));
        }
        Some(out)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryManager {
    policy: MemoryPolicy,
}

impl MemoryManager {
    pub fn new(policy: MemoryPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> MemoryPolicy {
        self.policy
    }

    /// What a character remembers of this playthrough.
    ///
    /// Reads `characters.long_term_memory` and `session_summaries` and
    /// nothing else. It cannot reach a graph entity, which is how the
    /// biography-versus-session-memory separation is enforced rather than
    /// merely intended.
    pub fn session_memory(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<SessionMemory, StateError> {
        let long_term = store
            .character_state(campaign_id, entity_id)?
            .map(|s| s.long_term_memory)
            .unwrap_or_default();
        let summaries = store
            .session_summaries(campaign_id, entity_id, true)?
            .into_iter()
            .map(|s| s.summary)
            .collect();
        Ok(SessionMemory {
            long_term,
            summaries,
        })
    }

    /// Whether this character's live transcript has outgrown its budget, and
    /// what to summarise if so.
    ///
    /// `count_tokens` is the backend's real tokenizer. It is a parameter
    /// rather than an assumption for the same reason `format_context` takes
    /// one: nothing in this crate may fall back to `length / 4` (B-4).
    pub fn plan_compaction(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
        history_budget_tokens: usize,
        count_tokens: impl Fn(&str) -> usize,
    ) -> Result<Option<CompactionPlan>, StateError> {
        let rows = store.character_history(campaign_id, entity_id, 10_000)?;
        if rows.len() <= self.policy.keep_recent_entries + self.policy.min_entries_to_summarise {
            return Ok(None);
        }

        let live_tokens: usize = rows.iter().map(|(_, e)| count_tokens(&e.content)).sum();
        if live_tokens < self.policy.compaction_threshold(history_budget_tokens) {
            return Ok(None);
        }

        let cut = rows.len() - self.policy.keep_recent_entries;
        let older = &rows[..cut];
        let covers_from = older.first().map(|(id, _)| *id).unwrap_or_default();
        let covers_to = older.last().map(|(id, _)| *id).unwrap_or_default();
        Ok(Some(CompactionPlan {
            entity_id: entity_id.to_string(),
            covers_from,
            covers_to,
            tokens: older.iter().map(|(_, e)| count_tokens(&e.content)).sum(),
            entries: older.iter().map(|(_, e)| e.clone()).collect(),
        }))
    }

    /// Record a summary and retire the lines it stands in for.
    ///
    /// The lines are marked, not deleted: `MemoryManager._apply_medium_term_
    /// summary()` rebuilt `history_logs` without them, which loses the
    /// transcript permanently in exchange for a lossy paraphrase.
    pub fn apply_compaction(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        plan: &CompactionPlan,
        summary: &str,
        timestamp: &str,
    ) -> Result<i64, StateError> {
        let id = store.add_session_summary(
            campaign_id,
            &SessionSummary {
                id: 0,
                entity_id: plan.entity_id.clone(),
                summary: summary.trim().to_string(),
                created_at: timestamp.to_string(),
                covers_from: plan.covers_from,
                covers_to: plan.covers_to,
                distilled: false,
            },
        )?;
        store.mark_history_compacted(campaign_id, plan.covers_from, plan.covers_to)?;

        if let Some(mut state) = store.character_state(campaign_id, &plan.entity_id)? {
            state.turns_since_last_summary = 0;
            store.save_character_state(campaign_id, &state)?;
        }
        Ok(id)
    }

    /// Whether enough summaries have piled up to distill.
    pub fn plan_distillation(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        entity_id: &str,
    ) -> Result<Option<DistillationPlan>, StateError> {
        let pending = store.session_summaries(campaign_id, entity_id, true)?;
        if pending.len() < self.policy.distill_after_summaries {
            return Ok(None);
        }
        let existing = store
            .character_state(campaign_id, entity_id)?
            .map(|s| s.long_term_memory)
            .unwrap_or_default();
        Ok(Some(DistillationPlan {
            entity_id: entity_id.to_string(),
            summary_ids: pending.iter().map(|s| s.id).collect(),
            summaries: pending.into_iter().map(|s| s.summary).collect(),
            existing,
        }))
    }

    /// Replace long-term memory and mark the summaries it absorbed.
    pub fn apply_distillation(
        &self,
        store: &CampaignStore,
        campaign_id: &str,
        plan: &DistillationPlan,
        distilled: &str,
    ) -> Result<(), StateError> {
        let distilled = distilled.trim();
        if distilled.is_empty() {
            // An empty distillation would erase everything the character
            // remembers. The Godot version guarded against this by comparing
            // the response to the string "null"; the guard is kept because
            // the failure it prevents is total.
            return Ok(());
        }
        let mut state = store
            .character_state(campaign_id, &plan.entity_id)?
            .unwrap_or_else(|| crate::state::CharacterState::new(&plan.entity_id));
        state.long_term_memory = distilled.to_string();
        store.save_character_state(campaign_id, &state)?;
        store.mark_summaries_distilled(campaign_id, &plan.summary_ids)?;
        Ok(())
    }

    /// The transcript a summarisation prompt reads.
    pub fn format_transcript(&self, entries: &[HistoryEntry], character_name: &str) -> String {
        let mut out = String::new();
        for entry in entries {
            let who = match entry.role {
                crate::state::HistoryRole::Player => "Player",
                crate::state::HistoryRole::Narrator => "Narrator",
                crate::state::HistoryRole::Character => character_name,
                crate::state::HistoryRole::System => "System",
            };
            out.push_str(&format!("{who}: {}\n", entry.content));
        }
        out
    }
}
