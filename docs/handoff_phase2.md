# Phase 2 Handoff — Rust workspace and the inference layer

> **Read first**: [migration_plan.md](migration_plan.md) §3 (target architecture),
> §4 (ordering principle), Phase 2, and Appendix B.
> **Then read**: [eval_baseline.md](eval_baseline.md). It contains the numbers
> this phase is graded against.
> **Prerequisite**: Phases 0 and 1, both complete and merged.
> **This document is self-contained.** It assumes no knowledge of the
> conversation that produced it.

---

## Where the project is

Orison is a local-first, offline AI storytelling engine, currently Godot 4.x /
GDScript, ~19,600 lines. It ingests a Markdown vault, builds a knowledge graph,
and runs an interactive adventure through local models via Ollama.

Two phases are done:

**Phase 0** stabilised the repository: CI, hermetic tests, a root `AGENTS.md`,
tickets moved to GitHub Issues, a feature freeze on the Godot build, and the
B-1 context-budget fix.

**Phase 1** built the measurement layer and recorded the baseline. It also found
four critical defects that six months of playing the game had not, three of which
are now fixed in the Godot build:

| Defect | What it was | Status |
|---|---|---|
| B-13 | Lexical retrieval condition inverted; 13 of 14 queries returned nothing | Fixed |
| B-14 | Character nodes discarded their raw source entirely | Fixed |
| B-16 | Character extraction never called `JsonRepair`, so every fenced response failed | Fixed |
| B-15 | A dead model degrades silently and the engine reports success | **Phase 2 owns this** |

**Phase 2 builds no game logic.** It builds the inference layer the rest of the
port sits on, and nothing else. Resist scope creep into ingest or retrieval;
those are Phase 3.

---

## What Phase 2 is for, in one paragraph

Every current model call goes through `/api/generate` with a single concatenated
prompt string, using the legacy `format: "json"` flag, with token counts
estimated at `length / 4` and context lengths that until recently were hardcoded
and wrong. Phase 2 replaces all of that with a typed Rust inference layer:
role-separated chat messages, schema-constrained decoding, a real tokenizer,
native tool calling, and cache-stable prompt ordering. The measured prize is
latency; the structural prize is that a whole class of parsing and truncation
defects stops being possible.

---

## The number to beat

**Turn latency p50 is ~20 seconds** on `llama3.2:3b`, measured live on an Apple
Silicon MacBook Air. That is unplayable, and it is the single clearest
justification for this phase.

The cause is not the model. The engine calls `/api/generate` with a monolithic
prompt **rebuilt from scratch every turn**, so llama.cpp's KV cache prefix is
invalidated on every single call and thousands of already-seen tokens are
re-processed. Cache-stable ordering plus `/api/chat` is what fixes it.

Treat 20 s as the number Phase 2 must beat, and measure it rather than assuming
the fix worked.

Full baseline in [eval_baseline.md](eval_baseline.md). The short version:

| Metric | Godot baseline | Notes |
|---|---|---|
| Turn latency p50 | ~20 s | The target. |
| Schema validity | 9/9 | Already good. Do not regress it. |
| Retrieval recall | 1.000 / 1.000 | With embeddings. Phase 3's problem, not yours. |
| Pronoun consistency | Clean | Bug 3 did not reproduce once B-16 was fixed. |

---

## Tasks

Do them in this order. Each builds on the last.

| # | Task | Effort |
|---|---|---|
| 2.1 | Scaffold the workspace | Small |
| 2.2 | Backend trait + `OllamaBackend` on `/api/chat` | Medium |
| 2.3 | `LlamaCppBackend`, in-process | Medium |
| 2.4 | Schema-constrained decoding | Medium |
| 2.5 | Real tokenisation and honest budgeting | Small |
| 2.6 | Native tool calling | Medium |
| 2.7 | Cache-stable prompt ordering | Small |

Commit each separately.

---

### 2.1 Scaffold the workspace

Create the layout from [migration_plan.md](migration_plan.md) §3.2. For this
phase only `orison-core` matters; `orison-eval` and `orison-cli` come later.

```
crates/orison-core/src/
├── inference/     # this phase
├── prompt/        # budgeting only, this phase
├── ingest/        # Phase 3
├── knowledge/     # Phase 3
├── retrieval/     # Phase 3
├── turn/          # Phase 4
├── memory/        # Phase 4
└── state/         # Phase 3
```

Add `cargo clippy -- -D warnings` and `cargo fmt --check` to CI as a third job
alongside the existing Godot test and eval jobs. Do not remove or weaken those:
the Godot build remains the reference implementation until Phase 5.

**The rule that makes this whole architecture work**: `orison-core` never knows
what is rendering it. No Tauri import, ever. It must compile and pass its tests
with `cargo test` and no shell present.

**Exit**: workspace builds, empty test suite passes, CI runs clippy and fmt.

---

### 2.2 Backend trait and `OllamaBackend`

```rust
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;
    async fn chat_stream(&self, req: ChatRequest) -> Result<BoxStream<ChatDelta>>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn tokenizer(&self) -> &Tokenizer;
    fn context_length(&self) -> usize;
    fn capabilities(&self) -> Capabilities;
    async fn health(&self) -> Result<ModelHealth>;
}
```

**Use `/api/chat`, not `/api/generate`.** This is the whole point and it is not a
stylistic preference. `/api/generate` with a concatenated string bypasses the
model's own chat template, so an instruction-tuned model receives its
instructions in a format it was not tuned on. Every current call does this.

Requirements:

- Role-separated messages: `system`, `user`, `assistant`, `tool`.
- Streaming via the chat endpoint.
- `context_length()` **queried from the backend**, never hardcoded. The Godot
  build hardcoded `4096 if character_model else 8192`, which was both wrong and
  selected by string-comparing model names. See B-1.
- `keep_alive` configurable, defaulting to a bounded duration. The Godot build
  pins both models in VRAM forever with `keep_alive: -1`, which on an 8GB
  consumer GPU is hostile (B-6).
- **`health()` is not optional, and this closes B-15.** An unreachable model must
  surface as a typed error the caller has to handle. In the Godot build a missing
  model produced HTTP 404, a `push_warning` invisible outside the editor, empty
  character fields, and a cheerful "RAPTOR summaries generated successfully". The
  whole compile reported success. Do not reproduce that: **a success message must
  be conditional on the work having succeeded.**

**Exit**: conformance suite passes against a live Ollama; `health()` returns a
typed error for a model that is not installed.

---

### 2.3 `LlamaCppBackend`

In-process via `llama-cpp-2`, with memory-mapped model loading.

Build it now, not later. It removes the hard Ollama dependency and a whole class
of "is the server running" onboarding failures, and it is **the only backend that
could ever run on a phone**, since mobile cannot spawn a sidecar process. Phase 8
depends on it existing. Retrofitting it after the fact costs a redesign; building
it alongside `OllamaBackend` costs a few days and keeps the trait honest.

**Exit**: both backends pass the *same* conformance suite. That shared suite is
the deliverable as much as either implementation.

---

### 2.4 Schema-constrained decoding

Define every model response as a Rust type. Derive its JSON Schema with
`schemars`. Send that schema to the backend's structured-output facility.

`schemars` matters more than it looks: the schema sent to the decoder is
generated from the *same type* that deserialises the response, so drift between
what you asked for and what you parse becomes impossible.

Consequences:

- **`JsonRepair.gd` (152 lines) is not ported.** It exists only because the Godot
  build uses the legacy `format: "json"` flag, which guarantees syntactic JSON but
  not the fields, types or enum values you asked for.
- Every `parsing_failed` branch downstream disappears.
- Character response, director response, emotion update and ReAct step all become
  plain `#[derive(Deserialize, JsonSchema)]` structs.

**Port the prompts from `SystemPrompts.gd` verbatim on content**, but strip the
"CRITICAL: You MUST respond strictly in the following JSON format" blocks and the
hand-written schema text. The sampler enforces the constraint now; spending
prompt tokens begging for it is waste, and on small models it measurably degrades
the content itself.

**Two findings from Phase 1 that should calibrate your confidence here.**

Schema validity was already **9/9** in the Godot build with no repair at all, on
`llama3.2:3b`. So this task is formalising something that already mostly holds
rather than fixing something broken. Lower risk than it looks.

But B-16 is the counterexample and the reason not to get complacent: the
*extraction* path used a stricter parser and no repair, and **every single
character failed to parse** because the model wrapped its JSON in ```` ```json ````
fences. Same model, same absence of repair, catastrophic outcome. Constrained
decoding makes both cases impossible, which is exactly why it is worth doing
rather than relying on the character agent's good behaviour.

**Exit**: schema validity is exactly 100% by construction. Anything less means
the constraint is not actually being applied.

---

### 2.5 Real tokenisation and honest budgeting

Load the model's actual tokenizer via the `tokenizers` crate. Replace the
`length / 4` heuristic (B-4).

Rewrite the budget allocator so that:

- Budgets derive from `backend.context_length()`, never a literal.
- Allocations sum to at most 100% **minus** a reserve for the response. The Godot
  build's allocations summed to 105% *before* any reservation, against a limit
  that was itself double the window actually served. That is B-1, and it silently
  truncated the system prompt on every NPC turn.
- Overflow is a typed error the caller must handle, not a warning that gets
  discarded. `push_warning` is invisible outside the editor; that is the same
  failure mode as B-15.

**Exit**: prompt token counts measured with the real tokenizer; budget overflow
is unrepresentable rather than merely unlikely.

---

### 2.6 Native tool calling

Replace the hand-rolled ReAct loop (`GameLoopController.gd:773`) with the
backend's native `tools` interface.

Keep the tool count to **3-5**. Small models select tools far less reliably than
they format the calls, and the existing loop already exposes four.

**Restructure the loop, do not merely re-encode it.** The current design runs up
to five sequential Director calls with a 60-second budget *before narration even
starts*. At ~20 s per call that is catastrophic. In particular
`search_knowledge_graph` should not be a tool call at all: make it a single
deterministic pre-pass in Phase 3, leaving tools for genuinely unpredictable
lookups.

**Exit**: tool calls are typed and validated by code, not parsed from prose. The
pre-narration call count drops.

---

### 2.7 Cache-stable prompt ordering

Order assembled messages: static system instructions, then character card, then
retrieved lore, then session summaries, then recent turns, then the player's
input. **Volatile content last.**

This is where the 20 s comes from. Local inference engines reuse the KV cache for
a shared prefix; the current builder reassembles one monolithic string per turn,
so the shared prefix is destroyed every time.

**Measure it.** Do not declare victory on the theory. Record turn latency p50
against the same nine-turn scripted transcript the Phase 1 baseline used, on the
same models, and put the before/after in `eval_baseline.md`.

**Exit**: measured KV-cache reuse across consecutive turns, and a p50 turn
latency materially below 20 s.

---

## How to measure during this phase

An awkwardness worth naming up front: **the eval harness is GDScript** and lives
in `eval/`. It drives the Godot engine. During Phase 2 there is no Rust engine
for it to drive, so it cannot grade this phase.

That is fine, and here is the split:

- **Phase 2 grades itself** with the backend conformance suite (§2.2, §2.3), the
  100%-by-construction schema check (§2.4), and a latency measurement against the
  same scripted transcript (§2.7).
- **The Godot harness keeps running in CI** unchanged. The Godot build is the
  reference implementation until Phase 5, and its numbers are the baseline.
- **Phase 5 ports the harness to Rust.** The fixtures and ground truth are
  already language-neutral: `fixtures/vaults/*/ground_truth.json` and the vault
  Markdown are plain data. Only the runner is GDScript. A Rust runner reading the
  same fixtures and asserting the same metrics is what lets Phase 5 compare like
  with like.

Do not delete or weaken the GDScript harness in this phase. Do not port it early
either; it has no engine to drive yet.

---

## Things Phase 1 learned the hard way

Five lessons, each of which cost real time. They generalise directly.

**A metric that never fires is not a metric.** The eval harness has a
`--selftest` mode asserting it detects deliberately planted defects, and CI gates
on that rather than on metric values. Build the same discipline into the Phase 2
conformance suite: a test that cannot fail is a rubber stamp.

**A metric that cries wolf is worse than no metric.** The pronoun check
originally used substring matching, and `"she was"` contains `"he "`. It flagged
every correctly-gendered feminine line. People learn to ignore a noisy check.

**Assert against behaviour you control.** Two tests pointed at `192.0.2.1`
relying on packets being *blackholed* so a timeout would fire. GitHub runners do
blackhole it; sandboxed runners *refuse* instantly. Same latent bug, different
symptom per environment, invisible on a developer machine.

**Prove the negative result before believing it.** Retrieval scored 0.000 and it
looked like a damning finding. It was, but only after a positive control proved
the graph was actually loaded. The first zero was confounded by an unrelated
crash. Add a control whenever you report something returning nothing.

**Silent degradation is the recurring theme.** B-1, B-15 and B-16 are all the
same shape: a `push_warning` nobody sees, work continuing on empty data, and a
success message printed anyway. **In Rust this should be structurally impossible.**
Use `Result`, make callers handle errors, and never print success unconditionally.
If Phase 2 delivers nothing else, deliver that.

---

## Conventions

- **Branch**: `claude/phase2-<task>`. Do not push straight to `main`.
- **Commits**: one per task, explaining *why*.
- **Do not open a pull request** unless asked.
- **Do not touch the Godot build** beyond bug fixes. It is under feature freeze
  and remains the reference implementation until Phase 5.
- **Do not start Phase 3.** No ingest, no retrieval, no SQLite. If a task seems to
  need them, you have misread it.
- **Model IDs are configuration, never constants** (B-10). The Godot build hard
  codes stale ones in its README to this day.
- When this document contradicts the code, **trust the code and say so.** Phase 0
  and Phase 1 handoffs each contained an error found this way; that is the system
  working.

---

## Open questions Phase 2 does not resolve

Recorded so nobody re-litigates them here.

**D-4, the Director/Actor split.** Whether to keep two models is a Phase 4.2
experiment, not a Phase 2 decision. Build the backend so either works.

**Which model.** The live baseline used `gemma4:e2b` (Director) and
`llama3.2:3b` (Actor). `qwen2.5:7b-instruct` is also installed on the developer's
machine and is plausibly better at structured extraction. Worth an experiment
during Phase 3, not a Phase 2 blocker.

**Whether `JsonRepair` is needed anywhere.** Constrained decoding should make it
unnecessary, but keep the Godot copy until Phase 5 proves it in the new engine.
