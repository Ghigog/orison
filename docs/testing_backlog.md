# Testing backlog

Measurement work that is **deliberately not being done now**. Each entry says
what is unknown, what it would cost to find out, and — the part that matters —
**what would have to be true for it to be worth doing**.

This file exists because Phase 5 spent two days chasing a latency number
through three broken instruments. The engine was never the problem; the
instruments were. The lesson is not "measure less". It is that a measurement
earns its cost only when a decision hangs on it, and that a measurement taken
with an unvalidated instrument is worse than none, because it is believed.

**Default answer for everything below: leave it.** Promote an entry when its
trigger fires, not when someone has an afternoon.

---

## 1. `messy` p95 turn latency, 26.3 s against a 20.2 s baseline

**Unknown**: why `messy` gets near-zero KV-cache reuse where `minimal` gets
partial reuse, when the two build equally cache-friendly prompts.

**Ruled out already** — do not redo this work:

- The engine's prompt assembly. `tests/prefix_growth.rs` proves both fixtures
  build a shared prefix that grows every turn over eight turns.
- Interleaved model calls evicting the cache. The same test asserts exactly
  one Actor call per turn.
- A moving character card. Both fixtures have a single speaker throughout.
- `prompt_eval_count` as the signal. It reports the whole prompt cached or
  not; `tests/prefix_cache.rs` established that and the metric now uses
  `prompt_eval_time`.

**What is left**: Ollama's own cache behaviour when a conversation grows and
the tail moves, on a memory-constrained machine. `tests/prefix_cache.rs`'s
second probe is aimed at exactly this and has been run once, inconclusively —
the two probes shared a server and a system prefix, so the growing probe's
baseline was itself a cache hit. That specific bug is fixed; the probe now
needs `--test-threads=1`.

**Worth doing when**: turn latency becomes a complaint from someone actually
playing, or the target hardware changes. Not before. The engine is already
2-5x faster than the Godot build it replaces on every fixture, and this is the
difference between two slow numbers, not between working and broken.

**Cost**: an hour, on a quiet machine, plus the discipline to re-validate the
probe before believing it.

---

## 2. The judge suite has never been run, and has no baseline to run against

**Unknown**: whether narrative quality meets or exceeds the Godot build.

`turn::judge` implements the §1.3 rubric and `crates/orison-cli/tests/harness.rs`
runs it, gated on `ORISON_TEST_JUDGE_MODEL`. Two things are missing and they
are different problems:

1. It has never been executed. Needs one machine with an 8B-class model
   alongside the Actor model.
2. **There is no Godot judge baseline to compare against.** Phase 1 deferred
   building the suite, so the numbers it would have recorded do not exist.
   Running it now yields absolute scores with nothing to beat.

**Worth doing when**: a quality regression is suspected, or before any claim
that the Rust engine writes *better* than the Godot one. For the second
half — the missing baseline — the Godot build is under feature freeze and
still runnable, so the baseline is recoverable for as long as that stays true.
It will not be recoverable after the Godot build is deleted. **If the Godot
build is ever scheduled for removal, capture this first.** That is the one
trigger here with a deadline attached.

**Cost**: a few hours, mostly model runtime.

---

## 3. `LlamaCppBackend` reports no prompt-evaluation time

`ChatDelta::prompt_eval_time` is `None` on every path in
`inference/llamacpp.rs`, so cache reuse is unmeasurable on that backend and
`turn_latency` would print "-" throughout.

**Worth doing when**: llama.cpp becomes a backend anyone actually runs. It is
currently the second backend behind Ollama and nothing ships on it.

**Cost**: small — the field exists and is plumbed; llama.cpp's server reports
`prompt_ms` in its timings block.

---

## 4. B-15: the engine reports success after total model failure

Still **Open** in the Appendix B register, inherited from the Godot build and
not yet re-checked against the Rust path. An unreachable model degrading
silently is a correctness bug, not a measurement gap, and it is the only Open
register entry that is about *behaviour the port might have carried forward*
rather than about the Godot build being replaced wholesale.

**Worth doing when**: before any build reaches a user who is not the author.
A silent failure is tolerable in a CLI the author runs; it is not tolerable in
a shipped shell, because there is no terminal to notice the absence in.
**Promote this at the start of Phase 6.**

---

## 5. Real-vault typing: 48% of notes untyped without folder mappings

Measured in Phase 4. `--folder-type` mappings fix it, and the import summary
prints the count so a user can iterate. Untyped notes are retrievable but are
not offered as characters to talk to or places to stand.

**Worth doing when**: Phase 6 builds vault import in the UI. A CLI user can
read the summary and re-run with mappings; a UI user cannot be expected to.
This is a UI-design problem more than an ingest one — the fix is likely
"show what was typed and let them correct it", not a better heuristic.

---

## 6. The Director/Actor arm comparison predates the corrected metric

D-4 is **decided** (keep the split) and the decision rests on quality, not
latency, so the corrected `prompt_eval_time` metric does not reopen it. But
the latency half of that comparison was taken with the broken reuse column.

**Worth doing when**: never, unless the split is being reconsidered on
performance grounds. Recorded here so that nobody re-derives the concern from
scratch and assumes it was overlooked.
