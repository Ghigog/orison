# Phase 6 Handoff — the Tauri shell, and the UI

> **Read first**: [migration_plan.md](migration_plan.md) §3 (target
> architecture), §4 (ordering principle), Phase 6, and Appendix B.
> **Then read**: [eval_baseline.md](eval_baseline.md) "Turn latency on the
> rebuilt metric" — the numbers the UI has to be designed around.
> **Then read**: [testing_backlog.md](testing_backlog.md) — what was
> deliberately left unmeasured, and what would make each one worth doing.
> **Prerequisite**: Phases 0-5, complete and merged.
> **This document is self-contained.** It assumes no knowledge of the
> conversation that produced it.

---

## Start here: UI design starts now

The plan says design work belongs in Phase 6 "and not before", and gates it on
two things being known. **Both are now known**, so the gate is open:

1. **Is there a background process to represent?** Yes. D-4 is decided: keep
   the Director/Actor split. A beat composes asynchronously while the player
   reads the Actor's reply, and it has four observable states
   (`DirectorState`: `Idle`, `Researching`, `Composing`, `Ready`). The UI has
   to show this, or the player sits in front of a machine that is thinking
   and looks broken.

2. **What is the latency behaviour?** Measured, on the model the baseline was
   recorded on, on a MacBook Air:

   | Fixture | p50 | p95 |
   |---|---:|---:|
   | `minimal` | 14.1 s | 15.5 s |
   | `messy` | 16.9 s | 26.3 s |

**This is the single most important input to the design, and it is not a
number to design around. It is the design.** Fifteen seconds is far past the
point where a spinner is honest. Every screen decision follows from it:

- **Stream, always.** `TurnEngine` emits deltas and `time_to_first_token` runs
  2.7-6.3 s. First token is the moment the wait becomes tolerable; the UI must
  never buffer a complete response before showing it.
- **The Director is a feature, not a background job to hide.** It is already
  working while the player reads. Showing "Elara is thinking about what
  happens next" converts dead time into anticipation. Hiding it converts the
  same time into a freeze.
- **Give the wait something to be.** The player is waiting on a local model
  they own, not a server. That is the product's whole premise, and a UI that
  apologises for the wait argues against it.
- **Never block the whole window on a turn.** The transcript, the character
  sidebar and the map stay live while a turn runs. `TurnEngine::submit_player_input`
  returns a ticket; `cancel_current_turn` exists and works.

Do not open Figma before reading `docs/eval_baseline.md`'s latency section and
playing the CLI for ten minutes. `docs/first_run.md` tells you how. The feel of
a 15-second turn is not conveyed by the number.

---

## Where the project is

Orison is a local-first, offline AI storytelling engine: it reads an Obsidian
vault and lets you talk to the people in it. The Godot 4.x / GDScript build
(~19,600 lines) is the reference implementation and remains under feature
freeze. A Rust core (`crates/orison-core`) has replaced it, phase by phase,
and a headless CLI (`crates/orison-cli`) proves the whole loop works.

**Phases 0-4** stabilised the repo, built the measurement layer, the inference
layer (`InferenceBackend`, `OllamaBackend`, schema-constrained decoding, real
tokenization), and the data layer (SQLite state, ingest, a `petgraph`
knowledge graph, hybrid retrieval, RAPTOR). See
[handoff_phase4.md](handoff_phase4.md) and
[handoff_phase5.md](handoff_phase5.md); nothing there is repeated here.

**Phase 5** made it playable. `orison play <campaign>` gives you a terminal
conversation with a character from your vault, with movement, memory,
persistence and streaming. It is genuinely fun for about ten minutes, which is
the first time that has been true.

### Phase 5's exit criteria, honestly

| Criterion | State |
|---|---|
| A campaign is playable start to finish | **Met** |
| Every deterministic metric meets or exceeds the Godot baseline | **Met** — retrieval recall 1.000 against 0.875-0.933; ingest 6/6 against 3/6; schema validity 100% |
| Judge scores meet or exceed the baseline | **Not run, and not runnable** — no Godot baseline exists to compare against. See [testing_backlog.md](testing_backlog.md) §2 |
| p95 turn latency no worse than the baseline | **`minimal` meets it, `messy` misses by 30%** |

**The latency criterion was amended rather than met, and you should know
exactly what that means.** It does not mean the number was waved through. It
means the engine side was chased to the end and finished:

- The Rust engine is **2-5x faster** than the Godot build it replaces on every
  fixture. Phase 4 measured 63.2 s / 52.9 s; it is now 15.5 s / 26.3 s.
- Two real defects were found and fixed along the way (B-17 ordering, B-18
  `num_ctx`), both of which the port had introduced.
- The prompt assembly is **proven** cache-stable: `tests/prefix_growth.rs`
  runs eight turns against both fixtures and asserts the shared prefix grows
  every turn, with exactly one model call per turn.
- `messy` builds a prefix as cache-friendly as `minimal`'s and still gets
  almost no reuse, which rules the engine out as the difference.

What remains is Ollama's cache behaviour on a memory-constrained laptop. No
prompt ordering reaches it. The gate existed to stop a prettier shell going
over a worse engine; that is not the situation, so it was amended to say what
is true rather than left as a checkbox no engine work could tick.

### The expensive lesson, so it is not repeated

Phase 5.6 spent two days on a latency number that turned out to be three
successive instrument failures, not an engine defect:

1. `prompt_eval_count` is documented as excluding cached tokens. **It does
   not.** It reports the whole prompt either way. A run read its flat output
   as "0% cache reuse" and nearly recorded a third structural defect.
2. `FakeOllama` answered the Director's non-streamed path in microseconds,
   which closed the only window in which a concurrency bug could exist. A
   stand-in faster than the thing it stands in for cannot test concurrency.
3. The probe written to settle the question ran two tests in parallel against
   one server with a shared prefix, so one test's baseline was the other's
   cache hit, and the verdict came out inverted.

All three are fixed. The rule they cost: **validate the instrument against a
case where you know the answer, before believing what it says about a case
where you do not.** `tests/prefix_cache.rs` now checks its own verdicts
against servers built to exhibit each behaviour, for exactly this reason.

---

## What Phase 6 builds

Full scope is in [migration_plan.md](migration_plan.md) Phase 6. In short:
Tauri 2, a typed command layer over `orison-core`, streaming deltas over
Tauri's event channel, and a port of the existing information architecture —
onboarding and vault import, model configuration with connection checking,
campaign list, gameplay viewport, character detail, mind map, settings. The 24
`.tscn` scenes under `scenes/` are an accurate inventory of what must exist.

[design_philosophy.md](../design_philosophy.md) already specifies colour
tokens, typography, spacing, radii, shadows and motion timing.
[orison_audit.md §22](orison_audit.md) notes only colours and font sizes were
ever implemented in Godot. In CSS the rest is nearly free.

### Three things to do before the first screen

1. ~~**Promote B-15.**~~ **Done.** The register entry was stale, not the code:
   `InferenceError::Unreachable`/`ModelNotFound` have been unconditional since
   Phase 2.2, and `FailureKind::ModelUnreachable` already carries a dead model
   through `TurnEngine` to `orison-cli`'s shell (`conformance.rs`'s
   `unreachable_ollama_endpoint_is_a_typed_error_not_a_silent_failure`, no live
   server required). See migration_plan.md's B-15 row. What Phase 6 still owes
   is the screen that renders it — a silent failure is survivable in a CLI the
   author runs and is not survivable in a shipped window with no terminal to
   notice the absence — and the round 2 design canvas's "Interrupted" screen is
   built against exactly this typed failure.

2. **Design vault import around the 48% problem.** A real vault imported
   without `--folder-type` mappings left 48% of notes untyped, and untyped
   people are not offered as characters to talk to. The CLI prints a summary
   and the user re-runs. A UI user will not. The fix is probably "show what
   was typed and let them correct it", not a cleverer heuristic. See
   [testing_backlog.md](testing_backlog.md) §5.

3. **Play the CLI.** Ten minutes, `docs/first_run.md`. Everything above is a
   number until you have waited fifteen seconds for Bram Holt to tell you the
   toll.

---

## Where Phase 6 actually got to

Everything above this heading was written before the first screen existed. It
still describes the brief correctly; this section says what came of it.

**Built and wired to real commands** — eight screens: Campaigns, Models
(with live reachability), Starters, Play, Character, Map, Import, Settings.
All ten follow-up tickets filed after the scaffold ([#29-#38](https://github.com/Ghigog/orison/issues/29))
are merged, plus adventure-starter generation ported to the core
([#41](https://github.com/Ghigog/orison/issues/41)).

**The three §6.4 components**, with one correction to the plan:

- **Markdown in chat** — bought (`marked` + `DOMPurify`). Streaming stays
  plain, because half-arrived markdown is broken markdown; links and images
  never become elements, because §2.3 makes no-egress a pillar and this
  renderer sees model output that has passed through a vault.
- **The mind map** — bought (Cytoscape.js), lazily imported, with the node
  and edge lists kept beside the canvas as the keyboard path.
- **Chat virtualisation** — *not* bought. Measured against an idealised
  windowed list and lost to one CSS rule at every transcript size; see
  `migration_plan.md` §6.4 for the table and
  `apps/desktop/bench/transcript-streaming.mjs` to re-run it.
  `VirtualScrollContainer.gd` existed because Godot `Control` nodes are
  expensive to hold in quantity. Paragraphs are not. The port inherited the
  constraint along with the component.

**The three things this document said to do before the first screen**: B-15
was already done and the register was stale (recorded above); vault import
*was* designed around the 48% problem — `scan_vault_folders` shows every
folder with its note count and the ingest layer's own guess, preselected, so
the player corrects rather than re-runs ([#36](https://github.com/Ghigog/orison/issues/36));
the CLI playthrough is the one that did not happen, and that matters more
than it looks.

### The thing to know before doing anything else

**Nobody has ever run this application.** Every screen has been verified by
`cargo clippy`, `cargo test`, `tsc`, `vitest` and `vite build`, in an
environment with no display and no Ollama.
[#30](https://github.com/Ghigog/orison/issues/30) existed to fix that and was
closed without a playthrough, on the reasoning that a playtest before parity
would rediscover known gaps rather than surface new ones. That is a
defensible sequencing call. It is also why no Phase 6 exit criterion has been
confirmed by use — including "keyboard-navigable throughout", which is
currently an argument from how the markup was written, not an observation of
anyone navigating it.

So the advice at the top of this document — *do not open Figma before playing
the CLI for ten minutes* — has a successor, and it is the same advice:
**do not call Phase 6 done before playing the shell.**

### What remains

In the order it should be done, and the reasoning is in `migration_plan.md`'s
Phase 6 exit criteria:

1. **The first-run path** (`WelcomeScreen`, `SetupWizard`). The shell opens
   on an empty campaign list, which assumes somebody who already knows what
   Orison is. Small, and it is the first thing a new player meets.
2. **The character creator.** `OnboardingFlow.gd`'s adventure-starter half
   was ported; the player-character half — avatar, description, the
   vision-model magic wand — was not, and the core has no equivalent.
3. ~~**Decide [D-7](migration_plan.md#appendix-d--decisions-and-open-questions)**
   (image generation).~~ **Done: not ported.** The Draw Things / A1111 contract
   stays in the Godot build and `orison-core` gets no `media/` module, so
   `AssetStatusOverlay`, `DrawThingsTutorial` and `ImageGenSettingsPanel` stop
   being parity gaps. Nothing to build.
4. **Rewrite [design_philosophy.md](../design_philosophy.md) to round 2.** It
   still specifies the round 1 dark presets and the Lora/Outfit type stack;
   the shell ships Lamplight/E-ink with Spectral and IBM Plex Mono. Until it
   is rewritten, "the design token set" names two different things depending
   on which file you open.
5. **Then playtest**, and let that decide whether Phase 6 is done.

Items 1 and 2 are the only building left, and item 1 is also Phase 7's main
screen — see [handoff_phase7.md](handoff_phase7.md). Build them together.

---

## What is deliberately not being done

[testing_backlog.md](testing_backlog.md) holds six items with a trigger each.
Two have a bearing on Phase 6:

- **B-15** (above), which should be promoted now.
- **The judge baseline**, which is recoverable only while the Godot build
  still runs. It is under feature freeze, not deleted. **If the Godot build is
  ever scheduled for removal, capture that baseline first** — it is the only
  item in the backlog with a deadline rather than a trigger.

Everything else in that file is genuinely fine to leave. The default answer is
"leave it", and an entry gets promoted when its trigger fires, not when
somebody has a free afternoon.
