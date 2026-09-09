//! Prompt assembly.
//!
//! The division [orison_audit.md §14] asked for, made mechanical:
//!
//! | Module | Owns | May read state |
//! |---|---|---|
//! | [`templates`] | Every word the model is told | No, and a test enforces it |
//! | [`assembly`] | Turning typed state into blocks | Yes |
//! | [`ordering`] | Which block goes where | It is given them |
//! | [`budget`] | What fits | Counts, with the real tokenizer |
//! | [`player_input`] | Sanitising and parsing what the player typed | No |
//!
//! "Where does this change go" has one answer: if it is words, `templates`;
//! if it reads the campaign, `assembly`; if it is about position, `ordering`.
//!
//! [orison_audit.md §14]: ../../../../docs/orison_audit.md

pub mod assembly;
pub mod budget;
pub mod ordering;
pub mod player_input;
pub mod schemas;
pub mod templates;

pub use assembly::{CharacterCard, DirectorPrompt, PlayerCard, TurnPrompt, WorldSnapshot};
pub use budget::{allocate, BudgetAllocation, BudgetFractions, PromptBudget, PromptError};
pub use ordering::PromptSections;
pub use player_input::{
    parse as parse_player_input, sanitize as sanitize_player_input, InputStyle, ParsedInput,
};
pub use templates::Speech;
