# Phase 3 Handoff — Data, knowledge graph and retrieval

> **Read first**: [migration_plan.md](migration_plan.md) §3 (target architecture),
> §4 (ordering principle), Phase 3, and Appendix B.
> **Then read**: [eval_baseline.md](eval_baseline.md) (the numbers this phase is
> graded against) and [rag_architecture.md](rag_architecture.md) §1 and §3
> (why retrieval and RAPTOR are shaped the way they are).
> **Prerequisite**: Phases 0, 1 and 2, all complete and merged.
> **This document is self-contained.** It assumes no knowledge of the
> conversation that produced it.

---

## Where the project is

Orison is a local-first, offline AI storytelling engine. The Godot 4.x /
GDScript build (~19,600 lines) is the reference implementation and remains
under feature freeze until Phase 5. A parallel Rust core
(`crates/orison-core`) is being built alongside it.

Three phases are done:

**Phase 0** stabilised the repository: CI, hermetic tests, a root `AGENTS.md`,
tickets moved to GitHub Issues, the feature freeze, and the B-1 context-budget
fix.

**Phase 1** built the measurement layer and recorded the baseline, finding and
fixing three critical defects (B-13, B-14, B-16) that six months of playing
the game had not.

**Phase 2** built the inference layer: `InferenceBackend` (role-separated
chat, streaming, embeddings, a real tokenizer, typed health), `OllamaBackend`
on `/api/chat`, `LlamaCppBackend` in-process, schema-constrained decoding,
real token budgeting, typed native tool calling, and cache-stable message
ordering. It closed B-1, B-2, B-3, B-4, B-6, B-8, B-10 and B-15.

Two things Phase 2 leaves open, not because they're wrong but because they
were unverifiable in the environment that built them:

- **Turn latency p50 has not been re-measured live.** The cache-stability
  precondition is tested (`crate::prompt::ordering`), but nobody has run the
  actual nine-turn transcript against a live model yet to record the
  before/after number `eval_baseline.md` calls for. Do this if you have access
  to a machine with Ollama; it isn't blocking for Phase 3, but it's the one
  number in the whole migration plan that actually justifies Phase 2's
  existence, and it is still unmeasured.
- **`LlamaCppBackend` compiles against the real `llama-cpp-2` API but has
  never run against real weights** (no GGUF file, no network path to fetch
  one, in the environment that built it). If Phase 3 code ends up calling
  into it (e.g. for in-process embeddings), verify it actually works before
  depending on it, per `crate::inference::llamacpp`'s module doc.

**Phase 3 builds no turn loop or orchestration.** It builds the data layer the
rest of the engine reads from: state, ingest, knowledge graph, retrieval.
Resist scope creep into `GameLoopController`'s state machine or the
Director/Actor split — those are Phase 4.

---

## What Phase 3 is for, in one paragraph

The Godot build's data layer is a set of in-memory dictionaries backed by
whole-document JSON saves (`SaveManager.gd`), a hand-rolled brute-force
cosine-similarity vector store (`EmbeddingStore.gd`, 112 lines), a lexical
retrieval path that was inverted until Phase 1 (B-13) and is still containment
scoring rather than BM25, and an ingest pipeline (`VaultCompiler.gd`, 1,358
lines) that silently drops any Markdown section it doesn't recognise. Phase 3
replaces all of that with: SQLite as the single source of truth, a real
knowledge graph on `petgraph` with no parallel dictionaries, hybrid retrieval
(BM25 + dense ANN + rank fusion + rerank), and an ingest pipeline that keeps
everything it's given rather than discarding what it can't classify. The
measured prize is retrieval recall and ingest completeness; the structural
prize is that "this vault has data the engine silently threw away" stops
being possible.

---

## The numbers to beat

Full baseline in [eval_baseline.md](eval_baseline.md). The short version, all
from the *post-B-13/B-14-fix* Godot build (the port must beat these, not the
as-found numbers — the as-found numbers were broken and the whole point of
Phase 1 was fixing them before anyone measured against them):

| Metric | Godot baseline (fixed) | Notes |
|---|---:|---|
| Retrieval recall, lexical only | `minimal` 1.000 / `messy` 0.933 / `large` 0.875 | Term-overlap scoring, not BM25. See B-9/§3.4 below. |
| Retrieval recall, + embeddings | `minimal` 1.000 / `messy` 1.000 | `nomic-embed-text` live. `large` not run live. |
| Ingest completeness | `messy` 6 of 6 required source substrings | Character nodes now keep raw source (B-14 fix). |
| Retrieval latency | p50 0 ms, p95 1 ms | Brute-force, small corpus. Watch this at `large` scale with real BM25/ANN. |

**Precision is unmeasured and known bad.** On `large`, `"what stopped the
boundary war"` retrieves the right note but drags in 74 of 207 nodes, because
term overlap plus one-degree neighbour expansion casts very wide. This is
explicitly the gap a cross-encoder reranker (§3.4) closes — measure precision
this time, not just recall, since the current baseline can't tell you if
you've made it worse while keeping recall the same.

**Two specific queries are documented targets, not blockers.** `messy` /
`"who was at the granary"` (0.67) and `large` / `"who keeps the accord"`
(0.50) both need genuine multi-hop reasoning the current design doesn't do.
Hybrid retrieval alone may not close these; note whether it does.

---

## Tasks

Do them in this order. Each builds on the last.

| # | Task | Effort |
|---|---|---|
| 3.1 | SQLite state layer | Medium |
| 3.2 | Port the ingest pipeline | Large |
| 3.3 | Knowledge graph on `petgraph` | Medium |
| 3.4 | Hybrid retrieval | Large |
| 3.5 | Port RAPTOR hierarchical summaries | Medium |
| 3.6 | Chunking | Small |

Commit each separately.

---

### 3.1 SQLite state layer

Replace the JSON-document save model (`SaveManager.gd`, `CampaignState.gd`)
with SQLite via `rusqlite`, schema migrations via `rusqlite_migration`. Tables
for: campaigns, characters, emotion events, inventory, plot flags, history
logs, knowledge nodes and edges, embeddings, RAPTOR summary nodes.

**Why, concretely**: `SaveManager.gd` loads and rewrites an entire campaign
document on every save. `CampaignState.gd` has no query capability — hence
the in-memory dictionaries everywhere it's read from — and no concurrency
story, hence the hand-rolled `_mutex: Mutex` guarding reads and writes
(`CampaignState.gd:10`). A real schema with real queries removes the need for
both.

This lands in `crates/orison-core/src/state/`.

**Exit**: campaign state round-trips through SQLite; a schema migration
applies cleanly to a save created by an earlier schema version (even if
that's just version 1 → version 1 for now — the migration *mechanism* needs
to exist and be tested, not just the schema).

---

### 3.2 Port the ingest pipeline

`VaultCompiler.gd` (1,358 lines) is the highest-value single file in the
repository. Its heading-alias heuristics, character-property inference,
asset discovery and writing-style extraction encode a lot of hard-won
knowledge about real, messy vaults.

**Port its behaviour, not its structure.** Do not attempt a line-by-line
translation of a 1,358-line GDScript file into idiomatic Rust in one pass;
identify the actual heuristics (which headings map to which fields, how
frontmatter is merged with body content, how ambiguous sections are
classified) and re-implement each one, pinned by a test against the `messy/`
fixture the moment it's ported. `messy/` exists specifically because every
file in it breaks ingest in one particular, documented way — treat each of
those as an acceptance test, not a smoke test.

Improvements to make during the port, each flagged as a gap in
[orison_audit.md §11](orison_audit.md) and **currently violated by the Godot
build**:

- **Wiki-links (`[[Entity]]`) parsed and used as graph edges.** Currently
  ignored entirely outside of dangling-link detection.
- **Tags (`#tag`) parsed and used for categorisation.** Currently ignored.
- **Tables, callouts (`> [!secret]`), and embedded images handled**, not
  silently dropped.
- **Code fences respected**, so a `---` inside a fenced code block is never
  mistaken for frontmatter (`messy/` has a fixture file testing exactly this).
- **An overflow bucket for unrecognised sections.** Any section that can't be
  mapped to a canonical field must be retained and made retrievable, not
  discarded. This is [rag_architecture.md §1.1](rag_architecture.md)
  ("Everything Must Be Ingested") stated as a product principle, and B-14 is
  the reason it matters in practice: characters kept *nothing* raw before the
  Phase 1 fix, and whatever the single LLM extraction pass missed was
  permanently gone. Don't rebuild a version of that gap for sections instead
  of characters.

This lands in `crates/orison-core/src/ingest/`.

**Exit**: `messy/` fixture ingests with the same 6-of-6 required source
substrings as the Godot baseline, plus zero silently dropped sections
(new metric — the Godot build has no way to report this, so this task adds
the check that would have caught B-14's shape one level up).

---

### 3.3 Knowledge graph on `petgraph`

Typed nodes and typed edges, persisted in SQLite (§3.1), traversed with
`petgraph`. **Make it the only entity store.**
[orison_audit.md §9](orison_audit.md) records that `VaultCompiler` and
`CampaignState` each maintain their own entity dictionaries in the Godot
build and bypass the graph entirely for large parts of their logic — three
sources of truth that can and do disagree. In the Rust port, every entity
query routes through the graph. If a caller wants entity data and reaches for
anything but the graph, that's a design smell worth stopping and questioning.

This lands in `crates/orison-core/src/knowledge/`.

**Exit**: no parallel entity dictionaries anywhere in `orison-core`. A quick
grep-able test: nothing outside `knowledge/` should hold a
`HashMap<EntityId, _>` of graph entities as a cache of graph state.

---

### 3.4 Hybrid retrieval

The second-largest quality win in the whole migration plan, after Phase 2's
inference layer.

Pipeline: metadata filter → parallel BM25 (`tantivy`) and dense ANN
(`sqlite-vec`) → reciprocal rank fusion → cross-encoder rerank →
budget-aware truncation (using `crate::prompt::budget` from Phase 2, unchanged).

**Why this matters specifically for Orison, not retrieval in general**: a
personal worldbuilding vault is *almost entirely* rare proper nouns — invented
character names, place names, faction names. Dense embeddings are
structurally weak on exactly that (B-9): a name the embedding model has never
seen doesn't cluster near anything useful. BM25 handles rare-token exact
match natively. The `large` fixture's `"Quillion"` query exists specifically
to demonstrate this — trivial for BM25, hard for dense.

`EmbeddingStore.get_knn` (B-7) computes cosine similarity against *every*
stored vector, held in a `Dictionary` and persisted as one JSON file.
Acceptable at the few hundred nodes in `messy`/`minimal`; this is exactly what
`large` (207 nodes, deliberately) exists to break, and a real vault can be an
order of magnitude bigger than `large`. `sqlite-vec` replaces the brute force;
`tantivy` replaces the containment/term-overlap scoring the current build
uses (see B-13's history — the Godot build never had real BM25 despite
advertising it).

**Measure each stage's contribution separately** against
`fixtures/vaults/*/ground_truth.json` (recall@5, and this time precision too
— see "the numbers to beat" above). Add a stage only where the number
actually moves. Do not adopt the full pipeline on faith just because the plan
describes it; if reranking doesn't move recall@5 on these fixtures, say so
and record why before deciding whether to keep it anyway for precision.

This lands in `crates/orison-core/src/retrieval/`.

**Exit**: recall@5 exceeds the Godot baseline above on every fixture; each
stage's individual contribution is measured and recorded, not just the final
number; the two documented hard queries (granary, accord) are re-tested and
their outcome — improved, unchanged, or still failing — is recorded plainly
either way.

---

### 3.5 Port RAPTOR hierarchical summaries

Levels 0/1/2 as described in
[rag_architecture.md §1.3 and §3.2](rag_architecture.md), preserved as
designed — this is not a redesign task.

Two specific replacements:

- Replace the hand-rolled k-means (`VaultCompiler.gd:1071-1156`,
  `_kmeans_cluster`) with `linfa-clustering`.
- Replace the round-robin partitioning fallback (used when k-means isn't
  viable — e.g. too few candidate nodes) with a proper fallback path now that
  embeddings are always available in-process via `InferenceBackend::embed`
  (Phase 2) rather than gated behind an optional Ollama model being installed.

This lands in `crates/orison-core/src/knowledge/` alongside the graph, or a
`knowledge::raptor` submodule — RAPTOR summary nodes are graph nodes with a
level, not a separate store.

**Exit**: L1/L2 summary generation runs on the `messy`/`large` fixtures and
produces the same cluster-count shape as the Godot build
(`max(3, ceil(n/5))` for L1, `max(1, ceil(l1_count/5))` for L2) — not
necessarily identical clusters, since `linfa-clustering`'s k-means won't
match a hand-rolled implementation exactly, but the same *shape* of the
hierarchy.

---

### 3.6 Chunking

The current design embeds whole nodes, which is part of why lore context gets
truncated (`rag_architecture.md` Bug 5, the 409-token truncation). Introduce
explicit chunking with overlap for long notes, and record chunk-to-note
provenance so retrieved context can cite the note it came from.

This matters for two separate things: retrieval precision now (a whole-node
embedding for a long note is a blurry average of everything in it), and
eventually showing the player *why* the story knows something — provenance is
worth building even though nothing consumes it yet, since retrofitting it
after Phase 4's turn loop exists to display it is much more expensive than
carrying it from the start.

This lands in `crates/orison-core/src/ingest/` (chunking is an ingest-time
concern) with the provenance data flowing into `retrieval/`.

**Exit**: a long note (pick one from `large`) is chunked with overlap;
retrieved chunks carry their source note ID; a test asserts the ID is present
and correct.

---

## How to measure during this phase

Phase 2's awkwardness (no Rust engine for the GDScript harness to drive)
doesn't fully apply here — Phase 3 has real data to test against, namely
`fixtures/vaults/*/ground_truth.json`, which is already plain,
language-neutral JSON. But there's a narrower version of the same problem:
**the full eval harness port is Phase 5's job, not this phase's.** Do not
build `orison-eval` as a crate this phase. Instead:

- **Ingest completeness and knowledge-graph structure** (§3.2, §3.3) are
  measurable with `cargo test` alone: read a fixture vault from disk, ingest
  it, assert against `ground_truth.json`'s `entities`/`required_fields`. No
  model needed.
- **Lexical retrieval (BM25) and rank fusion** (§3.4) are measurable with
  `cargo test` alone — no model needed, same as the ingest tests.
- **Dense retrieval and RAPTOR summarisation** need real embeddings, which
  means a live backend. Gate these behind an environment variable exactly the
  way Phase 2's conformance suite gates live-Ollama cases
  (`ORISON_TEST_OLLAMA_URL`/`ORISON_TEST_OLLAMA_MODEL` already exist; reuse
  them rather than inventing new ones). Skip loudly when unset, per the
  Phase 1 lesson below about metrics that never fire.
- **The Godot harness keeps running in CI, unchanged.** It's still the
  reference implementation's own measurement, and Phase 5 is still where it
  gets ported, not before.

---

## Things Phase 1 and Phase 2 learned the hard way

Carried forward because they generalise directly, plus what Phase 2 added.

**A metric that never fires is not a metric.** Phase 1's `--selftest` mode
and Phase 2's conformance suite both gate CI on self-test / typed-error
assertions rather than live numbers. Do the same for §3.2/§3.3/§3.4's
`cargo test`-only cases: assert against fixtures you control, not against
"did this print a plausible-looking number."

**Prove the negative result before believing it.** Phase 1's retrieval-scored-
0.000 finding was only trustworthy after a positive control proved the graph
was loaded at all. If a Phase 3 retrieval stage scores surprisingly low (or
surprisingly high), find the control case before reporting the number.

**Silent degradation is the recurring theme, and it isn't fully closed yet.**
Phase 2 made backend and budget failures structurally typed, but ingest is a
new surface for the same bug shape: a section VaultCompiler's port doesn't
recognise must go to the overflow bucket (§3.2), never silently vanish the
way ordinary sections did pre-port. If you catch yourself writing a `match`
over section types with no `_ =>` arm that keeps the content, that's the bug
recurring in a new file.

**A dependency compiling is not the same as a dependency working.** Phase 2
verified `llama-cpp-2` compiles cleanly against its real API but never ran it
against real model weights, because the environment had none. The same trap
is live for `tantivy`, `sqlite-vec`, and `linfa-clustering` in this phase:
get a real smoke test running against the `messy` fixture early — ingest a
handful of real nodes, index them, query them — rather than writing the full
pipeline and discovering at the end that an index format assumption was
wrong. A dependency that only compiles is an unverified dependency.

---

## Conventions

- **Branch**: `claude/phase3-<task>`. Do not push straight to `main`.
- **Commits**: one per task, explaining *why*.
- **Do not open a pull request** unless asked.
- **Do not touch the Godot build** beyond bug fixes. It is under feature
  freeze and remains the reference implementation until Phase 5.
- **Do not start Phase 4.** No turn state machine, no Director/Actor
  orchestration, no emotion engine wiring. If a task seems to need them, you
  have misread it — Phase 3 is data in, data structured, data retrievable;
  nothing in this phase runs a turn.
- **Model IDs are configuration, never constants** (B-10, unresolved — still
  applies to any model reference this phase introduces, including embedding
  models).
- When this document contradicts the code, **trust the code and say so.**
  Every prior phase's handoff contained at least one error found this way;
  that is the system working, not a failure of this document.

---

## Open questions Phase 3 does not resolve

Recorded so nobody re-litigates them here.

**Exact BM25/dense fusion weighting.** The plan specifies reciprocal rank
fusion as the mechanism, not the weighting between BM25 and dense scores.
Tune it against `ground_truth.json` during §3.4; don't guess a weighting up
front and ship it unmeasured.

**Whether `sqlite-vec`'s ANN index holds up past `large`'s 207 nodes.** A real
personal vault can be considerably bigger. `large` is what's available to
test against now; if the index type chosen here doesn't scale, that's a
Phase 3.4-follow-up finding to record, not something to solve speculatively
now with no bigger fixture to measure against.

**Chunk size and overlap parameters (§3.6).** Not prescribed here — these
need experimentation against real notes, and the "right" answer plausibly
differs between a two-paragraph character bio and a multi-page lore document.
Record what you tried and why you landed where you did, the same way §3.4
should record why fusion weighting landed where it did.

**Whether RAPTOR cluster counts should scale differently at real-vault
scale.** The `max(3, ceil(n/5))` / `max(1, ceil(l1_count/5))` shape is ported
from the Godot build as-is (§3.5); it was never validated against anything
bigger than `large`. Worth an experiment in a later phase, not a blocker here.
