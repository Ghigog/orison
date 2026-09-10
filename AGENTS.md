# AGENTS.md

Guidance for anyone working in this repository, human or AI. This is the single
canonical entry point; `CLAUDE.md` points here and adds nothing.

---

## What Orison is

Orison ingests a Markdown archive of worldbuilding notes (typically an Obsidian
vault), compiles it into a knowledge graph, and uses local language models to run
an interactive D&D or visual-novel style adventure through it. Built in Godot 4.6
with GDScript.

Everything runs on the user's machine. There is no server component and no
account.

---

## The one inviolable constraint

**No vault content, gameplay text, or user data ever leaves the machine.**

Inference runs against a local endpoint the user configures, by default Ollama on
`127.0.0.1:11434`, and optionally a local Stable Diffusion API on
`127.0.0.1:7860`. There is no telemetry, no analytics, no crash reporting, no
cloud fallback when a local model is missing or slow.

If a change would introduce an outbound network call to anything other than a
user-configured local endpoint, it is wrong. This is a product pillar, not an
implementation detail.

---

## Current status: feature freeze

The Godot build is in maintenance. A migration to a Rust core with a Tauri 2
desktop shell is planned and documented in
[docs/migration_plan.md](docs/migration_plan.md).

**Bug fixes only.** Every feature added to the Godot build now is a feature that
must be built twice. If you believe something warrants an exception, raise it
rather than assuming.

Known defects are catalogued in the migration plan, Appendix B, each scheduled
against the phase that resolves it. Read that register before reporting or
fixing anything in the inference or retrieval layers; several known-wrong things
are deliberately left alone until the port.

---

## Running things

```bash
# Full test suite. Boots a scene, because autoloads must exist in the tree.
godot --headless --path . res://tests/TestRunner.tscn

# On a cold checkout, import assets first if the suite misbehaves.
godot --headless --path . --import

# The Rust core and the headless client. No model, no network.
cargo test --workspace

# Play a campaign. Needs a local Ollama and a model pulled.
cargo run -p orison-cli -- new --title "Thornwick" --vault fixtures/vaults/minimal
cargo run -p orison-cli -- play thornwick --actor-model llama3.2:3b
```

The runner discovers every method named `test_*` on `TestRunnerNode`, runs them
in sorted order with `await` support, prints `[PASS]` / `[FAIL]` per test and a
summary, and exits non-zero if any test failed.

Tests must be hermetic: no live Ollama, no live image-generation backend, no
reliance on a warm `user://` directory. `LLMClient.mock_response_handler` and
`ImageGenClient.mock_handler` exist for this. Three tests were previously
environment-dependent and had to be repaired; do not add a fourth.

---

## Layout and responsibilities

```
crates/         The Rust core (orison-core) and the headless client (orison-cli)
src/autoload/   Global singletons, registered in project.godot
src/core/       Engine logic with no scene dependencies
src/resources/  Typed Resource models
src/ui/         Controllers bound to scenes in scenes/ui/
scenes/ui/      Scene trees (.tscn)
resources/      Themes and static assets
tests/          TestRunner scene and the suite
docs/           Design documents and planning
```

**Autoloads.** `EventBus` (global signals), `CampaignState` (live campaign state
and saves), `LLMClient` (inference endpoint, request queue, config),
`ThemeManager` (design tokens, font scaling), `ImageGenClient` and
`ImageGenManager` (local Stable Diffusion plus procedural fallback),
`MediaManager` (audio), `EmbeddingStore` (vector store for semantic retrieval).

**Ingest.** `VaultScanner` walks the vault, `MarkdownParser` handles frontmatter,
wiki-links, tags, callouts and tables, and `VaultCompiler` turns the result into
knowledge-graph nodes and edges. Compilation also runs LLM-based field extraction
per character file and builds two levels of RAPTOR summary nodes. `VaultCompiler`
is the highest-value file in the repository; its heading aliases and property
inference encode a lot of hard-won knowledge about real, messy vaults.

**Knowledge and retrieval.** `KnowledgeGraphManager` is the authoritative store
for entities, relationships and tags, and does the retrieval that feeds prompts.
Do not reintroduce parallel entity dictionaries elsewhere.

**The Rust core.** `crates/orison-core` is the migration target and is built
alongside the Godot build, not instead of it yet. It has no UI and no platform
assumptions. Run its suite with `cargo test --workspace`; CI runs `cargo fmt
--check`, `cargo clippy --workspace --all-targets -- -D warnings` and the tests.
Everything in it runs without a model or a network by default. Cases that need a
live endpoint are gated on environment variables and **skip loudly** rather than
silently passing:

| Variable | Gates |
|---|---|
| `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_OLLAMA_MODEL` | Phase 2's live chat conformance cases, a live turn, and `tests/turn_latency.rs` |
| `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_OLLAMA_EMBED_MODEL` | The dense half of retrieval (a chat model id is not an embedding model id) |
| `ORISON_EXPERIMENT_DIRECTOR_MODEL` + `_ACTOR_MODEL` + `_SINGLE_MODEL` | The Director/Actor experiment (migration plan §4.2) |
| `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_JUDGE_MODEL` (+ `ORISON_TEST_ACTOR_MODEL`) | The judge suite, in `crates/orison-cli/tests/harness.rs`. The judge should be a larger model than the one under test |
| `ORISON_TEST_VAULT` | `tests/real_vault.rs`: ingest a real Obsidian vault and report. Prints counts and note paths, never vault content |
| `--features llama-cpp` | `LlamaCppBackend`; builds llama.cpp from source, so it is off by default |
| `--features test-support` | `orison_core::testing`: the stand-in Ollama and fixture response bodies. Enabled by the crates' own dev-dependencies, never by a release build |

**The headless client.** `crates/orison-cli` is Phase 5's playable milestone:
create a campaign, import a vault, converse with streaming output, travel
between locations, save and load. It is a library as well as a binary, because
the evaluation harness runs *against* it — `shell::Shell` reads any `BufRead`
and writes any `Write`, so a scripted transcript plays through exactly the code
a person types into. It is built against `TurnEngine` and `TurnEvent` and
nothing under them; if a shell needs something the engine does not expose, the
answer is to add it to the engine, not to reach past it. Which models play
which role is configuration (B-10) and never a comparison against a model name.

**The Rust core's turn loop.** `crates/orison-core/src/turn/` is the port:
`state` owns which transitions are legal, `queue` sequential execution and
real cancellation, `engine` the turn itself. `emotion/` is the single home of
the model in [docs/emotions.md](docs/emotions.md); `memory/` owns the three
tiers and never touches a biography; `prompt/` splits into `templates` (every
word the model is told, and it may not read state) and `assembly` (typed state
into blocks). `tests/prompt_boundary.rs` enforces that split. `experiment/` scores an arm on
what a program can check; `judge/` scores it on the §1.3 rubric with a model,
and gates nothing — the deterministic suite is the gate.

**Turn loop (Godot).** `GameLoopController` orchestrates a turn: player input through
`PlayerInputParser`, an agentic Director research loop over the knowledge graph,
a streaming Character Agent response parsed by `LLMStreamParser`, emotion updates
via `EmotionEngine`, and memory compaction via `MemoryManager`.

**Prompts.** Strict division, and it matters: `SystemPrompts` holds static
templates, persona rules and response schemas, and is state-free. `PromptBuilder`
gathers runtime state and assembles the final prompt within a token budget.
Static text goes in the former, anything that reads `CampaignState` goes in the
latter.

**Two model roles.** The Director (world builder) manages world state, scene
transitions and narrative beats. The Actor (character agent) speaks in character.
See [docs/rag_architecture.md](docs/rag_architecture.md) for the rationale; note
that the migration plan schedules an experiment on whether the split still earns
its cost.

---

## Rules

### Locating code

Search for it. Do not expect a map of file paths to line numbers to be accurate;
one used to exist here and rotted. When you add a system, give it a clear name
and put it where the layout above says it goes.

### Context lengths and token budgets

`LLMClient` owns context window sizes and exposes `get_context_length(role)` and
`get_prompt_budget(role)`. **Never hardcode a context size anywhere else**, and
never select one by comparing a model name. Pass the explicit role
(`LLMClient.ROLE_CHARACTER` or `ROLE_WORLD_BUILDER`), because both roles can be
configured to the same model, in which case the name says nothing about which
window applies.

Budget fractions in `PromptBuilder` must sum to at most 1.0, against
`get_prompt_budget()`, which already excludes the response reserve. Getting this
wrong is invisible at runtime and corrupts every response, because the model
silently drops the front of the context, which is the system prompt.

### Prompt injection

Player text is wrapped in `<player_message>` delimiters and sanitized by
`PlayerInputParser.sanitize_input`. The prompts instruct the model to treat
everything inside as in-character speech or action. Preserve this whenever you
touch input handling or prompt assembly.

### UI

1. **Scene-first.** Build UI in `.tscn` files and bind with `@onready`. Do not
   assemble container hierarchies programmatically with `.new()`.
2. **Responsive.** Use Godot containers (`MarginContainer`, `HBoxContainer`,
   `VBoxContainer`, `PanelContainer`, `ScrollContainer`) and layout anchors, not
   hardcoded coordinates or manual offsets.
3. **No layout bleed.** Keep controls inside their container bounds. Set
   `clip_contents` or minimum sizes on parents where appropriate.
4. **Follow the design system** in
   [design_philosophy.md](design_philosophy.md) for colour, typography, spacing
   and motion.

### Godot practices

1. **Decoupled architecture.** Core logic (state, persistence, network) lives in
   autoloads and `src/core/`. UI scripts listen to `EventBus` signals rather than
   reaching into subsystems.
2. **No `class_name` on autoloaded scripts.** Declaring `class_name Foo` on a
   script registered as autoload `Foo` collides in the compiler. Rely on the
   autoload name.
3. **Type-safe models.** Use custom `Resource` scripts with `@export` typed
   properties rather than untyped dictionaries for profiles, inventory and
   events.
4. **Test through the scene.** Autoload-dependent tests must boot
   `TestRunner.tscn`. Running `godot -s script.gd` bypasses autoload injection.

### Documentation

Keep links repository-relative. Absolute paths to a particular machine were
purged once and must not come back.

---

## Reference documents

| Document | What it is |
|---|---|
| [docs/migration_plan.md](docs/migration_plan.md) | The plan off Godot. Phases, exit criteria, defect register. |
| [docs/handoff_phase5.md](docs/handoff_phase5.md) | Executable brief for the current phase (the headless playable milestone). |
| [docs/handoff_phase4.md](docs/handoff_phase4.md) | The completed orchestration brief. |
| [docs/handoff_phase3.md](docs/handoff_phase3.md) | The completed data-layer brief. Useful as the record of what `orison-core` now provides. |
| [docs/handoff_phase0.md](docs/handoff_phase0.md) | The original Phase 0 brief, kept as history. |
| [docs/rag_architecture.md](docs/rag_architecture.md) | Retrieval philosophy, Director/Actor rationale, June diagnosis. Authoritative. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Subsystem layout and turn-flow diagrams. |
| [design_philosophy.md](design_philosophy.md) | Colour tokens, typography, spacing, motion. |
| [docs/emotions.md](docs/emotions.md) | Tri-dimensional emotion model. |
| [docs/orison_audit.md](docs/orison_audit.md) | June 2026 audit. Largely remediated; useful as history. |
| [docs/proposal.md](docs/proposal.md) | Original technical proposal. Platform assumptions superseded by the migration plan; product pillars still hold. |
| [docs/done.md](docs/done.md) | Closed historical record of completed tickets. Do not append. |
