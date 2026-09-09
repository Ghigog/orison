//! The turn loop (§4.1).
//!
//! One player turn, end to end: sanitise, log, retrieve, assemble, call the
//! Actor, apply what came back, and decide whether the Director should compose
//! the next beat. `GameLoopController.gd` does the same work across nine
//! callbacks and five booleans; the shape of the port is the point of it.
//!
//! Three properties this has and that does not:
//!
//! - **A turn is one future.** Not a chain of callbacks that mutate shared
//!   flags from whatever context they happen to run in.
//! - **Cancelling stops the model.** Dropping the future drops the response
//!   stream, which closes the connection. See [`super::queue`].
//! - **The machine cannot be left mid-turn.** State is restored by a guard on
//!   drop, so a cancelled turn cannot strand the engine in `Streaming` — the
//!   failure mode a `finally`-less callback chain has by construction.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures::StreamExt;
use tokio::sync::broadcast;

use crate::emotion::{EmotionEngine, EmotionState};
use crate::inference::{
    ChatMessage, ChatRequest, InferenceBackend, InferenceError, ResponseFormat,
};
use crate::knowledge::{CanonicalField, EntityId, EntityKind};
use crate::memory::{CompactionPlan, DistillationPlan, MemoryManager};
use crate::prompt::assembly::{
    player_message, CharacterCard, DirectorPrompt, PlayerCard, TurnPrompt, WorldSnapshot,
};
use crate::prompt::budget::{allocate, check_overflow, count_tokens, PromptBudget};
use crate::prompt::player_input::sanitize as sanitize_player_input;
use crate::prompt::schemas::{
    BaselineDisposition, CharacterResponse, CombinedTurnResponse, DirectorResponse,
    DistilledMemoryResponse, EscalationSignal, InventoryAction, SessionSummaryResponse,
};
use crate::prompt::templates::Speech;
use crate::prompt::PromptSections;
use crate::retrieval::{format_context, retrieve, NoRerank, PassageReranker, Reranker};
use crate::state::{Campaign, HistoryEntry, HistoryRole, InventoryItem};

use super::config::{TurnConfig, TurnProfile};
use super::error::{CancelReason, FailureKind, TurnError};
use super::event::{Speaker, TurnEvent};
use super::queue::{CancelToken, Priority, RequestQueue, Ticket};
use super::session::Session;
use super::state::{DirectorState, TurnState};
use super::stream::{FieldStreamer, StreamPiece};

/// What one turn produced.
///
/// `latency` is here rather than logged because it is the number Phase 4 owes
/// `eval_baseline.md`: the Godot baseline is ~20 s p50 per conversational turn
/// and cache-stable ordering (§2.7) is supposed to have fixed it. The
/// precondition is unit-tested; this is what makes the before/after
/// measurable.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    pub speaker: String,
    pub response: CharacterResponse,
    /// Wall clock from accepting the input to the response being applied.
    pub latency: Duration,
    /// Wall clock to the first display token the player could see. The half
    /// of latency that a person actually feels.
    pub time_to_first_token: Option<Duration>,
    pub prompt_tokens: usize,
    /// Prompt tokens the backend reported evaluating, when it reports them.
    ///
    /// Read against `prompt_tokens`: the two are close when the backend
    /// re-processed the whole prompt and far apart when it served most of it
    /// from a cached prefix. This is what §2.7's ordering work is for, and
    /// measuring it directly is how Phase 5 tells a cache hit from a fast
    /// machine — `docs/eval_baseline.md`'s Phase 4 run had only
    /// time-to-first-token to go on and could conclude nothing from it.
    pub evaluated_prompt_tokens: Option<usize>,
    pub completion_tokens: usize,
    pub retrieved: usize,
    pub director_triggered: bool,
    /// The world-state half of the response, on the single-call arm. `None`
    /// on the two-call arm, where the Director composes it separately.
    pub beat: Option<DirectorResponse>,
}

struct Machine {
    turn: TurnState,
    director: DirectorState,
}

pub struct TurnEngine {
    session: Session,
    actor: Arc<dyn InferenceBackend>,
    director: Arc<dyn InferenceBackend>,
    config: TurnConfig,
    emotion: EmotionEngine,
    memory: MemoryManager,
    queue: Arc<RequestQueue>,
    events: broadcast::Sender<TurnEvent>,
    machine: Mutex<Machine>,
    /// The turn currently in flight, so new player input can stop it.
    in_flight: Mutex<Option<CancelToken>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl TurnEngine {
    pub fn new(
        session: Session,
        actor: Arc<dyn InferenceBackend>,
        director: Arc<dyn InferenceBackend>,
        config: TurnConfig,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(256);
        Arc::new(Self {
            session,
            actor,
            director,
            emotion: EmotionEngine::new(config.emotion),
            memory: MemoryManager::new(config.memory),
            config,
            queue: RequestQueue::spawn(),
            events,
            machine: Mutex::new(Machine {
                turn: TurnState::Idle,
                director: DirectorState::Idle,
            }),
            in_flight: Mutex::new(None),
        })
    }

    /// Every turn event from now on. Late subscribers miss earlier events;
    /// the transcript is in the database, not in this channel.
    pub fn subscribe(&self) -> broadcast::Receiver<TurnEvent> {
        self.events.subscribe()
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn config(&self) -> &TurnConfig {
        &self.config
    }

    pub fn emotion(&self) -> &EmotionEngine {
        &self.emotion
    }

    pub fn memory(&self) -> &MemoryManager {
        &self.memory
    }

    pub fn queue(&self) -> &Arc<RequestQueue> {
        &self.queue
    }

    pub fn state(&self) -> TurnState {
        lock(&self.machine).turn
    }

    pub fn director_state(&self) -> DirectorState {
        lock(&self.machine).director
    }

    /// Accept player input and run a turn.
    ///
    /// If a turn is already in flight it is cancelled first, for real: the
    /// in-flight request stops rather than continuing to generate output that
    /// will be discarded ([orison_audit.md §16]).
    ///
    /// [orison_audit.md §16]: ../../../../docs/orison_audit.md
    pub fn submit_player_input(self: &Arc<Self>, text: impl Into<String>) -> Ticket<TurnOutcome> {
        self.cancel_current_turn(CancelReason::PlayerActedAgain);

        let engine = Arc::clone(self);
        let text = text.into();
        let ticket = self
            .queue
            .submit("player-turn", Priority::High, move |cancel| async move {
                engine.run_turn(text, cancel).await
            });
        *lock(&self.in_flight) = Some(ticket.token());
        ticket
    }

    /// Submit and wait. The convenient form for a caller that has nothing
    /// else to do, which is every test and the Phase 5 CLI.
    pub async fn player_input(
        self: &Arc<Self>,
        text: impl Into<String>,
    ) -> Result<TurnOutcome, TurnError> {
        self.submit_player_input(text).join().await
    }

    /// Stop the turn in flight, if any.
    pub fn cancel_current_turn(&self, reason: CancelReason) {
        if let Some(token) = lock(&self.in_flight).take() {
            token.cancel(reason);
        }
    }

    /// Change who the player is talking to.
    ///
    /// The Godot `select_character` emitted `character_visual_update_requested`
    /// with the character's *old* emotion, which was the third of RAG003's
    /// three reaction generations per turn. This emits the current state once
    /// and requests no reaction: selecting somebody is not an emotional event.
    ///
    /// It also queues the baseline deduction when one is missing, at low
    /// priority, so it yields to the next player turn.
    pub fn select_character(
        self: &Arc<Self>,
        entity_id: impl Into<String>,
    ) -> Result<(), TurnError> {
        let entity_id = entity_id.into();
        let mut campaign = self.load_campaign()?;
        campaign.active_character = entity_id.clone();
        self.session
            .with_store(|store| store.save_campaign(&campaign))?;

        let campaign_id = self.session.campaign_id().to_string();
        let (state, affinity, needs_baseline) = self.session.with_store(|store| {
            let state = self.emotion.current(store, &campaign_id, &entity_id)?;
            let affinity = store
                .character_state(&campaign_id, &entity_id)?
                .map(|s| s.affinity)
                .unwrap_or(0.0);
            let needs = self
                .emotion
                .needs_baseline(store, &campaign_id, &entity_id)?;
            Ok((state, affinity, needs))
        })?;

        self.emit(TurnEvent::EmotionChanged {
            entity_id: entity_id.clone(),
            emotion: state.emotion,
            intensity: state.intensity,
            affinity,
        });
        if needs_baseline {
            self.spawn_baseline_deduction(entity_id);
        }
        Ok(())
    }

    /// Deduce a character's resting disposition from their biography.
    ///
    /// Background work, so a player turn preempts it. A character with no
    /// biography gets the neutral baseline written directly rather than a
    /// model call that could only guess.
    fn spawn_baseline_deduction(self: &Arc<Self>, entity_id: String) {
        let id = EntityId::from_stored(&entity_id);
        let Some(entity) = self.session.graph().get(&id) else {
            return;
        };
        let label = entity.label.clone();
        let Some(biography) = entity.field(CanonicalField::Biography).map(str::to_string) else {
            let campaign_id = self.session.campaign_id().to_string();
            let timestamp = self.session.clock().timestamp();
            let _ = self.session.with_store(|store| {
                self.emotion.set_baseline(
                    store,
                    &campaign_id,
                    &entity_id,
                    &BaselineDisposition {
                        base_emotion: crate::prompt::schemas::Emotion::Serenity,
                        base_intensity: 0.5,
                        reason: "Default baseline: no biography to deduce one from.".to_string(),
                    },
                    &timestamp,
                )
            });
            return;
        };

        let engine = Arc::clone(self);
        let ticket = self.queue.submit(
            "baseline-emotion",
            Priority::Low,
            move |cancel| async move {
                let request = ChatRequest {
                    sampling: engine.config.director_sampling.clone(),
                    keep_alive: engine.config.keep_alive,
                    ..ChatRequest::new(vec![
                        ChatMessage::system(BASELINE_INSTRUCTIONS),
                        ChatMessage::user(format!("Character: {label}\nBiography: {biography}")),
                    ])
                    .with_response_format(ResponseFormat::for_type::<BaselineDisposition>())
                };
                let response = tokio::select! {
                    biased;
                    reason = cancel.cancelled() => return Err(TurnError::Cancelled(reason)),
                    result = engine.actor.chat(request) => result?,
                };
                let deduced: BaselineDisposition = response.parse()?;
                let campaign_id = engine.session.campaign_id().to_string();
                let timestamp = engine.session.clock().timestamp();
                engine.session.with_store(|store| {
                    engine.emotion.set_baseline(
                        store,
                        &campaign_id,
                        &entity_id,
                        &deduced,
                        &timestamp,
                    )
                })
            },
        );
        drop(ticket);
    }

    /// Stop everything and stop accepting work.
    pub fn shutdown(&self) {
        self.cancel_current_turn(CancelReason::Shutdown);
        self.queue.close();
    }

    fn emit(&self, event: TurnEvent) {
        // No subscribers is normal — a headless run has none — and is not a
        // failure to report.
        let _ = self.events.send(event);
    }

    fn transition(&self, to: TurnState) -> Result<(), TurnError> {
        {
            let mut machine = lock(&self.machine);
            if !machine.turn.can_advance_to(to) {
                return Err(TurnError::IllegalTransition {
                    from: machine.turn,
                    to,
                });
            }
            machine.turn = to;
        }
        self.emit(TurnEvent::StateChanged(to));
        Ok(())
    }

    fn set_director_state(&self, to: DirectorState) {
        {
            let mut machine = lock(&self.machine);
            if !machine.director.can_advance_to(to) {
                return;
            }
            machine.director = to;
        }
        self.emit(TurnEvent::DirectorStateChanged(to));
    }

    /// Force the turn machine back to `Idle` from wherever it is.
    ///
    /// Only [`TurnGuard`] calls this, and only on the way out of a turn.
    fn force_idle(&self) {
        let changed = {
            let mut machine = lock(&self.machine);
            let was = machine.turn;
            machine.turn = TurnState::Idle;
            was != TurnState::Idle
        };
        if changed {
            self.emit(TurnEvent::StateChanged(TurnState::Idle));
        }
    }

    async fn run_turn(
        self: Arc<Self>,
        raw_input: String,
        cancel: CancelToken,
    ) -> Result<TurnOutcome, TurnError> {
        // Restores the machine however this future ends, including when it is
        // dropped mid-await by cancellation. A trailing statement would not
        // run in that case, which is precisely the case that matters.
        let _guard = TurnGuard {
            engine: Arc::clone(&self),
        };

        let result = self.turn_body(&raw_input, &cancel).await;
        match &result {
            Ok(_) => self.emit(TurnEvent::TurnCompleted),
            Err(error) => {
                let kind = FailureKind::from(error);
                if kind.is_reportable() {
                    self.emit(TurnEvent::Failed {
                        kind,
                        detail: error.to_string(),
                    });
                }
            }
        }
        result
    }

    async fn turn_body(
        self: &Arc<Self>,
        raw_input: &str,
        cancel: &CancelToken,
    ) -> Result<TurnOutcome, TurnError> {
        let started = Instant::now();

        let text = sanitize_player_input(raw_input);
        if text.trim().is_empty() {
            return Err(TurnError::EmptyInput);
        }

        self.transition(TurnState::Preparing)?;

        let campaign = self.load_campaign()?;
        let speaker = campaign.active_character.clone();
        if speaker.trim().is_empty() {
            return Err(TurnError::NoActiveCharacter);
        }

        // Read the transcript before the new line joins it: the player's
        // input is the volatile tail of the prompt, not part of its history
        // (§2.7).
        let prior = self
            .session
            .with_store(|store| store.recent_history(&campaign.id, self.config.history_window))?;

        self.append_history(HistoryRole::Player, &text, Some("player"), Some(&speaker))?;
        self.emit(TurnEvent::PlayerMessage { text: text.clone() });

        // Turn accounting, before the model call so a cancelled turn still
        // counts as a turn. The Godot build does the same, deliberately.
        self.advance_turn_counters(&campaign)?;
        self.decay_bystanders(&speaker, "Emotional decay over time.")?;

        let budget = PromptBudget::new(self.actor.context_length(), self.config.response_reserve);
        let allocation = allocate(&budget, self.config.fractions)?;

        let lore = self.retrieve_lore(
            &text,
            &self.config.actor_retrieval,
            allocation.lore,
            &PassageReranker::default(),
        )?;

        let sections = self.assemble_actor_sections(&campaign, &speaker, &text, &lore, prior)?;
        let messages = sections.into_messages();

        let prompt_tokens = self.count_messages(&messages)?;
        check_overflow(prompt_tokens, budget.available())?;

        let format = match self.config.profile {
            TurnProfile::TwoCalls => ResponseFormat::for_type::<CharacterResponse>(),
            TurnProfile::SingleCall => ResponseFormat::for_type::<CombinedTurnResponse>(),
        };
        let request = ChatRequest {
            sampling: self.config.actor_sampling.clone(),
            keep_alive: self.config.keep_alive,
            ..ChatRequest::new(messages).with_response_format(format)
        };

        self.transition(TurnState::Streaming)?;
        let streamed = self.stream_actor(request, &speaker, cancel).await?;

        self.transition(TurnState::Applying)?;
        let (response, beat) = match self.config.profile {
            TurnProfile::TwoCalls => {
                let parsed: CharacterResponse = serde_json::from_str(&streamed.text)
                    .map_err(|e| TurnError::Inference(e.into()))?;
                (parsed, None)
            }
            TurnProfile::SingleCall => {
                let parsed: CombinedTurnResponse = serde_json::from_str(&streamed.text)
                    .map_err(|e| TurnError::Inference(e.into()))?;
                (parsed.as_character(), Some(parsed.as_director()))
            }
        };

        self.apply_actor_response(&speaker, &response)?;
        self.consider_memory_maintenance(&speaker, allocation.history)?;
        let director_triggered = match &beat {
            // The single-call arm has already done the Director's work, in
            // the same response. Applying it here is what makes the arm a
            // fair comparison rather than a cheaper turn that does less.
            Some(beat) => {
                self.apply_beat(beat)?;
                true
            }
            None => self.consider_director(&text, response.escalation_signal)?,
        };

        let completion_tokens = count_tokens(self.actor.tokenizer(), &streamed.text)?;

        Ok(TurnOutcome {
            speaker,
            response,
            latency: started.elapsed(),
            time_to_first_token: streamed.time_to_first_token,
            prompt_tokens,
            evaluated_prompt_tokens: streamed.evaluated_prompt_tokens,
            completion_tokens,
            retrieved: lore.count,
            director_triggered,
            beat,
        })
    }

    // ---------------------------------------------------------------- state

    fn load_campaign(&self) -> Result<Campaign, TurnError> {
        let id = self.session.campaign_id().to_string();
        self.session
            .with_store(|store| store.load_campaign(&id))?
            .ok_or(TurnError::NoCampaign)
    }

    fn append_history(
        &self,
        role: HistoryRole,
        content: &str,
        sender: Option<&str>,
        active_character: Option<&str>,
    ) -> Result<(), TurnError> {
        let entry = HistoryEntry {
            role,
            content: content.to_string(),
            timestamp: self.session.clock().timestamp(),
            sender: sender.map(str::to_string),
            active_character: active_character.map(str::to_string),
        };
        let id = self.session.campaign_id().to_string();
        self.session
            .with_store(|store| store.append_history(&id, &entry))
    }

    /// One turn older: the Director gets closer to due, its cooldown gets
    /// closer to spent.
    fn advance_turn_counters(&self, campaign: &Campaign) -> Result<(), TurnError> {
        let mut next = campaign.clone();
        next.turns_since_last_director += 1;
        next.director_cooldown = (next.director_cooldown - 1).max(0);
        next.last_played = self.session.clock().timestamp();
        self.session.with_store(|store| store.save_campaign(&next))
    }

    // ------------------------------------------------------------ retrieval

    fn retrieve_lore(
        &self,
        query: &str,
        config: &crate::retrieval::RetrievalConfig,
        token_budget: usize,
        reranker: &dyn Reranker,
    ) -> Result<Lore, TurnError> {
        // No dense half: an embedding model is configuration and may be
        // absent. That is a degraded pipeline, not a broken one — BM25 alone
        // beats the Godot baseline on two of three fixtures — and it is
        // explicit here rather than an empty vector that silently contributes
        // nothing.
        let result = retrieve(
            query,
            self.session.graph(),
            self.session.lexical(),
            None,
            reranker,
            config,
        )?;
        let tokenizer = self.actor.tokenizer();
        let text = format_context(&result.hits, self.session.graph(), token_budget, |s| {
            count_tokens(tokenizer, s).unwrap_or(0)
        });
        Ok(Lore {
            count: result.hits.len(),
            text,
        })
    }

    // ------------------------------------------------------------- assembly

    /// Assemble the Actor's prompt.
    ///
    /// Deliberately thin at this task: §4.5 ports `PromptBuilder.gd` and
    /// `SystemPrompts.gd` properly and this becomes a call into it. What is
    /// already load-bearing is the *ordering* — [`PromptSections`] puts stable
    /// content first and the player's line last, which is §2.7's cache
    /// stability and the thing the latency measurement is about.
    fn assemble_actor_sections(
        &self,
        campaign: &Campaign,
        speaker: &str,
        text: &str,
        lore: &Lore,
        prior: Vec<HistoryEntry>,
    ) -> Result<PromptSections, TurnError> {
        let entity = self.session.graph().get(&EntityId::from_stored(speaker));
        let campaign_id = self.session.campaign_id().to_string();
        let (plot_flags, inventory) = self.session.with_store(|store| {
            Ok((
                store.plot_flags(&campaign_id)?,
                store
                    .inventory(&campaign_id, speaker)?
                    .into_iter()
                    .map(|item| (item.item, item.quantity))
                    .collect::<Vec<_>>(),
            ))
        })?;
        let location = self.location_label(campaign);
        let player = parse_player_card(campaign);

        let prompt = TurnPrompt {
            character: CharacterCard {
                name: entity.map(|e| e.label.as_str()).unwrap_or(speaker),
                gender: entity.and_then(|e| e.field(CanonicalField::Gender)),
                biography: entity.and_then(|e| e.field(CanonicalField::Biography)),
                personality: entity.and_then(|e| e.field(CanonicalField::Personality)),
                appearance: entity.and_then(|e| e.field(CanonicalField::Appearance)),
                goals: entity.and_then(|e| e.field(CanonicalField::Goals)),
                // The character's own sample, or the vault's house style.
                writing_style: entity
                    .and_then(|e| e.field(CanonicalField::WritingStyle))
                    .or(Some(campaign.writing_style.as_str()))
                    .filter(|s| !s.trim().is_empty()),
            },
            speech: speech_of(entity).into(),
            combined: self.config.profile == TurnProfile::SingleCall,
            player: player.as_ref().map(PlayerCardOwned::borrow),
            world: WorldSnapshot {
                location: location.as_deref(),
                plot_flags: &plot_flags,
                inventory: &inventory,
            },
            campaign_memory: campaign_memory_block(campaign),
            session_memory: self.session_memory_block(speaker)?,
            // No hits means no block, not an empty one: `format_context`
            // always emits its header, and a header with nothing under it
            // costs tokens and teaches the model nothing.
            lore: (lore.count > 0).then(|| lore.text.clone()),
            history: prior.iter().map(history_to_message).collect(),
            emotional_profile: Some(self.volatile_state(speaker)?),
            player_input: text,
        };
        Ok(prompt.sections())
    }

    /// What this character remembers of the adventure, as its own block.
    ///
    /// Never their biography, which is in the card: [rag_architecture.md §1.5]
    /// is emphatic that the two must never be conflated, and [`crate::memory`]
    /// has no way to reach a graph entity at all.
    ///
    /// [rag_architecture.md §1.5]: ../../../../docs/rag_architecture.md
    fn session_memory_block(&self, speaker: &str) -> Result<Option<String>, TurnError> {
        let campaign_id = self.session.campaign_id().to_string();
        let memory = self
            .session
            .with_store(|store| self.memory.session_memory(store, &campaign_id, speaker))?;
        let label = self.label_of(speaker);
        Ok(memory.block(&label))
    }

    fn label_of(&self, entity_id: &str) -> String {
        self.session
            .graph()
            .get(&EntityId::from_stored(entity_id))
            .map(|e| e.label.clone())
            .unwrap_or_else(|| entity_id.to_string())
    }

    /// The active location's label, resolved through the graph rather than
    /// shown to the model as a slug.
    fn location_label(&self, campaign: &Campaign) -> Option<String> {
        let id = campaign.active_location.trim();
        if id.is_empty() {
            return None;
        }
        Some(self.label_of(id))
    }

    /// How the character feels right now, and where rapport stands.
    ///
    /// Separate from the card because it moves almost every turn: folding it
    /// into the card changes the first message of every request and
    /// invalidates the whole cache prefix (§2.7). `PromptSections` orders it
    /// next to the player's input for that reason.
    fn volatile_state(&self, entity_id: &str) -> Result<String, TurnError> {
        let id = EntityId::from_stored(entity_id);
        let label = self
            .session
            .graph()
            .get(&id)
            .map(|e| e.label.clone())
            .unwrap_or_else(|| entity_id.to_string());
        let campaign_id = self.session.campaign_id().to_string();
        let (feeling, affinity): (EmotionState, f64) = self.session.with_store(|store| {
            let feeling = self.emotion.current(store, &campaign_id, entity_id)?;
            let affinity = store
                .character_state(&campaign_id, entity_id)?
                .map(|s| s.affinity)
                .unwrap_or(0.0);
            Ok((feeling, affinity))
        })?;
        Ok(self.emotion.profile_block(&label, &feeling, affinity))
    }

    fn count_messages(&self, messages: &[ChatMessage]) -> Result<usize, TurnError> {
        let tokenizer = self.actor.tokenizer();
        let mut total = 0;
        for message in messages {
            if let Some(content) = &message.content {
                total += count_tokens(tokenizer, content)?;
            }
        }
        Ok(total)
    }

    // ------------------------------------------------------------ streaming

    async fn stream_actor(
        &self,
        request: ChatRequest,
        speaker: &str,
        cancel: &CancelToken,
    ) -> Result<Streamed, TurnError> {
        let started = Instant::now();
        let mut stream = self.actor.chat_stream(request).await?;
        let mut parser = FieldStreamer::new(&["narration", "dialogue"]);
        let mut text = String::new();
        let mut time_to_first_token = None;
        let mut evaluated_prompt_tokens = None;
        let mut zone: Option<&'static str> = None;

        loop {
            let next = tokio::select! {
                biased;
                reason = cancel.cancelled() => return Err(TurnError::Cancelled(reason)),
                item = stream.next() => item,
            };
            let Some(delta) = next else { break };
            let delta = delta?;
            if let Some(evaluated) = delta.evaluated_prompt_tokens {
                evaluated_prompt_tokens = Some(evaluated);
            }
            if let Some(content) = delta.content {
                if !content.is_empty() {
                    text.push_str(&content);
                    for piece in parser.push(&content) {
                        match piece {
                            StreamPiece::ZoneStarted(field) => {
                                zone = Some(field);
                                self.emit(TurnEvent::StreamStarted {
                                    speaker: speaker_of(field, speaker),
                                    field,
                                });
                            }
                            StreamPiece::Text(chunk) => {
                                if zone.is_some() && time_to_first_token.is_none() {
                                    time_to_first_token = Some(started.elapsed());
                                }
                                self.emit(TurnEvent::StreamDelta { text: chunk });
                            }
                            StreamPiece::ZoneEnded(field) => {
                                zone = None;
                                self.emit(TurnEvent::StreamEnded { field });
                            }
                        }
                    }
                }
            }
            if delta.done {
                break;
            }
        }

        Ok(Streamed {
            text,
            time_to_first_token,
            evaluated_prompt_tokens,
        })
    }

    // ---------------------------------------------------------------- apply

    /// Write what the Actor said to the transcript, and apply its emotional
    /// update.
    ///
    /// **RAG003 is structural here.** In the Godot build `apply_emotion` was
    /// reachable from three places per turn — stream completion, emotion
    /// reflection, and `select_character` — so every player message queued
    /// three physical-reaction generations. This is the only place a turn
    /// applies an emotion, and it emits at most one
    /// [`TurnEvent::ReactionRequested`], so the debounce is a property of the
    /// call graph rather than a counter that has to be reset.
    fn apply_actor_response(
        &self,
        speaker: &str,
        response: &CharacterResponse,
    ) -> Result<(), TurnError> {
        let narration = response.narration.trim();
        if !narration.is_empty() {
            self.append_history(
                HistoryRole::Narrator,
                narration,
                Some("narrator"),
                Some(speaker),
            )?;
            self.emit(TurnEvent::Message {
                speaker: Speaker::Narrator,
                text: narration.to_string(),
            });
        }

        let dialogue = response.dialogue.trim();
        if !dialogue.is_empty() {
            self.append_history(
                HistoryRole::Character,
                dialogue,
                Some(speaker),
                Some(speaker),
            )?;
            self.emit(TurnEvent::Message {
                speaker: Speaker::Character(speaker.to_string()),
                text: dialogue.to_string(),
            });
        }

        if narration.is_empty() && dialogue.is_empty() {
            // A response that says nothing is a failure, not a quiet turn.
            // The Godot build renders it as an awkward silence, which is
            // exactly the silent degradation this phase is warned about.
            self.emit(TurnEvent::Failed {
                kind: FailureKind::Backend,
                detail: "the character's response contained no narration and no dialogue"
                    .to_string(),
            });
        }

        let campaign_id = self.session.campaign_id().to_string();
        let timestamp = self.session.clock().timestamp();
        let outcome = self.session.with_store(|store| {
            self.emotion.apply(
                store,
                &campaign_id,
                speaker,
                &response.emotional_update,
                &timestamp,
            )
        })?;

        // RAG006: a state that did not move emits nothing. The event and the
        // rapport change are recorded either way — the history is the record —
        // but a visual update the player cannot perceive is what queued a
        // model call every turn in the Godot build.
        if outcome.changed {
            self.emit(TurnEvent::EmotionChanged {
                entity_id: outcome.entity_id.clone(),
                emotion: outcome.state.emotion,
                intensity: outcome.state.intensity,
                affinity: outcome.affinity,
            });
            self.emit(TurnEvent::ReactionRequested {
                entity_id: outcome.entity_id,
                emotion: outcome.state.emotion,
            });
        }

        if response.escalation_signal != EscalationSignal::None {
            self.emit(TurnEvent::Escalation(response.escalation_signal));
        }
        Ok(())
    }

    /// Fade everyone the player did not address one step toward their
    /// baseline.
    fn decay_bystanders(&self, speaker: &str, context: &str) -> Result<(), TurnError> {
        let characters: Vec<String> = self
            .session
            .graph()
            .by_kind(EntityKind::Character)
            .map(|e| e.id.as_str().to_string())
            .collect();
        let campaign_id = self.session.campaign_id().to_string();
        let timestamp = self.session.clock().timestamp();
        let outcomes = self.session.with_store(|store| {
            self.emotion.decay(
                store,
                &campaign_id,
                &characters,
                Some(speaker),
                context,
                &timestamp,
            )
        })?;
        for outcome in outcomes.into_iter().filter(|o| o.changed) {
            self.emit(TurnEvent::EmotionChanged {
                entity_id: outcome.entity_id,
                emotion: outcome.state.emotion,
                intensity: outcome.state.intensity,
                affinity: outcome.affinity,
            });
        }
        Ok(())
    }

    // --------------------------------------------------------------- memory

    /// Queue a summary or a distillation if either is due.
    ///
    /// Both are background work at `Low` priority, so a player turn preempts
    /// them: the alternative is what `MemoryManager` does, which is to fire a
    /// summarisation model call from inside the turn's own completion callback
    /// and block the queue behind it.
    fn consider_memory_maintenance(
        self: &Arc<Self>,
        speaker: &str,
        history_budget: usize,
    ) -> Result<(), TurnError> {
        let campaign_id = self.session.campaign_id().to_string();
        let tokenizer = self.actor.tokenizer();
        let plan = self.session.with_store(|store| {
            self.memory
                .plan_compaction(store, &campaign_id, speaker, history_budget, |text| {
                    count_tokens(tokenizer, text).unwrap_or(0)
                })
        })?;
        if let Some(plan) = plan {
            self.spawn_compaction(plan);
            // Distillation reads the summaries this compaction is about to
            // add, so it is considered after that lands, not alongside it.
            return Ok(());
        }

        let distillation = self
            .session
            .with_store(|store| self.memory.plan_distillation(store, &campaign_id, speaker))?;
        if let Some(plan) = distillation {
            self.spawn_distillation(plan);
        }
        Ok(())
    }

    fn spawn_compaction(self: &Arc<Self>, plan: CompactionPlan) {
        let engine = Arc::clone(self);
        let label = self
            .session
            .graph()
            .get(&EntityId::from_stored(&plan.entity_id))
            .map(|e| e.label.clone())
            .unwrap_or_else(|| plan.entity_id.clone());

        let ticket = self.queue.submit(
            "memory-compaction",
            Priority::Low,
            move |cancel| async move {
                let transcript = engine.memory.format_transcript(&plan.entries, &label);
                let request = ChatRequest {
                    sampling: engine.config.director_sampling.clone(),
                    keep_alive: engine.config.keep_alive,
                    ..ChatRequest::new(vec![
                        ChatMessage::system(SUMMARY_INSTRUCTIONS),
                        ChatMessage::user(format!(
                            "Character: {label}\n\nDialogue segment:\n{transcript}"
                        )),
                    ])
                    .with_response_format(ResponseFormat::for_type::<SessionSummaryResponse>())
                };
                let response = tokio::select! {
                    biased;
                    reason = cancel.cancelled() => return Err(TurnError::Cancelled(reason)),
                    result = engine.actor.chat(request) => result?,
                };
                let summary: SessionSummaryResponse = response.parse()?;
                if summary.summary.trim().is_empty() {
                    // An empty summary would retire transcript lines and put
                    // nothing in their place. Leave the lines live.
                    return Err(TurnError::Inference(InferenceError::Unsupported(
                        "the summariser returned an empty summary".to_string(),
                    )));
                }
                let campaign_id = engine.session.campaign_id().to_string();
                let timestamp = engine.session.clock().timestamp();
                engine.session.with_store(|store| {
                    engine.memory.apply_compaction(
                        store,
                        &campaign_id,
                        &plan,
                        &summary.summary,
                        &timestamp,
                    )
                })?;
                engine.emit(TurnEvent::MemoryUpdated {
                    entity_id: Some(plan.entity_id.clone()),
                });
                Ok(())
            },
        );
        drop(ticket);
    }

    fn spawn_distillation(self: &Arc<Self>, plan: DistillationPlan) {
        let engine = Arc::clone(self);
        let label = self
            .session
            .graph()
            .get(&EntityId::from_stored(&plan.entity_id))
            .map(|e| e.label.clone())
            .unwrap_or_else(|| plan.entity_id.clone());

        let ticket = self.queue.submit(
            "memory-distillation",
            Priority::Low,
            move |cancel| async move {
                let mut prompt = format!("Character: {label}\n\n");
                if !plan.existing.trim().is_empty() {
                    prompt.push_str(&format!(
                        "Existing long-term memory:\n{}\n\n",
                        plan.existing.trim()
                    ));
                }
                prompt.push_str("New summaries to absorb:\n");
                for (i, summary) in plan.summaries.iter().enumerate() {
                    prompt.push_str(&format!("{}. {}\n", i + 1, summary.trim()));
                }

                let request = ChatRequest {
                    sampling: engine.config.director_sampling.clone(),
                    keep_alive: engine.config.keep_alive,
                    ..ChatRequest::new(vec![
                        ChatMessage::system(DISTILLATION_INSTRUCTIONS),
                        ChatMessage::user(prompt),
                    ])
                    .with_response_format(ResponseFormat::for_type::<DistilledMemoryResponse>())
                };
                let response = tokio::select! {
                    biased;
                    reason = cancel.cancelled() => return Err(TurnError::Cancelled(reason)),
                    result = engine.actor.chat(request) => result?,
                };
                let distilled: DistilledMemoryResponse = response.parse()?;
                let campaign_id = engine.session.campaign_id().to_string();
                engine.session.with_store(|store| {
                    engine
                        .memory
                        .apply_distillation(store, &campaign_id, &plan, &distilled.memory)
                })?;
                engine.emit(TurnEvent::MemoryUpdated {
                    entity_id: Some(plan.entity_id.clone()),
                });
                Ok(())
            },
        );
        drop(ticket);
    }

    // ------------------------------------------------------------- director

    /// Decide whether a beat is due, and start composing one if so.
    fn consider_director(
        self: &Arc<Self>,
        player_text: &str,
        escalation: EscalationSignal,
    ) -> Result<bool, TurnError> {
        let policy = self.config.director;
        if !policy.enabled {
            return Ok(false);
        }
        let campaign = self.load_campaign()?;
        if campaign.pending_scene.is_some() {
            return Ok(false);
        }
        {
            let machine = lock(&self.machine);
            if machine.director != DirectorState::Idle {
                return Ok(false);
            }
        }

        let escalating = escalation != EscalationSignal::None;
        let due = campaign.turns_since_last_director >= policy.turn_threshold
            && campaign.director_cooldown <= 0;
        let should = due || (escalating && policy.escalation_bypasses_cooldown);
        if !should {
            return Ok(false);
        }

        self.spawn_director(player_text.to_string(), campaign.active_character.clone());
        Ok(true)
    }

    /// Compose the next beat in the background, at `Low` priority.
    ///
    /// Low is what makes the next player turn preempt it: a queued `High`
    /// cancels a running `Low` outright (see [`super::queue`]), which is why
    /// `SystemPrompts.get_director_busy_stalling_prompt` — a prompt that
    /// exists purely to paper over Director latency — has no equivalent here.
    fn spawn_director(self: &Arc<Self>, player_text: String, speaker: String) {
        let engine = Arc::clone(self);
        self.set_director_state(DirectorState::Researching);
        let ticket = self
            .queue
            .submit("director-beat", Priority::Low, move |cancel| async move {
                let guard = DirectorGuard {
                    engine: Arc::clone(&engine),
                };
                let result = engine.compose_beat(&player_text, &speaker, &cancel).await;
                if result.is_ok() {
                    guard.disarm();
                }
                result
            });
        // Fire and forget: the beat lands in `pending_scene` and is consumed
        // when the player is next idle. Dropping the ticket does not cancel
        // it — only a preempting turn does.
        drop(ticket);
    }

    async fn compose_beat(
        self: &Arc<Self>,
        player_text: &str,
        speaker: &str,
        cancel: &CancelToken,
    ) -> Result<(), TurnError> {
        let campaign = self.load_campaign()?;
        let budget =
            PromptBudget::new(self.director.context_length(), self.config.response_reserve);
        let allocation = allocate(&budget, self.config.fractions)?;

        // §2.6: the knowledge-graph search is a deterministic pre-pass, not a
        // tool call. It needs no model and returns in microseconds, so the
        // Director's ReAct research loop — and the `ReActSignalCarrier` that
        // existed only to make a callback awaitable — is not ported.
        let lore = self.retrieve_lore(
            player_text,
            &self.config.director_retrieval,
            allocation.lore,
            &NoRerank,
        )?;
        if cancel.is_cancelled() {
            return Err(TurnError::Cancelled(
                cancel.reason().unwrap_or(CancelReason::Requested),
            ));
        }
        self.set_director_state(DirectorState::Composing);

        let prior = self.session.with_store(|store| {
            store.recent_live_history(&campaign.id, self.config.history_window)
        })?;

        let player = parse_player_card(&campaign);
        let active_label = (!speaker.trim().is_empty()).then(|| self.label_of(speaker));
        let plot_flags = self
            .session
            .with_store(|store| store.plot_flags(&campaign.id))?;
        let location = self.location_label(&campaign);
        let sections = DirectorPrompt {
            campaign_title: &campaign.title,
            active_character: active_label.as_deref(),
            previous_beat: Some(campaign.last_director_beat.as_str()),
            player: player.as_ref().map(PlayerCardOwned::borrow),
            world: WorldSnapshot {
                location: location.as_deref(),
                plot_flags: &plot_flags,
                inventory: &[],
            },
            campaign_memory: campaign_memory_block(&campaign),
            lore: (lore.count > 0).then(|| lore.text.clone()),
            history: prior.iter().map(history_to_message).collect(),
            player_input: player_text,
        }
        .sections();

        let request = ChatRequest::new(sections.into_messages())
            .with_response_format(ResponseFormat::for_type::<DirectorResponse>());
        let request = ChatRequest {
            sampling: self.config.director_sampling.clone(),
            keep_alive: self.config.keep_alive,
            ..request
        };

        let response = tokio::select! {
            biased;
            reason = cancel.cancelled() => return Err(TurnError::Cancelled(reason)),
            result = self.director.chat(request) => result?,
        };
        let beat: DirectorResponse = response.parse()?;

        let mut next = campaign.clone();
        next.pending_scene = Some(
            serde_json::to_string(&beat)
                .map_err(|e| TurnError::Inference(crate::inference::InferenceError::Decode(e)))?,
        );
        next.turns_since_last_director = 0;
        next.director_cooldown = self.config.director.cooldown_turns;
        self.session
            .with_store(|store| store.save_campaign(&next))?;

        self.set_director_state(DirectorState::Ready);
        Ok(())
    }

    /// Show a composed beat and apply its state changes.
    ///
    /// Called when the player is idle. Returns `false` when nothing was
    /// waiting.
    pub fn consume_pending_scene(&self) -> Result<bool, TurnError> {
        let campaign = self.load_campaign()?;
        let Some(raw) = campaign.pending_scene.clone() else {
            return Ok(false);
        };
        let beat: DirectorResponse = serde_json::from_str(&raw)
            .map_err(|e| TurnError::Inference(crate::inference::InferenceError::Decode(e)))?;

        let narration = beat.narration.trim().to_string();
        if !narration.is_empty() {
            self.append_history(HistoryRole::Narrator, &narration, Some("narrator"), None)?;
            self.emit(TurnEvent::Message {
                speaker: Speaker::Narrator,
                text: narration.clone(),
            });
        }

        self.apply_beat(&beat)?;

        let mut next = self.load_campaign()?;
        next.pending_scene = None;
        next.last_director_beat = narration;
        self.session
            .with_store(|store| store.save_campaign(&next))?;

        self.emit(TurnEvent::SceneBeatApplied {
            choices: beat.choices,
            dice_roll: beat.dice_roll,
        });
        self.set_director_state(DirectorState::Idle);
        Ok(true)
    }

    /// Apply a beat's world-state changes: plot flags, inventory and the
    /// campaign memory tiers.
    ///
    /// Shared by both arms. On the two-call arm this runs when a composed
    /// beat is consumed; on the single-call arm it runs as part of the turn,
    /// because the same response carried it.
    fn apply_beat(&self, beat: &DirectorResponse) -> Result<(), TurnError> {
        let campaign = self.load_campaign()?;
        for (flag, value) in &beat.plot_updates {
            let campaign_id = self.session.campaign_id().to_string();
            self.session
                .with_store(|store| store.set_plot_flag(&campaign_id, flag, &value.to_string()))?;
        }

        let holder = campaign.active_character.clone();
        for update in &beat.inventory_updates {
            let campaign_id = self.session.campaign_id().to_string();
            let item = update.item_id.clone();
            let quantity = update.quantity as i64;
            match update.action {
                InventoryAction::Add => {
                    let row = InventoryItem {
                        entity_id: holder.clone(),
                        item,
                        quantity,
                        properties: None,
                    };
                    self.session
                        .with_store(|store| store.add_to_inventory(&campaign_id, &row))?;
                }
                InventoryAction::Remove => {
                    // `false` means the holder did not have that many. The
                    // Director asked for something the state does not support;
                    // the inventory is the authority, and the row is left
                    // alone rather than going negative.
                    let removed = self.session.with_store(|store| {
                        store.remove_from_inventory(&campaign_id, &holder, &item, quantity)
                    })?;
                    if !removed {
                        self.emit(TurnEvent::SystemMessage {
                            text: format!("Could not remove {quantity} x {item}: not held."),
                        });
                    }
                }
            }
        }

        // A tier the model left empty keeps what was there: an empty string
        // is "nothing to add", not "forget everything".
        let mut next = campaign.clone();
        for (slot, update) in [
            (&mut next.memory_short_term, &beat.memory_updates.short_term),
            (
                &mut next.memory_medium_term,
                &beat.memory_updates.medium_term,
            ),
            (&mut next.memory_long_term, &beat.memory_updates.long_term),
        ] {
            if !update.trim().is_empty() {
                *slot = update.clone();
            }
        }
        self.session
            .with_store(|store| store.save_campaign(&next))?;
        self.emit(TurnEvent::MemoryUpdated { entity_id: None });
        Ok(())
    }
}

struct Lore {
    count: usize,
    text: String,
}

struct Streamed {
    text: String,
    time_to_first_token: Option<Duration>,
    evaluated_prompt_tokens: Option<usize>,
}

/// Restores the turn machine however the turn ends, cancellation included.
struct TurnGuard {
    engine: Arc<TurnEngine>,
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        self.engine.force_idle();
    }
}

/// Returns the Director to `Idle` unless the beat was composed.
struct DirectorGuard {
    engine: Arc<TurnEngine>,
}

impl DirectorGuard {
    fn disarm(self) {
        std::mem::forget(self);
    }
}

impl Drop for DirectorGuard {
    fn drop(&mut self) {
        let mut machine = lock(&self.engine.machine);
        if machine.director.is_running() {
            machine.director = DirectorState::Idle;
            drop(machine);
            self.engine
                .emit(TurnEvent::DirectorStateChanged(DirectorState::Idle));
        }
    }
}

fn speaker_of(field: &str, character: &str) -> Speaker {
    if field == "narration" {
        Speaker::Narrator
    } else {
        Speaker::Character(character.to_string())
    }
}

/// Render one transcript row as a chat message.
///
/// A player row is wrapped exactly as it was when it was sent, by the same
/// function. That is not cosmetic: this turn's `<player_message>` block is
/// next turn's history, and if the two renderings differ by a single byte the
/// shared prefix ends where the history begins — which is to say the KV cache
/// is invalidated on every turn, which is the defect §2.7 exists to fix. The
/// transcript itself stays clean; the delimiters are a prompt concern and
/// live only here.
/// `campaigns.player_character` is free-form JSON carried over from the Godot
/// save, so it is parsed leniently: a field that is missing or the wrong type
/// is absent rather than an error. A malformed protagonist should cost the
/// player their character sheet in the prompt, not their turn.
struct PlayerCardOwned {
    name: String,
    physical_description: Option<String>,
    personality: Option<String>,
    backstory: Option<String>,
}

impl PlayerCardOwned {
    fn borrow(&self) -> PlayerCard<'_> {
        PlayerCard {
            name: &self.name,
            physical_description: self.physical_description.as_deref(),
            personality: self.personality.as_deref(),
            backstory: self.backstory.as_deref(),
        }
    }
}

fn parse_player_card(campaign: &Campaign) -> Option<PlayerCardOwned> {
    let raw = campaign.player_character.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Some(PlayerCardOwned {
        name: text("name").unwrap_or_else(|| "The player".to_string()),
        physical_description: text("physical_description"),
        personality: text("personality"),
        backstory: text("backstory"),
    })
}

/// Whether this character speaks in words.
///
/// The three ingest flags, folded into one decision. An entity that is not in
/// the graph is assumed to speak: the alternative is silently muting a
/// character because their note failed to load.
fn speech_of(entity: Option<&crate::knowledge::Entity>) -> Speech {
    let flag = |key: &str, default: bool| {
        entity
            .and_then(|e| e.properties.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    };
    Speech::from_flags(
        flag("is_creature", false),
        flag("can_speak", true),
        flag("humanoid", true),
    )
}

fn history_to_message(entry: &HistoryEntry) -> ChatMessage {
    match entry.role {
        HistoryRole::Player => ChatMessage::user(player_message(&entry.content)),
        HistoryRole::Character | HistoryRole::Narrator => {
            ChatMessage::assistant(entry.content.clone())
        }
        HistoryRole::System => ChatMessage::system(entry.content.clone()),
    }
}

/// The campaign's three memory tiers, or nothing when they are all empty.
fn campaign_memory_block(campaign: &Campaign) -> Option<String> {
    let tiers = [
        ("Short-term", campaign.memory_short_term.trim()),
        ("Medium-term", campaign.memory_medium_term.trim()),
        ("Long-term", campaign.memory_long_term.trim()),
    ];
    if tiers.iter().all(|(_, v)| v.is_empty()) {
        return None;
    }
    let mut out = String::from("ADVENTURE MEMORY:\n");
    for (label, value) in tiers {
        if !value.is_empty() {
            out.push_str(&format!("- {label}: {value}\n"));
        }
    }
    Some(out)
}

/// Asks for a character's resting disposition. Static: the biography it reads
/// is a user message, so this text never changes and the prefix stays cached
/// across characters.
const BASELINE_INSTRUCTIONS: &str = "\
Read the character biography and answer with the emotional disposition this \
character rests in when nothing in particular is happening to them, and how \
strongly they hold it. Answer about their baseline, not about any single \
event in the biography.";

/// Asks for one medium-term summary. The transcript arrives as a user
/// message, so this stays byte-stable across every compaction.
const SUMMARY_INSTRUCTIONS: &str = "\
You are the memory summariser. Summarise the dialogue segment in one compact, \
objective paragraph of at most four sentences, written in the third person. \
Keep key events, decisions, actions, reactions and changes in the \
relationship. Core character traits, relationship status and critical plot \
points must survive; compress narrative detail, not those.";

/// Asks for long-term memory, integrating rather than replacing.
const DISTILLATION_INSTRUCTIONS: &str = "\
You are the memory coordinator. Rewrite the character's long-term memory so \
that it integrates the existing memory with the new summaries into one \
coherent narrative of one or two paragraphs, in the objective third person. \
Keep long-term character development, major milestones, relationship shifts \
and plot outcomes. Do not discard what the existing memory already \
established.";
