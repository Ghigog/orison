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
//! assembles messages in exactly this order — static system instructions and
//! the character card, session summaries and world state, the transcript,
//! then the three blocks that are new on every call: retrieved lore, the
//! character's volatile state, and the player's input.
//!
//! **The rule is "stable-to-volatile", and it has no exceptions.** Two
//! blocks were caught breaking it by measurement rather than by reading, and
//! both cost the whole cache:
//!
//! - The emotional profile sat inside the character card, which is the
//!   *first* message of every request. Emotion moves on almost every turn, so
//!   the cache was invalidated from token zero. `tests/turn_loop.rs` found
//!   that one.
//! - Retrieved lore sat ahead of the growing transcript, on the reasoning
//!   that a repeated query keeps its prefix stable. Real play never repeats
//!   the query: `TurnEngine::retrieve_lore` uses the player's line as the
//!   query, so the lore block changes on every turn and everything after it —
//!   which was the entire transcript — was re-processed every time. The wire
//!   test asserted the prefix using *the same question twice*, so it passed
//!   while live turns measured at 2x the Godot baseline
//!   (`docs/eval_baseline.md`, "Turn latency — measured"). Phase 5.0 moved
//!   lore into the volatile tail, where its own volatility says it belongs.
//!
//! The cost of the second move is that lore is no longer "context you read
//! before the conversation" but "context you are handed just before
//! answering". That reads at least as naturally, and it is the only placement
//! that leaves a growing transcript inside the cached prefix.

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
    /// Session/medium-term summaries, world state and campaign memory.
    /// Stable across a scene: they move when the player moves or when the
    /// summariser runs, not turn to turn.
    pub session_summaries: Option<String>,
    /// Prior turns in the conversation, oldest first. The growing but
    /// stable-prefixed part: each new turn appends one entry rather than
    /// rewriting the ones before it. This is the largest block in a long
    /// scene, which is why nothing volatile may be ordered ahead of it.
    pub recent_turns: Vec<ChatMessage>,
    /// Retrieved lore for this turn's query. Volatile: the query is the
    /// player's line, so in play this block changes on every single turn.
    /// It is ordered *after* the transcript for that reason — see the module
    /// note, and the live measurement that put it here.
    pub retrieved_lore: Option<String>,
    /// State that changes turn to turn: the emotional profile and rapport
    /// band. Ordered here because it is nearly as volatile as the player's
    /// input — and because recency is where a model attends to "how do you
    /// feel right now" anyway.
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

        if let Some(summaries) = self.session_summaries {
            messages.push(ChatMessage::system(summaries));
        }

        // Everything above this line is the cached prefix; everything below
        // it is new on this call. The transcript belongs above because it
        // only ever grows by appending.
        messages.extend(self.recent_turns);

        if let Some(lore) = self.retrieved_lore {
            messages.push(ChatMessage::system(lore));
        }
        if let Some(state) = self.volatile_state {
            messages.push(ChatMessage::system(state));
        }
        messages.push(ChatMessage::user(self.player_input));
        messages
    }

    /// How many messages at the back of [`into_messages`]'s output are new on
    /// every call: the lore, the volatile state and the player's line, minus
    /// whichever of the first two are absent.
    ///
    /// Tests use it instead of hardcoding an index, so a future block added
    /// to either half moves the boundary in one place.
    pub fn volatile_tail_len(&self) -> usize {
        1 + usize::from(self.retrieved_lore.is_some()) + usize::from(self.volatile_state.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::Role;

    fn sections(
        feeling: &str,
        lore: &str,
        recent_turns: Vec<ChatMessage>,
        player_input: &str,
    ) -> PromptSections {
        PromptSections {
            system_instructions: "You are the DM.".to_string(),
            character_card: Some("Name: Elowen".to_string()),
            session_summaries: Some("The party arrived in town.".to_string()),
            recent_turns,
            retrieved_lore: Some(lore.to_string()),
            volatile_state: Some(format!("Active feeling: {feeling}.")),
            player_input: player_input.to_string(),
        }
    }

    #[test]
    fn volatile_content_is_always_last() {
        let messages = sections(
            "serenity",
            "The tavern is called The Guttering Lamp.",
            vec![
                ChatMessage::user("I look around."),
                ChatMessage::assistant("The tavern is dim and smoky."),
            ],
            "I approach the bar.",
        )
        .into_messages();

        let last = messages.last().expect("at least one message");
        assert_eq!(last.role, Role::User);
        assert_eq!(last.content.as_deref(), Some("I approach the bar."));
    }

    /// The regression `tests/turn_loop.rs` found: state that moves every turn
    /// must not sit in the block the cache prefix depends on.
    #[test]
    fn volatile_state_does_not_touch_the_stable_prefix() {
        let make = |feeling: &str| {
            sections(
                feeling,
                "The tavern is called The Guttering Lamp.",
                vec![ChatMessage::user("I look around.")],
                "I approach the bar.",
            )
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

    /// The second half of the same rule, and the one that was wrong until
    /// Phase 5.0: the retrieved lore changes with the query, so it must not
    /// sit ahead of the transcript either.
    #[test]
    fn retrieved_lore_does_not_touch_the_stable_prefix() {
        let history = vec![
            ChatMessage::user("I look around."),
            ChatMessage::assistant("The tavern is dim and smoky."),
        ];
        let make = |lore: &str| sections("serenity", lore, history.clone(), "I approach the bar.");

        let one = make("LORE: the Guttering Lamp.").into_messages();
        let two = make("LORE: the counting house.").into_messages();

        assert_eq!(one.len(), two.len());
        let lore_at = one.len() - 3;
        for i in 0..lore_at {
            assert_eq!(
                one[i].content, two[i].content,
                "message {i} must not depend on what this turn retrieved"
            );
        }
        assert_ne!(one[lore_at].content, two[lore_at].content);
        assert_eq!(
            one[lore_at - 1].role,
            Role::Assistant,
            "the transcript must end immediately before the lore block"
        );
    }

    #[test]
    fn the_volatile_tail_is_where_the_accessor_says_it_is() {
        let full = sections("serenity", "LORE", vec![ChatMessage::user("Hello.")], "Hi.");
        assert_eq!(full.volatile_tail_len(), 3);
        let messages_len = full.clone().into_messages().len();
        assert_eq!(messages_len, 6); // system, summaries, one turn, lore, state, input

        let bare = PromptSections {
            system_instructions: "You are the DM.".to_string(),
            player_input: "Hi.".to_string(),
            ..Default::default()
        };
        assert_eq!(bare.volatile_tail_len(), 1);
        assert_eq!(bare.into_messages().len(), 2);
    }

    #[test]
    fn unchanged_sections_produce_a_byte_identical_prefix_across_turns() {
        // Turn N: history is empty, player says something, and this turn
        // retrieves one thing.
        let turn_n = sections(
            "serenity",
            "LORE: the Guttering Lamp.",
            vec![],
            "I look around.",
        )
        .into_messages();

        // Turn N+1: a different subject, so different lore and a different
        // emotional state, with turn N appended to the transcript.
        let turn_n_plus_1 = sections(
            "anger",
            "LORE: the counting house.",
            vec![
                ChatMessage::user("I look around."),
                ChatMessage::assistant("The tavern is dim and smoky."),
            ],
            "I approach the bar.",
        )
        .into_messages();

        // Every message from turn N's *stable* half reappears, unchanged, as
        // a prefix of turn N+1's request — and so does turn N's own player
        // line, now the first entry of the transcript. That prefix is exactly
        // what a KV-cache-reusing backend needs to see to skip re-processing
        // it, and it grows rather than shrinking as the scene goes on.
        let stable = turn_n.len() - 3; // system, summaries
        assert_eq!(stable, 2);
        for i in 0..stable {
            assert_eq!(
                turn_n[i].content, turn_n_plus_1[i].content,
                "static section {i} must not change between turns"
            );
        }
        assert_eq!(
            turn_n.last().unwrap().content,
            turn_n_plus_1[stable].content,
            "turn N's player line must replay identically as turn N+1's history"
        );
    }
}
