//! The play loop: commands, a turn, and the response as it arrives (§5.3).
//!
//! Built against [`TurnEngine`] and [`TurnEvent`] and nothing below them.
//! The shell never touches `InferenceBackend`, `MemoryManager` or
//! `EmotionEngine` individually — those exist under the engine precisely so a
//! shell does not have to know about them, and reaching past it is how
//! `GameLoopController` came to read six autoloads from one file.
//!
//! **Input and output are parameters, not `stdin` and `stdout`.** A [`Shell`]
//! reads lines from anything and writes to anything, which is what lets the
//! evaluation harness play a scripted transcript through the same code the
//! player uses rather than through a reimplementation of it that could drift.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use orison_core::turn::{
    CancelReason, FailureKind, Speaker, TurnEngine, TurnError, TurnEvent, TurnOutcome,
};

use crate::error::{advice, CliError};

/// One line the player typed, understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    /// Who can be spoken to from here.
    Who,
    /// Address somebody. Everything typed afterwards goes to them.
    Talk(String),
    /// Where the player is, and what leads away from it.
    Where,
    /// Travel (§5.4).
    Go(String),
    /// The turn machine's state, and the Director's.
    Status,
    /// Flush the playtime clock. Everything else is already committed.
    Save,
    Quit,
    /// Anything that is not a command is something the player said.
    Say(String),
}

/// Parse a typed line.
///
/// A leading `/` marks a command, so a player may say "go on, then" without
/// travelling. An unrecognised `/word` is a mistake worth naming rather than
/// something to pass to the model as dialogue.
pub fn parse_command(line: &str) -> Result<Command, String> {
    let trimmed = line.trim();
    let Some(rest) = trimmed.strip_prefix('/') else {
        return Ok(Command::Say(trimmed.to_string()));
    };
    let (word, argument) = match rest.split_once(char::is_whitespace) {
        Some((w, a)) => (w, a.trim()),
        None => (rest, ""),
    };
    let need = |what: &str| {
        if argument.is_empty() {
            Err(format!("/{word} needs {what}, for example: /{word} <name>"))
        } else {
            Ok(argument.to_string())
        }
    };
    match word.to_lowercase().as_str() {
        "help" | "?" => Ok(Command::Help),
        "who" => Ok(Command::Who),
        "talk" | "speak" => need("somebody to talk to").map(Command::Talk),
        "where" | "look" => Ok(Command::Where),
        "go" | "travel" => need("somewhere to go").map(Command::Go),
        "status" | "state" => Ok(Command::Status),
        "save" => Ok(Command::Save),
        "quit" | "exit" => Ok(Command::Quit),
        other => Err(format!(
            "no command called /{other}. /help lists them. To say it out loud, drop the slash."
        )),
    }
}

pub const HELP: &str = "\
  /who              who is here to talk to
  /talk <name>      address somebody
  /where            where you are, and what leads away
  /go <place>       travel there
  /status           what the engine is doing
  /save             flush the playtime clock (turns are already saved)
  /quit             leave
  anything else     is said out loud to whoever you are addressing";

/// A writer both the command handler and the streaming task hold.
type Sink = Arc<Mutex<dyn Write + Send>>;

pub struct Shell {
    engine: Arc<TurnEngine>,
    out: Sink,
    /// Whether the last thing written ended a line, so the prompt and the
    /// streamed text do not run together.
    started_at: std::time::Instant,
}

impl Shell {
    pub fn new(engine: Arc<TurnEngine>, out: impl Write + Send + 'static) -> Self {
        Self {
            engine,
            out: Arc::new(Mutex::new(out)),
            started_at: std::time::Instant::now(),
        }
    }

    pub fn engine(&self) -> &Arc<TurnEngine> {
        &self.engine
    }

    fn say(&self, line: impl AsRef<str>) {
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{}", line.as_ref());
        let _ = out.flush();
    }

    /// Read lines until the input ends or the player quits.
    ///
    /// Returns the outcome of every turn that ran, which is what makes a
    /// scripted run measurable.
    pub async fn run(&mut self, input: impl BufRead) -> Result<Vec<TurnOutcome>, CliError> {
        let mut outcomes = Vec::new();
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match self.handle(&line).await? {
                Step::Continue(Some(outcome)) => outcomes.push(*outcome),
                Step::Continue(None) => {}
                Step::Quit => break,
            }
            self.prompt();
        }
        self.flush_playtime()?;
        Ok(outcomes)
    }

    /// Write the input prompt, naming who is being addressed.
    pub fn prompt(&self) {
        let who = self
            .active_character_label()
            .unwrap_or_else(|| "nobody".to_string());
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = write!(out, "\n[{who}] > ");
        let _ = out.flush();
    }

    /// Everything a fresh session should know before the first prompt.
    pub fn greet(&self, model_summary: &str) -> Result<(), CliError> {
        let campaign = self.campaign_title()?;
        self.say(&campaign);
        self.say(format!("  {model_summary}"));
        match self.engine.current_location()? {
            Some(here) => self.say(format!("  You are at {}.", here.label)),
            None => self.say("  You are nowhere in particular. /where lists somewhere to start."),
        }
        match self.active_character_label() {
            Some(who) => self.say(format!("  You are speaking with {who}.")),
            None => self.say("  Nobody is being addressed. /who, then /talk <name>."),
        }
        self.say("  /help lists the commands.");
        Ok(())
    }

    async fn handle(&mut self, line: &str) -> Result<Step, CliError> {
        let command = match parse_command(line) {
            Ok(command) => command,
            Err(complaint) => {
                self.say(complaint);
                return Ok(Step::Continue(None));
            }
        };

        match command {
            Command::Help => self.say(HELP),
            Command::Quit => return Ok(Step::Quit),
            Command::Who => self.who()?,
            Command::Talk(name) => self.talk(&name)?,
            Command::Where => self.where_am_i()?,
            Command::Go(name) => self.go(&name)?,
            Command::Status => self.status(),
            Command::Save => {
                self.flush_playtime()?;
                self.say("Saved. (Every turn was already committed; this wrote the clock.)");
            }
            Command::Say(text) => {
                return Ok(Step::Continue(self.take_turn(text).await?.map(Box::new)))
            }
        }
        Ok(Step::Continue(None))
    }

    // ------------------------------------------------------------- commands

    fn who(&self) -> Result<(), CliError> {
        let present = self.engine.characters_present()?;
        if present.is_empty() {
            self.say("Nobody here. /where, then /go somewhere with people in it.");
            return Ok(());
        }
        let active = self.engine.session().with_store(|store| {
            Ok(store
                .load_campaign(self.engine.session().campaign_id())?
                .map(|c| c.active_character))
        })?;
        self.say("Here:");
        for entity in present {
            let marker = if active.as_deref() == Some(entity.id.as_str()) {
                " (speaking with)"
            } else {
                ""
            };
            self.say(format!("  {}{marker}", entity.label));
        }
        Ok(())
    }

    fn talk(&self, name: &str) -> Result<(), CliError> {
        let graph = self.engine.session().graph();
        let Some(entity) = graph.resolve(name).and_then(|id| graph.get(id)) else {
            self.say(format!("Nobody here is called {name:?}. /who lists them."));
            return Ok(());
        };
        let label = entity.label.clone();
        self.engine.select_character(entity.id.as_str())?;
        self.say(format!("You turn to {label}."));
        Ok(())
    }

    fn where_am_i(&self) -> Result<(), CliError> {
        match self.engine.current_location()? {
            Some(here) => {
                self.say(&here.label);
                for line in prose(here.description.trim()) {
                    self.say(format!("  {line}"));
                }
            }
            None => self.say("Nowhere in particular yet."),
        }
        let exits = self.engine.exits()?;
        if exits.is_empty() {
            self.say("  Nothing leads away from here.");
        } else {
            self.say("  From here:");
            for exit in exits {
                self.say(format!("    {}", exit.label));
            }
        }
        Ok(())
    }

    fn go(&self, name: &str) -> Result<(), CliError> {
        match self.engine.move_to_location(name) {
            Ok(arrived) => {
                self.say(format!("You travel to {}.", arrived.label));
                self.where_am_i()
            }
            // A refusal to travel is the player's to fix, not a failure of
            // the run: say which of the three it was and carry on.
            Err(TurnError::CannotTravel { detail, .. }) => {
                self.say(format!("You cannot: {detail}."));
                Ok(())
            }
            Err(other) => Err(other.into()),
        }
    }

    fn status(&self) {
        self.say(format!(
            "turn {:?} | director {:?} | {}",
            self.engine.state(),
            self.engine.director_state(),
            if self.engine.state().is_busy() {
                "busy"
            } else {
                "ready for input"
            }
        ));
    }

    // ----------------------------------------------------------- a turn

    /// Run one turn, rendering the response as it streams.
    ///
    /// The events are drained on their own task so display begins with the
    /// first decoded token rather than after the whole response parses —
    /// which is the difference between a 40-second wait and a 6-second one,
    /// and the only part of latency a person actually feels.
    async fn take_turn(&mut self, text: String) -> Result<Option<TurnOutcome>, CliError> {
        let events = self.engine.subscribe();
        let sink = Arc::clone(&self.out);
        let render = tokio::spawn(render_events(events, sink));

        let result = self.engine.player_input(text).await;

        // Let the renderer see `TurnCompleted` (or the failure) before the
        // next prompt is written, so output never interleaves.
        let _ = render.await;

        match result {
            Ok(outcome) => Ok(Some(outcome)),
            // Cancellation is not a failure to report: the player caused it
            // by acting again, and that turn is already on its way.
            Err(TurnError::Cancelled(reason)) => {
                if !matches!(reason, CancelReason::PlayerActedAgain) {
                    self.say(format!("Stopped: {reason}."));
                }
                Ok(None)
            }
            Err(e) => {
                let kind = FailureKind::from(&e);
                self.say(format!("\n{e}"));
                self.say(advice(kind));
                Ok(None)
            }
        }
    }

    // --------------------------------------------------------------- state

    fn campaign_title(&self) -> Result<String, CliError> {
        let id = self.engine.session().campaign_id().to_string();
        Ok(self
            .engine
            .session()
            .with_store(|store| store.load_campaign(&id))?
            .map(|c| c.title)
            .unwrap_or(id))
    }

    fn active_character_label(&self) -> Option<String> {
        let id = self.engine.session().campaign_id().to_string();
        let active = self
            .engine
            .session()
            .with_store(|store| store.load_campaign(&id))
            .ok()??
            .active_character;
        if active.trim().is_empty() {
            return None;
        }
        let graph = self.engine.session().graph();
        Some(
            graph
                .get(&orison_core::knowledge::EntityId::from_stored(&active))
                .map(|e| e.label.clone())
                .unwrap_or(active),
        )
    }

    /// The one thing a turn does not already persist.
    fn flush_playtime(&mut self) -> Result<(), CliError> {
        let id = self.engine.session().campaign_id().to_string();
        let elapsed = self.started_at.elapsed().as_secs_f64();
        self.started_at = std::time::Instant::now();
        self.engine.session().with_store(|store| {
            if let Some(mut campaign) = store.load_campaign(&id)? {
                campaign.playtime_seconds += elapsed;
                campaign.last_played = timestamp();
                store.save_campaign(&campaign)?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

/// Boxed because a `TurnOutcome` is several hundred bytes and `Quit` is
/// none: the enum is returned from every command, and most of them are not
/// turns.
enum Step {
    Continue(Option<Box<TurnOutcome>>),
    Quit,
}

/// Render the turn's events until it ends.
///
/// Matches on the enum rather than binding fourteen signals, so a variant
/// added to `TurnEvent` is a compile error here instead of an event nobody
/// renders (§3.1).
async fn render_events(mut events: tokio::sync::broadcast::Receiver<TurnEvent>, out: Sink) {
    use tokio::sync::broadcast::error::RecvError;

    let write = |s: String| {
        let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = write!(out, "{s}");
        let _ = out.flush();
    };

    // Which fields this turn already showed token by token. `Message`
    // arrives afterwards carrying the same text, so the set — not "is a
    // field open right now", which is false by then — decides whether it is
    // a repeat or the first sight of it.
    let mut streamed: Vec<&'static str> = Vec::new();
    loop {
        match events.recv().await {
            Ok(TurnEvent::StreamStarted { speaker, field }) => {
                streamed.push(field);
                write(format!("\n{}", prefix(&speaker, field)));
            }
            Ok(TurnEvent::StreamDelta { text }) => write(text),
            Ok(TurnEvent::StreamEnded { .. }) => write("\n".to_string()),
            // The complete line, after parsing. Printed only if the stream
            // never reached it — a response whose watched field never opened
            // is still shown rather than becoming the awkward silence.
            Ok(TurnEvent::Message { speaker, text }) => {
                let field = match speaker {
                    Speaker::Narrator => "narration",
                    _ => "dialogue",
                };
                if !streamed.contains(&field) && !text.trim().is_empty() {
                    write(format!("{}{text}\n", prefix(&speaker, field)));
                }
            }
            Ok(TurnEvent::SceneBeatApplied { choices, dice_roll }) => {
                let mut block = String::new();
                if dice_roll.required {
                    let reason = dice_roll
                        .reason
                        .as_deref()
                        .unwrap_or("a check is called for");
                    let dc = dice_roll
                        .dc
                        .map(|dc| format!(" (DC {dc})"))
                        .unwrap_or_default();
                    block.push_str(&format!("\n  [{}{dc}]\n", reason.trim()));
                }
                for (i, choice) in choices.iter().enumerate() {
                    block.push_str(&format!("  {}. {}\n", i + 1, choice.text));
                }
                if !block.is_empty() {
                    write(block);
                }
            }
            Ok(TurnEvent::LocationChanged { label, .. }) => write(format!("\n  — {label} —\n")),
            Ok(TurnEvent::SystemMessage { text }) => write(format!("\n  {text}\n")),
            Ok(TurnEvent::Failed { kind, detail }) => {
                if kind.is_reportable() {
                    write(format!("\n  {detail}\n  {}\n", advice(kind)));
                }
            }
            Ok(TurnEvent::TurnCompleted) => return,
            // Everything else is state a terminal has nothing to draw for.
            Ok(TurnEvent::StateChanged(_))
            | Ok(TurnEvent::DirectorStateChanged(_))
            | Ok(TurnEvent::PlayerMessage { .. })
            | Ok(TurnEvent::EmotionChanged { .. })
            | Ok(TurnEvent::ReactionRequested { .. })
            | Ok(TurnEvent::Escalation(_))
            | Ok(TurnEvent::MemoryUpdated { .. }) => {}
            // A slow renderer that fell behind has lost lines. Say so: a gap
            // in a transcript that is not marked is worse than one that is.
            Err(RecvError::Lagged(n)) => write(format!("\n  [{n} lines dropped]\n")),
            Err(RecvError::Closed) => return,
        }
    }
}

/// Vault text as a terminal should show it: wiki-link syntax unwrapped to the
/// name it displays, and one entry per line so the caller can indent.
///
/// The brackets are Obsidian's, not the author's prose, and the exits are
/// already listed separately — printing `[[Thornwick Archive]]` shows the
/// player markup and tells them nothing the next three lines do not.
fn prose(text: &str) -> Vec<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        match after.find("]]") {
            Some(close) => {
                let inner = &after[..close];
                // `[[Target|Alias]]` displays the alias; `[[Target#Anchor]]`
                // displays the target.
                let shown = inner
                    .split_once('|')
                    .map(|(_, alias)| alias)
                    .unwrap_or_else(|| inner.split('#').next().unwrap_or(inner));
                out.push_str(shown.trim());
                rest = &after[close + 2..];
            }
            // An unclosed `[[` is the author's text, not a link.
            None => {
                out.push_str("[[");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn prefix(speaker: &Speaker, field: &str) -> String {
    match (speaker, field) {
        (Speaker::Character(_), "dialogue") => String::new(),
        (Speaker::Narrator, _) | (_, "narration") => String::new(),
        (Speaker::Player, _) => "> ".to_string(),
        (Speaker::System, _) => "  ".to_string(),
        _ => String::new(),
    }
}

/// RFC 3339, seconds resolution, from the system clock.
///
/// `orison-core` never reads a wall clock (it takes one), so the shell is
/// where a real timestamp is allowed to come from.
pub fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Civil date from a Unix timestamp, by Howard Hinnant's algorithm. Small
    // enough to write, and smaller than the dependency that would replace it.
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}
