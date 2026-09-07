# Phase 0 Handoff — Stabilise the current repository

> **Read first**: [migration_plan.md](migration_plan.md), specifically §4 (ordering principle), Phase 0, and Appendix B.
> **Prerequisite**: none. Phase 0 is the first work in the plan.
> **Branch**: `claude/project-revival-modernization-6mpddx`
> **This document is self-contained.** It assumes no knowledge of the conversation that produced the migration plan.

> ## Phase 0 is complete.
>
> All seven tasks are done and merged. `main` carries CI, the B-1 fix, hermetic
> tests, `AGENTS.md`, portable documentation links, the feature freeze, and the
> ticket migration to GitHub Issues. The test suite is green at 46/46 on a cold
> runner.
>
> This document is retained as the record of what was done and why. **The next
> work is Phase 1** (build the evaluation harness and baseline the Godot build)
> in [migration_plan.md](migration_plan.md). Do not start Phase 2 before Phase 1
> has recorded a baseline: the whole migration is graded against it.

---

## What Phase 0 is for

Orison is a local-first, offline AI storytelling engine written in Godot 4.x / GDScript. A migration to a Rust core with a Tauri 2 desktop shell is planned, but has not started. Phase 0 makes the *current* repository safe to work in and safe to leave.

Nothing in Phase 0 is throwaway. Every task here delivers value whether or not the migration ever proceeds. Two of the six tasks fix defects that have plausibly been causing user-visible bugs since June.

**Do not begin any migration work.** No Rust, no Tauri, no new crates. Phase 0 touches the Godot build, CI configuration and documentation only.

---

## Repository orientation

```
orison/
├── project.godot          # Godot 4.6, "Mobile" feature set, 8 autoloads
├── src/
│   ├── autoload/          # Global singletons: EventBus, CampaignState,
│   │                      #   LLMClient, ThemeManager, ImageGenClient,
│   │                      #   ImageGenManager, MediaManager, EmbeddingStore
│   ├── core/              # Engine logic: VaultCompiler, GameLoopController,
│   │                      #   PromptBuilder, SystemPrompts, KnowledgeGraphManager,
│   │                      #   MemoryManager, EmotionEngine, SaveManager, ...
│   ├── resources/         # Typed Resource models
│   └── ui/                # Scene controllers
├── scenes/ui/             # 25 .tscn scene files
├── tests/
│   ├── TestRunner.tscn    # Boot scene for the suite
│   └── TestRunnerNode.gd  # 3,750 lines, ~40 tests, auto-discovered
└── docs/                  # Design docs and the markdown ticket system
```

~19,600 lines of GDScript. Tests are run by booting a scene, not by a standalone script, because autoloads must be present in the scene tree.

**Run the suite:**

```bash
godot --headless --path . res://tests/TestRunner.tscn
```

It prints `[PASS]` / `[FAIL]` per test, a summary line, and exits `0` only if every test passed.

---

## Task order

Do them in this order. 0.2 is the highest-value change in the phase, but 0.1 comes first so that the fix is protected by CI the moment it lands.

| # | Task | Effort | Risk | Status |
|---|---|---|---|---|
| 0.1a | Make environment-dependent tests hermetic | Small | Low | **Done** |
| 0.1 | Continuous integration | Medium | Low | **Done** |
| 0.2 | Fix defect B-1 (context budget) | Small | Medium | **Done** |
| 0.3 | Repair documentation portability | Small | None | **Done** |
| 0.4 | Replace agent guidance files | Medium | Low | **Done** |
| 0.5 | Move tickets to GitHub Issues | Medium | Low | **Done** |
| 0.6 | Declare the feature freeze | Trivial | None | **Done** |

Commit each task separately. Do not bundle them.

### Task 0.1a (done) — read this before doing 0.1

Three of the 46 tests failed on a clean checkout and would have failed on every
CI run. They are fixed, but the reasons matter for how you write the workflow:

- `test_atomic_writes_and_upgrade` wrote a fixture into `user://adventures/`
  before any `SaveManager` call created that directory. Every CI run starts with
  a cold `user://`.
- Two `LLMStreamRequest` timeout tests pointed at `192.0.2.1` (RFC 5737
  TEST-NET-1) and depended on packets being **blackholed** so elapsed time would
  accumulate. Sandboxed runners **refuse** instantly instead, so the
  connection-error path ran rather than the timeout path.

Both classes of failure are invisible on a developer machine and certain in CI.
When you add the workflow, confirm the suite is green on a genuinely cold runner
rather than trusting a local pass.

Verified with Godot 4.6.stable headless: 46/46 on two cold runs and one warm run,
exit code 0. **The runner's exit code does propagate correctly**, so 0.1 does not
need the fallback output-parsing described below; keep it only if you find
otherwise on the CI runner.

---

## Task 0.1 — Continuous integration

**Problem**: there is no `.github/` directory. Nothing runs on push. Every regression, human or agent authored, lands silently. This is a direct contributor to how the project stalled in June.

**Deliverable**: `.github/workflows/ci.yml` running on `push` and `pull_request`.

**Job 1 — tests**
- Obtain a Godot 4.6 headless binary. Prefer a maintained setup action; otherwise download the official release and cache it.
- Run `godot --headless --path . res://tests/TestRunner.tscn`.
- The runner already exits non-zero on failure (`TestRunnerNode.gd`, final line: `get_tree().quit(0 if pass_count == total_tests else 1)`). **Verify that this exit code actually propagates through the Godot binary in headless mode.** Do not assume it. Godot has historically been inconsistent about exit codes across versions and platforms. If it does not propagate, parse the summary line `Test Results: N / M Tests Passed` as a fallback and fail the step on `N != M`.
- Godot may need to import assets on first run in a clean checkout. If the suite fails on a cold runner but passes locally, try a preliminary `godot --headless --import` step.

**Job 2 — lint**
- `gdlint` and `gdformat --check` over `src/` and `tests/`, from the `gdtoolkit` package.
- The repository has never been linted, so expect a large number of findings on first run. **Do not mass-reformat the codebase in this task.** Land the lint job as non-blocking (`continue-on-error: true`) with a follow-up issue to triage findings and turn it blocking. A 19,600-line reformatting diff in the same PR as CI setup is unreviewable.

**Acceptance**
- Deliberately break one assertion in `TestRunnerNode.gd`; CI fails. Revert; CI passes.
- Both jobs complete in under ten minutes.

---

## Task 0.2 — Fix defect B-1 (DONE, retained for context)

> Completed. The record below explains what was wrong and what changed; you do
> not need to act on it, but the rules it establishes about context lengths and
> budgets are now enforced in `AGENTS.md` and apply to any code you write.

**Read [migration_plan.md](migration_plan.md) Appendix B-1 in full before editing anything.**

### The defect

Two independent errors compound.

**Error one — the assumed limit is wrong.** `PromptBuilder.build_prompt()` (`src/core/PromptBuilder.gd:142`) assembles the character-agent prompt against:

```gdscript
var context_limit = 8192 # NPC character model context limit
```

But `LLMClient.send_prompt()` (`src/autoload/LLMClient.gd:214`) sends the character model with half that:

```gdscript
var active_ctx = 4096 if active_model == character_model else 8192
```

The same expression appears in `_raw_send_custom_request` (`LLMClient.gd:638`) and `_raw_send_custom_vision_request` (`LLMClient.gd:711`).

**Error two — the allocations exceed the whole.** `PromptBuilder.gd:145-148`:

```gdscript
var sys_budget  = int(context_limit * 0.30)
var id_budget   = int(context_limit * 0.30)
var hist_budget = int(context_limit * 0.30)
var lore_budget = int(context_limit * 0.15)   # sums to 1.05
```

105% of an already-doubled figure, before reserving anything for the response.

**Consequence**: worst case is roughly 2.1x the served context window. The model silently discards the oldest tokens, which is the system prompt and the character profile. This is a highly plausible direct cause of the long-standing "NPC breaks character" behaviour, and of Bug 5 in [rag_architecture.md](rag_architecture.md).

There is also a latent trap on the same line: `active_ctx` is selected by comparing the model *name* to `character_model`. If a user configures the same model for both roles, which is a reasonable configuration, the Director silently receives 4096 while `build_world_builder_prompt` (`PromptBuilder.gd:320`) budgets for 8192.

### Required fix

1. **Single source of truth for context length.** Add a configurable context length per role on `LLMClient`, persisted in `client_config.json` alongside the existing model settings. Default both to 8192. `PromptBuilder` reads this value; it must never contain a context literal.
2. **Remove the name-comparison logic.** Select `num_ctx` by the *role* the request is being made for, not by string-comparing model names. This is the trap above and it will bite during the Phase 4.2 experiment.
3. **Make allocations sum correctly.** Budgets must total at most 100% of the context length *minus* a response reservation. Reserve at least 1024 tokens for the response; the character agent emits a JSON object with `thinking`, `narration` and `dialogue` fields and can be verbose.
4. **Make overflow visible.** `PromptBuilder.gd:314` currently calls `push_warning` on overflow and discards it. Overflow must be surfaced: at minimum a `printerr`, ideally an `EventBus` signal the UI can toast, per the error-handling gap in [orison_audit.md](orison_audit.md) §5.

### Do not

- Do not simply raise `num_ctx` to 8192 everywhere and call it fixed. That doubles KV cache memory for both models, and both are currently pinned in VRAM with `keep_alive: -1` (defect B-6). On the 8GB consumer GPUs this project targets, that risks trading a truncation bug for an out-of-memory failure. If you raise it, say so explicitly in the commit message and note the VRAM implication.
- Do not touch `keep_alive` in this task. B-6 is scheduled for Phase 2.2.
- Do not rewrite the prompt content. Only the budgeting arithmetic and the plumbing that feeds it.

### Tests

`test_prompt_budgeting` (`TestRunnerNode.gd:1089`) already exists. Extend it to assert:
- Assembled prompt token count never exceeds `context_length - response_reserve`.
- Budget fractions sum to at most 1.0.
- Configuring the same model for both roles yields the correct context length for each.

Note that token counts here still use the `length / 4` heuristic in `PromptBuilder.estimate_tokens` (defect B-4). **Leave that heuristic alone**; replacing it requires a real tokenizer and is Phase 2.5. Assert against the heuristic and accept that it is approximate. This is the reason the response reservation should be generous.

### Acceptance

- No context-length literal exists in `PromptBuilder.gd`.
- Budget fractions sum to at most 1.0 minus the response reserve, asserted by test.
- The role-versus-name-comparison trap is gone.
- Full suite passes.

---

## Task 0.3 — Repair documentation portability

**Problem**: 62 occurrences of the absolute path `/Users/dylangrowcoot/Documents/Personal Apps/orison/` across `gemini.md`, `design_philosophy.md`, `ARCHITECTURE.md`, `docs/ticket_template.md`, `docs/orison_audit.md`, `docs/in_progress.md` and `docs/backlog.md`. They are broken for every reader other than the original machine, including every AI agent that tries to follow them.

They appear as `file:///Users/dylangrowcoot/Documents/Personal%20Apps/orison/path/to/file.gd` and as unencoded variants with literal spaces.

**Scope has narrowed since this was written.** Task 0.4 is complete: `gemini.md`
and `.agents/AGENTS.md` are deleted, and the one live absolute path inside
`design_philosophy.md` that pointed at the old feature map is already fixed. Your
remaining targets are `ARCHITECTURE.md`, `docs/ticket_template.md`,
`docs/orison_audit.md`, and any residue in `design_philosophy.md`.

`docs/backlog.md` and `docs/in_progress.md` are owned by Task 0.5 and
`docs/done.md` is frozen history; leave all three alone. Re-run the grep below to
get the live list rather than trusting these filenames.

**Fix**: replace with repository-relative links. Mind the directory depth: a link from `docs/foo.md` to a root file needs `../`, and one to a sibling in `docs/` needs no prefix.

Find them all:

```bash
grep -rn "Users/dylangrowcoot" --include='*.md' .
```

**Acceptance**: that command returns nothing. Spot-check a handful of the rewritten links resolve to files that exist.

---

## Task 0.4 — Replace the agent guidance files (DONE, retained for context)

> Completed. `AGENTS.md` and `CLAUDE.md` exist at the repository root;
> `gemini.md` and `.agents/AGENTS.md` are deleted; `README.md` and
> `design_philosophy.md` are repointed.

**Problem**: guidance is split across `gemini.md` (118 lines, addressed to a specific tool) and `.agents/AGENTS.md` (5 lines, UI rules only). Neither is at the root, and `AGENTS.md` at the repository root is now the cross-tool convention.

Worse, `gemini.md` contains a **Feature Map** mapping features to file paths and line ranges, with an explicit instruction to keep the line numbers updated. This was reasonable in 2024 and is an anti-pattern now: line numbers rot within a commit or two, and the map is already stale in places. Current agents locate code by searching.

**Deliverable**

1. **Root `AGENTS.md`** covering:
   - What Orison is, in three sentences.
   - The local-only constraint, stated as inviolable. No content ever leaves the machine.
   - Repository layout and module responsibilities, described in prose. **No line numbers.**
   - How to run tests and lint.
   - The scene-first UI rule and the responsive-layout rule, carried over verbatim from `.agents/AGENTS.md` — they are still correct.
   - The autoload naming-collision rule and the other four Godot practices from `README.md`.
   - A pointer to [migration_plan.md](migration_plan.md) and a note that the Godot build is under feature freeze (Task 0.6).
2. **Root `CLAUDE.md`** that points at `AGENTS.md`. Do not duplicate content.
3. **Delete the Feature Map** from `gemini.md`. Replace `gemini.md` with a stub pointing at `AGENTS.md`, or delete it and update the `README.md` reference. Prefer deleting; a stub is one more file to drift.
4. **Remove `.agents/AGENTS.md`** once its rules are carried across.

**Acceptance**: one canonical guidance file. No line-number references to source anywhere in it. `README.md` links resolve.

---

## Task 0.5 — Move tickets to GitHub Issues

**Problem**: the ticket system is three markdown files moved by hand: `docs/backlog.md` (358 lines) → `docs/in_progress.md` (33 lines, currently empty) → `docs/done.md` (945 lines). It is merge-conflict-prone, and `done.md` will grow without bound.

**Scope**: open tickets only. These are in `docs/backlog.md`:
- **OBD001** — onboarding tool requirements and model pull guide (1 ticket)
- **STT001-STT009** — stats, dice and combat (9 tickets)
- **TKT027** — rich markdown and BBCode rendering in chat rows (1 ticket)

Plus the defects from [migration_plan.md](migration_plan.md) Appendix B that are not being fixed in Phase 0: B-2 through B-10. File these as issues too, labelled by their target phase, so the register in Appendix B-0 has something to point at.

**How**
- Preserve each ticket's full body. The format in `docs/ticket_template.md` (user story, context, description, requirements, acceptance criteria) is good and should survive.
- Label by series: `onboarding`, `stats`, `chat`, `defect`, plus `phase-2` / `phase-3` for the deferred defects.
- Note in the issue body that several backlog tickets reference Godot-specific implementation detail that will need reinterpretation after the migration. Do not rewrite them now.
- Empty `docs/backlog.md` and `docs/in_progress.md`, replacing each with a pointer to Issues.
- **Leave `docs/done.md` untouched** as a frozen historical record of 60+ completed tickets. Add a header noting it is closed to new entries.

**Acceptance**: all 11 open tickets plus 9 defect issues exist and are labelled. `backlog.md` and `in_progress.md` are pointers. `done.md` is unchanged apart from its header.

---

## Task 0.6 — Declare the feature freeze

From the end of Phase 0, the Godot build receives bug fixes only. Every feature added after this point is a feature that must be built twice.

Add a short, prominent notice to `README.md` immediately below the project description: the Godot build is in maintenance, new feature work is paused pending the migration, and link to [migration_plan.md](migration_plan.md).

Do not remove the existing Godot setup and local AI documentation. It is still how the current build runs, and it will remain accurate for months.

---

## Phase 0 exit criteria

Copied from [migration_plan.md](migration_plan.md). All must hold before Phase 1 begins.

- [ ] CI runs tests and lint on every push, and fails correctly on a broken assertion.
- [ ] B-1 fixed and verified by test.
- [ ] Zero absolute filesystem paths in documentation.
- [ ] `AGENTS.md` exists at the repository root; the line-number Feature Map is gone.
- [ ] Open tickets exist as GitHub Issues.
- [ ] Feature freeze announced in `README.md`.

---

## Conventions

- **Branch**: `claude/project-revival-modernization-6mpddx`. Do not push elsewhere.
- **Commits**: one per task, descriptive body explaining *why*, not just what.
- **Do not open a pull request** unless explicitly asked.
- **Do not start migration work.** If a task seems to require Rust, Tauri or a new crate, you have misread it. Stop and ask.
- **Do not mass-reformat.** Lint findings get an issue, not a 19,600-line diff.
- When something in this document contradicts what you find in the code, **trust the code and say so**. This handoff was written from a reading of the repository at commit `c353081` and may be stale or wrong in places. One such error has already been found and corrected: an earlier draft claimed the test runner needed a non-zero exit code added, when it already had one.
