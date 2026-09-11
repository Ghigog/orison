# Orison Evaluation Baseline

> **Purpose**: the numbers the migration is graded against. Phase 5 of
> [migration_plan.md](migration_plan.md) cannot be declared complete until the
> Rust engine meets or beats every figure here.
>
> **Recorded**: 7-8 September 2026 against `main`, in three passes: as found at
> the close of Phase 0, after fixing the defects the harness surfaced (B-13,
> B-14, B-16), and finally live against real local models. The port must beat the
> *fixed* figures.
> **Engine**: Godot / GDScript, ~19,600 lines. Identical results on 4.6 and 4.7;
> CI runs 4.7.
> **Models** (live run): Director `gemma4:e2b`, Actor `llama3.2:3b`, embeddings
> `nomic-embed-text`, via Ollama on an Apple Silicon MacBook Air. Quote no
> narrative number without these; a baseline is meaningless without the models
> that produced it.
> **Harness**: `eval/EvalRunner.tscn`. See [Running it](#running-it).

---

## Read this before quoting any number below

The harness runs in two modes and they measure different things.

| Mode | What it proves | Status |
|---|---|---|
| **Replay, synthetic cassette** | The *harness* works. Responses are canned and deliberately defective. | Runs in CI. |
| **Replay, recorded cassette** | The *engine* works, deterministically. | `eval/cassettes/baseline.json` |
| **Live** | Same, calling Ollama directly and recording a cassette. | Recorded 8 Sep. |

Narrative figures below are from a **live run against real models**, not the
synthetic cassette. Structural figures are from replay, which isolates the
lexical retrieval path.

Two caveats that matter:

- **The recorded cassette is now stale against `main`.** It keys on full prompts,
  and the B-16 fix changed those prompts by populating character fields that were
  previously empty. That is the intended brittleness: a cassette recorded against
  different prompts is not evidence about the current ones. Re-record before
  quoting narrative numbers as current.
- **Retrieval differs between modes.** Replay stubs embeddings out; the live run
  had `nomic-embed-text`, activating the dense half of the rank fusion. Both are
  recorded below and the difference is itself a finding.

---

## Structural baseline

### Compilation

| Fixture | Notes | Compile time |
|---|---:|---:|
| `minimal` | 5 | 2 ms |
| `messy` | 8 | 8 ms |
| `large` | 207 | 100 ms |

Compilation is not a bottleneck and is not expected to become one. Recorded so a
future regression is visible, not because it is currently a concern. Note these
are with a mocked LLM: real compilation makes one model call per character file,
so wall-clock time on `large` will be dominated entirely by inference.

### Entity and edge extraction

| Fixture | Entity presence | Edges | Dangling links |
|---|---|---|---|
| `minimal` | 5/5 | 1/1 | n/a |
| `messy` | 7/7 | 3/3 | tolerated correctly |

All entities were found, including the four hostile character files, and the
dangling `[[The Chancellor]]` link did not produce a phantom entity. Edge
extraction from wiki-links works.

This is genuinely good and worth saying plainly: the ingest pipeline's *structure*
holds up against deliberately hostile input.

### Retrieval — the headline result, and its fix

Retrieval was measured, found to be almost entirely non-functional, fixed, and
re-measured. Both states are recorded: the first is what the harness found, the
second is the bar the Rust port must actually beat.

| Fixture | As found | After fix (lexical only) | Live (+ embeddings) |
|---|---:|---:|---:|
| `minimal` | 0.200 | **1.000** | **1.000** |
| `messy` | 0.000 | **0.933** | **1.000** |
| `large` | 0.000 | **0.875** | not run live |

Queries returning nothing went from 13 of 14 to zero.

**Hybrid retrieval earns its place, measurably.** Adding `nomic-embed-text` took
`messy` from 0.933 to 1.000, and specifically fixed `"who was at the granary"`,
which went 0.67 → 1.00. That is the multi-hop case whose connecting fact lives
only in an untyped scratch note: lexical scoring could not reach it and dense
retrieval could. That single query is the clearest empirical argument in this
document for the fusion design in Phase 3.4.

**As found, 13 of 14 queries retrieved literally zero nodes.** The single success
was a positive control deliberately written to embed a node's full label verbatim.

This was defect **B-13**: a single inverted condition at
`KnowledgeGraphManager.gd:179`.

```gdscript
if normalized_prompt.contains(label) or normalized_prompt.contains(id.to_lower()):
```

It asked whether the **query contained the node's label**, not whether the node
matched the query. Lexical retrieval therefore fired only when the player typed
an entity's full name inside their sentence, and note bodies were never searched
at all: only labels and ids. There was also **no BM25 anywhere in the codebase**,
despite the retired feature map advertising "Hybrid BM25 + KNN semantic
retrieval", so with no embedding model installed retrieval returned nothing
whatsoever.

The control query is what makes those zeros trustworthy rather than a suspected
harness fault: `"tell me about thornwick archive"` contains the label verbatim
and scored 1.00, while `"Thornwick"` alone scored 0.00, because
`"thornwick".contains("thornwick archive")` is false. Keep that control.

**The fix** replaces the containment test with term-overlap scoring over each
node's label, id, tags, description and body, with stopword filtering and
weighting that favours label matches. Retrieval latency: p50 0 ms, p95 1 ms both
before and after.

**What the fix is not.** It is not BM25. There is no corpus-wide inverse document
frequency and no length normalisation, so a long note is easier to match than a
short one. Phase 3.4 still replaces it with real BM25 via `tantivy` plus dense
ANN and rank fusion; this is a large improvement over a broken path, not a
substitute for the planned work.

**Precision is now the weak point, and it is the honest caveat on those numbers.**
On `large`, `"what stopped the boundary war"` retrieves the right note but drags
in 74 of 207 nodes alongside it, because term overlap plus one-degree neighbour
expansion casts very wide. Recall is fixed; precision is not measured yet and
should be. This is exactly the gap a cross-encoder reranker closes, and it
strengthens rather than weakens the Phase 3.4 argument.

Two cases remain imperfect and are worth keeping as targets:

- `messy` / `"who was at the granary"` scores 0.67. The connection lives only in
  an untyped scratch note; genuine multi-hop reasoning is still missing.
- `large` / `"who keeps the accord"` scores 0.50. It finds the Accord but not the
  chapel that holds it, which is the same two-hop limitation.

### Ingest completeness

| Fixture | As found | After fix |
|---|---|---|
| `messy` | 3 of 6 required source substrings | **6 of 6** |

Originally dropped: `"salt throne at nineteen"`, `"abolished before he dies"`,
`"loyal to individuals rather than institutions"` — all three from character
files.

This was defect **B-14**, and it was architectural rather than a parsing slip.
`VaultCompiler.gd:254` built character nodes with frontmatter as properties and
the LLM-extracted biography as `desc`. Scenes and locations kept the original
prose in `props["body"]`; **characters kept nothing.** Whatever the single
extraction pass missed was unrecoverable, and nothing downstream could tell
"this character has no personality written" from "the extractor failed on this
file".

That violated [rag_architecture.md](rag_architecture.md) §1.1 outright, and it is
why the bold-numeric-heading class of bug (Bug 1) was ever catastrophic: those
bugs are only unrecoverable *because* there was no raw text to fall back on.

**The fix** retains the full source body on every node type, so extraction is now
additive to the source rather than a replacement for it. All six required
substrings survive compilation.

One caveat on interpreting the original 3/6: under a synthetic cassette the LLM
extraction returns canned filler, so the metric alone could not distinguish "the
compiler dropped it" from "the mock replaced it". Code inspection settled that,
and the fix confirms it. Whether Bug 1's heading parsing itself still misfires is
a separate question that needs a recorded cassette; it now merely degrades
quality rather than destroying data.

---

## Narrative baseline

Measured live, 8 September 2026. Nine scripted turns: four as Bram Holt
(`minimal`), five as Lord Anneke (`messy`).

| Metric | Result | Notes |
|---|---|---|
| **Schema validity** | **9/9 (1.000)** | Every response parsed strictly, without `JsonRepair`. |
| **Pronoun consistency** | Clean on `messy`; 1 flag on `minimal` | The flag is a probable false positive, see below. |
| **Loop detection** | Clean | Including a deliberately repeated player question. |
| **Forbidden phrasing** | Clean | No third-person self-reference, no leaked affinity. |
| **Turn latency p50** | **21.2 s / 20.2 s** | The number that should worry you. |

### Bug 3 did not reproduce

This is the headline narrative result. `messy` runs five turns as **Lord Anneke**,
constructed as the hardest possible pronoun trap: no gender field, a title and
body that are unambiguously masculine, a name carrying a strong feminine prior,
and a final player line that says "her" about a third party to bait an echo.

`llama3.2:3b` handled all five turns cleanly.

The single flag is on `minimal` turn 3, where Bram Holt is asked who keeps the
archive. Elara Voss keeps it and is a woman, so "she" in that narration is
probably correct. This is the documented residual limitation of the metric: it
cannot distinguish a wrong pronoun for the speaker from a right one for a third
party. **Treat any single pronoun flag as needing a human read**, not as a defect.

Bug 3 was diagnosed in June against a model receiving *empty* character data,
because extraction was silently failing (B-16). With fields actually populated,
the gender line reaches the prompt and the model uses it. Worth re-testing after
re-recording, but the June diagnosis may simply have been downstream of B-16.

### Schema validity at 9/9 is a real finding

Every response parsed on the first attempt, strictly, with no repair. The harness
deliberately does **not** route transcript responses through `JsonRepair`,
because scoring post-repair output would hide the defect being measured.

That is evidence `JsonRepair` is not load-bearing for the character agent on this
model, which lowers the risk of the Phase 2.4 constrained-decoding work: it is
formalising something that already mostly holds rather than fixing something
broken. Note the contrast with the *extraction* path, where the same absence of
repair was catastrophic (B-16) because that path used a stricter parser.

### Latency is the worst number in this document

**p50 ~20 s per turn** on a 3B model, and 157 s / 283 s to compile five and eight
notes respectively. Effectively all of it is inference.

Twenty seconds per conversational turn is not a playable experience, and it is
the strongest evidence yet for the Phase 2 inference work. The engine calls
`/api/generate` with a monolithic prompt rebuilt every turn, so there is no KV
cache reuse whatsoever (B-2). Cache-stable prompt ordering and `/api/chat` exist
precisely to fix this. Treat 20 s as the number Phase 2 must beat.

## What is still missing

The narrative half. It needs one run on a machine with Ollama.

### How the transcript suite works

The harness drives **real scripted turns**: for each player line in a fixture's
`transcript_script`, it builds the prompt with the real `PromptBuilder`, sends it
through the real `LLMClient` as the character agent, parses the response strictly
(deliberately *not* through `JsonRepair`, since the point of migration plan 2.4 is
that repair should be unnecessary), scores it, and feeds the dialogue back into
history so the next turn sees it.

The same code path runs whether a cassette or a live model is behind it. That is
what makes a recorded cassette a baseline rather than a prop.

`messy` carries the transcript that matters. Its character is **Lord Anneke**: no
gender field, a title and body that are unambiguously masculine, and a name with a
strong feminine prior. The last player line deliberately says "her" about a third
party, so a model that simply echoes the player's pronouns gets caught.

### Recording a baseline

On a machine with Ollama running and your configured models pulled:

```bash
# Recommended. Records everything the narrative baseline needs, in ~7 model calls.
godot --headless --path . res://eval/EvalRunner.tscn -- --live --fixture=minimal,messy
```

**Do not use `--fixture=all` for a live run unless you mean it.** Compilation
makes one model call per character file, and `large` has 170 of them, so `all` is
roughly 177 sequential calls plus RAPTOR summaries: tens of minutes on local
hardware. `large` contributes only retrieval numbers, and retrieval in live mode
differs from replay solely by having embeddings available. Every narrative metric
comes from `minimal` and `messy`, which are seven character files between them.

The cassette is flushed after each fixture, so an interrupted long run keeps what
it recorded rather than discarding all of it.

On macOS, Godot is not on `$PATH`; the binary lives inside the app bundle:

```bash
/Applications/Godot.app/Contents/MacOS/Godot --headless --path . \
  res://eval/EvalRunner.tscn -- --live --fixture=minimal,messy
```

Then copy the cassette out of Godot's user data directory, which differs by OS:

| OS | Path |
|---|---|
| macOS | `~/Library/Application Support/Godot/app_userdata/Orison/` |
| Linux | `~/.local/share/godot/app_userdata/Orison/` |
| Windows | `%APPDATA%\Godot\app_userdata\Orison\` |

```bash
# macOS
cp ~/Library/Application\ Support/Godot/app_userdata/Orison/recorded_cassette.json \
   eval/cassettes/baseline.json
```

Commit it, then re-run in replay and update the narrative section of this file:

```bash
godot --headless --path . res://eval/EvalRunner.tscn -- --cassette=baseline --fixture=all
```

### Three things to know before recording

**Embeddings change the result.** Replay stubs out `get_embedding`, which isolates
the lexical path. A live run with `nomic-embed-text` installed activates the dense
half of the rank fusion, so retrieval recall may differ from the figures above.
That is a better number, not a contradictory one, but note which you are quoting.

**Live mode bypasses the request queue.** The recording proxy re-enters at
`_raw_send_custom_request` to capture responses, which skips the serialisation in
`send_custom_request`. The harness awaits each turn, so calls are sequential in
practice, but if Ollama returns concurrency errors during a live run, this is why.

**The cassette keys on full prompts.** Exact-match replay. Reword a prompt and the
cassette stops matching, which surfaces as a schema failure rather than a silent
wrong answer. That is intended: a cassette recorded against a different prompt is
not evidence about the current one. Re-record after prompt changes.

## Phase 2 measurement

Phase 2 replaces the inference layer with a typed Rust core
(`crates/orison-core`). The GDScript harness above has no Rust engine to
drive during this phase (see the Phase 2 handoff's "How to measure during
this phase"), so Phase 2 grades itself with its own conformance suite
(`crates/orison-core/tests/conformance.rs`) instead:

- `cargo test -p orison-core` (no live model needed): typed-error coverage
  for an unreachable backend (B-15), the bounded `keep_alive` default
  (B-6), the three-tool Director set with `search_knowledge_graph`
  excluded (§2.6), typed tool-call dispatch and its failure mode, budget
  allocation/overflow (B-1, B-4), cache-stable message ordering (§2.7),
  and schema round-trips for `CharacterResponse`/`DirectorResponse`/
  `ReactStep` (§2.4). `cargo test -p orison-core --features llama-cpp`
  additionally runs the JSON-Schema-to-GBNF grammar converter's tests.
- `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_OLLAMA_MODEL` (live Ollama):
  `health()` reporting `Available`, and a schema-constrained chat round
  trip via `/api/chat`.
- `ORISON_TEST_GGUF_MODEL` with `--features llama-cpp`: not yet wired into
  the suite — `LlamaCppBackend` (§2.3) compiles cleanly against real
  `llama-cpp-2` APIs (verified in this environment) but has not been run
  against real weights, since this sandbox has neither a GGUF file nor a
  network path to fetch one. Verify on a machine with a model before
  relying on it in place of `OllamaBackend`.

**Turn latency p50 (§2.7) was not re-measured live in this environment.**
This sandbox has no Ollama server and no GPU, so there is no live 9-turn
transcript run to report a before/after number against the ~20s baseline
above — reporting one without actually measuring it would repeat exactly
the mistake this document warns against ("prove the negative result before
believing it" / "measure it, don't declare victory on the theory"). What
*is* verified here is the mechanism the speedup depends on:
`prompt::ordering` has a test asserting that everything but the player's
final input is byte-identical between consecutive turns, which is the
precondition for KV-cache reuse under `/api/chat`. Run
`cargo test -p orison-core -- --nocapture` plus a live transcript against
Ollama on a real machine to record the actual number.

### Not built: the judge suite

Plan §1.3. Scores in-character consistency, use of vault-sourced facts, narrative
progression and absence of sycophancy on a 1-5 rubric. Deliberately last: judge
scores are noisy and only meaningful as a trend across many samples, the
deterministic suite above is the actual gate, and there is not yet a single real
transcript to judge.

---

## Phase 3 measurement

Phase 3 replaces the data layer: SQLite state, the ported ingest pipeline, the
`petgraph` knowledge graph, hybrid retrieval, RAPTOR and chunking. Like Phase 2
it grades itself with `cargo test`, because the eval harness port is Phase 5's
job. Unlike Phase 2 it has real data to test against — `ground_truth.json` is
already plain, language-neutral JSON — so the structural numbers below are
measured, not asserted.

```bash
cargo test -p orison-core                                    # everything below
cargo test -p orison-core --test retrieval_quality -- --nocapture   # the tables
```

### Retrieval — recall against the Godot baseline

Measured at each query's own `k` from `ground_truth.json` (5 for `minimal` and
`messy`, 10 for `large`), which is the same basis as the Godot figures.

| Fixture | Godot (fixed, lexical) | Rust BM25 | Rust BM25 + graph expansion |
|---|---:|---:|---:|
| `minimal` | 1.000 | **1.000** | **1.000** |
| `messy` | 0.933 | **1.000** | **1.000** |
| `large` | 0.875 | 0.875 | **1.000** |

No query returns empty on any fixture, against 13 of 14 as found in Phase 1.

### The two documented hard queries

Both are closed, not merely re-tested. They were recorded as targets rather than
blockers, and both needed the multi-hop reasoning the Godot design does not do.

| Query | Godot | Rust | What carried it |
|---|---:|---:|---|
| `messy` / `"who was at the granary"` | 0.67 | **1.00** | Real BM25. The connecting fact is in an untyped scratch note, which term-overlap scoring could not surface and IDF-weighted BM25 does. |
| `large` / `"who keeps the accord"` | 0.50 | **1.00** | Prose-name edges plus graph expansion. The chapel note says nothing about accords, so no scoring of its text can reach it; only the edge can. |

### Each stage's contribution

The handoff asks for stages to be earned rather than adopted on faith. What each
one actually did:

| Stage | Effect | Kept? |
|---|---|---|
| BM25 (`tantivy`) | `messy` 0.933 → 1.000 on its own; fixes the granary query outright. Real IDF and length normalisation are the difference — the Phase 1 fix had neither, and on `large` 170 of 207 files share a handful of surnames. | Yes |
| Prose-name edges (ingest) | `large` 0.875 → 1.000. Without them the accord query stays at the documented 0.50 and `large` cannot beat its baseline. | Yes |
| Graph expansion | The stage that spends those edges. Seeded from the top 2 results at 1 hop, not from every match at 1 degree the way `retrieve_context` did. | Yes |
| Reciprocal rank fusion | Structurally required for the dense half; with BM25 alone it is a no-op that preserves order. | Yes |
| Cross-encoder rerank | **Not built.** No model was reachable to be one; see below. | — |
| Passage rerank (the stand-in) | **Moves no recall on any fixture.** Moves MRR on `messy`, 0.900 → 1.000. | Yes, with the null result recorded |

**The reranker's null result, stated plainly.** Every relevant result these
queries can reach is already inside the top `k` before reranking, so a set
measure at `k` cannot see the stage at all. That is why MRR was added: recall and
precision are set measures and a reranker reorders. It is kept because its cost
is one pass over candidates already in hand and because the precision problem it
targets is real at a scale no fixture here reaches — not because the plan lists
it.

**It is not a cross-encoder and does not claim to be.** A cross-encoder is a
model, and no weights were reachable from the environment this was built in.
Shipping something named for a model it does not have would repeat Phase 2's
`LlamaCppBackend`. `PassageReranker` scores a candidate's best window rather than
its whole text, which targets the same failure the cross-encoder is wanted for:
the 74-of-207 result recorded above happens because a whole-note score rewards a
long note for containing a query term anywhere in it.

### Precision, measured for the first time

The Godot build has no precision figure, and this document records the one
anecdote — 1 relevant note in 74 returned — as the known weak point. Numbers now
exist for it:

| Fixture | BM25 | + graph expansion |
|---|---:|---:|
| `minimal` | 0.460 | 0.410 |
| `messy` | 0.440 | 0.360 |
| `large` | 0.325 | 0.225 |

**Graph expansion costs precision everywhere it is used.** That is a trade, and
it is stated as one: recall is the exit criterion and the accord query is a
documented target, so it is the right trade here, but a future phase with a
precision budget should revisit the expansion seeds before anything else.

### Ingest

| Metric | Godot | Rust |
|---|---|---|
| `messy` required source substrings | 6 of 6 | **6 of 6** |
| `messy` silently dropped sections | not measurable | **0** |

Dropped sections are checked by coverage, not by a counter: every parsed
section's text must be findable on the entity it came from. A counter
incremented by hand cannot fire on the bug it is meant to catch.

Ingest also no longer needs a model. The Godot pipeline made one LLM call per
character file inside its first pass, so a vault could not be compiled at all
without a live endpoint, and a fenced JSON response (B-16) left the character
with nothing. Deterministic section parsing fills the canonical fields; a model
can only top up what the source did not say.

### RAPTOR

Cluster-count shape matches the Godot build exactly:

| Fixture | Candidates | L1 | L2 |
|---|---:|---:|---:|
| `messy` | 6 | 3 | 1 |
| `large` | 200 | 40 | 8 |

`max(3, ceil(n/5))` and `max(1, ceil(l1/5))`. Membership is not compared:
`linfa-clustering` will not reproduce a hand-rolled ten-iteration k-means and is
not expected to.

### What still needs a live model

Gated and skipping loudly, per the Phase 1 lesson about metrics that never fire:

- `ORISON_TEST_OLLAMA_URL` + `ORISON_TEST_OLLAMA_EMBED_MODEL` →
  `tests/retrieval_dense.rs`. A chat model identifier is not an embedding model
  identifier, which is why this is a second variable rather than a reuse of
  `ORISON_TEST_OLLAMA_MODEL`. The dense stage's *contribution to recall is
  therefore unmeasured*: every retrieval figure above is the lexical and
  structural half of the pipeline only. `eval_baseline.md`'s live Godot run
  showed embeddings taking `messy` from 0.933 to 1.000, so there is reason to
  expect it helps; nobody has run it here.
- RAPTOR summary *quality*. The hierarchy's shape is verified without a model;
  whether the summaries are any good is a different question and needs one.

### Two findings worth recording

**`ground_truth.json` overstates one rationale.** The `large` fixture says BM25
"cannot" carry `"what stopped the boundary war"` because the query shares no rare
term with the target. It does, at rank 1: the note contains the phrase "boundary
war" verbatim. BM25 needs a *distinctive* term, not a globally rare one. The
query is still a good dense test and the `why` field is worth keeping — with that
sentence corrected.

**There are no long notes in the fixture set.** The longest file in `large` is 53
words. `large` is a scale fixture, built to break brute-force vector search, and
nothing in it exercises multi-chunk behaviour at the default chunk size. The
chunking parameters (§3.6) are therefore mechanism-verified and not
corpus-tuned; tuning them needs documents that do not exist here yet.

---

## Phase 4 measurement

Phase 4 builds the turn loop. Two numbers this document had been carrying as
outstanding — turn latency p50 on the Rust stack, and dense retrieval's
contribution to recall — are measured below, on a machine with Ollama and a
real vault. The environment that wrote Phase 4's code had neither; the harness
for each was exercised only against a loopback stand-in, so the numbers below
are the first live evidence either way.

**Models for the live run**: `llama3.2:3b` (turn latency, matching the Godot
baseline's model), `nomic-embed-text` (dense retrieval), and the
Director/Actor experiment below. Apple Silicon MacBook Air, recorded 9 Sep.

### What is measured, and holds

Everything below runs in `cargo test` with no model and no network.

| Property | How it is checked |
|---|---|
| Cancellation actually stops the request | `tests/turn_cancellation.rs` asserts the *server* saw the connection close mid-response, with an uncancelled control case in the same file |
| The queue's sequential guarantee | `tests/turn_queue.rs` runs the jobs, where `test_llm_request_queue` inspects the bookkeeping and clears the queue before anything executes |
| A turn runs end to end | `tests/turn_loop.rs`, against `OllamaBackend` talking to a loopback stand-in — the shipping path, including `/api/chat`, NDJSON framing and constrained decoding |
| Cache-stable ordering, at the wire | The same file compares two consecutive request bodies byte for byte |
| RAG006 and RAG003 | `tests/emotion.rs` |
| Token-based compaction | `tests/memory.rs`: 40 short turns do not compact, 20 long ones do |
| Biography and session memory stay apart | `tests/memory.rs`, asserted on the request body |
| The prompt-module boundary | `tests/prompt_boundary.rs` |

Two defects were found by these rather than by reading, both in the
cache-stability work and both invisible to a unit test:

- The player's line was sent wrapped in `<player_message>` delimiters and
  replayed as bare text once it became history, so the shared prefix ended
  where the history began — the KV cache was invalidated on every turn, which
  is the opposite of §2.7's purpose.
- The emotional profile sat inside the character card, which is the *first*
  message of every request. Emotion moves nearly every turn, so the cache was
  invalidated from token zero. Identity and volatile state are now separate
  blocks, ordered apart.

### Turn latency — measured

```bash
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_OLLAMA_MODEL=llama3.2:3b \
  cargo test -p orison-core --test turn_latency -- --nocapture
```

Run against **the model the baseline was recorded on**, per the harness's own
requirement — an 8B model here would measure a change of model and a change of
engine at once, and the result could not be attributed to either.

| Fixture | p50 | p95 | Godot p50 | Ratio |
|---|---:|---:|---:|---:|
| `minimal` | 54.4 s | 63.2 s | 21.2 s | 0.39x |
| `messy` | 42.0 s | 52.9 s | 20.2 s | 0.48x |

**Cache-stable ordering is not paying off on this backend yet.** Time to first
token rose with prompt length in both fixtures (`minimal`: 25.2 s → 17.1 s
across turns, noisy rather than flat; `messy` similar) instead of staying flat,
which per the harness's own reading is the sign the prefix is not being
reused. p50 came in at roughly **2x the Godot baseline, not faster** — the
opposite of Phase 2's justification. Phase 5 must not treat p95 latency as
closed; it is the migration gate and this run fails it. Likely next step:
confirm the request bodies against `OllamaBackend` actually share a byte-
identical prefix turn to turn on a live server, not just in `tests/turn_loop.rs`
against the loopback stand-in — the unit-level guarantee held, the live
number did not, so the gap is somewhere between the two.

### Dense retrieval's contribution — measured

```bash
ollama pull nomic-embed-text

ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_OLLAMA_EMBED_MODEL=nomic-embed-text \
  cargo test -p orison-core --test retrieval_dense -- --nocapture
```

On `messy` (768-dimension embeddings from `nomic-embed-text`):

| | recall | precision | mrr |
|---|---:|---:|---:|
| BM25 only | 1.000 | 0.440 | 0.900 |
| BM25 + dense + RRF | 1.000 | 0.360 | 0.900 |

Recall was already at 1.000 from BM25 alone on this fixture, so the test's
hard assertion (fusion must not cost recall) passed trivially rather than
demonstrating a lift — `messy` is not the fixture that will show dense
retrieval earning its place. What it did show: fusing dense in **cost
precision** (0.440 → 0.360), the opposite direction from the Godot live run's
`messy` improvement (0.933 → 1.000 recall) cited above. The rare-proper-noun
case (B-9) came out strong — `The Quillion Accord` ranked 1st by dense
similarity for both `"Quillion"` and a paraphrased query with the name absent
— so dense is not broken, but the precision cost on `messy` is worth tuning
`RetrievalConfig`'s fusion weights against before trusting it in play.

### The Director/Actor experiment — measured

See [migration_plan.md](migration_plan.md) Appendix D for the full numbers,
the arms, and the decision recorded from them. Summary: arm A (the current
split) won or tied on quality-per-second on both fixtures, and its two
pronoun flags — the only reason its quality composite trailed the two
single-model arms — **were read by hand in Phase 5.0 and are both false
positives**. In each, the flagged pronoun belongs to a correctly-gendered
third party, not to the transcript character. Discounting them puts arm A's
quality at 1.000 on both fixtures and makes it the outright winner on both
rather than winning one and tying the other. D-4 stands: keep the split.
The unresolved caveat is the model class, not the pronouns — a 7B stood in
for the 8B arms.

### A real vault — the fastest way to find out whether any of this is right

```bash
ORISON_TEST_VAULT=/path/to/vault \
  cargo test -p orison-core --test real_vault -- --nocapture
```

No vault content is printed — counts, and note paths where a file needs
looking at. The one inviolable constraint applies to test output too.

It reports what the fixtures cannot: how many notes the classifier leaves
untyped, how many links dangle, how many characters have frontmatter that
disagrees with their headings, whether any note is long enough to chunk more
than once, and what retrieval costs at that scale with and without graph
expansion. One assertion is hard rather than informational —
`unaccounted_sections` must be empty, because "nothing is silently discarded"
is an invariant, not a target.

The open questions it speaks to, all recorded in the Phase 4 handoff:
precision after graph expansion at a real scale; whether the passage reranker
earns its place; whether chunk size and overlap survive documents longer than
53 words; whether `sqlite-vec` holds up past 207 nodes; and whether RAPTOR's
cluster counts scale.

---

## Phase 5 measurement

Phase 5 builds the playable CLI and the judge suite, and resolves the two
items Phase 4 handed forward. The machine that wrote it had **no Ollama and
no models**, which decides the shape of everything below: what could be
measured without one was measured, what could not is named as unmeasured
rather than estimated, and the harnesses were changed so the machine that
does have one gets an answer instead of a hint.

### The turn-latency regression: two causes, both structural, both fixed

Phase 4 measured the Rust turn loop at roughly 2x the Godot baseline while
`tests/turn_loop.rs` asserted byte-identical prompt prefixes at the wire.
Both were true, and two things sat in the gap.

**B-17: retrieved lore was ordered ahead of the growing transcript.**
`prompt::ordering` put it there on the reasoning that a repeated query keeps
its prefix stable. Play never repeats the query — `TurnEngine::retrieve_lore`
retrieves with the player's line — so the lore block changed on every turn and
everything behind it, which is the entire transcript, was re-processed every
time. The wire test did not see it because it asked *the same question twice*.
Lore now sits in the volatile tail, beside the emotional profile.

**B-18: the backend asked for the model's full advertised context window.**
`/api/show` reports what a model *can* serve; `llama3.2:3b` reports 131072,
sixteen times the 8192 the Godot baseline was recorded at. Ollama sizes the
runner's KV cache from `num_ctx` at load time whether the prompt fills it or
not, so this bought nothing and cost the whole allocation — tens of gigabytes
of cache for a 3B model, on a MacBook Air. `OllamaBackend` now caps at
`DEFAULT_CONTEXT_LIMIT` (8192, the baseline's own figure), overridable through
`OllamaConfig`, and the capped number is the single value feeding both
`num_ctx` and the prompt budget.

**Neither is confirmed by a live number, and this document will not pretend
otherwise.** What changed instead is what a live run measures. Ollama reports
`prompt_eval_count`: the prompt tokens it actually evaluated, excluding
whatever it served from its own prompt cache. That now reaches
`TurnOutcome::evaluated_prompt_tokens`, and `turn_latency` prints it:

```
turn |      sent | evaluated |  reused |      ttft |     total
   0 |      1130 |       880 |     22% |      6 ms |     64 ms
   1 |      1184 |       326 |     72% |      5 ms |     44 ms
   ...
   7 |      1475 |       333 |     77% |      6 ms |     58 ms
```

Cache reuse is a measured fraction now, not an inference from
time-to-first-token. Phase 4 could only say "TTFT rose, so probably no
reuse"; a re-run can say what fraction of the prompt the server skipped.

### Turn latency — the live re-run (Phase 5.6)

Run on a MacBook Air against a local Ollama, `llama3.2:3b`, the model the
baseline was recorded on. Director disabled, so this is the Actor turn alone,
which is what the Godot figure measured.

| Fixture | Godot p50 | Rust p50 | Rust p95 | Phase 4 p50 | Reuse, as then measured |
|---|---:|---:|---:|---:|---:|
| `minimal` | 21200 ms | 12570 ms | 15083 ms | 63200 ms | 0% — unsound, see below |
| `messy` | 20200 ms | 25028 ms | 30887 ms | 52900 ms | 0% — unsound, see below |

**The gate is p95 against the baseline, and `messy` does not meet it.**
`minimal` clears it with room (15.1 s against 21.2 s); `messy` misses by half
again (30.9 s against 20.2 s). Phase 5 does not exit on these numbers.

**The improvement over Phase 4 is real and large — 5.0x on `minimal`, 2.1x on
`messy`.** The p95 figures stand; the reuse column does not, and the paragraph
that used to stand here read it as evidence that B-17 had not reached the
backend. That reading was wrong, and this is why.

### The reuse column was measuring nothing

Two readings survived the run above. Either the prefix genuinely was not being
reused, or `prompt_eval_count` reports the full prompt whether or not it was
cached. `tests/prefix_cache.rs` settled it: three requests sharing one
byte-identical 2232-token system prefix, straight at `/api/chat`, no engine
involved.

| call | `prompt_eval_count` | `prompt_eval_duration` |
|---:|---:|---:|
| 0 | 2232 | 9858 ms |
| 1 | 2233 | 167 ms |
| 2 | 2232 | 191 ms |

**The count is flat and the work collapsed by 98%.** Ollama reuses the prefix
and reports the whole prompt anyway. `prompt_eval_count` is not a cache signal
on this build, whatever its documentation says, and every 0% above is an
artifact of reading it as one. The constant ~1.11 ratio between `evaluated` and
`sent` was never a full re-evaluation — it is the chat template's per-message
overhead on a count that reports the prompt's length and nothing about the
cache.

The metric is rebuilt rather than patched. `ChatDelta` and `TurnOutcome` now
carry `prompt_eval_time`, and `turn_latency` derives reuse from what a thousand
tokens of prompt cost the server, read against turn 0 where nothing is cached
yet. Time falls when the cache is used, which is the whole requirement. It is
also narrower than time-to-first-token, which carries queueing, sampling and
the first token's own decode.

The stand-in reported 75% over these transcripts while the live server reported
0%, and **the stand-in was the honest one** — the first time in this project
the disagreement ran that way. It now models the server it stands in for on
both fields: the full count, and a duration proportional to what a caching
server would actually have to evaluate.

**What this does not settle is whether the engine's own turns get that reuse.**
The probe shares a prefix by construction. Live turns rose from 2760 ms to
12630 ms time-to-first-token as the prompt grew, which is what re-processing
looks like and is not what this server does when handed a stable prefix.
Re-running `turn_latency` on the rebuilt metric is what answers it:

```bash
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_OLLAMA_MODEL=llama3.2:3b \
  cargo test -p orison-core --test turn_latency -- --nocapture
```

Until then B-17's effect on a live run is unmeasured, not absent, and the
`messy` p95 is the open regression.

### Turn latency on the rebuilt metric (Phase 5.6)

Same machine, same model, Director disabled. This is the first reading of the
gate that measures reuse in work rather than in a token count that does not
move.

| Fixture | Godot p50 | Rust p50 | Rust p95 | Gate | Mean reuse after turn 0 |
|---|---:|---:|---:|---|---:|
| `minimal` | 21200 ms | 14100 ms | **15500 ms** | met | 47%, falling 57% → 36% |
| `messy` | 20200 ms | 16907 ms | **26339 ms** | **missed** | 1% |

`minimal` clears the gate. `messy` does not, and it is the only thing standing
between Phase 5 and Phase 6 on latency.

**The reuse is real, partial, and decays.** On `minimal` the cost per thousand
prompt tokens drops from 4566 ms on turn 0 to 1986 ms on turn 1, then climbs
back to 2925 ms by turn 7. Turning that into tokens: the cached region is flat
at **roughly 715 tokens** across the whole transcript, while the prompt grows
from 1130 to 2011. A fixed head is being reused. The transcript behind it is
not. On `messy` not even the head survives — every turn pays the uncached rate.

**The engine is not what differs.** `tests/prefix_growth.rs` runs eight turns
against both fixtures with no model and asserts what the requests actually
look like:

```
=== minimal ===
turn 1:  7 messages,   921 words | shared with previous:  1 messages,  592 words ( 64%)
turn 7: 25 messages,  1136 words | shared with previous: 19 messages,  800 words ( 70%)

=== messy ===
turn 1:  7 messages,   949 words | shared with previous:  1 messages,  539 words ( 57%)
turn 7: 25 messages,  1247 words | shared with previous: 19 messages,  787 words ( 63%)
```

The shared prefix grows every turn, on both fixtures, and each turn makes
**exactly one** model call — so nothing is interleaving into the backend's
cache slot between turns. `messy` builds a prefix as cache-friendly as
`minimal`'s and gets none of the reuse, which rules the prompt out as the
difference. The two fixtures also share a single speaker each, so the
character card at the front of the prompt never moves.

Reverting §2.7's ordering freezes that prefix at 592 words and the test fails
with the live signature exactly: *"the shared prefix did not grow (592 → 592
words)"*. That the live run shows a frozen cached region while the message
list shows a growing prefix is the whole of the remaining puzzle.

**What is left to establish is whether this server reuses the shape a turn
actually has.** `tests/prefix_cache.rs`'s first probe holds one system message
fixed and varies the question; `llama3.2:3b` passes it outright. A turn is
harder: request *n* is `[system][turns 1..n-1][fresh tail][question]`, so the
prefix shared with request *n-1* ends where *n-1*'s tail began, and the server
must keep a proper prefix of what it holds and discard the rest. The second
probe sends exactly that and reports which of three things happened:

```bash
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_OLLAMA_MODEL=llama3.2:3b \
  cargo test -p orison-core --test prefix_cache -- --nocapture
```

If it reuses and holds, the engine's prompts are cache-stable and the
shortfall is below the message list, in how the prompt renders. If it reuses
and decays, the server keeps a fixed head by design and §2.7 cannot buy more
than that. If it does not reuse at all in this shape, the volatile tail
*behind* the transcript is itself the problem, and no amount of ordering
within the tail will help — the blocks have to come out from behind the
history.

Both verdicts in that probe were checked against servers built to exhibit each
behaviour, so it can tell them apart rather than defaulting to the reassuring
one.
### The stand-in was flattering the code in three places

Phase 4's lesson — "a test that passes against a stand-in is evidence about
the stand-in" — cost three more findings when the stand-in was made to behave
like the thing it stands in for.

| It did | A real Ollama does | What that hid |
|---|---|---|
| Advertised `context_length` 8192 | `llama3.2:3b` advertises 131072 | 8192 is exactly the cap, so the stand-in agreed with the code by coincidence and B-18 was unreachable in `cargo test` |
| Reported `prompt_eval_count: 0` | Reports the prompt's full length on the last chunk, cached or not, and bills the real work to `prompt_eval_duration` | Cache reuse was unmeasurable without a live server. Phase 5.6 corrected the second half of this row: the stand-in first modelled a count that *excluded* cached tokens, which is what Ollama documents and not what it does |
| Emitted response fields alphabetically | Emits them in the schema's order | `serde_json::json!` builds a `BTreeMap`, so the stand-in streamed **dialogue before narration** — every shell rendering that stream would show the character's answer before the narration setting it up, and only against the stand-in |

The stand-in now advertises a real window, models a prompt cache (billing
`prompt_eval_duration` for the tokens not shared as a prefix with the previous
request, while reporting `prompt_eval_count` in full as Ollama does), and
serialises the same Rust types the response is parsed back into, so its field
order cannot drift from the schema's.

**And the cache property became assertable without a model.** What the
stand-in charges for a prompt is a deterministic function of the message list
rather than of the machine running it, so `turn_latency`'s harness self-test
asserts that reuse *rises* as the transcript grows. Reverting the ordering fix
makes it fall — 65% to 54% on `minimal` — and the assertion fires. An absolute
floor could not tell the two apart on a fixture this short, because the
character card dominates either way.

### D-4: the human read, done

Both flagged narrations are **false positives**. `minimal`'s transcript
character is Bram Holt (he/him) and the flagged "her" belongs to Elara Voss,
who is being described; `messy`'s is Lord Anneke, the fixture's designated
pronoun trap, and every she/her belongs to Sergeant Adah, who is female.
Neither speaker takes a pronoun in either passage. Discounting them puts arm
A at quality 1.000 on both fixtures and makes it the outright winner rather
than winning one and tying the other. Full working in
[migration_plan.md](migration_plan.md) Appendix D. **D-4 stands: keep the
split.** The open caveat is the model class — a 7B stood in for the 8B arms —
not the pronouns.

### The deterministic suite, run against the CLI

`crates/orison-cli/tests/harness.rs` types each fixture's `ground_truth.json`
transcript into `Shell::run`, which is the function the binary hands `stdin`
to. Against the stand-in, so it runs in CI:

| Fixture | parsed | prn | rep | bad | notes |
|---|---:|---:|---:|---:|---|
| `minimal` | 4/4 | 0 | 3 | 0 | loop detection fires on a stand-in that repeats itself |
| `messy` | 5/5 | 0 | 4 | 0 | same |

The latencies from a stand-in are meaningless and are not recorded. What the
run establishes is that the shell drives a whole scripted transcript to
completion, that every deterministic metric is computable from what comes
back, and — through the verbatim-repeat assertion — that the metrics can
still fire. Schema validity is 100%, which under constrained decoding is the
only acceptable value.

Retrieval and ingest are unchanged from Phase 3/4 and still clear the
baseline: recall 1.000 on all three fixtures against 0.875-0.933, `large`
without mention edges still 0.875 (the positive control), ingest 6/6 required
substrings against the Godot build's 3/6.

### The judge suite: built, gated, unrun

`turn::judge` implements §1.3's rubric — in-character consistency, use of
vault-sourced facts, narrative progression, absence of sycophancy, each 1-5 —
as a schema-constrained response, with the rubric prose in `prompt::templates`
where `tests/prompt_boundary.rs` can enforce that it reads no state.

Three of §1.3's own rules are enforced rather than documented:

- The judge holds its own backend, never the Actor's, and `JudgeReport`
  records the judge's model id — a model grading itself is not a measurement
  and a report should not be able to hide that it was one.
- `JudgeReport::mean()` returns `None` below ten scored turns. Judge scores
  are noisy; a mean of four turns is noise with a decimal point, and it would
  be quoted.
- Nothing in it returns a pass or a fail. The deterministic suite is the gate.

Every score comes with a sentence naming the line it is about, and `render()`
prints those sentences with the numbers, because a judge score without its
justification is exactly the kind of figure that gets quoted without its
caveats.

**It has not been run against a model.** There is also no Godot judge baseline
to compare it against: Phase 1 deferred building the suite, so the number that
exit criterion compares to does not exist yet. Running it on the Godot build
is not possible under feature freeze either — which means the honest form of
that criterion is "record the Rust numbers, and treat the first run as the
baseline rather than as a comparison."

### What is still unmeasured, and the commands that would fix that

One machine with Ollama, `llama3.2:3b`, and a larger model closes all four.

```bash
# 1. Did B-17 and B-18 fix the regression? The gate.
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_OLLAMA_MODEL=llama3.2:3b \
  cargo test -p orison-core --test turn_latency -- --nocapture

# 2. The judge suite, and the first numbers for it.
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_ACTOR_MODEL=llama3.2:3b \
ORISON_TEST_JUDGE_MODEL=<a larger model> \
  cargo test -p orison-cli --test harness -- --nocapture

# 3. D-4 against the model class it is actually about.
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_EXPERIMENT_DIRECTOR_MODEL=<a true 8B> \
ORISON_EXPERIMENT_ACTOR_MODEL=llama3.2:3b \
ORISON_EXPERIMENT_SINGLE_MODEL=<the same 8B> \
  cargo test -p orison-core --test director_actor_experiment -- --nocapture

# 4. And the tuning work the real vault surfaced (62% overflow, 48% untyped).
ORISON_TEST_VAULT=/path/to/vault \
  cargo test -p orison-core --test real_vault -- --nocapture
```

Read (1) first, and read the `reused` column before the latency column. If
reuse is near zero after turn 0, the prefix is still not being served from
cache and there is a third cause; if it is high and p95 is still above the
baseline, the cause is not the cache and the ordering work is finished
either way. Phase 4 could not tell those two apart. This is the change that
matters most in Phase 5.

---

## Running it

```bash
# Full suite, synthetic cassette, harness self-test
godot --headless --path . res://eval/EvalRunner.tscn -- --fixture=all --selftest

# One fixture
godot --headless --path . res://eval/EvalRunner.tscn -- --fixture=messy

# Against a recorded cassette (the real baseline)
godot --headless --path . res://eval/EvalRunner.tscn -- --cassette=baseline --fixture=all

# Regenerate the large fixture (deterministic, fixed seed)
python3 fixtures/generate_large_vault.py
```

Machine-readable results are written to `user://eval_results.json` by default,
or wherever `--out=` points.

**Always pass a timeout when running this in automation.** If the script fails to
parse, Godot loads the scene with no script attached and the process sits there
forever rather than exiting. That cost a 10-minute CI-style hang during
development. The CI job sets one.

---

## Fixtures

| Fixture | Notes | Purpose |
|---|---:|---|
| [`minimal`](../fixtures/vaults/minimal) | 5 | Smoke test. Clean frontmatter, canonical headings. If this fails, the vault is not the problem. |
| [`messy`](../fixtures/vaults/messy) | 8 | The one that matters. Every file breaks ingest in a specific documented way: bold-numeric headings, absent frontmatter, `---` inside a code fence, a missing gender field against a misleading name, non-canonical sections, a dangling wiki-link, an untyped scratch note. |
| [`large`](../fixtures/vaults/large) | 207 | Scale. Generated with a fixed seed by `fixtures/generate_large_vault.py`. Contains planted needles: a rare proper noun appearing in exactly one note, and a purely thematic target with zero lexical overlap with its query. |

Each carries a `ground_truth.json` with labelled retrieval targets and a `why`
field on every case explaining what it is testing and which retrieval strategy
should win. Those `why` fields are the useful part; keep them current.

The `large` fixture's two needle queries are the clearest single argument in the
set for hybrid retrieval: `"Quillion"` should be trivial for BM25 and hard for a
dense model that has never seen the token, while `"what stopped the boundary war"`
is the exact inverse. Any retrieval design that wins on both has earned its
complexity.
