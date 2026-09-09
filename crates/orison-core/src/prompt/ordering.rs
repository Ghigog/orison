//! Cache-stable prompt ordering (§2.7).
//!
//! This is where the ~20s p50 in `eval_baseline.md` comes from. Local
//! inference engines (llama.cpp under Ollama, and `LlamaCppBackend`
//! directly) reuse the KV cache for tokens shared with the previous request,
//! but only for a shared *prefix*. The Godot build rebuilds one monolithic
//! prompt string from scratch every turn, so the prefix — including content
//! that has not actually changed, like the character card — is invalidated
//! on every single call and thousands of already-seen tokens are
//! re-processed.
//!
//! The fix is ordering: put content that is stable across a scene first,
//! and content that changes every turn last. [`PromptSections::into_messages`]
//! assembles messages in exactly this order — static system instructions,
//! character card, retrieved lore, session summaries, recent turns, the
//! character's volatile state, then the player's input — so a backend serving
//! consecutive turns in the same scene sees an unchanged prefix up to the
//! newest turn.
//!
//! **`volatile_state` is separate from `character_card` for a measured
//! reason.** The card originally carried the emotional profile too, and
//! `tests/turn_loop.rs` caught what that costs: emotion moves on almost every
//! turn, so a card that contains it changes the *first* message of every
//! request and invalidates the entire cache from token zero — the exact
//! defect this module exists to prevent, in the block that was supposed to be
//! the stable one. Identity is stable across a scene and stays at the front;
//! how the character feels right now is as volatile as the player's input and
//! sits next to it.

use crate::inference::ChatMessage;

/// The six blocks assembled into one chat request, ordered stable-to-volatile.
/// `recent_turns` must already be in chronological order (oldest first): each
/// message that was present in the previous turn's request must appear
/// byte-for-byte identical here too, or the shared prefix breaks anyway.
#[derive(Debug, Clone, Default)]
pub struct PromptSections {
    /// Static instructions: the character/DM persona and core rules. Never
    /// changes within a turn budget's lifetime.
    pub system_instructions: String,
    /// The active character's identity: name, pronouns, biography,
    /// personality, appearance, goals. What the vault says, which does not
    /// change while the player is talking to them.
    pub character_card: Option<String>,
    /// Retrieved lore for this turn's query. More volatile than the card,
    /// but still ordered ahead of the growing history so a repeated query
    /// keeps its prefix stable.
    pub retrieved_lore: Option<String>,
    /// Session/medium-term summaries. Stable across a scene, refreshed only
    /// when the summarizer runs.
    pub session_summaries: Option<String>,
    /// Prior turns in the conversation, oldest first. The growing but
    /// stable-prefixed part: each new turn appends one entry rather than
    /// rewriting the ones before it.
    pub recent_turns: Vec<ChatMessage>,
    /// State that changes turn to turn: the emotional profile and rapport
    /// band. Ordered here, second-to-last, because it is nearly as volatile
    /// as the player's input — and because recency is where a model attends
    /// to "how do you feel right now" anyway.
    pub volatile_state: Option<String>,
    /// The player's input for *this* turn. Always last: the only content
    /// that is new on every single call.
    pub player_input: String,
}

impl PromptSections {
    /// Assemble into the exact message order a backend should receive.
    /// Static content first, volatile content last.
    pub fn into_messages(self) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        let mut system = self.system_instructions;
        if let Some(card) = &self.character_card {
            system.push('\n');
            system.push_str(card);
        }
        if !system.is_empty() {
            messages.push(ChatMessage::system(system));
        }

        if let Some(lore) = self.retrieved_lore {
            messages.push(ChatMessage::system(lore));
        }
        if let Some(summaries) = self.session_summaries {
            messages.push(ChatMessage::system(summaries));
        }

        messages.extend(self.recent_turns);
        if let Some(state) = self.volatile_state {
            messages.push(ChatMessage::system(state));
        }
        messages.push(ChatMessage::user(self.player_input));
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::Role;

    #[test]
    fn volatile_content_is_always_last() {
        let sections = PromptSections {
            system_instructions: "You are the DM.".to_string(),
            character_card: Some("Name: Elowen".to_string()),
            retrieved_lore: Some("The tavern is called The Guttering Lamp.".to_string()),
            session_summaries: Some("The party arrived in town.".to_string()),
            recent_turns: vec![
                ChatMessage::user("I look around."),
                ChatMessage::assistant("The tavern is dim and smoky."),
            ],
            volatile_state: Some("Active feeling: serenity.".to_string()),
            player_input: "I approach the bar.".to_string(),
        };

        let messages = sections.into_messages();
        let last = messages.last().expect("at least one message");
        assert_eq!(last.role, Role::User);
        assert_eq!(last.content.as_deref(), Some("I approach the bar."));
    }

    /// The regression `tests/turn_loop.rs` found: state that moves every turn
    /// must not sit in the block the cache prefix depends on.
    #[test]
    fn volatile_state_does_not_touch_the_stable_prefix() {
        let make = |feeling: &str| PromptSections {
            system_instructions: "You are the DM.".to_string(),
            character_card: Some("Name: Elowen".to_string()),
            retrieved_lore: Some("The tavern is called The Guttering Lamp.".to_string()),
            session_summaries: None,
            recent_turns: vec![ChatMessage::user("I look around.")],
            volatile_state: Some(format!("Active feeling: {feeling}.")),
            player_input: "I approach the bar.".to_string(),
        };

        let calm = make("serenity").into_messages();
        let angry = make("anger").into_messages();

        assert_eq!(calm.len(), angry.len());
        let volatile_at = calm.len() - 2;
        for i in 0..volatile_at {
            assert_eq!(
                calm[i].content, angry[i].content,
                "message {i} must not depend on how the character feels"
            );
        }
        assert_ne!(calm[volatile_at].content, angry[volatile_at].content);
    }

    #[test]
    fn unchanged_sections_produce_a_byte_identical_prefix_across_turns() {
        let make = |player_input: &str, recent_turns: Vec<ChatMessage>| PromptSections {
            system_instructions: "You are the DM.".to_string(),
            character_card: Some("Name: Elowen".to_string()),
            retrieved_lore: Some("The tavern is called The Guttering Lamp.".to_string()),
            session_summaries: Some("The party arrived in town.".to_string()),
            recent_turns,
            volatile_state: Some("Active feeling: serenity.".to_string()),
            player_input: player_input.to_string(),
        };

        // Turn N: history is empty, player says something.
        let turn_n = make("I look around.", vec![]);
        let turn_n_messages = turn_n.into_messages();

        // Turn N+1: the same static sections, one more history entry
        // appended for what was volatile last turn, and a new player input.
        let turn_n_plus_1 = make(
            "I approach the bar.",
            vec![
                ChatMessage::user("I look around."),
                ChatMessage::assistant("The tavern is dim and smoky."),
            ],
        );
        let turn_n_plus_1_messages = turn_n_plus_1.into_messages();

        // Every message from turn N's request reappears, unchanged, as a
        // prefix of turn N+1's request. That prefix is exactly what a
        // KV-cache-reusing backend needs to see to skip re-processing it.
        let static_prefix_len = 3; // system+card, lore, summaries
        for i in 0..static_prefix_len {
            assert_eq!(
                turn_n_messages[i].content, turn_n_plus_1_messages[i].content,
                "static section {i} must not change between turns"
            );
        }
    }
}
