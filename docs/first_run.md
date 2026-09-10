# Running Orison for the first time

The Rust engine is playable in a terminal as of Phase 5. This is how to play
it, and how to run the four measurements that decide whether Phase 6 can
start.

> **Read this line before anything else.** Phase 5's exit criteria are the
> migration gate, and two of the four are still open — not because work is
> missing, but because they need a machine with Ollama on it and the machine
> that wrote the code had none. Closing them is a run, not a rewrite. The
> commands are in [What still has to be measured](#what-still-has-to-be-measured).

---

## Prerequisites

```bash
# Rust, if it isn't already there.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Ollama, and the model the baseline was recorded on.
ollama pull llama3.2:3b

# Optional but wanted for the measurements: something 8B-class, for the
# Director role and for the judge.
ollama pull qwen2.5:7b-instruct   # or a true 8B; see the note in D-4
```

Ollama must be running (`ollama serve`, or the desktop app). Nothing in Orison
reaches anything but the endpoint you configure — that is a product pillar,
not an implementation detail.

```bash
cargo build --release -p orison-cli    # the binary is target/release/orison
```

---

## Ten minutes: play the fixture vault

`fixtures/vaults/minimal` is five notes: two locations joined by a mill road,
a talkative guard at one end, a dry archivist at the other, and a copper
kettle that has never been seen to move.

```bash
cargo run --release -p orison-cli -- \
  new --title "Thornwick" --vault fixtures/vaults/minimal

cargo run --release -p orison-cli -- \
  play thornwick --actor-model llama3.2:3b
```

You land at Stonebridge, addressing Bram Holt. Things to try, roughly in the
order that shows the engine off:

| Type this | What it exercises |
|---|---|
| `Morning. What's the toll?` | A turn: retrieval, prompt assembly, streaming |
| `And if I've no coin?` | The transcript — he should remember the first question |
| `/where` | The vault's own connections, as exits |
| `/go Thornwick Archive` | Movement, written into the transcript |
| `/who` | Who the graph says is here |
| `/talk Elara Voss` | Changing who you address, mid-scene |
| `Who keeps the archive?` | Retrieval against her biography |
| `/go Ashmere` | A refusal that says *which* kind of refusal it is |
| `/quit` | Everything was already saved; this writes the clock |

Then `play thornwick` again and ask Elara something. The transcript, her
disposition toward you, and where you were standing are all still there.

**What to watch for while you play**, because these are the things the whole
migration was about:

- **Does she stay herself?** Elara answers questions with questions when she
  suspects you have not earned the answer. Bram cannot keep a secret. If they
  both sound like the same helpful assistant, that is the failure the judge
  suite exists to score.
- **Do the pronouns hold?** This is Bug 3, the defect this project has fought
  longest. The fixtures are built as traps for it — `messy`'s Lord Anneke has
  no gender field, a masculine body, and a name with a strong feminine prior.
- **How long is the wait, honestly?** The Godot build's p50 was ~21 s per
  turn on this model. That number is the bar.

### The other two fixtures

```bash
# Deliberately hostile: inconsistent frontmatter, a YAML code sample that
# looks like frontmatter, bold numeric headings, a dangling wiki-link.
cargo run --release -p orison-cli -- \
  new --title "Saltmarsh" --vault fixtures/vaults/messy --folder-type "notes=lore"

# 200-odd notes, generated with a fixed seed.
cargo run --release -p orison-cli -- \
  new --title "Pale Reach" --vault fixtures/vaults/large
```

`messy`'s transcript character is Lord Anneke, and the interesting question to
ask him is one where you use "her" about somebody else — Mira, at the Landing.
A model that just echoes your pronouns gets caught.

---

## Playing your own vault

```bash
cargo run --release -p orison-cli -- new --title "My Campaign" \
  --vault ~/Documents/MyVault \
  --folder-type "People=character" \
  --folder-type "Places/Cities=location" \
  --folder-type "Lore=lore"
```

Three things worth knowing before you point it at a real vault.

**`--folder-type` matters more than it looks.** The key is the note's folder
path relative to the vault root, matched exactly. It beats every heuristic.
The real-vault run in Phase 4 left **48% of notes untyped** without any
mappings, and an untyped note is retrievable but claimed to be nothing in
particular — so the engine will not offer its people as characters or its
places as somewhere to stand. Read the import summary: it tells you how many
notes came out untyped, and you can re-run `import` with more mappings until
that number is what you want.

**Nothing is discarded, but a lot lands in overflow.** The same run put
**62% of sections** in the catch-all bucket rather than a typed field: still
retrievable, but not on the character card the model reads first. If a
character feels thin in play, that is where their detail went. The section
aliases are in `crates/orison-core/src/ingest/sections.rs` and are the highest
-value thing in the ingest layer to tune.

**Without `--tokenizer`, the prompt budget is a guess, and it guesses low.**
Token counts fall back to whitespace-separated words, which under-counts a
real BPE vocabulary by roughly a third. On the fixtures this cannot bite. On a
vault with long notes it can: the budget check passes, the real prompt exceeds
the window, and the model silently drops the front of the context — which is
the system prompt. That is exactly defect B-1, from the other direction. Point
`--tokenizer` at the model's `tokenizer.json` (from its Hugging Face repo) for
a real count.

---

## What still has to be measured

Four commands. They need Ollama and two models, and they are the reason this
document exists — nothing else in Phase 5 is outstanding.

### 1. Turn latency. This is the gate.

```bash
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_OLLAMA_MODEL=llama3.2:3b \
  cargo test -p orison-core --test turn_latency -- --nocapture
```

Phase 4 measured p95 at 63.2 s / 52.9 s against a 21.2 s / 20.2 s baseline —
about 2x *slower* than the engine it replaces. Two causes were found and
fixed (B-17, B-18); this run says whether that was enough.

**Read the `reused` column before the latency column.** It is the fraction of
each prompt Ollama served from its own cache rather than re-evaluating:

```
turn |      sent | evaluated |  reused |      ttft |     total
   0 |      1130 |       880 |     22% |      6 ms |     64 ms
   1 |      1184 |       326 |     72% |      5 ms |     44 ms
```

- **Reuse high and rising, p95 at or under the baseline** → the gate is met.
- **Reuse high, p95 still over** → the cache was never the problem. The
  ordering work is finished either way and the cause is somewhere else.
- **Reuse near zero after turn 0** → the prefix is still not being served from
  cache and there is a third cause neither fix reached.

Phase 4 had only time-to-first-token to read and could not tell those apart.
That is the change that matters most in this phase.

### 2. The judge suite, and its first numbers

```bash
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_TEST_ACTOR_MODEL=llama3.2:3b \
ORISON_TEST_JUDGE_MODEL=qwen2.5:7b-instruct \
  cargo test -p orison-cli --test harness -- --nocapture
```

Scores each fixture transcript 1-5 on in-character consistency, use of
vault-sourced facts, narrative progression and absence of sycophancy — played
through the CLI itself, not a replica of it.

**Treat the first run as the baseline, not as a comparison.** Phase 1 deferred
building this suite, so the Godot number that exit criterion compares against
does not exist, and the Godot build is under feature freeze so it cannot be
recorded now. And the scores gate nothing by design: they are noisy and are
for trend detection across many samples. Read the sentence printed beside each
score before the score.

### 3. D-4, against the model class it is actually about

```bash
ORISON_TEST_OLLAMA_URL=http://127.0.0.1:11434 \
ORISON_EXPERIMENT_DIRECTOR_MODEL=<a true 8B> \
ORISON_EXPERIMENT_ACTOR_MODEL=llama3.2:3b \
ORISON_EXPERIMENT_SINGLE_MODEL=<the same 8B> \
  cargo test -p orison-core --test director_actor_experiment -- --nocapture
```

Whether to keep the Director/Actor split. The decision is recorded as **keep**
and the two pronoun flags that clouded it were read by hand and are false
positives — but the run used a 7B standing in for the 8B class, and a 7B
stand-in flatters the split, because the split's cost is paid by the larger
model and its benefit by the smaller. Re-run before treating the margin as
final. The decision itself does not depend on it; the size of the win does.

### 4. Your real vault, as a measurement

```bash
ORISON_TEST_VAULT=/path/to/vault \
  cargo test -p orison-core --test real_vault -- --nocapture
```

Prints counts and note paths, never vault content — the one inviolable
constraint applies to test output too. This is what produced the 62%-overflow
and 48%-untyped figures above, and it is the fastest way to find out which
parts of the ingest heuristics your vault disagrees with.

---

## Where things are

| | |
|---|---|
| `crates/orison-cli` | What you just ran |
| `crates/orison-core` | The engine: ingest, graph, retrieval, prompts, inference, the turn loop |
| `docs/migration_plan.md` | The plan, the phases, the defect register |
| `docs/eval_baseline.md` | Every number, and what each one is and is not evidence of |
| `fixtures/vaults/` | The three fixture vaults and their ground truth |

`cargo test --workspace` runs the whole suite with no model and no network.

---

## Known rough edges

Small, and stated so they are not surprises:

- **No `--tokenizer` means an approximate budget**, as above. It warns at
  startup, once.
- **The Director is on by default** and runs in the background after every
  couple of turns, so some turns cost a second model call. `--profile
  single-call` collapses it into one call; the two are §4.2's arms A/B and C,
  and which is better on your hardware is a measurement, not a default.
- **Re-importing a vault replaces the graph wholesale.** Your transcript,
  emotions and inventory are keyed separately and survive; where you were
  standing survives; the compiled notes do not.
- **`/save` writes only the playtime clock.** Every turn was already committed
  to SQLite as it happened. The command exists because people look for it.
