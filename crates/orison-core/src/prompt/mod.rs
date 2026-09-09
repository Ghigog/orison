//! Prompt assembly: budgeting and cache-stable ordering only, this phase.
//! Retrieval, knowledge-graph queries, and everything else that fills these
//! sections in with real content is Phase 3/4.

pub mod budget;
pub mod ordering;
pub mod player_input;
pub mod schemas;

pub use budget::{allocate, BudgetAllocation, BudgetFractions, PromptBudget, PromptError};
pub use ordering::PromptSections;
pub use player_input::{
    parse as parse_player_input, sanitize as sanitize_player_input, InputStyle, ParsedInput,
};
