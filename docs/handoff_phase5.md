# Phase 5 Handoff — Headless playable milestone

> **Read first**: [migration_plan.md](migration_plan.md) §3 (target architecture),
> §4 (ordering principle), Phase 5, and Appendix B.
> **Then read**: [eval_baseline.md](eval_baseline.md) "Phase 4 measurement" —
> the numbers just recorded, including the one Phase 5 is gated on and does
> not yet meet.
> **Then read**: [migration_plan.md](migration_plan.md) Appendix D, entry D-4
> — a decision was recorded from live numbers, with one loose end attached.
> **Prerequisite**: Phases 0-4, all complete and merged.
> **This document is self-contained.** It assumes no knowledge of the
> conversation that produced it.

---

## Where the project is

Orison is a local-first, offline AI storytelling engine. The Godot 4.x /
GDScript build (~19,600 lines) is the reference implementation and remains
under feature freeze until Phase 5 closes. A parallel Rust core
(`crates/orison-core`) has been built alongside it, phase by phase.

**Phase 0-3** stabilised the repository, built the measurement layer, built
the inference layer (`InferenceBackend`, `OllamaBackend`, schema-constrained
decoding, real tokenization), and built the data layer (SQLite state, ingest,
the `petgraph` knowledge graph, hybrid retrieval, RAPTOR). See
[handoff_phase4.md](handoff_phase4.md) for the detail; nothing there is
repeated here.

**Phase 4 built the turn loop**, and — for the first time — ran it against
live models instead of a loopback stand-in. That live run is why this handoff
looks different from the last one: two numbers that were carried forward as
"unmeasured" for three phases are now measured, and one of them does not pass.

### The number that matters most: turn latency regressed, not improved

Phase 2's entire justification was cache-stable prompt ordering fixing the
Godot build's ~20 s p50. Measured live on 9 Sep against `llama3.2:3b` (the
model the baseline was recorded on):

| Fixture | p50 | p95 | Godot p50 | Ratio |
|---|---:|---:|---:|---:|
| `minimal` | 54.4 s | 63.2 s | 21.2 s | 0.39x |
| `messy` | 42.0 s | 52.9 s | 20.2 s | 0.48x |

Roughly **2x slower**, not faster. Time to first token rose with prompt
length in both fixtures instead of staying flat, which per the harness's own
reading (`docs/eval_baseline.md`, "Turn latency — measured") means the KV
cache is not being reused on this backend, even though `tests/turn_loop.rs`
asserts byte-identical prefixes turn to turn against the loopback stand-in.
The unit-level guarantee holds; the live number contradicts it. Something
between the two is wrong — possibly Ollama-side (its own prompt cache has
conditions this project does not control), possibly a header or field the
stand-in doesn't check. Either way:

**p95 turn latency is Phase 5's migration gate, and it currently fails.**
Investigate this early, not as cleanup after `orison-cli` is built — a
playable CLI over an engine that is twice as slow as the thing it replaces is
not a milestone worth reaching first and fixing second.

### The other open item: D-4's decision has a one-read condition on it

The Director/Actor experiment ran (Appendix D). Arm A (the current split)
won or tied on quality-per-second on every fixture measured, which is the
mechanical decision rule the Phase 4 handoff set in advance — so the split is
recorded as **kept**. But arm A was also the only arm with pronoun flags (1 on
`minimal`, 2 on `messy`), which is why its quality composite trailed the
single-model arms despite winning on speed. The scoring's own legend says a
pronoun flag needs a human read before being counted as a real defect. Both
flagged narrations are quoted in Appendix D. **Read them before building
anything that assumes the split is settled.** If they turn out to be genuine
cross-model consistency errors — the exact failure mode the split was
warned to cost — D-4 reopens and Phase 5 should not have spent time on
Director-specific plumbing in the meantime.

The models used for this run were a caveat in themselves: no true 8B model
was available, so `qwen2.5:7b-instruct` (7B) stood in for both the Director
and the single-model arms. Re-run with an actual 8B-class model before
treating the win margin as final; a 7B stand-in makes the split look
relatively better than it might against the model class the question is
actually about.

### The real vault run: confirms the pipeline holds, surfaces tuning work

`tests/real_vault.rs` ran for the first time against a real 151-note vault
(counts only; no content is or should ever be printed — see the constraint
in AGENTS.md). It passed — `unaccounted_sections` was empty, the one hard
assertion — but surfaced two things the fixtures are too small to show:

- **62% of sections overflowed** their canonical fields (retained, not lost,
  but landing in the catch-all bucket rather than a typed field).
- **48% of notes were untyped**, meaning `IngestOptions::folder_types` does
  not yet recognise this vault's folder conventions.

Neither blocks Phase 5. Both are exactly the kind of heuristic-tuning work
the Phase 3/4 handoffs predicted a real vault would surface, and now there is
a real vault's numbers to tune against instead of guessing.

---

## What Phase 4 handed you

The turn loop, so you do not have to reverse-engineer it from
`crates/orison-core/src/turn/`.

### `turn::TurnEngine` — the state machine

```rust
let session = Session::open(campaign_id, store)?;
let engine = TurnEngine::new(session, actor_backend, director_backend, TurnConfig::default());

let mut events = engine.subscribe();          // broadcast::Receiver<TurnEvent>
let ticket = engine.submit_player_input("I open the door.");
let outcome = ticket.join().await?;           // TurnOutcome, or a cancellation

engine.cancel_current_turn(CancelReason::PlayerActedAgain); // real cancellation, not a flag
```

`TurnState` (`Idle` → `Preparing` → `Streaming` → `Applying`) and
`DirectorState` are queried via `engine.state()` / `engine.director_state()`
or watched on the event stream — a shell derives "is input enabled" from
`TurnState::is_busy()` rather than a signal every exit path has to remember
to emit. Cancellation is verified server-side in `tests/turn_cancellation.rs`:
the test asserts the *server* observed the connection close mid-response, not
that a flag got set.

`Session` bundles one campaign's store, graph and lexical index behind
`Arc`s — cheap to clone, and the thing `GameLoopController.gd` never had
(it reached into six autoloads from one file).

### `turn::TurnConfig` and `TurnProfile` — the Director/Actor arms are configuration

```rust
pub enum TurnProfile { TwoCalls, SingleCall }  // A/B are TwoCalls with different backends; C is SingleCall
```

Per B-10, arm identity is never a code branch on a model name — `orison-cli`
should expose which arm is running as a config choice (director/actor/single
model + `TurnProfile`), not hardcode one arm's shape into the CLI's flow.

### `emotion` and `memory` — consolidated, typed

Emotion updates arrive as a typed field on the schema-constrained response
(no extract-and-repair pass). `MemoryManager` handles short/medium/long tiers;
`MemoryPolicy::compaction_threshold` is based on real token counts from
Phase 2's tokenizer, not the Godot build's `COMPACTION_THRESHOLD = 30` turn
count. Biography (`knowledge::Entity` fields) and session memory
(`characters.long_term_memory`, `history_logs`) remain structurally separate —
`tests/memory.rs` asserts this on the request body; do not let them meet in a
struct a CLI introduces.

### `prompt` — assembly is a builder, not string concatenation

`prompt::assembly` builds `CharacterCard`, `PlayerCard`, `WorldSnapshot` and
`TurnPrompt`/`DirectorPrompt`, each rendering to `PromptSections` and then
`into_messages() -> Vec<ChatMessage>`. Use `retrieval::format_context` for the
lore block; never re-derive token counts as `length / 4` (B-4).

---

## Tasks

**Goal**: play a complete campaign in a terminal. This is the milestone that
proves the migration succeeded — see migration_plan.md's own framing:
*"If these are not met, do not proceed to Phase 6. A prettier shell over a
worse engine is the failure mode this ordering exists to prevent."*

Do these in order.

| # | Task | Effort |
|---|---|---|
| 5.0 | Resolve the turn-latency regression, and the D-4 human read | Small, but blocking |
| 5.1 | `orison-cli` skeleton: load or create a campaign, save and load | Medium |
| 5.2 | Vault import from the CLI | Small (`ingest_vault` already exists; this is wiring) |
| 5.3 | Conversation loop: converse with characters, streaming output | Large |
| 5.4 | Movement between locations | Medium |
| 5.5 | Run the full evaluation harness against the CLI; compare to `eval_baseline.md` | Medium |

### 5.0 Resolve the turn-latency regression, and the D-4 human read

Two independent, cheap-to-resolve items, both blocking honest judgment of
everything after them:

- Read the two flagged narrations quoted in migration_plan.md Appendix D.
  Confirm or reject them as genuine pronoun errors. If genuine, reopen D-4
  before building further on the assumption the split is settled.
- Find out why cache reuse holds at the wire level (`tests/turn_loop.rs`) but
  not against a live Ollama server. Candidates: confirm the live request
  bodies actually match what the wire test asserts, check whether Ollama's
  own prompt-cache has preconditions this project isn't meeting (model
  reload between calls, a changed `keep_alive`, request field ordering
  Ollama itself is sensitive to beyond JSON key order), and re-run
  `turn_latency` after any fix to confirm the ratio moves back toward or past
  1.0x before spending more time on `orison-cli` itself.

### 5.1-5.4 The CLI

`orison-cli`: load or create a campaign, import a vault, converse with
characters, move between locations, save and load, streaming output, no
graphics. Build it against `TurnEngine` and `TurnEvent`/`TurnState` directly —
those exist precisely so a shell doesn't need to know about `InferenceBackend`,
`MemoryManager` or `EmotionEngine` individually.

### 5.5 Full evaluation harness, compared to baseline

Once the CLI can play a transcript end to end, run the same harness this
phase ran manually (`turn_latency`, `retrieval_dense`, `director_actor_experiment`,
`real_vault`, plus the judge suite this phase does not build) against it, and
update `eval_baseline.md` and migration_plan.md's exit-criteria checklist with
the result — the same way this handoff's numbers got there.

---

## Phase 5 exit criteria — the migration gate

Copied from migration_plan.md so it's in front of you, not just linked:

- [ ] A campaign is playable start to finish through the CLI.
- [ ] Every deterministic metric meets or exceeds the Godot baseline.
- [ ] Judge scores meet or exceed the baseline. (The judge suite itself is
      Phase 5's to build — it does not exist yet.)
- [ ] p95 turn latency is no worse than the baseline. **Currently failing**:
      measured p95 is 63.2 s / 52.9 s against a 21.2 s / 20.2 s baseline. See
      task 5.0.

If these are not met, do not proceed to Phase 6.

---

## Things earlier phases learned the hard way

Carried forward because they keep generalising, plus one Phase 4 added.

**A metric that never fires is not a metric.** Every phase's exit criteria
gate on assertions against fixtures under our control, not "did this print a
plausible number." Phase 4's cancellation test asserts the request actually
stopped server-side, not that a flag was set.

**Prove the negative result before believing it.** Phase 1 and Phase 3 both
found "the idea doesn't work" was actually "the wiring is wrong," and only a
positive control told the difference. Phase 4 adds a variant of this: a
passing unit-level guarantee (`tests/turn_loop.rs`'s byte-identical prefixes)
did **not** predict the live behaviour (`turn_latency`'s rising
time-to-first-token). A test that passes against a stand-in is evidence about
the stand-in until a live run confirms it is evidence about the real thing.

**A dependency compiling is not a dependency working.** Smoke-test early,
against the real target, not just the mock.

**Silent degradation is still the recurring theme.** Every failure that
reaches the player should be a typed error the loop decided how to present,
never an empty string that became an awkward silence.

---

## Conventions

- **Branch**: `claude/phase5-<task>`. Do not push straight to `main`.
- **Commits**: one per task, explaining *why*.
- **Do not touch the Godot build** beyond bug fixes. It remains under feature
  freeze and the reference implementation until this phase's exit criteria
  are met.
- **Model IDs are configuration, never constants** (B-10).
- **CI runs** `cargo fmt --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, and `cargo test --workspace`, plus the Godot suite and the
  eval harness. All must stay green.
- When this document contradicts the code, **trust the code and say so.**
  Every prior handoff contained at least one error found this way; that is
  the system working.

---

## Open questions this phase does not resolve

Carried forward from the Phase 4 handoff, updated with what the real-vault
run found:

**Retrieval precision at real-vault scale.** Still open. The real vault run
measured retrieval *timing* (67-87 ms mean query, both with and without graph
expansion) but not precision against ground truth — there is no
ground-truth answer key for a personal vault the way there is for the
fixtures. If context feels padded in play, `RetrievalConfig::expand_from` and
`expand_hops` are still the dials.

**Whether the passage reranker earns its place.** Unchanged: it moves no
recall on any fixture and MRR on one. Still unresolved by the real vault run,
which used it in the retrieval probes but did not compare with and without.

**Chunk size and overlap.** Unchanged from Phase 4 — but no longer
unmeasurable: the real vault's longest note was 15,349 words, with 49% of
notes chunking more than once. That is real multi-chunk behaviour to tune
`DEFAULT_TARGET_WORDS` / `DEFAULT_OVERLAP_WORDS` against, where the fixtures
(longest note 53 words) offered none.

**Whether `sqlite-vec` holds up past `large`'s 207 nodes.** Partially
answered: the real vault's 150 entities is still within `large`'s order of
magnitude, so this remains open for a larger vault.

**Whether RAPTOR cluster counts should scale differently at real-vault
scale.** Unchanged from Phase 4.

**Dense retrieval's precision cost.** New from this phase's live run:
fusing dense retrieval into `messy` held recall at 1.000 but cost precision
(0.440 → 0.360) — the opposite direction from the Godot build's live result
on the same fixture. Worth revisiting the RRF fusion weights against a
larger corpus before trusting dense-by-default in `orison-cli`.
