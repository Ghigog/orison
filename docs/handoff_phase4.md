# Phase 4 Handoff — Orchestration and the turn loop

> **Read first**: [migration_plan.md](migration_plan.md) §3 (target architecture),
> §4 (ordering principle), Phase 4, and Appendix B.
> **Then read**: [rag_architecture.md](rag_architecture.md) §1.4 (the
> Director/Actor rationale you are about to test) and §1.5 (biography versus
> session memory, which is easy to lose in a rewrite).
> **Then read**: [eval_baseline.md](eval_baseline.md), "Phase 3 measurement" —
> the numbers Phase 4 must not regress, and the two it must finally fill in.
> **Prerequisite**: Phases 0, 1, 2 and 3, all complete and merged.
> **This document is self-contained.** It assumes no knowledge of the
> conversation that produced it.

---

## Where the project is

Orison is a local-first, offline AI storytelling engine. The Godot 4.x /
GDScript build (~19,600 lines) is the reference implementation and remains
under feature freeze until Phase 5. A parallel Rust core
(`crates/orison-core`) is being built alongside it.

Four phases are done.

**Phase 0** stabilised the repository: CI, hermetic tests, a root `AGENTS.md`,
tickets moved to GitHub Issues, the feature freeze, and the B-1 context-budget
fix.

**Phase 1** built the measurement layer and recorded the baseline, finding and
fixing three critical defects (B-13, B-14, B-16) that six months of playing the
game had not.

**Phase 2** built the inference layer: `InferenceBackend` (role-separated chat,
streaming, embeddings, a real tokenizer, typed health), `OllamaBackend` on
`/api/chat`, `LlamaCppBackend` in-process, schema-constrained decoding, real
token budgeting, typed native tool calling, and cache-stable message ordering.
It closed B-1, B-2, B-3, B-4, B-6, B-8, B-10 and B-15.

**Phase 3** built the data layer: SQLite state, the ported ingest pipeline, the
`petgraph` knowledge graph, hybrid retrieval, RAPTOR and chunking. It closed
B-7 and B-9. Retrieval recall now beats the Godot baseline on every fixture
(`minimal` 1.000, `messy` 0.933 → 1.000, `large` 0.875 → 1.000) and both
documented hard queries are closed.

**Phase 4 builds the turn loop.** Everything below it exists: a backend that
can be called, state that can be queried, a graph that can be retrieved from.
Nothing yet runs a turn.

---

## What Phase 3 handed you

The whole of `orison-core` in one page, so you do not have to reverse-engineer
it from the source. Everything here is synchronous unless it says otherwise.

### `state` — the campaign database

```rust
let mut store = CampaignStore::open(Path::new("campaign.sqlite3"))?;
store.save_campaign(&Campaign::new("id", "Title", now))?;

store.append_history(campaign_id, &HistoryEntry { .. })?;
let recent = store.recent_history(campaign_id, 12)?;   // oldest first
let state  = store.character_state(campaign_id, entity_id)?;
let value  = store.adjust_affinity(campaign_id, entity_id, 0.1)?;
```

One SQLite file per campaign, migrated on open. Emotion events and history logs
are **not** capped the way the Godot save was; readers pass the limit they want.
`CampaignState._mutex` is gone: open a second connection rather than sharing
one, and let WAL do its job.

Schema changes go in a new `M::up` in `state::schema`, never as an edit to a
migration that has shipped. `CURRENT_SCHEMA_VERSION` is 2.

### `knowledge` — the only entity store

```rust
let graph = KnowledgeGraph::load(&store, campaign_id)?;

let entity = graph.get(&id);                    // Option<&Entity>
let id     = graph.resolve("The Landing");      // label, alias, id or slug
let nearby = graph.within(&id, 2);              // BFS to 2 hops
for character in graph.by_kind(EntityKind::Character) { .. }
```

`tests/knowledge_graph.rs` fails the build if any module outside `knowledge/`
grows a map keyed by `EntityId`. That test is the §3.3 exit criterion, and the
defect it guards against is real: `orison_audit.md` §9 records three
disagreeing entity stores in the Godot build.

An `Entity` carries `fields` (the canonical biography / personality /
appearance / goals / gender), `overflow` (sections that matched no field, kept
and indexed), `body` (the complete original note, always), `tags`, `aliases`
and `properties`. `Entity::merge_extracted_fields` fills *empty* fields from an
external pass and can never overwrite what the source said.

### `ingest` — vault to graph

```rust
let outcome = ingest_vault(vault_root, &IngestOptions::default())?;
outcome.graph.save(&mut store, campaign_id)?;
```

Synchronous and needs no model. `IngestReport` carries the counts Phase 3 is
graded on, including `unaccounted_sections`, which must stay empty.

### `retrieval` — picking what the model sees

```rust
let lexical = LexicalIndex::build(&graph)?;     // in-memory, rebuild on load
let result  = retrieve(query, &graph, &lexical, dense, &reranker, &config)?;
let context = format_context(&result.hits, &graph, budget, count_tokens);
```

`dense` is `Option<(&DenseIndex, &[f32])>` and is `None` when no embedding model
is configured. That is a degraded pipeline, not a broken one: BM25 alone beats
the Godot baseline on `minimal` and `messy`.

`RetrievalConfig::filter` is how the Director gets campaign-level summaries and
the Actor gets concrete facts: `MetadataFilter::level(2)` and `level(0)`
respectively, per `rag_architecture.md` §1.3.

### `prompt` — budgeting and ordering, from Phase 2

`budget::allocate` and `ordering::PromptSections` are unchanged and already
tested. Use them; do not reimplement token counting.

---

## Two numbers Phase 4 must finally measure

Both have been outstanding since the phase that created them, in both cases
because the environment that built them had no model running. Neither is
optional now: Phase 4 is the first phase that cannot be honestly graded without
them.

**Turn latency p50.** Phase 2's entire justification. The Godot baseline is
~20 s per conversational turn, and `eval_baseline.md` calls it the worst number
in the document. Cache-stable ordering (§2.7) is supposed to have fixed it, and
the *precondition* is tested (`prompt::ordering` asserts byte-identical prefixes
between turns) but the actual before/after has never been run. Phase 4 builds
the loop that generates real turns, so this becomes measurable for the first
time. Measure it early, not at the end.

**Dense retrieval's contribution to recall.** Every retrieval figure in
`eval_baseline.md`'s Phase 3 section is the lexical and structural half only.
The dense half is built, unit-tested against fixed vectors, and gated:

```bash
ollama pull nomic-embed-text

ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_OLLAMA_EMBED_MODEL=nomic-embed-text \
  cargo test -p orison-core --test retrieval_dense -- --nocapture
```

The Godot live run took `messy` from 0.933 to 1.000 with embeddings, so there
is reason to expect it helps. Nobody has confirmed it on the Rust side.

---

## Tasks

Do them in this order. Each builds on the last.

| # | Task | Effort |
|---|---|---|
| 4.1 | Turn state machine | Large |
| 4.2 | Director/Actor experiment | Medium |
| 4.3 | Port the emotion engine | Medium |
| 4.4 | Port the memory tiers | Medium |
| 4.5 | Prompt assembly | Medium |

Commit each separately.

---

### 4.1 Turn state machine

Port `GameLoopController.gd` (1,077 lines) as an explicit, testable state
machine on `tokio`. This lands in `crates/orison-core/src/turn/`.

Three things the current implementation lacks or handles awkwardly:

- **Real cancellation.** When the player acts again mid-generation, the
  in-flight request must actually stop ([orison_audit.md §16](orison_audit.md)).
  The Godot version cannot cancel; it sets a flag and discards the result when
  it eventually arrives, so the model keeps burning a local GPU on output
  nobody will read.
- **Deterministic ordering under concurrency.** Replace the flag-and-callback
  coordination noted in [orison_audit.md §12](orison_audit.md). A state machine
  with explicit transitions is the point; a struct full of `bool`s that
  callbacks mutate is the thing being replaced.
- **The queue's sequential-execution guarantee.** `test_llm_request_queue` in
  the Godot suite is its specification. Read that test before designing the
  replacement.

**Exit**: a turn executes end to end against `OllamaBackend`; cancellation is
verified under test, not asserted in a comment.

---

### 4.2 Re-evaluate the Director/Actor split

**Run this as an experiment, not an assumption.** It is the most consequential
open question in the migration and the one most likely to be decided by
sentiment if nobody measures it.

The split exists because a 3B Actor could not also do Director work. Current
8B-class models are substantially more capable, and the split costs: two models
resident in memory, doubled load time, cross-model consistency problems, and
the whole background-director apparatus including
`SystemPrompts.get_director_busy_stalling_prompt`, which exists purely to paper
over Director latency.

Compare on identical transcripts:

- **A**: the current split (Director 8B + Actor 3B).
- **B**: one 8B model, two system prompts, two calls per turn.
- **C**: one 8B model, one call per turn with a combined schema.

Adopt whichever wins on quality-per-second. Do not preserve the split out of
sunk cost and do not collapse it out of enthusiasm. Record the decision in
[migration_plan.md](migration_plan.md) Appendix D with the numbers that
produced it.

**Note what Phase 3 changed about this question.** The plan's §2.6 says
`search_knowledge_graph` should not be a tool call at all but a deterministic
pre-pass. That pre-pass now exists and is fast: `retrieve()` with no dense index
needs no model and returns in microseconds on the fixtures. A large part of the
Director's original job was research it can no longer justify doing
conversationally. Weigh option C accordingly.

**Exit**: experiment concluded, decision recorded, numbers in
`eval_baseline.md`.

---

### 4.3 Port the emotion engine

Consolidate properly this time. [orison_audit.md §7](orison_audit.md) found
emotion logic scattered across five files with the file *named*
`EmotionEngine.gd` nearly empty. It has since grown to 192 lines, but the
tri-dimensional model in [emotions.md](emotions.md) should live in exactly one
module.

Emotion updates now arrive as a typed field on a schema-constrained response
(Phase 2.4), so the extract-and-repair path disappears entirely.

Two fixes to preserve, both of which were real bugs:

- The RAG006 no-op delta skip: do not emit a visual update when the emotion and
  intensity are unchanged.
- The RAG003 reaction debounce: one physical-reaction generation per turn, not
  three.

State goes to `characters` and `emotion_events` via `CampaignStore`. Note that
those tables no longer discard anything past 20 events, so decay and history
logic can read as far back as it wants.

---

### 4.4 Port the memory tiers

`MemoryManager.gd` port: short-term sliding window, medium-term session
summaries, long-term distilled character memories, per
[rag_architecture.md §1.5](rag_architecture.md).

**Keep biography and session memory strictly separated.** That section is
emphatic about it and it is easy to lose in a rewrite. Phase 3 already enforces
the split structurally: biography is a `CanonicalField` on a graph `Entity`,
session memory is `characters.long_term_memory` and `history_logs` in the state
database. Do not let them meet in a struct.

Replace the turn-count compaction threshold (`COMPACTION_THRESHOLD = 30`) with
one based on real token counts, which Phase 2 made available.

---

### 4.5 Prompt assembly

Port `PromptBuilder.gd` and `SystemPrompts.gd` into `prompt/`. The boundary
between them was flagged as blurry in
[orison_audit.md §14](orison_audit.md); resolve it cleanly. Static templates are
`const` strings or template files; assembly is a builder that takes typed state
and returns an ordered message list.

Retain the `<player_message>` delimiter treatment for injection resistance. It
is the right approach and role separation reinforces it.

Use `retrieval::format_context` for the lore block rather than writing a second
formatter, and pass it a real tokenizer, never `length / 4` (B-4).

---

## Things earlier phases learned the hard way

Carried forward because they keep generalising.

**A metric that never fires is not a metric.** Phase 1's `--selftest`, Phase 2's
conformance suite and Phase 3's `unaccounted_sections` all gate on assertions
against fixtures under our control rather than on "did this print a plausible
number". Phase 4's cancellation test is the same shape: assert the request
actually stopped, not that a flag was set.

**Prove the negative result before believing it.** Phase 1's retrieval-scored-
0.000 finding was only trustworthy after a positive control proved the graph was
loaded. Phase 3 hit this twice: graph expansion appeared to do nothing because
expanded results were appended below every BM25 hit and never survived
`take(limit)`, and mention edges appeared not to help until the ranking was
inspected directly. Both looked like "the idea doesn't work" and were
"the wiring is wrong". Find the control case before reporting a surprise.

**A dependency compiling is not a dependency working.** Phase 2 verified
`llama-cpp-2` compiles and never ran it. Phase 3 caught two of these before they
cost anything: `ndarray` 0.16 versus 0.17 compiles cleanly per-crate and then
cannot hand an array to `KMeans::fit`, and `sqlite-vec` needed a runtime
`vec_version()` call to prove it was loaded at all. Smoke-test early.

**Silent degradation is still the recurring theme.** Phase 4's version is the
turn loop swallowing a failed call and narrating around it. Every failure that
reaches the player should be a typed error the loop decided how to present, not
an empty string that became an awkward silence.

---

## Conventions

- **Branch**: `claude/phase4-<task>`. Do not push straight to `main`.
- **Commits**: one per task, explaining *why*.
- **Do not touch the Godot build** beyond bug fixes. It is under feature freeze
  and remains the reference implementation until Phase 5.
- **Do not start Phase 5.** No `orison-cli`, no terminal UI. Phase 4 is a turn
  executing correctly; playing a campaign is the next milestone.
- **Model IDs are configuration, never constants** (B-10). This applies to the
  Director/Actor experiment above: the three arms are configuration profiles,
  not three code paths.
- **CI runs** `cargo fmt --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, and `cargo test --workspace`, plus the Godot suite and the eval
  harness. All five must stay green.
- When this document contradicts the code, **trust the code and say so.** Every
  prior handoff contained at least one error found this way; that is the system
  working.

---

## Open questions Phase 4 does not resolve

Recorded so nobody re-litigates them here.

**Retrieval precision at real-vault scale.** Phase 3 measured precision for the
first time and found graph expansion costs it: on `large`, 0.325 → 0.225. That
was the right trade against a recall exit criterion, but nobody has felt what it
does to a prompt in play. If context feels padded once turns are running,
`RetrievalConfig::expand_from` and `expand_hops` are the dials, and the
measurement to re-run is `tests/retrieval_quality.rs`.

**Whether the passage reranker earns its place.** It moves no recall on any
fixture and moves MRR on one. It was kept because it is nearly free and because
the precision problem it targets appears at a scale no fixture reaches. A real
vault settles it. A genuine cross-encoder needs model weights that were not
reachable in the environment that built Phase 3; `retrieval::rerank::Reranker`
is a trait precisely so one can be dropped in without touching the pipeline.

**Chunk size and overlap.** `DEFAULT_TARGET_WORDS = 180`,
`DEFAULT_OVERLAP_WORDS = 40`. Mechanism-verified, not corpus-tuned: the longest
note in any fixture is 53 words, so nothing in the fixture set exercises
multi-chunk behaviour at the default size. Tuning needs long documents that do
not exist in the repository yet.

**Whether `sqlite-vec` holds up past `large`'s 207 nodes.** Unchanged from
Phase 3. A real personal vault can be an order of magnitude bigger, and `large`
is what there is to measure against.

**Whether RAPTOR cluster counts should scale differently at real-vault scale.**
The `max(3, ceil(n/5))` / `max(1, ceil(l1_count/5))` shape is ported from the
Godot build as-is and was never validated against anything bigger than `large`.

---

## The fastest way to find out whether any of this is right

Ingest a real vault. The fixtures are small and deliberately hostile; a real
Obsidian vault is neither, and it will surface heuristics that are wrong far
faster than any test written against `messy/`. There is no command-line entry
point yet — that is Phase 5's `orison-cli` — but `ingest_vault()` plus
`IngestReport` is four lines in a test, and `report.notes_without_a_type`,
`report.dangling_links` and `report.unaccounted_sections` will tell you
immediately whether the classification heuristics survive contact with real
data.
