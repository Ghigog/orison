# Orison Evaluation Baseline

> **Purpose**: the numbers the migration is graded against. Phase 5 of
> [migration_plan.md](migration_plan.md) cannot be declared complete until the
> Rust engine meets or beats every figure here.
>
> **Recorded**: 7 September 2026, against `main` at the close of Phase 0.
> **Engine**: Godot 4.6 / GDScript, ~19,600 lines.
> **Harness**: `eval/EvalRunner.tscn`. See [Running it](#running-it).

---

## Read this before quoting any number below

The harness has two modes, and they measure completely different things.

| Mode | What it proves | Status |
|---|---|---|
| **Replay, synthetic cassette** | The *harness* works. Model responses are canned. | Recorded below. |
| **Replay, recorded cassette** | The *engine* works. Model responses are real, captured once from Ollama. | **Not yet recorded.** |
| **Live** | Same as above, but calls Ollama directly and writes a cassette. | Requires a machine with Ollama. |

**Everything in this document is from the synthetic cassette**, because the
environment it was produced in has no Ollama and no local models. That is not a
gap in the harness; it is the reason the harness has a record/replay mode at all.

What this means in practice:

- **Structural metrics are real.** Compilation, entity extraction, edge
  extraction and retrieval do not depend on model output for their *shape*, and
  the retrieval numbers in particular are genuine and damning.
- **Narrative metrics are not yet measured.** Schema validity, pronoun
  consistency, loop detection and forbidden phrasing currently run against
  deliberately defective canned text whose only job is to prove the metrics fire.
- **One metric is actively misleading under synthetic replay** and is flagged
  inline below: `entity_fields_populated`.

Filling in the narrative half is one command on a machine with Ollama. See
[What is still missing](#what-is-still-missing).

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

### Retrieval — the headline result

| Fixture | Mean recall | Queries returning nothing at all |
|---|---:|---:|
| `minimal` | **0.200** | 4 of 5 |
| `messy` | **0.000** | 5 of 5 |
| `large` | **0.000** | 4 of 4 |

**13 of 14 queries retrieved literally zero nodes.** The single success is a
positive control deliberately written to embed a node's full label verbatim.

Retrieval latency: p50 0 ms, p95 1 ms. Fast, because it is doing almost nothing.

This is defect **B-13**, confirmed empirically and recorded in
[migration_plan.md](migration_plan.md) Appendix B. The cause is a single inverted
condition at `KnowledgeGraphManager.gd:179`:

```gdscript
if normalized_prompt.contains(label) or normalized_prompt.contains(id.to_lower()):
```

It asks whether the **query contains the node's label**, not whether the node
matches the query. Lexical retrieval therefore only fires when the player happens
to type an entity's full name inside their sentence. Note bodies are never
searched at all: only labels and ids.

The control query proves this is the cause rather than a harness wiring fault.
`"tell me about thornwick archive"` contains the label `"thornwick archive"`
verbatim, and scores 1.00 with two nodes retrieved. `"Thornwick"` alone scores
0.00, because `"thornwick".contains("thornwick archive")` is false. Do not delete
that control: it is what makes every zero above trustworthy.

There is also **no BM25 implementation anywhere in the codebase**, despite the
retired feature map advertising "Hybrid BM25 + KNN semantic retrieval". The
reciprocal rank fusion in `retrieve_context` is real, but one of its two inputs
is a near-dead lexical path and the other requires `nomic-embed-text` to be
installed. With no embedding model present, retrieval returns nothing whatsoever.

### Ingest completeness

| Fixture | Retained |
|---|---|
| `messy` | **3 of 6** required source substrings |

Dropped: `"salt throne at nineteen"`, `"abolished before he dies"`,
`"loyal to individuals rather than institutions"`.

**Attribute this carefully.** Under a synthetic cassette the LLM extraction
returns canned filler, so this metric cannot distinguish "the compiler dropped
it" from "the mock replaced it". What it *does* establish, and what code
inspection confirms, is architectural:

`VaultCompiler.gd:254` builds character nodes with frontmatter as properties and
the LLM-extracted biography as `desc`. Unlike scenes and locations, which keep
`props["body"]` (lines 515, 548), **character nodes retain no raw source text at
all.** Whatever the extraction pass misses is unrecoverable, because the original
prose is never stored anywhere in the graph.

That is a direct violation of the data-lake principle in
[rag_architecture.md](rag_architecture.md) §1.1 ("no information should ever be
silently discarded"), and it is recorded as defect **B-14**. It also means the
bold-numeric-heading class of bug (Bug 1) has no safety net: if extraction fails
on a file, that character is simply blank forever.

Whether Bug 1 itself still reproduces cannot be answered from this run. It needs
a recorded cassette.

---

## Narrative baseline

**Not yet measured.** The figures the harness currently prints for
`schema_validity`, `pronoun_consistency`, `loop_detection` and
`forbidden_phrasing` come from the synthetic cassette, which contains one
deliberately planted defect per metric. They are a self-test, not a measurement.

The self-test passes: the harness detected all four planted defect classes. That
matters more than it sounds. A metric that never fires is indistinguishable from
a metric that always passes, and this suite is the thing the whole migration is
graded on. `--selftest` runs in CI so the harness cannot rot into a rubber stamp.

| Metric | Planted defect | Detected |
|---|---|---|
| `schema_validity` | Conversational prefix before the JSON object | yes |
| `pronoun_consistency` | `she` used for a he/him character | yes |
| `loop_detection` | Dialogue byte-identical to an earlier turn | yes |
| `forbidden_phrasing` | Third-person self-reference; leaked affinity score | yes |

One near-miss worth recording, because it is the kind of thing that quietly
destroys trust in an eval suite: the pronoun metric originally used substring
matching, and `"she was"` contains `"he "`. It flagged every correctly-gendered
feminine line as an error. It now uses word-boundary matching. A metric that
cries wolf is worse than no metric, because people learn to ignore it.

---

## What is still missing

Two things, both requiring a machine with Ollama.

**1. Record a cassette.** On a machine with Ollama running and the configured
models pulled:

```bash
godot --headless --path . res://eval/EvalRunner.tscn -- --live --fixture=all
cp ~/.local/share/godot/app_userdata/Orison/recorded_cassette.json \
   eval/cassettes/baseline.json
```

Commit that cassette. Every later run replays it deterministically, in CI, with
no Ollama needed. Then re-run and replace the narrative section above:

```bash
godot --headless --path . res://eval/EvalRunner.tscn -- --cassette=baseline --fixture=all
```

**2. The judge suite** (plan §1.3) is not built. It scores in-character
consistency, use of vault-sourced facts, narrative progression and absence of
sycophancy on a 1-5 rubric. It is deliberately last: judge scores are noisy, they
are for trend detection across many samples, and the deterministic suite above is
the actual gate. Building it before there is a single real transcript to judge
would be premature.

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
