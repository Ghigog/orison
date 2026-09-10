//! Prompt assembly (`PromptBuilder.gd`).
//!
//! The other half of the boundary [`super::templates`] describes: this module
//! takes typed campaign state and produces an ordered message list. It writes
//! no prose of its own beyond the field labels that structure the state it was
//! given, and it never decides what the model is told to *do* — that is the
//! templates' job.
//!
//! It also owns nothing about *order*: [`super::ordering::PromptSections`]
//! does, because ordering is a cache-stability property (§2.7) rather than an
//! assembly preference. Assembly fills slots; the slots are already sorted
//! stable-to-volatile.

use crate::inference::ChatMessage;

use super::ordering::PromptSections;
use super::player_input::parse;
use super::templates::{self, Speech};

/// Who the player is talking to, as the vault records them.
///
/// Borrowed rather than owned: every field already exists on a graph
/// [`Entity`], and copying it to build a prompt is how a second entity store
/// starts (§3.3).
///
/// [`Entity`]: crate::knowledge::Entity
#[derive(Debug, Clone, Default)]
pub struct CharacterCard<'a> {
    pub name: &'a str,
    pub gender: Option<&'a str>,
    pub biography: Option<&'a str>,
    pub personality: Option<&'a str>,
    pub appearance: Option<&'a str>,
    pub goals: Option<&'a str>,
    /// A sample of the author's prose, for voice matching.
    pub writing_style: Option<&'a str>,
}

impl CharacterCard<'_> {
    pub fn render(&self) -> String {
        let mut out = format!("CHARACTER PROFILE:\n- Name: {}\n", self.name);

        // Stated when known, and explicitly left to inference when not.
        // A missing pronoun field is [rag_architecture.md] Bug 3: with nothing
        // to go on the model takes a pronoun from the statistical prior on the
        // name, which is how King Yuna became "she".
        //
        // [rag_architecture.md]: ../../../../docs/rag_architecture.md
        match self.gender {
            Some(g) => out.push_str(&format!("- Gender and pronouns: {g}\n")),
            None => out.push_str(
                "- Gender and pronouns: not recorded. Infer them from this character's \
                 title and biography, never from their name alone.\n",
            ),
        }

        for (label, value) in [
            ("Biography", self.biography),
            ("Personality", self.personality),
            ("Appearance", self.appearance),
            ("Goals and motivations", self.goals),
        ] {
            if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                out.push_str(&format!("- {label}: {value}\n"));
            }
        }

        if let Some(style) = self.writing_style.map(str::trim).filter(|s| !s.is_empty()) {
            out.push_str(
                "\nVOICE:\nMatch the tone, syntax and word choice of this sample in your \
                 dialogue:\n\"\"\"\n",
            );
            out.push_str(style);
            out.push_str("\n\"\"\"\n");
        }
        out
    }
}

/// The protagonist, as onboarding recorded them.
#[derive(Debug, Clone, Default)]
pub struct PlayerCard<'a> {
    pub name: &'a str,
    pub physical_description: Option<&'a str>,
    pub personality: Option<&'a str>,
    pub backstory: Option<&'a str>,
}

impl PlayerCard<'_> {
    pub fn render(&self) -> String {
        let mut out = format!(
            "PLAYER CHARACTER (the protagonist you are interacting with):\n- Name: {}\n",
            self.name
        );
        for (label, value) in [
            ("Appearance", self.physical_description),
            ("Personality", self.personality),
            ("Backstory", self.backstory),
        ] {
            if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                out.push_str(&format!("- {label}: {value}\n"));
            }
        }
        out
    }
}

/// The parts of campaign state a prompt states outright rather than retrieving.
#[derive(Debug, Clone, Default)]
pub struct WorldSnapshot<'a> {
    pub location: Option<&'a str>,
    pub plot_flags: &'a [(String, String)],
    /// The active character's inventory: item and quantity.
    pub inventory: &'a [(String, i64)],
}

impl WorldSnapshot<'_> {
    pub fn render(&self, holder: &str) -> Option<String> {
        if self.location.is_none() && self.plot_flags.is_empty() && self.inventory.is_empty() {
            return None;
        }
        let mut out = String::from("WORLD STATE:\n");
        if let Some(location) = self.location {
            out.push_str(&format!("- Current location: {location}\n"));
        }
        if !self.plot_flags.is_empty() {
            out.push_str("- Plot flags:\n");
            for (key, value) in self.plot_flags {
                out.push_str(&format!("  - {key}: {value}\n"));
            }
        }
        if !self.inventory.is_empty() {
            out.push_str(&format!("- {holder} is carrying:\n"));
            for (item, quantity) in self.inventory {
                out.push_str(&format!("  - {item} (x{quantity})\n"));
            }
        }
        Some(out)
    }
}

/// Everything one turn's prompt is made of.
///
/// A struct rather than fifteen arguments, and every field typed rather than
/// pulled out of a `Dictionary` at the point of use, which is the difference
/// this port is for.
#[derive(Debug, Clone, Default)]
pub struct TurnPrompt<'a> {
    pub character: CharacterCard<'a>,
    pub speech: SpeechKind,
    /// Whether one model is answering as both character and world builder
    /// (§4.2, arm C).
    pub combined: bool,
    pub player: Option<PlayerCard<'a>>,
    pub world: WorldSnapshot<'a>,
    /// The adventure's memory tiers.
    pub campaign_memory: Option<String>,
    /// What this character remembers of the adventure. Never their biography:
    /// see [`crate::memory`].
    pub session_memory: Option<String>,
    /// Formatted by `retrieval::format_context`, never re-formatted here.
    pub lore: Option<String>,
    /// Prior turns, oldest first, already rendered as messages.
    pub history: Vec<ChatMessage>,
    /// How the character feels right now. Volatile; ordered last but one.
    pub emotional_profile: Option<String>,
    /// The player's line, already sanitised.
    pub player_input: &'a str,
}

/// Newtype so `TurnPrompt` can derive `Default`; [`Speech`] has no sensible
/// default of its own and guessing one for a character would be exactly the
/// kind of silent fallback this codebase keeps removing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpeechKind(pub Option<Speech>);

impl SpeechKind {
    fn resolve(self) -> Speech {
        self.0.unwrap_or(Speech::Verbal)
    }
}

impl From<Speech> for SpeechKind {
    fn from(speech: Speech) -> Self {
        SpeechKind(Some(speech))
    }
}

impl<'a> TurnPrompt<'a> {
    /// Fill the ordered slots.
    ///
    /// Which slot each block goes in is a cache decision, not a stylistic one:
    ///
    /// | Block | Slot | Why |
    /// |---|---|---|
    /// | Instructions, character card, player card | first | fixed for a scene |
    /// | World state, memory | after | changes occasionally |
    /// | Transcript | after that | appends, never rewrites |
    /// | Retrieved lore | last but two | the query is the player's line, so it changes every turn |
    /// | Emotional profile | last but one | changes most turns |
    /// | The player's line | last | new every single call |
    ///
    /// Lore was ordered ahead of the transcript until Phase 5.0, on the
    /// reasoning that a repeated query keeps its prefix stable. Play does not
    /// repeat the query. See [`super::ordering`].
    pub fn sections(self) -> PromptSections {
        let speech = self.speech.resolve();
        let instructions = if self.combined {
            templates::combined_instructions(speech)
        } else {
            templates::actor_instructions(speech)
        };

        let mut card = self.character.render();
        if let Some(player) = &self.player {
            card.push('\n');
            card.push_str(&player.render());
        }

        let mut middle = Vec::new();
        if let Some(world) = self.world.render(self.character.name) {
            middle.push(world);
        }
        if let Some(memory) = self.campaign_memory {
            middle.push(memory);
        }
        if let Some(memory) = self.session_memory {
            middle.push(memory);
        }

        PromptSections {
            system_instructions: instructions,
            character_card: Some(card),
            retrieved_lore: self.lore,
            session_summaries: (!middle.is_empty()).then(|| middle.join("\n")),
            recent_turns: self.history,
            volatile_state: self.emotional_profile,
            player_input: player_message(self.player_input),
        }
    }
}

/// The Director's prompt: the same state, no character to be.
#[derive(Debug, Clone, Default)]
pub struct DirectorPrompt<'a> {
    pub campaign_title: &'a str,
    /// Who the player is addressing, so the Director does not answer for them.
    pub active_character: Option<&'a str>,
    pub previous_beat: Option<&'a str>,
    pub player: Option<PlayerCard<'a>>,
    pub world: WorldSnapshot<'a>,
    pub campaign_memory: Option<String>,
    pub lore: Option<String>,
    pub history: Vec<ChatMessage>,
    pub player_input: &'a str,
}

impl<'a> DirectorPrompt<'a> {
    pub fn sections(self) -> PromptSections {
        let mut card = format!("CAMPAIGN: {}\n", self.campaign_title);
        if let Some(character) = self.active_character {
            card.push_str(&format!(
                "- The player is speaking directly with {character}. Describe them, but do \
                 not write their words or answer for them.\n"
            ));
        }
        if let Some(player) = &self.player {
            card.push('\n');
            card.push_str(&player.render());
        }

        let mut middle = Vec::new();
        if let Some(world) = self
            .world
            .render(self.active_character.unwrap_or("The player"))
        {
            middle.push(world);
        }
        if let Some(memory) = self.campaign_memory {
            middle.push(memory);
        }
        // The previous beat is volatile — it is whatever was narrated last —
        // so it sits at the back rather than in the card.
        let volatile = self
            .previous_beat
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(|beat| format!("PREVIOUS BEAT:\n{beat}\n"));

        PromptSections {
            system_instructions: templates::director_instructions(),
            character_card: Some(card),
            retrieved_lore: self.lore,
            session_summaries: (!middle.is_empty()).then(|| middle.join("\n")),
            recent_turns: self.history,
            volatile_state: volatile,
            player_input: player_message(self.player_input),
        }
    }
}

/// The injection boundary, and the one piece of formatting that is a security
/// property rather than a presentation choice.
///
/// `AGENTS.md` makes the `<player_message>` treatment a rule. Role separation
/// reinforces it — this is the content of a `user` message now, not a segment
/// of one concatenated string — but does not replace it: the delimiters are
/// what let the instructions say "everything inside is in-character".
///
/// Rendered by exactly one function so that a line sent this turn and the same
/// line replayed as history next turn are byte-identical. They are not the
/// same code path by accident; making them different is a cache bug
/// `tests/turn_loop.rs` has already caught once.
pub fn player_message(text: &str) -> String {
    let parsed = parse(text);
    let or_none = |value: &str| {
        if value.is_empty() {
            "None".to_string()
        } else {
            value.to_string()
        }
    };
    format!(
        "<player_message>\n- Raw input: {text}\n- Parsed dialogue: {}\n- Parsed action: {}\n\
         - Syntax style: {}\n</player_message>\n",
        or_none(&parsed.dialogue),
        or_none(&parsed.action),
        parsed.style.as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> CharacterCard<'static> {
        CharacterCard {
            name: "King Yuna",
            gender: None,
            biography: Some("Took the salt throne at nineteen."),
            ..Default::default()
        }
    }

    /// Bug 3 as a test on the assembly rather than on the model: a character
    /// with no gender field must still leave the model something better than
    /// the prior on their name.
    #[test]
    fn a_character_without_a_gender_field_gets_an_explicit_instruction() {
        let rendered = card().render();
        assert!(rendered.contains("never from their name alone"));

        let known = CharacterCard {
            gender: Some("he/him"),
            ..card()
        };
        assert!(known.render().contains("- Gender and pronouns: he/him"));
        assert!(!known.render().contains("never from their name alone"));
    }

    #[test]
    fn empty_fields_produce_no_labels() {
        // A prompt full of "- Personality: " teaches the model that blank
        // fields are normal.
        let sparse = CharacterCard {
            name: "Bram Holt",
            personality: Some("   "),
            ..Default::default()
        };
        let rendered = sparse.render();
        assert!(!rendered.contains("Personality"));
        assert!(!rendered.contains("Biography"));
    }

    #[test]
    fn the_player_line_is_wrapped_and_parsed() {
        let rendered = player_message("\"Where is it?\" I ask, hand on the door.");
        assert!(rendered.starts_with("<player_message>"));
        assert!(rendered.contains("- Parsed dialogue: Where is it?"));
        assert!(rendered.contains("- Parsed action: I ask, hand on the door"));
    }

    #[test]
    fn slots_are_filled_stable_to_volatile() {
        let prompt = TurnPrompt {
            character: card(),
            speech: Speech::Verbal.into(),
            emotional_profile: Some("EMOTIONAL PROFILE:\n- calm\n".to_string()),
            lore: Some("### Retrieved Memory\n".to_string()),
            player_input: "Hello.",
            ..Default::default()
        };
        let messages = prompt.sections().into_messages();

        assert!(messages[0]
            .content
            .as_deref()
            .unwrap()
            .contains("King Yuna"));
        let volatile = &messages[messages.len() - 2];
        assert!(volatile
            .content
            .as_deref()
            .unwrap()
            .starts_with("EMOTIONAL PROFILE:"));
        assert!(messages
            .last()
            .unwrap()
            .content
            .as_deref()
            .unwrap()
            .contains("<player_message>"));
    }

    #[test]
    fn an_empty_world_snapshot_produces_no_block() {
        let empty = WorldSnapshot::default();
        assert!(empty.render("Bram").is_none());

        let flags = vec![("tithe_abolished".to_string(), "true".to_string())];
        let some = WorldSnapshot {
            plot_flags: &flags,
            ..Default::default()
        };
        assert!(some
            .render("Bram")
            .unwrap()
            .contains("tithe_abolished: true"));
    }
}
