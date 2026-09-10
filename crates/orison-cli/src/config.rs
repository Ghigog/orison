//! What model answers, where it lives, and how many calls a turn makes.
//!
//! **Everything here is configuration, and that is a requirement rather than
//! a preference** (B-10, and migration_plan.md Appendix D). The §4.2 arms are
//! three configurations of two code paths:
//!
//! | Arm | `--actor-model` | `--director-model` | `--profile` |
//! |---|---|---|---|
//! | A: the split | a 3B | an 8B | `two-calls` |
//! | B: one model, two calls | an 8B | the same 8B | `two-calls` |
//! | C: one model, one call | an 8B | (ignored) | `single-call` |
//!
//! Nothing in this crate compares a model name to decide anything. A build
//! that did would be wrong the moment both roles were configured to the same
//! model, and that configuration is arm B.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, ValueEnum};
use orison_core::inference::{InferenceBackend, OllamaBackend, OllamaConfig};
use orison_core::turn::TurnProfile;

use crate::error::CliError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Profile {
    /// The Actor answers; the Director composes the next beat separately.
    TwoCalls,
    /// One model answers as the character and updates world state at once.
    SingleCall,
}

impl From<Profile> for TurnProfile {
    fn from(p: Profile) -> Self {
        match p {
            Profile::TwoCalls => TurnProfile::TwoCalls,
            Profile::SingleCall => TurnProfile::SingleCall,
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct ModelArgs {
    /// The local inference endpoint. Must be local: no vault content, no
    /// gameplay text and no user data leaves the machine, ever.
    #[arg(
        long,
        env = "ORISON_OLLAMA_URL",
        default_value = "http://127.0.0.1:11434"
    )]
    pub url: String,

    /// The model that speaks in character.
    #[arg(long, env = "ORISON_ACTOR_MODEL")]
    pub actor_model: String,

    /// The model that manages world state and narrative beats. Defaults to
    /// the Actor's model, which is arm B rather than a fallback.
    #[arg(long, env = "ORISON_DIRECTOR_MODEL")]
    pub director_model: Option<String>,

    /// How many model calls a turn makes.
    #[arg(long, value_enum, default_value = "two-calls")]
    pub profile: Profile,

    /// The model's `tokenizer.json`, for real token counts (§2.5).
    ///
    /// Without it the CLI counts whitespace-separated words, says so loudly
    /// once at startup, and keeps playing. That approximation is not
    /// `length / 4` (B-4) and it is not silent, but it is an approximation
    /// and the budget is only as honest as it is.
    #[arg(long, env = "ORISON_TOKENIZER")]
    pub tokenizer: Option<PathBuf>,

    /// Cap on the context window asked of the model, in tokens.
    ///
    /// Left alone this is `orison-core`'s default. Raising it costs KV cache
    /// proportionally and is why Phase 4's live turn latency came in at twice
    /// the Godot baseline; see `inference::DEFAULT_CONTEXT_LIMIT`.
    #[arg(long)]
    pub context_limit: Option<usize>,
}

/// The two backends a turn runs against, plus how they were configured.
pub struct Backends {
    pub actor: Arc<dyn InferenceBackend>,
    pub director: Arc<dyn InferenceBackend>,
    pub profile: TurnProfile,
    /// What to print so a transcript records which arm produced it.
    pub summary: String,
}

impl ModelArgs {
    pub async fn connect(&self) -> Result<Backends, CliError> {
        let director_model = self
            .director_model
            .clone()
            .unwrap_or_else(|| self.actor_model.clone());
        let ollama = OllamaConfig {
            context_limit: self.context_limit.or(OllamaConfig::default().context_limit),
        };

        let actor: Arc<dyn InferenceBackend> = Arc::new(
            OllamaBackend::connect_with(
                &self.url,
                &self.actor_model,
                self.load_tokenizer()?,
                ollama.clone(),
            )
            .await?,
        );

        // One model configured for both roles is arm B, and it gets one
        // backend rather than two connections to the same server.
        let director: Arc<dyn InferenceBackend> = if director_model == self.actor_model {
            Arc::clone(&actor)
        } else {
            Arc::new(
                OllamaBackend::connect_with(
                    &self.url,
                    &director_model,
                    self.load_tokenizer()?,
                    ollama,
                )
                .await?,
            )
        };

        let summary = format!(
            "actor {} | director {} | {} | context {}",
            self.actor_model,
            director_model,
            match self.profile {
                Profile::TwoCalls => "two calls",
                Profile::SingleCall => "one call",
            },
            actor.context_length(),
        );

        Ok(Backends {
            actor,
            director,
            profile: self.profile.into(),
            summary,
        })
    }

    /// The model's own tokenizer, or the stated approximation.
    ///
    /// A tokenizer is loaded per backend rather than shared because
    /// `InferenceBackend` owns one, and two roles can be two models with two
    /// vocabularies — the case B-10 exists for.
    pub fn load_tokenizer(&self) -> Result<tokenizers::Tokenizer, CliError> {
        match &self.tokenizer {
            Some(path) => {
                tokenizers::Tokenizer::from_file(path).map_err(|source| CliError::Tokenizer {
                    path: path.display().to_string(),
                    detail: source.to_string(),
                })
            }
            None => Ok(word_tokenizer()),
        }
    }

    pub fn tokenizer_is_approximate(&self) -> bool {
        self.tokenizer.is_none()
    }
}

/// One token per whitespace-separated word.
///
/// Honest about being wrong in a way `length / 4` is not: it under-counts a
/// real BPE vocabulary by a predictable factor rather than by a made-up one,
/// and the caller is told it is in use.
fn word_tokenizer() -> tokenizers::Tokenizer {
    use std::str::FromStr;
    tokenizers::Tokenizer::from_str(
        r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":{"type":"Whitespace"},"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0},"unk_token":"[UNK]"}}"#,
    )
    .expect("the built-in word tokenizer is valid")
}
