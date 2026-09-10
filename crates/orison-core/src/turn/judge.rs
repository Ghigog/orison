//! The LLM-as-judge suite (§1.3), and the caveats that come with it.
//!
//! `turn::experiment` scores what a program can check: schema validity,
//! pronouns, verbatim repeats, forbidden phrasing, empty responses. It says
//! in its own documentation that it "cannot tell you whether a scene was any
//! good". This is the half that tries to, on the four axes §1.3 names:
//! in-character consistency, use of vault-sourced facts, narrative
//! progression, and absence of sycophancy, each 1-5.
//!
//! **Three rules the plan states and this module enforces rather than
//! documents.**
//!
//! - *"Judge with a larger local model than the one under test."* The judge
//!   is its own [`InferenceBackend`], never the Actor's, and [`JudgeReport`]
//!   records which model produced the scores — a judge grading itself is not
//!   a measurement and the report should not be able to hide that it was.
//! - *"Judge scores are noisy. Use them for trend detection across many
//!   samples, never to gate a single change."* Nothing here returns a
//!   pass/fail, and [`JudgeReport::mean`] is `None` below
//!   [`MINIMUM_SAMPLES`] rather than averaging three turns into a number
//!   somebody will quote.
//! - *"The deterministic suite is the gate."* It still is. This runs beside
//!   it, gated on `ORISON_TEST_JUDGE_MODEL`, and skips loudly when unset.
//!
//! **What it still cannot do.** A judge shares the failure modes of the thing
//! it grades: it is likelier to reward fluent prose than accurate prose, and
//! a local 8B judge is not a careful reader. The rubric pushes back where it
//! can — a 3 is the honest middle, a 5 has to be earned, and every score
//! comes with a sentence naming the line it is about, so a reader can check
//! the judge rather than trust it. That sentence is the most useful thing in
//! the report and is printed with the numbers for that reason.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::inference::{ChatMessage, ChatRequest, InferenceBackend, ResponseFormat};
use crate::prompt::templates;

use super::error::TurnError;

/// Below this many scored turns, a mean is noise with a decimal point.
///
/// Ten is not a statistical claim; it is a refusal to print a number from
/// four samples, which is how a noisy metric becomes a quoted one. The
/// fixtures run four and five turns, so a single-fixture run reports its
/// turns and no mean, and a suite across fixtures and repeats reports both.
pub const MINIMUM_SAMPLES: usize = 10;

/// One turn's scores, as the judge returns them.
///
/// Schema-constrained like every other response in this crate (§2.4), so an
/// unparseable judgement is a backend bug rather than something to repair.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JudgeScores {
    /// Does the reply sound like this character rather than an assistant?
    pub in_character: u8,
    /// Does it use the profile and retrieved lore without contradicting them?
    pub vault_grounded: u8,
    /// Does the scene move?
    pub progression: u8,
    /// Does the character keep their own judgement? High is good: the axis is
    /// named for the absence, so every axis points the same way and a mean
    /// across them means something.
    pub not_sycophantic: u8,
    /// One sentence naming what drove the scores. The part a human reads.
    pub note: String,
}

impl JudgeScores {
    /// Every axis, clamped into the rubric's range.
    ///
    /// The schema says `u8`; it does not say 1-5, because JSON Schema's
    /// `minimum` is advisory for most decoders and this crate does not
    /// pretend otherwise (see `prompt::schemas`). A judge that answers 0 or
    /// 9 is clamped here rather than silently skewing a mean.
    pub fn axes(&self) -> [u8; 4] {
        [
            self.in_character.clamp(1, 5),
            self.vault_grounded.clamp(1, 5),
            self.progression.clamp(1, 5),
            self.not_sycophantic.clamp(1, 5),
        ]
    }

    /// This turn's four axes averaged. Equal weights, because nothing has
    /// measured that they should not be.
    pub fn overall(&self) -> f64 {
        self.axes().iter().map(|v| *v as f64).sum::<f64>() / 4.0
    }
}

/// What the judge is shown about one turn.
///
/// Assembled by the caller from state, and *not* by this module: the same
/// boundary `prompt::assembly` keeps. Every field is evidence the judge is
/// allowed to score against, and the rubric tells it to score nothing else.
#[derive(Debug, Clone, Default)]
pub struct JudgedTurnInput {
    pub character_card: String,
    /// The lore the engine actually retrieved for this turn, not a fresh
    /// query. Judging against different material than the Actor saw would
    /// measure retrieval twice and the response not at all.
    pub retrieved_lore: Option<String>,
    pub player_input: String,
    pub narration: String,
    pub dialogue: String,
}

#[derive(Debug, Clone)]
pub struct JudgedTurn {
    pub index: usize,
    pub scores: JudgeScores,
}

/// One run's scores, and the model that produced them.
#[derive(Debug, Clone)]
pub struct JudgeReport {
    /// The judge's model id. Recorded because a score is not interpretable
    /// without it, and because comparing runs judged by different models is
    /// the mistake this field exists to make visible.
    pub judge_model: String,
    /// What was being judged: an arm, a fixture, a phase.
    pub subject: String,
    pub turns: Vec<JudgedTurn>,
}

impl JudgeReport {
    pub fn new(judge_model: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            judge_model: judge_model.into(),
            subject: subject.into(),
            turns: Vec::new(),
        }
    }

    pub fn push(&mut self, index: usize, scores: JudgeScores) {
        self.turns.push(JudgedTurn { index, scores });
    }

    pub fn len(&self) -> usize {
        self.turns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Mean of one axis, or `None` below [`MINIMUM_SAMPLES`].
    pub fn axis_mean(&self, axis: usize) -> Option<f64> {
        if self.turns.len() < MINIMUM_SAMPLES {
            return None;
        }
        let sum: f64 = self
            .turns
            .iter()
            .map(|t| t.scores.axes()[axis] as f64)
            .sum();
        Some(sum / self.turns.len() as f64)
    }

    /// The headline number, or `None` when there are too few turns for one.
    pub fn mean(&self) -> Option<f64> {
        if self.turns.len() < MINIMUM_SAMPLES {
            return None;
        }
        let sum: f64 = self.turns.iter().map(|t| t.scores.overall()).sum();
        Some(sum / self.turns.len() as f64)
    }

    /// The four axis means, in [`AXIS_NAMES`] order.
    pub fn axis_means(&self) -> Option<[f64; 4]> {
        Some([
            self.axis_mean(0)?,
            self.axis_mean(1)?,
            self.axis_mean(2)?,
            self.axis_mean(3)?,
        ])
    }

    /// The report as a reader should see it: the numbers, and then the notes
    /// the numbers came from.
    ///
    /// The notes are not an appendix. A judge score without the sentence
    /// justifying it is exactly the kind of figure that gets quoted without
    /// its caveats, which this crate has already had to write a paragraph
    /// about once (`turn::experiment`).
    pub fn render(&self) -> String {
        let mut out = format!(
            "judge {} on {} ({} turns)\n",
            self.judge_model,
            self.subject,
            self.turns.len()
        );
        match self.axis_means() {
            Some(means) => {
                for (name, mean) in AXIS_NAMES.iter().zip(means) {
                    out.push_str(&format!("  {name:<16} {mean:.2}\n"));
                }
                out.push_str(&format!(
                    "  {:<16} {:.2}\n",
                    "overall",
                    self.mean().unwrap_or_default()
                ));
            }
            None => out.push_str(&format!(
                "  no mean: {} turns is below the {MINIMUM_SAMPLES} this suite will \
                 average, and judge scores are noisy enough that a mean of fewer \
                 would be quoted and should not be.\n",
                self.turns.len()
            )),
        }
        out.push_str("  per turn:\n");
        for turn in &self.turns {
            let [a, b, c, d] = turn.scores.axes();
            out.push_str(&format!(
                "    {:>3}  {a} {b} {c} {d}  {:.2}  {}\n",
                turn.index,
                turn.scores.overall(),
                turn.scores.note.trim()
            ));
        }
        out
    }
}

pub const AXIS_NAMES: [&str; 4] = [
    "in character",
    "vault grounded",
    "progression",
    "not sycophantic",
];

/// Scores turns with a model.
///
/// Holds its own backend rather than borrowing the Actor's, which is what
/// makes "judge with a larger model" expressible at all.
pub struct Judge {
    backend: Arc<dyn InferenceBackend>,
    model: String,
}

impl Judge {
    pub fn new(backend: Arc<dyn InferenceBackend>, model: impl Into<String>) -> Self {
        Self {
            backend,
            model: model.into(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Score one turn.
    pub async fn score(&self, input: &JudgedTurnInput) -> Result<JudgeScores, TurnError> {
        let request = ChatRequest::new(vec![
            ChatMessage::system(templates::judge_instructions()),
            ChatMessage::user(render_evidence(input)),
        ])
        .with_response_format(ResponseFormat::for_type::<JudgeScores>());
        let response = self.backend.chat(request).await?;
        Ok(response.parse()?)
    }

    /// Score a whole transcript into a report.
    ///
    /// A turn the judge could not score is not silently dropped: it returns
    /// the error, because a report over an unknown subset of a transcript is
    /// worse than no report.
    pub async fn score_all(
        &self,
        subject: impl Into<String>,
        inputs: &[JudgedTurnInput],
    ) -> Result<JudgeReport, TurnError> {
        let mut report = JudgeReport::new(&self.model, subject);
        for (index, input) in inputs.iter().enumerate() {
            report.push(index, self.score(input).await?);
        }
        Ok(report)
    }
}

/// The evidence block, in a fixed order.
///
/// State goes in here and nowhere else; the words the judge is told live in
/// `prompt::templates` (`tests/prompt_boundary.rs`). Fixed order because a
/// judge is a model too, and one that sees the reply before the lore is
/// grading a different task than one that sees the lore first.
fn render_evidence(input: &JudgedTurnInput) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str("CHARACTER PROFILE THE ENGINE USED:\n");
    out.push_str(input.character_card.trim());
    out.push_str("\n\nLORE THE ENGINE RETRIEVED FOR THIS TURN:\n");
    match input.retrieved_lore.as_deref().map(str::trim) {
        Some(lore) if !lore.is_empty() => out.push_str(lore),
        // Stated rather than omitted: "nothing was retrieved" is evidence
        // about the turn, and a judge shown no lore block cannot tell it
        // apart from a prompt that forgot to include one.
        _ => out.push_str("(nothing was retrieved for this turn)"),
    }
    out.push_str("\n\nWHAT THE PLAYER SAID:\n");
    out.push_str(input.player_input.trim());
    out.push_str("\n\nWHAT THE CHARACTER PRODUCED:\nNarration: ");
    out.push_str(blank_or(&input.narration));
    out.push_str("\nDialogue: ");
    out.push_str(blank_or(&input.dialogue));
    out.push('\n');
    out
}

fn blank_or(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "(empty)"
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scores(a: u8, b: u8, c: u8, d: u8) -> JudgeScores {
        JudgeScores {
            in_character: a,
            vault_grounded: b,
            progression: c,
            not_sycophantic: d,
            note: "Because of the line about the ledger.".into(),
        }
    }

    #[test]
    fn a_mean_needs_enough_turns_to_mean_anything() {
        let mut report = JudgeReport::new("judge-model", "minimal");
        for i in 0..MINIMUM_SAMPLES - 1 {
            report.push(i, scores(4, 4, 4, 4));
        }
        assert!(
            report.mean().is_none(),
            "a mean of {} turns is noise and must not be offered",
            MINIMUM_SAMPLES - 1
        );
        assert!(report.render().contains("no mean"));

        report.push(MINIMUM_SAMPLES - 1, scores(4, 4, 4, 4));
        assert_eq!(report.mean(), Some(4.0));
        assert_eq!(report.axis_means(), Some([4.0, 4.0, 4.0, 4.0]));
    }

    /// A judge that answers outside the rubric skews a mean silently unless
    /// something clamps it.
    #[test]
    fn scores_outside_the_rubric_are_clamped_not_averaged() {
        let out_of_range = scores(0, 9, 3, 5);
        assert_eq!(out_of_range.axes(), [1, 5, 3, 5]);
        assert_eq!(out_of_range.overall(), 3.5);
    }

    /// The positive control: a judge shown none of the evidence would score
    /// blind, and the report would look exactly the same. So assert the
    /// evidence is in the prompt.
    #[test]
    fn every_piece_of_evidence_reaches_the_judge() {
        let input = JudgedTurnInput {
            character_card: "CHARACTER PROFILE:\n- Name: Elara Voss".into(),
            retrieved_lore: Some("The archive at Thornwick leans north.".into()),
            player_input: "Who keeps the archive?".into(),
            narration: "She does not look up.".into(),
            dialogue: "I do. Nineteen years.".into(),
        };
        let evidence = render_evidence(&input);
        for expected in [
            "Elara Voss",
            "leans north",
            "Who keeps the archive?",
            "She does not look up.",
            "I do. Nineteen years.",
        ] {
            assert!(
                evidence.contains(expected),
                "the judge is never shown {expected:?}:\n{evidence}"
            );
        }
    }

    /// An empty response must reach the judge as an empty response, not as a
    /// missing section it could mistake for a well-formed turn.
    #[test]
    fn an_empty_field_is_shown_as_empty_rather_than_omitted() {
        let evidence = render_evidence(&JudgedTurnInput {
            character_card: "CHARACTER PROFILE:\n- Name: Bram Holt".into(),
            player_input: "Morning.".into(),
            ..Default::default()
        });
        assert!(evidence.contains("Narration: (empty)"), "{evidence}");
        assert!(evidence.contains("Dialogue: (empty)"), "{evidence}");
        assert!(evidence.contains("(nothing was retrieved"), "{evidence}");
    }

    /// The rubric has to be able to produce a low score, or it is a rubber
    /// stamp with four axes.
    #[test]
    fn the_rubric_asks_for_the_bottom_of_its_range() {
        let rubric = templates::judge_instructions();
        assert!(rubric.contains("1-5"));
        assert!(rubric.contains("A 3 is the honest middle"));
        assert!(
            rubric.contains("use 1 and 2 without hesitation"),
            "a rubric that never asks for a low score will not produce one"
        );
        for axis in [
            "IN CHARACTER",
            "VAULT GROUNDED",
            "PROGRESSION",
            "NOT SYCOPHANTIC",
        ] {
            assert!(rubric.contains(axis), "the rubric is missing {axis}");
        }
    }
}
