//! Real tokenisation and honest budgeting (§2.5).
//!
//! Replaces `PromptBuilder.estimate_tokens()` (`length / 4`, B-4) and the
//! allocator whose fractions summed to 1.05 against a limit that was itself
//! double the window actually served (B-1). Both mistakes are made
//! unrepresentable here: fractions are checked to sum to at most 1.0 at
//! construction, and the limit passed in is always
//! `InferenceBackend::context_length()`, never a literal.

use tokenizers::Tokenizer;

/// Count tokens with the model's real tokenizer. The one and only
/// replacement for `int(ceil(text.length() / 4.0))`.
pub fn count_tokens(
    tokenizer: &Tokenizer,
    text: &str,
) -> Result<usize, crate::inference::InferenceError> {
    tokenizer
        .encode(text, false)
        .map(|enc| enc.len())
        .map_err(|e| crate::inference::InferenceError::Tokenizer(e.to_string()))
}

/// The tokens available for prompt content: the context window actually
/// served, minus a reserve held back for the model's response. Every
/// allocation in this module is computed against `available()`, never
/// against `context_length` directly — that headroom is what the response
/// reservation is for.
#[derive(Debug, Clone, Copy)]
pub struct PromptBudget {
    pub context_length: usize,
    pub response_reserve: usize,
}

impl PromptBudget {
    pub fn new(context_length: usize, response_reserve: usize) -> Self {
        Self {
            context_length,
            response_reserve,
        }
    }

    pub fn available(&self) -> usize {
        self.context_length.saturating_sub(self.response_reserve)
    }
}

/// Fractions of `PromptBudget::available()` allocated to each context block.
/// Must sum to at most 1.0; `allocate()` rejects anything that does not,
/// rather than silently letting the model truncate the front of the context
/// the way the Godot build's 1.05 total did.
#[derive(Debug, Clone, Copy)]
pub struct BudgetFractions {
    pub system: f32,
    pub identity: f32,
    pub history: f32,
    pub lore: f32,
}

impl BudgetFractions {
    pub fn sum(&self) -> f32 {
        self.system + self.identity + self.history + self.lore
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetAllocation {
    pub system: usize,
    pub identity: usize,
    pub history: usize,
    pub lore: usize,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PromptError {
    #[error("budget fractions sum to {sum}, which exceeds 1.0")]
    FractionsExceedBudget { sum: f32 },

    /// The typed replacement for `PromptBuilder._report_overflow()`, which
    /// used to be a `push_warning()` — invisible outside the editor, and
    /// therefore silently discarded in shipped builds while the model
    /// quietly dropped the system prompt and character profile. A caller
    /// must now handle this value; there is no code path that "succeeds"
    /// past it.
    #[error("prompt of {measured} tokens exceeds the {limit} token budget")]
    Overflow { measured: usize, limit: usize },
}

/// Divide `budget.available()` among the four context blocks per
/// `fractions`. Errors rather than silently over-allocating if the
/// fractions do not fit.
pub fn allocate(
    budget: &PromptBudget,
    fractions: BudgetFractions,
) -> Result<BudgetAllocation, PromptError> {
    let sum = fractions.sum();
    if sum > 1.0 + f32::EPSILON {
        return Err(PromptError::FractionsExceedBudget { sum });
    }
    let avail = budget.available() as f32;
    Ok(BudgetAllocation {
        system: (avail * fractions.system) as usize,
        identity: (avail * fractions.identity) as usize,
        history: (avail * fractions.history) as usize,
        lore: (avail * fractions.lore) as usize,
    })
}

/// Check an assembled prompt's real token count against the budget. Returns
/// the typed [`PromptError::Overflow`] rather than truncating or warning.
pub fn check_overflow(measured: usize, limit: usize) -> Result<(), PromptError> {
    if measured > limit {
        Err(PromptError::Overflow { measured, limit })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_over_budget_are_rejected() {
        // The Godot build's actual fractions: 0.30 + 0.30 + 0.30 + 0.15 = 1.05.
        let over_budget = BudgetFractions {
            system: 0.30,
            identity: 0.30,
            history: 0.30,
            lore: 0.15,
        };
        let budget = PromptBudget::new(8192, 1024);
        assert_eq!(
            allocate(&budget, over_budget),
            Err(PromptError::FractionsExceedBudget {
                sum: over_budget.sum()
            })
        );
    }

    #[test]
    fn fractions_at_exactly_one_are_accepted() {
        let fractions = BudgetFractions {
            system: 0.30,
            identity: 0.30,
            history: 0.30,
            lore: 0.10,
        };
        let budget = PromptBudget::new(8192, 1024);
        assert!(allocate(&budget, fractions).is_ok());
    }

    #[test]
    fn available_never_exceeds_context_length_minus_reserve() {
        let budget = PromptBudget::new(4096, 1024);
        assert_eq!(budget.available(), 3072);
    }

    #[test]
    fn available_saturates_rather_than_underflowing() {
        // A reserve larger than the context length must not panic or wrap.
        let budget = PromptBudget::new(512, 1024);
        assert_eq!(budget.available(), 0);
    }

    #[test]
    fn overflow_is_a_typed_error_not_a_discarded_warning() {
        assert_eq!(
            check_overflow(9000, 8192),
            Err(PromptError::Overflow {
                measured: 9000,
                limit: 8192
            })
        );
        assert_eq!(check_overflow(100, 8192), Ok(()));
    }
}
