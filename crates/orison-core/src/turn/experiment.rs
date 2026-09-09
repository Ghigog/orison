//! Scoring a transcript, for the §4.2 experiment.
//!
//! The handoff is explicit that the Director/Actor split is to be decided by
//! measurement — "do not preserve the split out of sunk cost and do not
//! collapse it out of enthusiasm" — so the arms need something to be scored
//! on. These are the same five metrics `eval/EvalRunnerNode.gd` records for
//! its transcript suite, ported so the numbers are comparable with the Godot
//! narrative baseline in `docs/eval_baseline.md` rather than merely
//! plausible.
//!
//! **What this is not.** It is not the judge suite. That scores narrative
//! coherence, character consistency, plot progression and absence of
//! sycophancy on a 1-5 rubric with a model, and it is Phase 5's job. These
//! are deterministic proxies: they catch the failures that have actually
//! happened here (Bug 3's pronouns, the conversation loops, leaked stats) and
//! they cannot tell you whether a scene was any good. `quality_per_second`
//! says so in its own documentation, because a composite number is exactly
//! the kind of thing that gets quoted without its caveats.

use std::collections::HashMap;
use std::time::Duration;

use super::engine::TurnOutcome;

/// A scripted conversation, as `ground_truth.json` records one.
#[derive(Debug, Clone)]
pub struct TranscriptScript {
    /// The character the player is talking to, by label.
    pub character: String,
    /// Pronouns that must not appear in *narration* about this character.
    pub forbidden_pronouns: Vec<String>,
    pub lines: Vec<String>,
}

/// One turn's worth of scored output.
#[derive(Debug, Clone)]
pub struct ScoredTurn {
    pub index: usize,
    /// `None` when the response did not parse at all.
    pub latency: Option<Duration>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub parsed: bool,
    pub findings: Vec<Finding>,
}

/// Something wrong with one turn, named so a reader can act on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// The response did not parse against its schema. Under constrained
    /// decoding (§2.4) this should be unreachable; if it fires, that claim is
    /// wrong and it matters more than anything else in the report.
    SchemaInvalid(String),
    /// Narration used a pronoun the character's gender rules out
    /// ([rag_architecture.md] Bug 3).
    ///
    /// **Read flagged turns by hand.** The metric cannot tell a wrong pronoun
    /// for the speaker from a right one for a third party; the Godot baseline
    /// records exactly one flag and believes it to be a false positive.
    ///
    /// [rag_architecture.md]: ../../../../docs/rag_architecture.md
    PronounFlag { pronoun: String, narration: String },
    /// Dialogue repeated verbatim from an earlier turn.
    VerbatimRepeat { of_turn: usize },
    /// Third-person self-reference, or a leaked affinity score.
    ForbiddenPhrasing(String),
    /// Neither dialogue nor narration. The awkward silence.
    EmptyResponse,
}

/// One arm's results.
#[derive(Debug, Clone)]
pub struct ArmReport {
    pub arm: String,
    pub turns: Vec<ScoredTurn>,
}

impl ArmReport {
    pub fn new(arm: impl Into<String>) -> Self {
        Self {
            arm: arm.into(),
            turns: Vec::new(),
        }
    }

    pub fn attempted(&self) -> usize {
        self.turns.len()
    }

    pub fn parsed(&self) -> usize {
        self.turns.iter().filter(|t| t.parsed).count()
    }

    /// Fraction of turns whose response parsed against its schema.
    pub fn schema_validity(&self) -> f64 {
        ratio(self.parsed(), self.attempted())
    }

    pub fn findings(&self, matching: impl Fn(&Finding) -> bool) -> usize {
        self.turns
            .iter()
            .flat_map(|t| t.turns_findings())
            .filter(|f| matching(f))
            .count()
    }

    pub fn pronoun_flags(&self) -> usize {
        self.findings(|f| matches!(f, Finding::PronounFlag { .. }))
    }

    pub fn verbatim_repeats(&self) -> usize {
        self.findings(|f| matches!(f, Finding::VerbatimRepeat { .. }))
    }

    pub fn forbidden_phrasing(&self) -> usize {
        self.findings(|f| matches!(f, Finding::ForbiddenPhrasing(_)))
    }

    pub fn empty_responses(&self) -> usize {
        self.findings(|f| matches!(f, Finding::EmptyResponse))
    }

    /// Latencies of the turns that produced a response, sorted.
    fn latencies_ms(&self) -> Vec<f64> {
        let mut all: Vec<f64> = self
            .turns
            .iter()
            .filter_map(|t| t.latency)
            .map(|d| d.as_secs_f64() * 1000.0)
            .collect();
        all.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        all
    }

    pub fn latency_p50_ms(&self) -> Option<f64> {
        percentile(&self.latencies_ms(), 0.50)
    }

    /// Phase 5's migration gate is p95, so it is recorded from the start.
    pub fn latency_p95_ms(&self) -> Option<f64> {
        percentile(&self.latencies_ms(), 0.95)
    }

    pub fn mean_latency_secs(&self) -> Option<f64> {
        let all = self.latencies_ms();
        if all.is_empty() {
            return None;
        }
        Some(all.iter().sum::<f64>() / all.len() as f64 / 1000.0)
    }

    /// A composite in `[0, 1]`: the mean of schema validity and the three
    /// clean rates.
    ///
    /// Every component is a *failure* rate subtracted from one, so a
    /// transcript with nothing wrong scores 1.0 and the score can only be
    /// lowered by an observed defect. It says nothing about whether the
    /// writing was good; see the module note.
    pub fn quality(&self) -> f64 {
        let n = self.attempted();
        if n == 0 {
            return 0.0;
        }
        let clean = |count: usize| 1.0 - ratio(count, n);
        (self.schema_validity()
            + clean(self.pronoun_flags())
            + clean(self.verbatim_repeats())
            + clean(self.forbidden_phrasing() + self.empty_responses()))
            / 4.0
    }

    /// What the arms are actually compared on: quality per second of wall
    /// clock. An arm that is twice as good and four times slower loses.
    pub fn quality_per_second(&self) -> Option<f64> {
        self.mean_latency_secs()
            .filter(|s| *s > 0.0)
            .map(|s| self.quality() / s)
    }

    pub fn total_prompt_tokens(&self) -> usize {
        self.turns.iter().map(|t| t.prompt_tokens).sum()
    }

    pub fn total_completion_tokens(&self) -> usize {
        self.turns.iter().map(|t| t.completion_tokens).sum()
    }

    /// One row of the comparison table.
    pub fn row(&self) -> String {
        format!(
            "| {:<28} | {:>5}/{:<3} | {:>3} | {:>3} | {:>3} | {:>9} | {:>9} | {:>7.3} | {:>8} |",
            self.arm,
            self.parsed(),
            self.attempted(),
            self.pronoun_flags(),
            self.verbatim_repeats(),
            self.forbidden_phrasing() + self.empty_responses(),
            self.latency_p50_ms()
                .map(|v| format!("{v:.0} ms"))
                .unwrap_or_else(|| "-".into()),
            self.latency_p95_ms()
                .map(|v| format!("{v:.0} ms"))
                .unwrap_or_else(|| "-".into()),
            self.quality(),
            self.quality_per_second()
                .map(|v| format!("{v:.4}"))
                .unwrap_or_else(|| "-".into()),
        )
    }

    pub fn header() -> String {
        format!(
            "| {:<28} | {:<9} | {:>3} | {:>3} | {:>3} | {:>9} | {:>9} | {:>7} | {:>8} |\n\
             |{:-<30}|{:-<11}|{:-<5}|{:-<5}|{:-<5}|{:-<11}|{:-<11}|{:-<9}|{:-<10}|",
            "arm",
            "parsed",
            "prn",
            "rep",
            "bad",
            "p50",
            "p95",
            "quality",
            "q/sec",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            ""
        )
    }
}

impl ScoredTurn {
    fn turns_findings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter()
    }
}

/// Scores one turn's response against the script's rules.
///
/// `seen` carries dialogue already said, so verbatim repeats across turns are
/// detectable; pass the same map through a whole transcript.
pub fn score_turn(
    index: usize,
    script: &TranscriptScript,
    outcome: &TurnOutcome,
    seen: &mut HashMap<String, usize>,
) -> ScoredTurn {
    let mut findings = Vec::new();
    let narration = outcome.response.narration.trim();
    let dialogue = outcome.response.dialogue.trim();

    // Pronouns: narration only, and word-bounded.
    //
    // Two false-positive traps the Godot metric fell into and this must not:
    // "she was" contains "he ", so substring matching flags every correctly
    // gendered line; and dialogue is the character *speaking*, where naming a
    // third party of another gender is simply correct.
    let lower_narration = narration.to_lowercase();
    for pronoun in &script.forbidden_pronouns {
        if contains_word(&lower_narration, &pronoun.to_lowercase()) {
            findings.push(Finding::PronounFlag {
                pronoun: pronoun.clone(),
                narration: narration.to_string(),
            });
        }
    }

    let normalised = dialogue.to_lowercase();
    if !normalised.is_empty() {
        match seen.get(&normalised) {
            Some(of_turn) => findings.push(Finding::VerbatimRepeat { of_turn: *of_turn }),
            None => {
                seen.insert(normalised, index);
            }
        }
    }

    let lower_dialogue = dialogue.to_lowercase();
    let self_reference = format!("{} says", script.character.to_lowercase());
    if contains_word(&lower_dialogue, &self_reference) {
        findings.push(Finding::ForbiddenPhrasing(
            "third-person self-reference in dialogue".to_string(),
        ));
    }
    if lower_dialogue.contains("affinity") {
        findings.push(Finding::ForbiddenPhrasing(
            "leaks the affinity score".to_string(),
        ));
    }

    if narration.is_empty() && dialogue.is_empty() {
        findings.push(Finding::EmptyResponse);
    }

    ScoredTurn {
        index,
        latency: Some(outcome.latency),
        prompt_tokens: outcome.prompt_tokens,
        completion_tokens: outcome.completion_tokens,
        parsed: true,
        findings,
    }
}

/// A turn whose response never parsed.
pub fn failed_turn(index: usize, detail: impl Into<String>) -> ScoredTurn {
    ScoredTurn {
        index,
        latency: None,
        prompt_tokens: 0,
        completion_tokens: 0,
        parsed: false,
        findings: vec![Finding::SchemaInvalid(detail.into())],
    }
}

/// Whole-word containment.
///
/// `"she was"` contains `"he "` as a substring, which is why the Godot
/// harness needed the same function: without it the pronoun metric fires on
/// every correctly gendered line.
pub fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(at) = haystack[from..].find(needle) {
        let start = from + at;
        let end = start + needle.len();
        let before_ok = start == 0
            || !haystack[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '\'');
        let after_ok = end == haystack.len()
            || !haystack[end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '\'');
        if before_ok && after_ok {
            return true;
        }
        from = start + needle.chars().next().map_or(1, char::len_utf8);
        if from >= haystack.len() {
            break;
        }
    }
    false
}

fn ratio(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64).clamp(0.0, 1.0)
    }
}

fn percentile(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() as f64) * q) as usize;
    Some(sorted[index.min(sorted.len() - 1)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::schemas::{CharacterResponse, Emotion, EmotionalUpdate, EscalationSignal};

    fn script() -> TranscriptScript {
        TranscriptScript {
            character: "Lord Anneke".to_string(),
            forbidden_pronouns: vec!["she".into(), "her".into(), "hers".into()],
            lines: vec!["Good evening.".to_string()],
        }
    }

    fn outcome(narration: &str, dialogue: &str) -> TurnOutcome {
        TurnOutcome {
            speaker: "anneke".to_string(),
            response: CharacterResponse {
                thinking: "…".into(),
                narration: narration.into(),
                dialogue: dialogue.into(),
                emotional_update: EmotionalUpdate {
                    emotion: Emotion::Serenity,
                    intensity: 0.5,
                    reason: "…".into(),
                    rapport_delta: 0.0,
                },
                escalation_signal: EscalationSignal::None,
            },
            latency: Duration::from_millis(1000),
            time_to_first_token: Some(Duration::from_millis(200)),
            prompt_tokens: 100,
            evaluated_prompt_tokens: Some(100),
            completion_tokens: 50,
            retrieved: 3,
            director_triggered: false,
            beat: None,
        }
    }

    /// The Phase 1 lesson as a test: every metric must be able to fire, or
    /// a clean report means nothing.
    #[test]
    fn every_metric_fires_on_output_that_should_trigger_it() {
        let script = script();
        let mut seen = HashMap::new();
        let mut report = ArmReport::new("self-test");

        report.turns.push(score_turn(
            0,
            &script,
            &outcome("She adjusts her cloak.", "Come in."),
            &mut seen,
        ));
        report
            .turns
            .push(score_turn(1, &script, &outcome("", "Come in."), &mut seen));
        report.turns.push(score_turn(
            2,
            &script,
            &outcome("", "Lord Anneke says the road is closed. Affinity holds."),
            &mut seen,
        ));
        report
            .turns
            .push(score_turn(3, &script, &outcome("", ""), &mut seen));
        report.turns.push(failed_turn(4, "did not parse"));

        assert_eq!(report.pronoun_flags(), 2, "'she' and 'her' both flag");
        assert_eq!(report.verbatim_repeats(), 1, "turn 1 repeats turn 0");
        assert_eq!(
            report.forbidden_phrasing(),
            2,
            "self-reference and affinity"
        );
        assert_eq!(report.empty_responses(), 1);
        assert_eq!(report.parsed(), 4);
        assert!(report.schema_validity() < 1.0);
        assert!(report.quality() < 1.0);
    }

    #[test]
    fn a_clean_transcript_scores_one() {
        let script = script();
        let mut seen = HashMap::new();
        let mut report = ArmReport::new("clean");
        for (i, dialogue) in ["Come in.", "The road is closed.", "Ask at the Landing."]
            .iter()
            .enumerate()
        {
            report.turns.push(score_turn(
                i,
                &script,
                &outcome("He inclines his head.", dialogue),
                &mut seen,
            ));
        }
        assert_eq!(report.quality(), 1.0);
        // One second per turn, so quality per second is quality.
        assert_eq!(report.quality_per_second(), Some(1.0));
    }

    /// The false positive the Godot metric originally had, kept as a test so
    /// the port cannot reintroduce it.
    #[test]
    fn a_correctly_gendered_line_does_not_flag_on_a_substring() {
        // "she was" contains "he ", and "her" appears inside "other".
        assert!(!contains_word("she was other than expected", "he"));
        assert!(!contains_word("the others waited", "her"));
        assert!(contains_word("she waited", "she"));
        assert!(contains_word("it was hers", "hers"));
        assert!(contains_word("her cloak", "her"));
    }

    #[test]
    fn quality_per_second_prefers_the_faster_arm_when_quality_ties() {
        let script = script();
        let mut fast = ArmReport::new("fast");
        let mut slow = ArmReport::new("slow");
        let mut seen_a = HashMap::new();
        let mut seen_b = HashMap::new();

        let mut turn = score_turn(0, &script, &outcome("He nods.", "Aye."), &mut seen_a);
        turn.latency = Some(Duration::from_millis(500));
        fast.turns.push(turn);

        let mut turn = score_turn(0, &script, &outcome("He nods.", "Aye."), &mut seen_b);
        turn.latency = Some(Duration::from_millis(4000));
        slow.turns.push(turn);

        assert_eq!(fast.quality(), slow.quality());
        assert!(fast.quality_per_second() > slow.quality_per_second());
    }
}
