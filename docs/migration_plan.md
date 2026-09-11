# Orison Migration Plan

> **Status**: Phases 0 and 1 complete. Structural and narrative baselines
> recorded in [eval_baseline.md](eval_baseline.md) against real local models.
> Phase 2 (Rust workspace and inference layer) is next.
> **Date**: September 2026
> **Supersedes**: the platform assumptions in [proposal.md](proposal.md) (Pillar 3, "Zero-Dependency Portability" via GDScript). The product pillars in that document still hold; the implementation strategy does not.
> **Companion documents**: [orison_audit.md](orison_audit.md) (June 2026 code audit, largely remediated), [rag_architecture.md](rag_architecture.md) (retrieval philosophy, still authoritative).

This document defines the migration of Orison from Godot 4.x / GDScript to a Rust core library with a Tauri 2 desktop shell. It is ordered. Phases are meant to be executed top to bottom, and each phase has an exit criterion that must be met before the next begins.

---

## Table of Contents

1. [Why migrate](#1-why-migrate)
2. [Scope and non-goals](#2-scope-and-non-goals)
3. [Target architecture](#3-target-architecture)
4. [Ordering principle](#4-ordering-principle)
5. [Phase 0 — Stabilise the current repository](#phase-0--stabilise-the-current-repository)
6. [Phase 1 — Build measurement before building anything else](#phase-1--build-measurement-before-building-anything-else)
7. [Phase 2 — Rust workspace and the inference layer](#phase-2--rust-workspace-and-the-inference-layer)
8. [Phase 3 — Data, knowledge graph and retrieval](#phase-3--data-knowledge-graph-and-retrieval)
9. [Phase 4 — Orchestration and the turn loop](#phase-4--orchestration-and-the-turn-loop)
10. [Phase 5 — Headless playable milestone](#phase-5--headless-playable-milestone)
11. [Phase 6 — Tauri shell and UI](#phase-6--tauri-shell-and-ui)
12. [Phase 7 — Packaging and distribution](#phase-7--packaging-and-distribution)
13. [Phase 8 — Mobile re-entry](#phase-8--mobile-re-entry-deferred-not-closed)
14. [Appendix A — Component port map](#appendix-a--component-port-map)
15. [Appendix B — Defects to fix during the port](#appendix-b--defects-to-fix-during-the-port)
16. [Appendix C — What carries over unchanged](#appendix-c--what-carries-over-unchanged)
17. [Appendix D — Decisions and open questions](#appendix-d--decisions-and-open-questions)

---

## 1. Why migrate

### 1.1 The case is not "Godot is bad"

Godot exports to Windows, macOS, Linux, iOS and Android today. The current build works. The 20k lines of GDScript in this repository implement a genuinely complete system: vault compilation with LLM-assisted field extraction, a knowledge graph, RAPTOR hierarchical summaries, semantic retrieval, an agentic Director loop, three-tier memory, an emotion engine, streaming dialogue, theming and onboarding. Most of the critical findings in [orison_audit.md](orison_audit.md) have been remediated since June.

The case for migrating is narrower and more specific.

### 1.2 Orison is a retrieval application wearing a game engine

Roughly 85% of Orison's value sits in text processing, retrieval and LLM orchestration. Roughly 15% sits in presentation (sprites, procedural art, a node graph view). Godot is excellent at the 15% and provides essentially nothing for the 85%, while removing access to the library ecosystem that solves the 85% off the shelf.

The concrete cost of that, visible in this repository today:

| Hand-written in GDScript | Lines | Off-the-shelf elsewhere |
|---|---:|---|
| HTTP streaming client polled from `_process()` | ~1,145 (`LLMClient.gd` + `LLMStreamRequest.gd`) | `reqwest` + `eventsource-stream` |
| Malformed-JSON repair | 152 (`JsonRepair.gd`) | Unnecessary under schema-constrained decoding |
| Vector store and cosine search | 112 (`EmbeddingStore.gd`) | `sqlite-vec` |
| k-means clustering for RAPTOR | ~80 (`VaultCompiler.gd:1240`) | `linfa-clustering` |
| Markdown parsing | 271 (`MarkdownParser.gd`) | `pulldown-cmark` + `gray_matter` |
| Virtual scroll container | 296 (`VirtualScrollContainer.gd`) | Any web list virtualiser |
| Test runner | 3,750 (`TestRunnerNode.gd`) | `cargo test` |
| Token estimation | `length / 4` heuristic | `tokenizers` (the model's real tokenizer) |

That is on the order of 5,800 lines, nearly 30% of the codebase, spent re-implementing solved problems. Every one of them is a maintenance surface, and several of them are actively wrong (see [Appendix B](#appendix-b--defects-to-fix-during-the-port)).

### 1.3 The frame loop is the wrong execution model for inference

`LLMClient.gd:77` drives HTTP streaming from `_process(delta)`. Stream throughput is coupled to frame rate, timeouts are accumulated in frame deltas, and every asynchronous LLM operation has to be expressed as callback plumbing or hand-rolled awaitable signal carriers (`GameLoopController.ReActSignalCarrier` exists solely to work around this). A local-inference application spends most of its wall-clock time waiting on a model. That work belongs on a runtime built for it.

### 1.4 The decisive argument: reversibility

Splitting the engine into a standalone Rust crate makes the UI decision reversible. If Tauri's mobile story does not hold up, the shell can be replaced with Flutter via `flutter_rust_bridge`, or with a native shell, without rewriting the engine. Today, the engine and the presentation layer are both GDScript and neither can move without the other.

---

## 2. Scope and non-goals

### 2.1 In scope

- A standalone Rust core crate containing all engine logic.
- A Tauri 2 desktop shell for Windows, macOS and Linux.
- An evaluation harness that measures narrative and retrieval quality, not just parser correctness.
- Correction of the inference-layer defects catalogued in Appendix B.
- Migration of existing save files, or an explicit decision to break them (see [D-6](#appendix-d--decisions-and-open-questions)).

### 2.2 Explicitly deferred: mobile

**Mobile is out of scope for this migration.** The reasoning:

- On-device inference on phones is practical at 1-3B parameters, viable at 4B only on flagships with 8-12GB RAM, and thermally constrained regardless. The binding limit is heat, not memory.
- Orison's current design runs two models concurrently (Director ~8B, Actor ~3B) pinned in memory with `keep_alive: -1`. That is a desktop-class design and cannot be made to fit a phone by tuning.
- Building for a target we cannot currently satisfy would distort every architectural decision in this plan for no delivered value.

**The door stays open.** Section [Phase 8](#phase-8--mobile-re-entry-deferred-not-closed) defines the concrete re-entry criteria and lists the constraints this plan accepts now specifically to keep mobile cheap later. In short: no desktop-only assumptions leak into `orison-core`, inference is behind a trait with an in-process `llama.cpp` implementation from day one, and the UI is web technology that a mobile shell can reuse.

### 2.3 Permanently out of scope

- Any cloud inference path. Local-only is a product pillar, not an implementation detail. See [rag_architecture.md §1.2](rag_architecture.md).
- Any telemetry or network egress beyond the user's own configured local endpoints.
- Web/HTML5 export. It cannot host local inference and defeats the product premise.

---

## 3. Target architecture

### 3.1 The core/shell split

```
┌─────────────────────────────────────────────────────────┐
│  SHELL (replaceable)                                    │
│  Tauri 2 · TypeScript · desktop windows, chat, graph    │
└───────────────────────┬─────────────────────────────────┘
                        │ typed commands + event stream
┌───────────────────────┴─────────────────────────────────┐
│  orison-core (the engine, no UI, no platform assumptions)│
│                                                          │
│  ingest ──▶ knowledge ──▶ retrieval ──▶ prompt ──▶ turn │
│     │           │             │            │        │    │
│     └───────────┴─────────────┴────────────┴────────┘    │
│                        state (SQLite)                    │
│                        inference (trait)                 │
└───────────────────────┬─────────────────────────────────┘
                        │
        ┌───────────────┴───────────────┐
        │                               │
   OllamaBackend                  LlamaCppBackend
   (desktop, /api/chat)           (in-process, mobile-ready)
```

The rule that makes this work: **`orison-core` never knows what is rendering it.** It exposes commands and emits events. It does not import Tauri. It compiles and its full test suite passes with `cargo test` and no shell present.

### 3.2 Crate layout

```
orison/
├── crates/
│   ├── orison-core/          # the engine
│   │   ├── src/
│   │   │   ├── ingest/       # vault scan, markdown parse, compile
│   │   │   ├── knowledge/    # graph, RAPTOR levels, entities
│   │   │   ├── retrieval/    # BM25, vector, fusion, rerank
│   │   │   ├── prompt/       # assembly, budgeting, cache ordering
│   │   │   ├── inference/    # backend trait + implementations
│   │   │   ├── turn/         # game loop, director, actor, emotion
│   │   │   ├── memory/       # short/medium/long tiers
│   │   │   └── state/        # SQLite schema, migrations, saves
│   │   └── tests/
│   ├── orison-eval/          # evaluation harness (Phase 1)
│   └── orison-cli/           # headless playable client (Phase 5)
├── apps/
│   └── desktop/              # Tauri 2 shell (Phase 6)
├── fixtures/
│   └── vaults/               # golden test vaults
└── docs/
```

### 3.3 Dependency selections

| Concern | Crate | Replaces |
|---|---|---|
| In-process inference | `llama-cpp-2` | Ollama-only HTTP coupling |
| HTTP inference backend | `reqwest` + `eventsource-stream` | `LLMClient.gd`, `LLMStreamRequest.gd` |
| Tokenisation | `tokenizers` | `PromptBuilder.estimate_tokens` |
| Relational state | `rusqlite` | `SaveManager.gd` JSON blobs |
| Vector search | `sqlite-vec` | `EmbeddingStore.gd` |
| Lexical search | `tantivy` | (nothing; this is new) |
| Markdown | `pulldown-cmark`, `gray_matter` | `MarkdownParser.gd` |
| Graph algorithms | `petgraph` | `KnowledgeGraphManager.gd` traversal |
| Clustering | `linfa-clustering` | hand-rolled k-means |
| Async runtime | `tokio` | `_process()` frame polling |
| Schemas | `serde` + `schemars` | hand-written JSON schema strings |

`schemars` matters more than it looks: it generates the JSON Schema sent to the model's constrained decoder *from the same Rust type* that deserialises the response. Schema drift between what you ask for and what you parse becomes impossible.

### 3.4 Inference backend trait

```rust
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;
    async fn chat_stream(&self, req: ChatRequest) -> Result<BoxStream<ChatDelta>>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec2>>;
    fn tokenizer(&self) -> &Tokenizer;
    fn context_length(&self) -> usize;   // queried, never hardcoded
    fn capabilities(&self) -> Capabilities; // tools, schema, vision
}
```

Two implementations from Phase 2: `OllamaBackend` (desktop default, easiest onboarding) and `LlamaCppBackend` (in-process, no server dependency, and the only path that could ever work on a phone). Keeping both honest from the start is the cheapest mobile insurance available.

---

## 4. Ordering principle

Three rules govern the sequence below.

**Rule 1 — Measure before you move.** The evaluation harness (Phase 1) is built before any engine code is ported, and is baselined against the *current Godot build*. Without a baseline you cannot tell whether the rewrite improved anything, and "the rewrite feels better" is how projects get abandoned twice.

**Rule 2 — Headless before pixels.** Phases 2 through 5 produce no user interface. The engine becomes playable through a terminal client first. This is not asceticism: it means the retrieval and prompting changes, which are where the quality lives, get validated without UI work confounding them, and it means the UI is designed against known real latency rather than guessed latency.

**Rule 3 — Nothing ports without a test.** Each module is ported alongside the assertions that pin its behaviour. The 46 existing tests in `TestRunnerNode.gd` are a written specification; treat them as the acceptance criteria for their respective modules rather than as code to translate.

---

## Phase 0 — Stabilise the current repository

**Goal**: make the existing repository safe to work in and safe to leave. Nothing here is throwaway; all of it survives the migration.

**Duration estimate**: 1-2 days.

### 0.1 Add continuous integration

There is no `.github/` directory. Nothing runs on push. Every agent-authored or human-authored regression lands silently, which is a direct contributor to how this project stalled.

- Add `.github/workflows/ci.yml`.
- Job 1: run the Godot test suite headlessly (`godot --headless --path . res://tests/TestRunner.tscn`). The runner already exits non-zero when any test fails (`TestRunnerNode.gd`, final line: `get_tree().quit(0 if pass_count == total_tests else 1)`), so CI needs no changes to the runner itself. Verify the exit code propagates through the Godot binary in headless mode rather than assuming it.
- Job 2: `gdlint` / `gdformat --check` over `src/` and `tests/`.
- Run on push and pull request.

**Exit criterion**: a deliberately broken assertion fails CI.

### 0.2 Fix the context-budget defect

This is the highest-value single change available in the current codebase and should not wait for the migration. See [B-1](#b-1-npc-prompts-are-budgeted-at-twice-their-actual-context-window). It is a small change to `LLMClient.gd` and `PromptBuilder.gd` and it plausibly fixes the long-standing "NPC breaks character" behaviour.

### 0.3 Repair documentation portability

62 occurrences of `/Users/dylangrowcoot/Documents/Personal Apps/orison/` remain across `gemini.md`, `design_philosophy.md`, `ARCHITECTURE.md`, `docs/ticket_template.md`, `docs/orison_audit.md`, `docs/in_progress.md` and `docs/backlog.md`. These are broken for every reader other than the original machine, including every AI agent that tries to follow them. Replace with repository-relative links.

### 0.4 Replace the agent guidance files

- Create a root `AGENTS.md` as the single cross-tool entry point. Content: repository layout, invariants, how to run tests, the local-only constraint, prompt-change protocol.
- Retire the Feature Map in `gemini.md`. Mapping features to line ranges was a reasonable idea in 2024 and is an anti-pattern now: the line numbers rot within a commit or two, and current agents locate code by searching. Replace it with a short description of module boundaries and responsibilities.
- Keep a `CLAUDE.md` that points at `AGENTS.md` rather than duplicating it.

### 0.5 Move tickets to Issues

`backlog.md` → `in_progress.md` → `done.md` moved by hand is fragile and merge-conflict-prone, and `done.md` is already 945 lines. Migrate open tickets (the OBD, STT and TKT series) to GitHub Issues with labels. Keep `done.md` as a frozen historical record; do not append to it.

### 0.6 Declare a feature freeze on the Godot build

From the end of Phase 0, the Godot build receives bug fixes only. No new features. Every feature added after this point is a feature that must be ported twice.

### 0.7 — Two things Phase 0 discovered

Recorded because both changed the plan.

**Three tests were already failing on a clean tree** (task 0.1a, added during
execution). One wrote into `user://adventures/` before anything created it; two
pointed at `192.0.2.1` and relied on packets being *blackholed* so a timeout
would fire. GitHub's runners do blackhole that address, so only the first failed
in CI; a sandboxed runner refuses instantly, so all three failed there. Same
latent bug, different symptom per environment, invisible on a developer machine.
All three now avoid the network entirely. **Lesson for Phase 1: assert against
behaviour you control, never against how an environment happens to treat an
unroutable address.**

**Two CI facts worth keeping.** The runner's exit code does propagate correctly
through the headless Godot binary, so the workflow's summary-line parsing is
belt-and-braces rather than necessary. And job-level `continue-on-error` does
not stop Actions from skipping later steps after an earlier one fails, so
`gdlint` was silently skipped behind a failing `gdformat --check` until each
step was made independent. The lint job reports green while `gdlint` is still
finding ~2,939 issues, almost all trailing whitespace; that is deliberate and
unresolved. Do not mass-reformat a codebase the migration deletes.

**Phase 0 exit criteria**
- [x] CI runs tests and lint on every push, and fails correctly.
- [x] B-1 fixed and verified.
- [x] Zero absolute filesystem paths in documentation.
- [x] `AGENTS.md` exists at the repository root; the line-number Feature Map is gone.
- [x] Open tickets exist as Issues.
- [x] Feature freeze announced in `README.md`.

---

## Phase 1 — Build measurement before building anything else

**Goal**: be able to answer, numerically, "is Orison's storytelling better than it was last week?"

**Duration estimate**: 1-2 weeks.

**Rationale**: this is the most important phase in the document. The project has 46 tests covering parsers, save round-trips and queue mechanics, and zero measurement of the thing users actually experience. Every prompt change to date has been a guess, and the effect of any given change has been invisible. That is not a discipline problem; it is a missing tool. Building it first means every subsequent phase can prove its worth.

### 1.1 Build golden fixture vaults

Create `fixtures/vaults/` containing at least three hand-authored Obsidian-style vaults:

| Fixture | Purpose |
|---|---|
| `minimal/` | 3 characters, 2 locations, clean frontmatter. Smoke tests. |
| `messy/` | Deliberately hostile: `**1. History**` style headings, missing frontmatter, wiki-links, callouts, tables, inconsistent naming, a character with no gender marker, images. Exercises the ingest path's real job. |
| `large/` | 200+ notes. Exercises retrieval quality and compile-time performance. |

Each fixture ships with a `ground_truth.toml`: for a set of queries, which note IDs *should* be retrieved. This is what makes retrieval measurable.

### 1.2 Build the deterministic evaluation suite

These assertions need no judge and no subjective call. They run fast and they run in CI.

| Check | Assertion |
|---|---|
| Schema validity | 100% of model responses deserialise into the target type. Under constrained decoding this must be exactly 100%; anything less is a bug in the harness or the backend. |
| Ingest completeness | No section of a fixture note is silently dropped. Every heading maps to a field or to an explicit overflow bucket. |
| Pronoun consistency | For characters with a declared gender, zero mismatched pronouns across a scripted 20-turn transcript. This directly regresses `rag_architecture.md` Bug 3. |
| Retrieval recall@k | Against `ground_truth.toml`, measured at k=5 and k=10. |
| Context safety | Assembled prompt token count, measured with the real tokenizer, never exceeds the backend's reported context length. |
| Turn latency | p50 and p95 wall-clock per turn, recorded per phase so regressions are visible. |
| Loop detection | No verbatim repetition of a prior assistant turn within a 20-turn transcript. |
| Forbidden phrasing | Character agent never emits third-person self-reference in the `dialogue` field, never reveals raw affinity values. |

### 1.3 Build the LLM-as-judge suite

For the subjective half. A rubric scoring 1-5 on: in-character consistency, use of vault-sourced facts, narrative progression (does the scene move?), and absence of sycophancy. Run against scripted transcripts. Judge with a larger local model than the one under test.

Judge scores are noisy. Use them for trend detection across many samples, never to gate a single change. The deterministic suite is the gate.

### 1.4 Baseline the current Godot build

Run the entire harness against the existing GDScript engine and commit the numbers to `docs/eval_baseline.md`. This is the bar the migration must clear. Without it, Phase 5's exit criterion is unfalsifiable.

**Phase 1 exit criteria**
- [x] Three fixture vaults with ground-truth retrieval labels.
- [x] Deterministic suite runs in CI (replay mode; no model needed).
- [x] Judge suite runs on demand. *(Built in Phase 5.5 as `turn::judge`, run by `crates/orison-cli/tests/harness.rs`, gated on `ORISON_TEST_JUDGE_MODEL`. It gates nothing, per §1.3; the deterministic suite is still the gate.)*
- [x] `docs/eval_baseline.md` records Godot-build numbers for every metric.

---

## Phase 2 — Rust workspace and the inference layer

> **Executable brief**: [handoff_phase2.md](handoff_phase2.md). It carries the
> task order, the measured target (turn latency p50 ~20 s), how to grade a phase
> the GDScript eval harness cannot yet drive, and the lessons Phase 1 paid for.

**Goal**: a correct, modern inference layer. No game logic yet.

**Duration estimate**: 2-3 weeks.

**Rationale**: this is where the largest quality wins in the entire plan sit, and they are wins that require no cleverness, only current practice. Ports later phases depend on this being right.

### 2.1 Scaffold the workspace

Create the layout in §3.2. `orison-core` compiles, has zero UI dependencies, and `cargo test` passes on an empty suite. Add `cargo clippy -- -D warnings` and `cargo fmt --check` to CI.

### 2.2 Implement the backend trait and `OllamaBackend`

Against `/api/chat`, **not** `/api/generate`.

This is not a stylistic preference. `/api/generate` with a single concatenated prompt string bypasses the model's own chat template, so an instruction-tuned model receives its instructions in a format it was not tuned on. Every current call in `LLMClient.gd` does this. Correcting it is likely the single largest instruction-following improvement available.

Requirements:
- Role-separated messages (`system`, `user`, `assistant`, `tool`).
- Streaming via the chat endpoint.
- `context_length()` queried from the backend at connection time. Never hardcoded. The current `4096 if character_model else 8192` logic (`LLMClient.gd:214`) is both hardcoded and, as B-1 shows, wrong.
- `keep_alive` configurable, defaulting to a bounded duration rather than `-1`. Pinning two models in VRAM indefinitely is a hostile default on consumer hardware.

### 2.3 Implement `LlamaCppBackend`

In-process via `llama-cpp-2`, with memory-mapped model loading. Removes the hard Ollama dependency, removes a whole class of "is the server running" onboarding failures, and is the only backend that could ever run on a phone. Building it now costs a few days; retrofitting it later costs a redesign.

### 2.4 Adopt schema-constrained decoding

Define every model response as a Rust type. Derive its JSON Schema with `schemars`. Send that schema to the backend's structured-output facility. The decoder is then grammar-constrained and cannot emit anything that fails to parse.

Consequences:
- `JsonRepair.gd` (152 lines) is not ported. It exists only because the current code uses the legacy `format: "json"` flag (`LLMClient.gd:647`), which guarantees *syntactic* JSON but not the fields, types or enum values you asked for.
- Every `parsing_failed` branch downstream of it disappears.
- The character-agent response, director response, emotion update and ReAct step all become plain `#[derive(Deserialize, JsonSchema)]` structs.

Port the prompts from `SystemPrompts.gd` verbatim on content, but strip out the "CRITICAL: You MUST respond strictly in the following JSON format" instruction blocks and the hand-written schema text. The constraint is now enforced by the sampler; spending prompt tokens begging for it is waste, and on small models it measurably degrades the content itself.

### 2.5 Real tokenisation and honest budgeting

Load the model's actual tokenizer. Replace `estimate_tokens` (`length / 4`) with real counts.

Rewrite the budget allocator so that:
- Budgets are derived from `backend.context_length()`, not a literal.
- Allocations sum to at most 100% minus a reserved margin for the response. The current allocations sum to 105% *before* the response reservation (`PromptBuilder.gd:145-148`).
- Overflow is a typed error that the caller must handle, not a `push_warning` that is discarded (`PromptBuilder.gd:314`).

### 2.6 Native tool calling

Replace the hand-rolled ReAct loop (`GameLoopController.gd:773`) with the backend's native `tools` interface. Keep the tool count at 3-5; small models select tools far less reliably than they format the calls, and the current loop exposes four, which is already at the practical limit.

Restructure the loop itself, do not merely re-encode it. The current design runs up to 5 sequential Director calls with a 60-second budget *before* narration begins, which is a severe latency cost for work that is mostly predictable. `search_knowledge_graph` in particular should not be a tool call at all: it should be a single deterministic pre-pass (Phase 3), leaving tools for the genuinely unpredictable lookups only.

### 2.7 Cache-stable prompt ordering

Order assembled messages stable-to-volatile: static system instructions, then character card, then world state and session summaries, then recent turns, then retrieved lore, then the character's volatile state, then the player's input. Volatile content last.

Local inference engines reuse the KV cache for a shared prefix. The current builder reassembles a single monolithic string each turn, so the shared prefix is destroyed and the engine re-processes thousands of already-seen tokens on every turn. Correct ordering is free latency.

> **Corrected in Phase 5.0.** This section originally put retrieved lore *ahead* of the recent turns, on the reasoning that a repeated query keeps its prefix stable. Play never repeats the query — the Actor retrieves with the player's line — so the lore block changed on every turn and invalidated the whole transcript behind it. That was B-17, it cost roughly half the cache, and the wire test that was supposed to catch it asked the same question twice. The ordering above is the corrected one; `prompt::ordering` carries the reasoning.

**Phase 2 exit criteria**
- [ ] Both backends pass a shared conformance test suite.
- [ ] Schema-validity metric reads exactly 100% on the eval harness.
- [ ] No hardcoded context lengths anywhere in the crate.
- [ ] Prompt token counts measured with the real tokenizer; budget overflow is unrepresentable.
- [x] Measured KV-cache reuse across consecutive turns. *(Phase 5.0 built this on `prompt_eval_count`; Phase 5.6 measured that field reporting the whole prompt cached or not and rebuilt it on `TurnOutcome::prompt_eval_time`, so reuse is a measured cost rather than an inference from time-to-first-token. `tests/turn_latency.rs` asserts it rises as the transcript grows, and `tests/prefix_cache.rs` checks the backend's accounting itself. Not yet re-run live on the rebuilt metric; see eval_baseline.md, "Phase 5 measurement".)*

---

## Phase 3 — Data, knowledge graph and retrieval

**Goal**: ingest a vault and retrieve from it better than the Godot build does.

**Duration estimate**: 3-4 weeks.

### 3.1 SQLite state layer

Replace the JSON-document save model with SQLite: campaigns, characters, emotion events, inventory, plot flags, history logs, knowledge nodes and edges, embeddings, RAPTOR summaries.

Reasons beyond tidiness: the current model loads and rewrites an entire campaign document on every save, has no query capability (hence the in-memory dictionaries everywhere), and has no concurrency story (hence `CampaignState._mutex`). Schema migrations become `rusqlite_migration` rather than the bespoke version-upgrade code in `SaveManager.gd`.

### 3.2 Port the ingest pipeline

`VaultCompiler.gd` (1,337 lines) is the most valuable single file in the repository. Its heading-alias heuristics, character-property inference, asset discovery and writing-style extraction encode a great deal of hard-won knowledge about real, messy vaults. **Port its behaviour, not its structure**, and pin every heuristic with a test against the `messy/` fixture.

Improvements to make during the port, all of them noted as gaps in [orison_audit.md §11](orison_audit.md):
- Wiki-links (`[[Entity]]`) parsed and used as graph edges. Currently ignored.
- Tags (`#tag`) parsed and used for categorisation. Currently ignored.
- Tables, callouts (`> [!secret]`) and embedded images handled.
- Code fences respected, so `---` inside a fence is not mistaken for frontmatter.
- Overflow bucket: any section that cannot be mapped to a canonical field is retained and made retrievable rather than discarded. This is [rag_architecture.md §1.1](rag_architecture.md) ("Everything Must Be Ingested") and the current compiler still violates it for unrecognised sections.

### 3.3 Knowledge graph on `petgraph`

Typed nodes and typed edges, persisted in SQLite, traversed with `petgraph`. Make it authoritative: [orison_audit.md §9](orison_audit.md) records that `VaultCompiler` and `CampaignState` each maintain their own entity dictionaries and bypass the graph entirely. All entity queries route through the graph in the new design, with no parallel dictionaries.

### 3.4 Hybrid retrieval

This is the second-largest quality win in the plan, after §2.2.

Pipeline: metadata filter → parallel BM25 (`tantivy`) and dense ANN (`sqlite-vec`) → reciprocal rank fusion → cross-encoder rerank → budget-aware truncation.

Why this matters specifically for Orison: the current retrieval is dense-only and brute-force (`EmbeddingStore.get_knn` cosines every stored vector on every query). Dense embeddings are structurally weak on rare proper nouns, and a personal worldbuilding vault is *almost entirely* rare proper nouns: invented character names, place names, faction names. BM25 handles exactly that case, and fusing the two consistently beats either alone. Reranking on top is the highest-return single addition to any retrieval pipeline.

Measure each stage against `ground_truth.toml`. Add stages only where recall@k actually improves; do not adopt the pipeline on faith.

### 3.5 Port RAPTOR hierarchical summaries

Levels 0/1/2 as described in [rag_architecture.md §1.3](rag_architecture.md), preserved as designed. Replace the hand-rolled k-means (`VaultCompiler.gd:1240`) with `linfa-clustering`, and replace the round-robin partitioning fallback with a proper fallback path now that embeddings are always available in-process.

### 3.6 Chunking

The current design embeds whole nodes. Introduce explicit chunking with overlap for long notes, and record chunk-to-note provenance so retrieved context can cite its source note. This matters for both retrieval precision and for eventually showing the player *why* the story knows something.

**Phase 3 exit criteria**
- [x] `messy/` fixture ingests with zero silently dropped sections.
- [x] Retrieval recall exceeds the Godot baseline in `eval_baseline.md` on every
      fixture: `minimal` 1.000, `messy` 0.933 -> 1.000, `large` 0.875 -> 1.000,
      measured at each query's own `k`. Both documented hard queries closed.
- [x] Each retrieval stage's contribution measured and recorded, including the
      reranker's null result on recall. See
      [eval_baseline.md](eval_baseline.md), "Phase 3 measurement".
- [x] Knowledge graph is the only entity store; no parallel dictionaries exist.
      `tests/knowledge_graph.rs` greps the crate for them.

Two caveats on those ticks, recorded rather than buried. The dense half of the
retrieval pipeline is built and unit-tested but its *contribution to recall is
unmeasured*, because it needs a live embedding model; every figure above is the
lexical and structural half only. And the cross-encoder the pipeline specifies
was not built: no model weights were reachable, and a model-free stand-in ships
in its place with its own measurement.

---

## Phase 4 — Orchestration and the turn loop

> Executable brief: [handoff_phase4.md](handoff_phase4.md).

**Goal**: a complete turn, end to end, on the new stack.

**Duration estimate**: 3-4 weeks.

### 4.1 Turn state machine

Port `GameLoopController.gd` (1,069 lines) as an explicit, testable state machine on `tokio`. Requirements the current implementation lacks or handles awkwardly:
- Real cancellation of in-flight requests when the player acts again ([orison_audit.md §16](orison_audit.md)).
- Deterministic ordering guarantees under concurrency, replacing the flag-and-callback coordination noted in [orison_audit.md §12](orison_audit.md).
- The request queue's sequential-execution guarantee preserved; `test_llm_request_queue` is its specification.

### 4.2 Re-evaluate the Director/Actor split

**Run this as an experiment, not an assumption.**

The split exists because a 3B Actor could not also do Director work. Current 8B-class models are substantially more capable, and the split costs: two models resident in memory, doubled load time, cross-model consistency problems, and the entire background-director coordination apparatus including the stalling prompt (`SystemPrompts.get_director_busy_stalling_prompt`), which exists purely to paper over Director latency.

Use the eval harness to compare, on identical transcripts:
- **A**: current split (Director 8B + Actor 3B).
- **B**: single 8B model, two system prompts, two calls per turn.
- **C**: single 8B model, one call per turn with a combined schema.

Adopt whichever wins on quality-per-second. Do not preserve the split out of sunk cost, and do not collapse it out of enthusiasm. This is exactly the kind of question Phase 1 exists to answer.

### 4.3 Port the emotion engine

Consolidate properly this time. [orison_audit.md §7](orison_audit.md) found emotion logic scattered across five files with the file *named* `EmotionEngine.gd` nearly empty; it has since grown to 192 lines but the tri-dimensional model in [emotions.md](emotions.md) should live in exactly one module. Emotion updates arrive as a typed field on the constrained response, so the extraction-and-repair path disappears.

Preserve the RAG006 no-op delta skip and the RAG003 reaction debounce; both were real fixes for real bugs.

### 4.4 Port the memory tiers

`MemoryManager.gd` port. Short-term sliding window, medium-term session summaries, long-term distilled character memories, per [rag_architecture.md §1.5](rag_architecture.md). Keep biography and session memory strictly separated, which that section is emphatic about and which is easy to lose in a rewrite.

Add compaction triggers based on real token counts rather than the current turn-count thresholds (`COMPACTION_THRESHOLD = 30`), now that real counts are available.

### 4.5 Prompt assembly

Port `PromptBuilder.gd` and `SystemPrompts.gd` into `prompt/`. The boundary between them was flagged as blurry in [orison_audit.md §14](orison_audit.md); resolve it cleanly. Static templates are `const` strings or template files; assembly is a builder that takes typed state and returns an ordered message list.

Retain the `<player_message>` delimiter treatment for injection resistance. It is the right approach, and role separation reinforces it.

**Phase 4 exit criteria**
- [x] A full turn executes end to end against `OllamaBackend`, in `tests/turn_loop.rs`, against a loopback stand-in that speaks the real protocol. **`LlamaCppBackend` is untested here**: it is behind `--features llama-cpp`, which builds llama.cpp from source, and no environment has yet run it. "Both backends" is not met and Phase 5 must not treat it as met.
- [x] Cancellation verified under test, from the server's side of the socket rather than by checking a flag (`tests/turn_cancellation.rs`).
- [x] Director/Actor experiment concluded, decision recorded in Appendix D. See [Appendix D](#appendix-d--decisions-and-open-questions).
- [ ] Judge-suite scores meet or exceed the Godot baseline. The judge suite is Phase 5's; the deterministic transcript metrics are ported and run.
- [x] Turn latency p50 measured against the ~20 s baseline: **worse, not better** — 0.39x and 0.48x of the Godot p50 on `minimal` and `messy`. See [eval_baseline.md](eval_baseline.md#turn-latency--measured). This does not pass Phase 5's gate and must be the first thing Phase 5 investigates, not deferred alongside the CLI work.

---

## Phase 5 — Headless playable milestone

**Goal**: play a complete campaign in a terminal. This is the milestone that proves the migration succeeded.

**Duration estimate**: 1 week.

`orison-cli`: load or create a campaign, import a vault, converse with characters, move between locations, save and load. Streaming output. No graphics.

Then run the full evaluation harness and compare against `docs/eval_baseline.md`.

**Phase 5 exit criteria — the migration gate**
- [x] A campaign is playable start to finish through the CLI. *(`crates/orison-cli`. `tests/play.rs` creates a campaign, imports a vault, travels the mill road both ways, changes who it is addressing, holds three turns and reloads with the transcript intact — through `Shell::run`, the same function the binary hands `stdin` to.)*
- [x] Every deterministic metric meets or exceeds the Godot baseline. *(Retrieval recall 1.000 on all three fixtures against a 0.875-0.933 baseline; ingest 6/6 required substrings against 3/6; schema validity 100%. `crates/orison-cli/tests/harness.rs` runs the transcript suite through the CLI itself.)*
- [ ] Judge scores meet or exceed the baseline. **Suite built, not yet run.** `turn::judge` implements the §1.3 rubric and `harness.rs` runs it, gated on `ORISON_TEST_JUDGE_MODEL`. There is also no Godot judge baseline to compare against — Phase 1 deferred building the suite, so the baseline it would have recorded does not exist. Both halves need one machine with two models on it.
- [ ] p95 turn latency is no worse than the baseline. **`minimal` meets it, `messy` does not.** On the rebuilt metric: `minimal` p95 15.5 s against 21.2 s, `messy` p95 26.3 s against 20.2 s. `messy` is the only thing standing between this phase and Phase 6 on latency. Cache reuse is real but partial and decaying — a flat ~715-token head reused on `minimal` while the prompt doubles, nothing on `messy` — and the engine is not the difference: `tests/prefix_growth.rs` shows both fixtures building a prefix that grows every turn, with exactly one model call per turn. Whether the server reuses the shape a turn actually has is the open question; `tests/prefix_cache.rs`'s second probe answers it. See [eval_baseline.md](eval_baseline.md#turn-latency-on-the-rebuilt-metric-phase-56).

**If these are not met, do not proceed to Phase 6.** Fix the engine or reconsider the plan. A prettier shell over a worse engine is the failure mode this ordering exists to prevent.

**What closing the gate now needs** is the `messy` p95, which is a real regression against the baseline rather than a missing measurement, and the judge suite on a machine with two models. `turn_latency` has been run; see eval_baseline.md. See eval_baseline.md, "Phase 5 measurement", for the commands and for what each one would settle.

---

## Phase 6 — Tauri shell and UI

**Goal**: the application people use.

**Duration estimate**: 4-6 weeks.

**This is where design work belongs**, and not before. Two reasons: the UI's shape depends on Phase 4's Director/Actor outcome (whether there is a background process to represent at all), and local-inference UI is dominated by latency behaviour, which is only known once Phase 5 has measured it.

### 6.1 Shell scaffold

Tauri 2, stable since October 2024 and on the 2.11 line as of mid-2026, with production users including Spacedrive and AppFlowy. Typed command layer over `orison-core`; streaming deltas over Tauri's event channel.

### 6.2 Screens

Port the existing information architecture, which is sound: onboarding and vault import, LLM configuration with connection checking, campaign list, main gameplay viewport with chat and character sidebar, character detail, mind map, settings. The current 25 `.tscn` scenes are an accurate inventory of what needs to exist.

### 6.3 Design system

[design_philosophy.md](../design_philosophy.md) already specifies colour tokens, typography, spacing scale, border radii, shadows and motion timing. [orison_audit.md §22](orison_audit.md) notes only colours and font sizes were ever implemented. In CSS, the rest is nearly free: implement the full token set as custom properties.

### 6.4 Components worth buying rather than building

- Chat virtualisation: a list virtualiser, replacing `VirtualScrollContainer.gd`.
- Markdown rendering in chat: this is backlog ticket TKT027, unbuilt in Godot, and roughly a one-line import on the web.
- Mind map: Cytoscape.js or similar, replacing `CampaignGraphView.gd`. Gets search, filter, force-directed layout and minimap for free, addressing [orison_audit.md §23](orison_audit.md).

### 6.5 Accessibility

Keyboard navigation, focus management, screen-reader semantics and contrast validation, all listed as unaddressed in the audit's minor findings. The web platform makes these tractable in a way Godot's Control nodes do not.

**Phase 6 exit criteria**
- [ ] Feature parity with the Godot build across all screens.
- [ ] Full design token set implemented.
- [ ] Keyboard-navigable throughout.

---

## Phase 7 — Packaging and distribution

**Duration estimate**: 1-2 weeks.

- Signed builds for macOS (notarised), Windows and Linux (AppImage / Flatpak).
- Model acquisition flow: guided download with progress and checksum verification, not "go install Ollama and run these commands." First-run friction is where most local-AI applications lose their users. Backlog ticket OBD001 already scoped this; build it properly here.
- Tauri's built-in updater.
- A save-migration path from Godot-era saves, or an explicit statement that they do not carry (see [D-6](#appendix-d--decisions-and-open-questions)).

---

## Phase 8 — Mobile re-entry (deferred, not closed)

Mobile is not being abandoned. It is being sequenced behind the constraint that currently blocks it.

### 8.1 What this plan does now to keep it cheap later

| Decision | Why it helps mobile |
|---|---|
| `orison-core` has no platform assumptions | The engine compiles for `aarch64-apple-ios` and `aarch64-linux-android` unchanged. |
| `LlamaCppBackend` built in Phase 2 | Mobile cannot spawn a sidecar process. In-process inference is the only viable path, and it will already exist and be tested. |
| SQLite for state | Runs identically on every mobile platform. |
| Web-technology UI | A mobile shell reuses the components rather than rewriting them. |
| Model choice is configuration, never code | Swapping to a 3B mobile profile is a settings change. |

### 8.2 Re-entry criteria

Revisit when **any** of these becomes true:

1. A 3-4B model reaches acceptable narrative quality on the eval harness. Measure this periodically; it costs an afternoon and the small-model frontier moves quickly.
2. Flagship phone RAM and sustained thermal budgets make 7-8B practical.
3. The companion-client model is deemed acceptable as the mobile product: the phone is a thin client to the user's own desktop over LAN. This is still fully local and fully private, requires no on-device inference, and could be built at any time. It is currently deferred because it is a different product, not because it is technically blocked.

### 8.3 What a mobile phase would then involve

Adding `apps/mobile` as a second Tauri target, a reduced UI for small screens, a mobile model profile, and thermal and battery management. Explicitly *not* a rewrite. If that turns out to be false, the Flutter path via `flutter_rust_bridge` remains open precisely because the engine is a standalone crate.

---

## Appendix A — Component port map

| Godot | Lines | Destination | Notes |
|---|---:|---|---|
| `VaultCompiler.gd` | 1,337 | `ingest/` | Highest-value port. Preserve heuristics, pin with `messy/` fixture. |
| `GameLoopController.gd` | 1,069 | `turn/` | Becomes an explicit state machine. |
| `LLMClient.gd` | 902 | `inference/` | Mostly deleted; replaced by crates. |
| `OnboardingFlow.gd` | 1,150 | `apps/desktop` | UI only. |
| `MainViewport.gd` | 865 | `apps/desktop` | UI only. |
| `CampaignState.gd` | 757 | `state/` | JSON document → SQLite tables. |
| `CampaignGraphView.gd` | 765 | `apps/desktop` | Replaced by a graph library. |
| `ThemeManager.gd` | 741 | `apps/desktop` | Replaced by CSS custom properties. |
| `SystemPrompts.gd` | 547 | `prompt/templates/` | Content survives; schema text stripped. |
| `ImageGenManager.gd` | 543 | `media/` | Keep the Draw Things / A1111 contract. |
| `PromptBuilder.gd` | 522 | `prompt/` | Rewrite budgeting against real tokens. |
| `ProceduralArtEngine.gd` | 413 | `apps/desktop` | Canvas or SVG. |
| `KnowledgeGraphManager.gd` | 347 | `knowledge/` | Becomes authoritative. |
| `VirtualScrollContainer.gd` | 296 | — | Deleted; use a list virtualiser. |
| `MarkdownParser.gd` | 271 | `ingest/` | Deleted; use `pulldown-cmark`. |
| `SaveManager.gd` | 260 | `state/` | Deleted; use `rusqlite_migration`. |
| `LLMStreamRequest.gd` | 243 | — | Deleted; use `reqwest`. |
| `MemoryManager.gd` | 229 | `memory/` | Direct port; thresholds become token-based. |
| `PlayerInputParser.gd` | 207 | `turn/` | Direct port. Visual-novel syntax rules preserved. |
| `EmotionEngine.gd` | 192 | `turn/emotion/` | Consolidate all emotion logic here. |
| `JsonRepair.gd` | 152 | — | Deleted; obsoleted by constrained decoding. |
| `EmbeddingStore.gd` | 112 | `retrieval/` | Deleted; use `sqlite-vec`. |
| `TestRunnerNode.gd` | 3,750 | `tests/` | Not ported. The 46 test cases become the spec for `cargo test`. |

---

## Appendix B — Defects to fix during the port

### B-0: Defect register

Every defect below is scheduled against the phase that resolves it. Three are fixed in the current Godot build during Phase 0 because they are cheap, independent of the migration, and deliver value even if the migration never happens. The rest are resolved *by* the port rather than before it: fixing them in GDScript would mean paying for the same work twice.

Update the Status column as work lands. Once Phase 0.5 moves tickets to GitHub Issues, mirror these there and keep this table as the index.

| ID | Defect | Severity | Fixed in | Where | Status |
|---|---|---|---|---|---|
| B-1 | NPC prompts budgeted at ~2.1x the served context window | **Critical** | **Phase 0.2** | Godot build, now | **Fixed** |
| B-11 | No continuous integration | **Critical** | **Phase 0.1** | Repository, now | **Fixed** |
| B-12 | 62 absolute filesystem paths in documentation | Minor | **Phase 0.3** | Repository, now | **Fixed** |
| B-2 | Chat template bypassed (`/api/generate`, concatenated prompt) | **Critical** | Phase 2.2 | Port | Open |
| B-3 | Legacy `format: "json"` instead of schema-constrained decoding | Major | Phase 2.4 | Port | Open |
| B-4 | Token counting by character division | Major | Phase 2.5 | Port | Open |
| B-6 | Both models pinned in VRAM indefinitely (`keep_alive: -1`) | Major | Phase 2.2 | Port | Open |
| B-8 | Up to 5 sequential Director calls before narration begins | Major | Phase 2.6 | Port | Open |
| B-10 | Stale model recommendations hardcoded rather than configured | Major | Phase 2.2 | Port | Open |
| B-5 | Frame-coupled HTTP streaming | Moderate | Phase 2.2 | Port | Open |
| B-9 | Dense-only retrieval on a proper-noun-dense corpus | Major | Phase 3.4 | Port | **Fixed** (BM25 via `tantivy` fused with dense ANN; `"Quillion"` retrieves at rank 1 with no model) |
| B-7 | Brute-force vector search over an in-memory JSON dictionary | Moderate | Phase 3.4 | Port | **Fixed** (`sqlite-vec` ANN in the campaign database) |
| B-13 | Lexical retrieval condition is inverted; 13 of 14 baseline queries retrieve nothing | **Critical** | **Phase 1** | Godot build | **Fixed** (recall 0.00-0.20 -> 0.88-1.00) |
| B-14 | Character nodes discard raw source text entirely | **Critical** | **Phase 1** | Godot build | **Fixed** (ingest 3/6 -> 6/6) |
| B-15 | Engine reports success after total model failure; an unreachable model degrades silently | **Critical** | Phase 2.2 | Port | Open |
| B-16 | Character extraction never used JsonRepair, so any fenced JSON response failed | **Critical** | **Phase 1** | Godot build | **Fixed** (fields empty -> all populated) |
| B-17 | Retrieved lore ordered ahead of the growing transcript, so the KV-cache prefix broke every turn | **Critical** | **Phase 5.0** | **Port** | **Fixed** (lore moved into the volatile tail) |
| B-18 | `OllamaBackend` asked for the model's full advertised context window as `num_ctx` | **Critical** | **Phase 5.0** | **Port** | **Fixed** (capped at `DEFAULT_CONTEXT_LIMIT`, overridable) |
| B-19 | A Director beat writes back the campaign row it read before its model call, reverting any move or change of speaker made while it composed | **Critical** | **Phase 5.6** | **Port** | **Fixed** (`compose_beat` reloads after the call; `tests/movement.rs`) |

**B-17 and B-18 are the first two defects in this register that the port
introduced rather than inherited**, which is why they are worth the same
treatment as the rest. Both were invisible to `cargo test` and both were paid
for on every single turn: together they are the best explanation available for
Phase 4's live turn latency landing at roughly 2x the engine it was meant to
beat. Neither was found by reading. They were found by asking why a measured
number disagreed with a passing test, which is the only reason this register
has entries at all.

**B-19 is the third, and it was found by playing rather than by measuring.** A
beat composed in the background wrote back the campaign row it had read before
its model call, so a `/go` or a `/talk` made while the Director was thinking
was silently reverted: a session that walked to Thornwick Archive and changed
speaker reopened at Stonebridge. `save_campaign` writes the whole row, and
`compose_beat` was the one caller whose read and write straddled an `await`.

The reason no test could have caught it is worth recording separately:
`FakeOllama` answered a non-streamed `/api/chat` — the path the Director uses —
in microseconds, which closes the entire window in which a player can act while
a beat composes. A stand-in that is faster than the thing it stands in for
cannot exercise concurrency. It now waits what the streamed path would wait.

**Why B-1 and B-11 are not deferred to the port.** B-1 is plausibly the cause of a user-visible bug that has been open since June, and the fix is small. B-11 means nothing currently protects against regressions, including regressions introduced while fixing B-1. Both are prerequisites for trusting any measurement taken in Phase 1, which in turn is what the entire migration is graded against.

**Why the rest wait.** B-2 through B-10 all live in the inference and retrieval layers, which Phase 2 and Phase 3 replace wholesale. Fixing them in GDScript first would mean writing each fix twice and would delay the baseline in §1.4 for no gain. The Godot baseline is *supposed* to include these defects: that is what makes the post-migration comparison meaningful.

### B-1: NPC prompts are budgeted at twice their actual context window

**Severity: critical. Fix in Phase 0, before the migration.**

`PromptBuilder.build_prompt()` (`PromptBuilder.gd:142`) assembles the character-agent prompt against `context_limit = 8192`.

`LLMClient.send_prompt()` (`LLMClient.gd:214`) sends the character model with `num_ctx = 4096`:

```gdscript
var active_ctx = 4096 if active_model == character_model else 8192
```

So every NPC turn is budgeted for twice the context it is actually served. Compounding it, the budget allocations sum to 105% of the assumed limit before any response reservation:

```gdscript
var sys_budget  = int(context_limit * 0.30)
var id_budget   = int(context_limit * 0.30)
var hist_budget = int(context_limit * 0.30)
var lore_budget = int(context_limit * 0.15)   # 0.30+0.30+0.30+0.15 = 1.05
```

Worst case is roughly 2.1x the served window. The model silently drops the beginning of the context, which is the system prompt and the character profile. This is a highly plausible direct cause of the long-standing "NPC breaks character" behaviour, and of [rag_architecture.md](rag_architecture.md) Bug 5.

There is a second-order trap in the same line: `active_ctx` is chosen by comparing the model name to `character_model`. If a user configures the same model for both roles, which is a sensible configuration and exactly what the Phase 4.2 experiment tests, the Director silently receives 4096 while its builder budgets for 8192.

**Fix**: query context length from the backend; derive all budgets from that value; make allocations sum to at most 100% minus a response reservation; make overflow an error rather than a discarded warning.

### B-2: Chat template bypassed

Every call uses `/api/generate` with a concatenated prompt string. See §2.2.

### B-3: Legacy JSON mode instead of schema constraint

`LLMClient.gd:647` sets `format: "json"`, guaranteeing syntax but not fields, types or enum values. `JsonRepair.gd` exists to clean up the difference. See §2.4.

### B-4: Token counting by character division

`PromptBuilder.gd:26`, `length / 4`. Error compounds with non-Latin text, proper nouns and structured markup, all of which Orison's prompts are dense in. See §2.5.

### B-5: Frame-coupled HTTP streaming

`LLMClient.gd:77` polls `HTTPClient` from `_process(delta)`. Throughput tracks frame rate; timeouts accumulate in frame deltas. Resolved by moving to `tokio`.

### B-6: Both models pinned indefinitely

`keep_alive: -1` on every request path, for both models. On a consumer GPU this permanently occupies VRAM that image generation also wants, which is the exact VRAM-wall risk [proposal.md §5.2](proposal.md) identified and then did not mitigate.

### B-7: Brute-force vector search

`EmbeddingStore.get_knn` computes cosine similarity against every stored vector, with all vectors held in a `Dictionary` and persisted as a single JSON file. Acceptable at a few hundred nodes; fails at vault scale, which is the `large/` fixture's purpose. See §3.4.

### B-8: Pre-narration research loop latency

`GameLoopController.gd:773` runs up to 5 sequential Director calls with a 60-second budget before narration begins. See §2.6.

### B-9: Dense-only retrieval on a proper-noun-dense corpus

See §3.4.

### B-10: Stale model recommendations

`README.md` recommends Llama 3.1 8B and Llama 3.2 3B; `rag_architecture.md` references `gemma4:e4b`. Current guidance for an 8GB consumer GPU centres on Qwen3 8B at Q4_K_M. More important than the specific name: **model identifiers must be configuration with a recommended default profile, never constants in code.** They will be stale again within a year, and the migration should make that a settings update rather than a code change.

### B-16: Character extraction never called JsonRepair

**Severity: critical. This is the actual root cause of Bug 2, and it was a
one-line integration miss, not a prompt problem.**

`VaultCompiler._extract_character_data_via_llm` parsed the model's reply with
`JSON.new().parse()` directly. The file referenced `JsonRepair` **zero times**,
despite `JsonRepair.extract_json` existing for exactly this purpose and already
stripping markdown code fences at `JsonRepair.gd:117`.

Models fence their JSON by default. In a live run against `gemma4:e2b`, **every
single character failed to parse**, and in every case the payload inside the
fence was perfectly valid:

```
WARNING: [VaultCompiler] Failed to parse LLM response JSON for King Yuna. Response was: ```json
{
  "biography": "Yuna took the salt throne at nineteen after his three older brothers drowned...",
  "personality": "Blunt to the point of rudeness. Dislikes ceremony and cuts speeches short.",
  ...
}
```
```

Every character therefore compiled with no personality, no appearance and no
goals, on any model that fences its output. Combined with B-14 (raw source
discarded), that data was then unrecoverable.

[rag_architecture.md](rag_architecture.md) Bug 2 diagnosed this symptom in June
as "extracted fields are never used in the NPC prompt" and attributed it to the
prompt's CHARACTER PROFILE block being too thin. That was treating a downstream
symptom: the fields were empty because extraction silently failed on every file.

**Fixed.** Extraction now routes through `JsonRepair.extract_json`. Verified by
replaying the recorded baseline cassette, which contains the real fenced
responses: `entity_fields_populated` went from 7 empty fields on `minimal` and 4
on `messy` to all populated on both.

**Why the harness caught it and six months of play did not**: the failure was a
`push_warning`, invisible outside the editor, and the compiled campaign looked
structurally fine. Every entity was present. Only their contents were empty, and
nothing asserted on contents until there was a fixture with known-correct
expected fields.

### B-15: A dead model degrades silently and reports success

**Severity: critical. Found by the first real live recording run.**

The configured Director model (`gemma4:e4b`) was not installed. Every call
returned HTTP 404. The engine's response:

- `VaultCompiler._extract_character_data_via_llm` logs a `push_warning` and
  returns an empty result, so every character compiles with blank fields.
- `_generate_raptor_summaries` prints
  `"RAPTOR summaries generated successfully. L1 count: 3, L2 count: 1"`
  unconditionally, **after all four of its summary calls 404'd**. The counts are
  from the round-robin fallback path.
- Compilation reports success. Nothing surfaces to the user.

So a campaign compiled against a missing Director model looks like it worked and
produces characters with no personality, no appearance, no goals, and fallback
summaries instead of RAPTOR ones. `push_warning` is invisible outside the editor
(the same problem as B-1's discarded overflow warning).

This is worth more than its immediate cause. The configured model here is the one
named in `rag_architecture.md`, so it is plausible the Director has been silently
non-functional for some time, which would independently explain gameplay feeling
flat regardless of prompt quality.

**Fix**: Phase 2.2. The backend trait verifies model availability at connection
time and surfaces an unreachable model as a typed error the caller must handle,
not a warning. Success messages must be conditional on the work actually
succeeding.

**Mitigated now** in the eval harness, which preflights every configured model
and refuses to start a live recording run if one does not answer. A baseline
recorded against a dead model is worse than no baseline, because it looks
complete.

### B-13: Lexical retrieval is inverted and searches nothing

**Severity: critical. Found by the Phase 1 harness before it had even finished
being written.**

`KnowledgeGraphManager.gd:179`:

```gdscript
if normalized_prompt.contains(label) or normalized_prompt.contains(id.to_lower()):
```

This asks whether the **query contains the node's label**, not whether the node
matches the query. Lexical retrieval therefore fires only when the player types
an entity's full name inside their sentence, and note *bodies* are never searched
at all: only labels and ids.

Measured on the Phase 1 fixtures (see [eval_baseline.md](eval_baseline.md)):

| Fixture | Mean recall | Queries returning nothing |
|---|---:|---:|
| `minimal` | 0.200 | 4 of 5 |
| `messy` | 0.000 | 5 of 5 |
| `large` | 0.000 | 4 of 4 |

The one non-zero score is a positive control written to embed a label verbatim,
which proves the graph is loaded and the inverted condition is the cause.

Compounding it, **there is no BM25 implementation anywhere in the codebase**,
despite the retired feature map advertising "Hybrid BM25 + KNN semantic
retrieval". The reciprocal rank fusion is real, but one input is this near-dead
lexical path and the other needs `nomic-embed-text` installed. Without an
embedding model, retrieval returns nothing at all.

**Fixed in the Godot build**, ahead of the port, because retrieval returning
nothing is not a degraded feature but an absent one. The containment test is
replaced with term-overlap scoring over each node's label, id, tags, description
and body, with stopword filtering and label-weighted scoring. Recall went from
0.00-0.20 to 0.88-1.00 across the three fixtures, with zero queries returning
nothing.

This is **not** BM25 and does not close Phase 3.4. There is no corpus-wide IDF
and no length normalisation. Precision is now the weak point: on `large`, one
query retrieves 74 of 207 nodes alongside the correct answer, because term
overlap plus one-degree neighbour expansion casts very wide. That is what a
cross-encoder reranker exists to fix, so Phase 3.4's argument is strengthened,
not weakened. Two-hop queries also still fail.

### B-14: Character nodes discard their raw source text

**Severity: critical. Architectural, not a typo.**

`VaultCompiler.gd:254` creates character nodes with frontmatter as properties and
the LLM-extracted biography as `desc`. Scenes and locations retain the original
prose in `props["body"]` (lines 515, 548). **Characters retain nothing.**

Whatever the single extraction pass misses is unrecoverable, because the source
is never stored. There is no overflow bucket and no fallback. A character whose
file confuses the extractor is simply blank forever, and nothing downstream can
tell the difference between "this character has no personality written" and "the
extractor failed on this file".

This directly violates [rag_architecture.md](rag_architecture.md) §1.1, which is
unambiguous: "No information should ever be silently discarded. If a file section
cannot be parsed into a canonical field, its content must still be available to
the system in some form."

It also removes the safety net from the entire Bug 1 class of heading-parsing
failures. Those bugs are only catastrophic *because* of this one.

**Fixed in the Godot build.** `VaultCompiler` now retains the full source body on
every node type, so extraction is additive to the source rather than a
replacement for it. Ingest completeness on the `messy` fixture went from 3/6 to
6/6.

Phase 3.2 still owns the proper version: chunked, with overlap and provenance, so
retrieval can cite the source note. What landed here is the safety net, not the
retrieval-quality work.

### B-11: No continuous integration

No `.github/`. See §0.1.

### B-12: Non-portable documentation

62 absolute paths. See §0.3.

---

## Appendix C — What carries over unchanged

Migration does not mean starting over. These survive intact and represent most of the project's accumulated thinking:

- **Every prompt in `SystemPrompts.gd`.** The persona rules, the eavesdropping and stealth handling, the creature and non-speaking-entity variants, the anti-sycophancy rule, the first-person-dialogue / third-person-narration boundary, the injection-resistant `<player_message>` framing. These are the product. Only the JSON-format instruction blocks are dropped, and only because the sampler now enforces them.
- **The response schemas.** Re-expressed as Rust types; semantically identical.
- **The tri-dimensional emotion model** in [emotions.md](emotions.md).
- **The retrieval philosophy** in [rag_architecture.md](rag_architecture.md), including the Director/Actor rationale, the biography-versus-session-memory distinction, and the hierarchical-retrieval argument. Phase 4.2 tests one of its conclusions; it does not discard the reasoning.
- **The vault-compiler heuristics**: heading aliases, character-property inference, asset discovery, writing-style extraction. Hard-won against real vaults, and not re-derivable from first principles.
- **The design system** in [design_philosophy.md](../design_philosophy.md), which becomes *more* implementable in CSS than it ever was in Godot themes.
- **The 46 test cases** in `TestRunnerNode.gd`, as a written specification.
- **The ticket corpus.** The OBD, STT and TKT series describe product intent that is independent of platform.
- **`done.md`**, as the historical record of what was tried and why.

---

## Appendix D — Decisions and open questions

| ID | Question | Status |
|---|---|---|
| D-1 | Shell framework | **Decided**: Tauri 2. Reversible by design; the engine is a standalone crate. |
| D-2 | Mobile | **Decided**: deferred, criteria in Phase 8. |
| D-3 | Default inference backend | **Decided**: Ollama for onboarding ease, `llama.cpp` in-process available from Phase 2 and likely the eventual default once model acquisition is guided. |
| D-4 | Director/Actor split | **Decided**: keep the split (arm A). The two flagged narrations were read in Phase 5.0 and are both false positives; arm A wins outright on both fixtures once discounted. See below. |
| D-5 | Frontend framework inside Tauri | **Open**: defer to Phase 6. Not load-bearing. |
| D-6 | Godot-era save compatibility | **Open**: a one-shot JSON-to-SQLite importer is cheap; whether it is worth writing depends on whether any saves worth keeping exist. Decide before Phase 3.1. |
| D-7 | Image generation | **Open**: keep the Draw Things / A1111 HTTP contract as-is, or reconsider given VRAM contention with two resident LLMs. Revisit after D-4. |
| D-8 | Reranker model | **Open**: which cross-encoder is small enough to run locally without materially hurting turn latency. Measure in Phase 3.4. |

### D-4 — the Director/Actor experiment, measured

Run 9 Sep on Apple Silicon, Director/Single `qwen2.5:7b-instruct` (7B, the
closest available to the 8B class this experiment calls for — no true 8B
model was on hand; re-run with one before treating this as final), Actor
`llama3.2:3b`.

| Arm | Fixture | parsed | prn | rep | bad | p50 | p95 | quality | q/sec |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| A: split | minimal | 4/4 | 1 | 0 | 0 | 20.8 s | 39.1 s | 0.938 | **0.0420** |
| A: split | messy | 5/5 | 2 | 0 | 0 | 24.2 s | 64.3 s | 0.900 | **0.0268** |
| B: one model, two calls | minimal | 4/4 | 0 | 0 | 0 | 39.0 s | 99.1 s | 1.000 | 0.0191 |
| B: one model, two calls | messy | 5/5 | 0 | 0 | 0 | 35.2 s | 54.7 s | 1.000 | **0.0268** |
| C: one model, one call | minimal | 4/4 | 0 | 0 | 0 | 55.6 s | 102.3 s | 1.000 | 0.0154 |
| C: one model, one call | messy | 5/5 | 0 | 0 | 0 | 61.2 s | 69.6 s | 1.000 | 0.0166 |

Per the rule this handoff set in advance — adopt whichever wins on
quality-per-second — **arm A wins**: outright on `minimal`, tied with B on
`messy`, and never behind. That makes the split the mechanical decision.

**The catch, stated so it isn't buried under the winning number.** Arm A is
the only arm with pronoun flags — 1 on `minimal`, 2 on `messy` — which is
exactly why its quality composite (0.938 / 0.900) trails B and C's clean
1.000. The scoring's own legend is explicit that a pronoun flag needs a human
read: "the metric cannot tell a wrong pronoun for the speaker from a right one
for a third party." The two flagged narrations:

> `minimal` turn 3: "She's been here for nineteen years, ever since the
> flooding of the lower library at Ashmere. It's a bit of a trek up to
> Thornwick, but she's been happy to have this place as her home."

> `messy` turn 1: "Sergeant Adah stepped forward from the nearby jetty, her
> eyes narrowing slightly as she took in the traveler's worn leather satchel.
> She eyed Lord Anneke with a mixture of curiosity and wariness, her hand
> resting on the hilt of her sword at her side."

### D-4 — the human read, done (Phase 5.0)

**Both flags are false positives. D-4 stands as decided: keep the split.**

The read is not a judgement call, because both fixtures declare who the
speaker is and what their pronouns are. `forbidden_pronouns` in
`TranscriptScript` means "pronouns that must not appear in narration *about
the transcript character*" — not "pronouns that must not appear at all".

- **`minimal`.** `transcript_character` is **Bram Holt**, `gender: male,
  he/him`, so she/her/hers are forbidden of *him*. The flagged narration
  answers the script's fourth line, "Who keeps the archive up the road?", and
  its subject is Elara Voss: nineteen years, the flooding of the lower library
  at Ashmere, Thornwick — three details taken verbatim from
  `characters/elara_voss.md`, whose frontmatter is `gender: female, she/her`.
  Bram is the speaker and takes no pronoun in the passage. A right pronoun for
  a third party, which is the exact case the legend names.

- **`messy`.** `transcript_character` is **Lord Anneke**, whom the fixture
  note calls "the pronoun trap": no gender field, an unambiguously masculine
  body, and a name carrying a strong feminine prior. Every she/her in the
  flagged narration attaches to Sergeant Adah, `gender: female, she/her`.
  Lord Anneke is named twice and pronominalised never — so the model did not
  fall into the trap; it stayed out of it. Also a right pronoun for a third
  party.

**And the counts are smaller than they look.** `score_turn` raises one flag
per *distinct forbidden pronoun* matched in a turn, so `messy`'s 2 is one
narration matching both "she" and "her", not two separate defects. `minimal`'s
1 is the single "her" in "her home"; "she's" does not match, because
`contains_word` treats an apostrophe as a word character. Two fixtures, two
flagged turns, zero defects.

**What this does to the decision.** Discounting both, arm A's quality
composite is 1.000 on each fixture, level with B and C, and its
quality-per-second rises to **0.0448** on `minimal` and **0.0298** on `messy`.
Arm A now wins **outright on both fixtures** rather than winning one and tying
the other. The cross-model consistency problem the split was warned to cost
did not appear in this run.

**One thing the earlier framing had wrong**, recorded because the correction
is the evidence: this appendix described both narrations as "a woman using
she/her in a scene with another woman present." Neither is. `minimal` has
exactly one woman in the entire fixture and she is not in the scene, she is
being described; `messy` pairs a woman with a man. The ambiguity the read was
asked to resolve was not there to resolve.

**What is still open**, unchanged by the read: the models. `qwen2.5:7b-instruct`
stood in for the 8B class in both the Director and the single-model arms
because no true 8B model was on hand. A 7B stand-in flatters the split, since
the split's cost is paid by the larger model and its benefit by the smaller.
Re-run with an actual 8B-class model before treating the margin as final. The
decision is the split; the size of the win is not yet.

**The arms are configuration, not code paths**, which the handoff asks for
explicitly and which B-10 makes more than a style preference: a build that
branched on a model name would be wrong the moment both roles were configured
to the same model, and that configuration is arm B.

| Arm | Configuration | Code path |
|---|---|---|
| A | Director 8B + Actor 3B, two calls | `TurnProfile::TwoCalls`, two backends |
| B | One 8B model, two system prompts, two calls | `TurnProfile::TwoCalls`, one backend twice |
| C | One 8B model, one call, `CombinedTurnResponse` | `TurnProfile::SingleCall` |

**To run it**, on a machine with the models pulled:

```bash
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_EXPERIMENT_DIRECTOR_MODEL=<8b-model> \
ORISON_EXPERIMENT_ACTOR_MODEL=<3b-model> \
ORISON_EXPERIMENT_SINGLE_MODEL=<8b-model> \
  cargo test -p orison-core --test director_actor_experiment -- --nocapture
```

It prints one row per arm per fixture: schema validity, pronoun flags,
verbatim repeats, forbidden phrasing, p50 and p95 latency, a quality composite
and quality per second. The metrics are the same five
`eval/EvalRunnerNode.gd` records, so the numbers are comparable with the Godot
narrative baseline rather than merely plausible. The scoring is unit-tested
against transcripts that should trigger each metric, and the whole harness is
exercised against a loopback stand-in in `cargo test`, so a live run tests the
models rather than the harness.

**What the experiment cannot settle**, stated so the eventual decision is not
overclaimed: quality here is a deterministic composite of defect rates. It
catches the failures that have actually happened in this project — Bug 3's
pronouns, conversation loops, leaked stats, empty responses — and it says
nothing about whether a scene was any good. The judge suite that would is
Phase 5's. If two arms come out close on quality per second, that is a tie the
judge suite has to break, not a result.

**One argument that has already changed**, and it favours C. §2.6 says
`search_knowledge_graph` should be a deterministic pre-pass rather than a tool
call. Phase 3 built that pre-pass and it needs no model: `retrieve()` without
a dense index returns in microseconds. A large part of the Director's original
job was research it can no longer justify doing conversationally, and the
`ReActSignalCarrier` that existed only to make a callback awaitable is not
ported. That lowers the cost of collapsing the split but is not by itself a
reason to; it is the reason C is worth measuring rather than dismissing.

---

## Summary of ordering

| Phase | Outcome | Estimate |
|---|---|---|
| 0 | Repository safe to work in; B-1 fixed; CI green | 1-2 days |
| 1 | Quality is measurable; Godot baseline recorded | 1-2 weeks |
| 2 | Correct, modern inference layer | 2-3 weeks |
| 3 | Ingest and retrieval beating the baseline | 3-4 weeks |
| 4 | Full turn loop on the new stack | 3-4 weeks |
| 5 | **Playable headless. Migration gate.** | 1 week |
| 6 | Tauri shell. **Design work starts here.** | 4-6 weeks |
| 7 | Signed, distributable, guided first run | 1-2 weeks |
| 8 | Mobile, on criteria | deferred |

Phases 0 and 1 deliver value even if the migration is never carried out. That is deliberate: the plan should not require an all-or-nothing commitment on day one.
