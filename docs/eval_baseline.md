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
