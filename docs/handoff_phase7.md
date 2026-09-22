# Phase 7 Handoff — packaging, distribution, and the first five minutes

> **Read first**: [migration_plan.md](migration_plan.md) Phase 7, §2.3
> (permanently out of scope) and [Appendix D](migration_plan.md#appendix-d--decisions-and-open-questions).
> **Then read**: [handoff_phase6.md](handoff_phase6.md) — in particular
> "Where Phase 6 actually got to", because Phase 6 is not closed and Phase 7
> inherits what is open.
> **Then read**: [testing_backlog.md](testing_backlog.md) §2. It holds the one
> item in this project with a deadline rather than a trigger, and Phase 7 is
> the phase most likely to trip it.
> **Prerequisite**: Phases 0-5 complete. **Phase 6 is not.** See
> "Before Phase 7 starts" below.
> **This document is self-contained.** It assumes no knowledge of the
> conversation that produced it.

---

## Start here: this phase is the first five minutes

Every phase so far has been measured against the Godot build. Phase 7 is the
first one measured against a stranger, and the thing being measured is not the
engine. It is whether somebody who downloaded Orison gets to a conversation
with a character before they give up.

The plan says this in one line — *"First-run friction is where most local-AI
applications lose their users"* — and then lists four tasks. The line is the
phase; the tasks serve it. Which means the ordering question for Phase 7 is
not "signing, then updater, then models". It is: **what does the first five
minutes actually consist of, and what has to exist for it to work?**

Today, honestly, it consists of this: install Ollama from a different website,
open a terminal, run two `ollama pull` commands, come back. That is the
[README](../README.md)'s own instructions, and it is three context switches and
a terminal before the first word of fiction. For a product whose whole pitch is
"the model is yours and it runs on your machine", that first impression argues
the opposite: that this is a developer tool wearing an application's clothes.

Everything below is in service of removing that.

---

## Where the project is

Orison is a local-first, offline AI storytelling engine: it reads an Obsidian
vault and lets you talk to the people in it. The Godot 4.x / GDScript build
(~19,600 lines) is the reference implementation and remains under feature
freeze. A Rust core (`crates/orison-core`) has replaced it, a headless CLI
(`crates/orison-cli`) proves the loop, and a Tauri 2 shell (`apps/desktop`)
is the application.

Phase 6 built eight screens wired to real commands, bought markdown rendering
and a Cytoscape knowledge graph, and measured its way out of buying a list
virtualiser. It did not reach feature parity, and **nobody has run the
application** — every screen has been verified by `clippy`, `cargo test`,
`tsc`, `vitest` and `vite build`, in an environment with no display and no
Ollama. See [handoff_phase6.md](handoff_phase6.md); none of it is repeated
here.

---

## Four findings that change what Phase 7 has to build

These were not known when the plan's Phase 7 was written. Each one changes the
shape of the "guided model acquisition" item, and the first two change what
that item is even *for*. **Read these before scoping the phase.**

### 1. The desktop shell hardcodes `tokenizer: None`

`apps/desktop/src-tauri/src/commands.rs:47`. Real tokenisation (§2.5, defect
B-4) needs a model's `tokenizer.json`; the CLI takes one via `--tokenizer`.
The shell passes `None`, unconditionally, so **every desktop session budgets
context by counting whitespace-separated words.**

This is not hidden — `ModelsSummaryDto.tokenizer_is_approximate` carries it to
the UI, which is the honest thing to have done. But B-1 was a *critical* defect
about prompts budgeted at 2.1x the served context window, and the fix for it is
only as good as the counter. A shipped application whose token budgeting is
permanently approximate has quietly re-opened the door B-4 and B-1 closed.

**The consequence for Phase 7**: a tokenizer is not a separate thing the user
finds. It arrives with the model or it does not arrive. Whatever acquisition
flow gets built has to deliver both, together, or the flow has not finished the
job.

### 2. Dense retrieval and RAPTOR are built, tested, and not wired

- `TurnEngine::retrieve_lore` (`crates/orison-core/src/turn/engine.rs:802`)
  passes `None` for the dense half. The comment above it is explicit and
  reasonable: an embedding model is configuration and may be absent, BM25 alone
  beats the Godot baseline on two of three fixtures, and an explicit `None`
  beats an empty vector that silently contributes nothing.
- RAPTOR (`knowledge/raptor.rs`) is exported from `knowledge/mod.rs` and called
  by **nothing outside its own tests**. No ingest path builds a hierarchy.

Both are good code. Neither runs in the product.

**The consequence for Phase 7 is specific and awkward.** Backlog ticket
OBD001 ([#5](https://github.com/Ghigog/orison/issues/5)) — which the plan names
as the thing to "build properly here" — specifies a **four-row** setup
checklist: Ollama, Director model, Actor model, and `nomic-embed-text` as
"optional, recommended", with a collapsible explainer saying it "enables
semantic scene retrieval and hierarchical narrative summaries (RAPTOR)".

Build that checklist today and the fourth row is a lie. The engine would never
call the model it asked the player to download, and the explainer would
describe two features that are not connected. This repository has spent five
phases refusing to show numbers it cannot back; a download prompt for an unused
model is the same defect in a different costume.

So Phase 7 has to pick one, deliberately:

- **Wire the dense half and RAPTOR**, then ship the four-row checklist honestly.
  This is engine work inside a packaging phase, which is scope creep — but it is
  the only version where OBD001 as written is true.
- **Ship a three-row checklist** and say plainly that retrieval is lexical.
  Cheaper, honest, and loses nothing the product has today.

The second is the right default. The first is only right if somebody has
measured that the dense half improves play, and [testing_backlog.md](testing_backlog.md)
records no such measurement.

### 3. `LlamaCppBackend` has never run against real weights

`crates/orison-core/src/inference/llamacpp.rs` says so in its own header. It
compiles under `--features llama-cpp` and its conformance cases are gated
behind `ORISON_TEST_GGUF_MODEL`, because the machine that wrote it had no GGUF
file and no way to fetch one. CI does not build it: the `rust` job's comment
explains that `llama-cpp-2` builds llama.cpp from source via cmake and a C++
toolchain.

This matters because [D-3](migration_plan.md#appendix-d--decisions-and-open-questions)
says the in-process backend is *"likely the eventual default once model
acquisition is guided"* — and Phase 7 **is** guided model acquisition. The
decision D-3 deferred has arrived, and the backend it points at is the one
piece of the engine that has never been executed.

### 4. The updater contradicts §2.3, and the plan lists both

§2.3 puts permanently out of scope: *"Any telemetry or network egress beyond
the user's own configured local endpoints."* Phase 7 lists *"Tauri's built-in
updater."*

An update check is network egress to an endpoint the user did not configure. It
is not telemetry — it need carry no identifier — but the pillar as written does
not say "no telemetry", it says no egress beyond local endpoints. A shipped
binary that silently contacts a release server on launch breaks the sentence
the README sells the product on, and the Settings screen currently answers
"WHAT LEAVES THIS MACHINE" with the single word **Nothing.**

This is not a reason to drop the updater. It is a reason to decide it in the
open and write the decision down. Three defensible answers:

| Option | What it costs |
|---|---|
| No updater; releases are downloads | Users run stale builds indefinitely. Honest, and the least work. |
| Updater, **opt-in**, off by default, no identifiers, shown in Settings | A checkbox, an honest Settings line, and §2.3 amended to name the exception. |
| Updater on by default | Contradicts the pillar. Only viable if the pillar is rewritten, which is a product decision, not a packaging one. |

Whichever is chosen, `migration_plan.md` §2.3 and the Settings screen's
"Nothing." must end up agreeing with the binary. Today they agree with each
other and Phase 7 is scheduled to break that.

---

## What Phase 7 builds

The plan's four items, with what each actually involves given the above.

### 7.1 Signed builds

`apps/desktop/src-tauri/tauri.conf.json` has `bundle.targets: "all"` and an
icon set, and nothing else — no signing identity, no notarisation config, no
updater block. That is the correct state for a scaffold and an incomplete one
for a release.

- **macOS**: an Apple Developer ID (paid, annual), then notarisation through
  `notarytool`. Unsigned and un-notarised builds are not merely warned about
  on current macOS; Gatekeeper refuses them by default, and "right-click,
  Open" is not an instruction to put in a README for a storytelling app.
- **Windows**: SmartScreen reputation is the real obstacle, not the signature.
  A fresh certificate earns warnings until it accumulates reputation.
- **Linux**: AppImage and Flatpak need no signing authority, which makes Linux
  the cheapest target to ship first and a reasonable way to exercise the whole
  pipeline before paying anyone.

**Secrets are the part to plan for, not the commands.** Signing in CI means
certificates and an Apple app-specific password living in repository secrets.
Decide before building it whether release builds run in CI at all, or on one
machine by hand. For a single-maintainer project, by hand is defensible and
avoids putting a Developer ID into a GitHub Actions secret store.

### 7.2 Model acquisition — the item that carries the phase

This is OBD001 reinterpreted, and findings 1-3 above are its real inputs. The
fork:

**Path A — drive Ollama.** Detect it, offer to install it, run the pulls with
a progress bar, verify with the existing `check_model_health` command (already
built, [#29](https://github.com/Ghigog/orison/issues/29)). Keeps the current
architecture. Does not remove the Ollama dependency, does not solve "is the
server running", and gives no natural place to deliver a `tokenizer.json`
(finding 1) — Ollama holds its own and does not hand it over.

**Path B — download weights, run them in-process.** `LlamaCppBackend` takes a
GGUF path. Downloading a known file means a known checksum, which is what the
plan asked for and what Path A cannot really offer. It deletes the Ollama
dependency, deletes the entire "is the server running" failure class, and is
the only path that also serves [Phase 8](migration_plan.md#phase-8--mobile-re-entry-deferred-not-closed),
since a phone cannot spawn a sidecar.

It also rests on a backend that has never executed (finding 3), and requires
cmake and a C++ toolchain in the release build.

**The recommendation**: Path B is where this is going, and D-3 already says so
— but the first task is not the download UI. It is **running
`LlamaCppBackend` against a real GGUF on a real machine and seeing whether it
works.** That is an afternoon, it is the cheapest possible test of the
assumption the whole path rests on, and if it fails, Phase 7 is Path A and
the tokenizer problem needs a different answer. Do not design the flow before
running the backend.

### 7.3 The updater

See finding 4. **Decide the §2.3 question first**, then build whatever the
decision permits. `tauri-plugin-updater` needs a signing keypair and a static
manifest endpoint; neither is hard, and both are wasted work if the answer
turns out to be "no updater".

### 7.4 Godot-era saves — resolve D-6

[D-6](migration_plan.md#appendix-d--decisions-and-open-questions) has been Open
since it was written, with a note to decide it before Phase 3.1. Phase 3.1
shipped. The plan permits an explicit "they do not carry", and §2.1 requires
one or the other.

This is a one-question decision and it belongs to whoever has the saves: **do
any Godot-era saves exist that somebody wants to keep?** If the honest answer
is no — and for a pre-release project with one maintainer it very likely is —
write that into D-6 and close it. A one-shot JSON-to-SQLite importer is cheap
but not free, and an importer nobody runs is worse than a sentence.

---

## The deadline, which Phase 7 is most likely to trip

[testing_backlog.md](testing_backlog.md) §2: there is no Godot judge baseline,
and there never will be once the Godot build is gone. It is the only item in
the backlog with a deadline instead of a trigger.

Packaging is the phase where somebody looks at a ~19,600-line GDScript build
that no longer ships and asks why it is still in the repository. **If the Godot
build is ever scheduled for removal, capture the judge baseline first.** It
costs a few hours of model runtime. After deletion it costs a git archaeology
expedition and a working Godot 4.7 install, or it is simply gone.

Nothing in Phase 7 requires deleting the Godot build. Do not let packaging
become the reason it happens by accident.

---

## Before Phase 7 starts

**Phase 6 is not closed**, and its exit criteria are in
[migration_plan.md](migration_plan.md). Four things remain:

1. The first-run path (`WelcomeScreen`, `SetupWizard`) — there is none.
2. The character creator — never ported.
3. [D-7](migration_plan.md#appendix-d--decisions-and-open-questions), image
   generation — a decision, not work.
4. `design_philosophy.md` rewritten to the round 2 palette it never caught up
   with.

Then the playtest that [#30](https://github.com/Ghigog/orison/issues/30)
deferred.

**Item 1 is not merely a Phase 6 leftover — it is Phase 7's main screen.**
"First-run path" and "guided model acquisition" are the same surface described
from two phases. Building the welcome flow without the model flow produces a
wizard that ends by telling the player to open a terminal; building the model
flow without the welcome flow produces a setup screen nobody is routed to.
Whoever picks up either should expect to build both.

And the blunt version of the dependency: **Phase 7 makes the first five
minutes good, and nobody has had a first five minutes yet.** Packaging an
application that has never been run means signing and notarising an unknown.
The playtest is not a Phase 6 formality to clear on the way past; it is the
only source of information about the thing Phase 7 exists to improve.

---

## Proposed Phase 7 exit criteria

The plan gives Phase 7 no exit criteria. Proposed, in the form the other
phases use — each one either verifiable or explicitly amended:

- [ ] A signed, notarised macOS build, a signed Windows build, and an AppImage,
      each installed and launched on a machine that did not build it.
- [ ] A new user reaches their first conversation without opening a terminal,
      on a machine with no Ollama and no models, observed rather than argued.
- [ ] Token counting is exact in the shipped application, or the approximation
      is a recorded decision with its consequences for B-1 written down.
- [ ] The setup checklist asks for no model the engine does not use.
- [ ] D-6 resolved; D-3 resolved; D-7 resolved.
- [ ] §2.3, the Settings screen, and the shipped binary agree about what leaves
      the machine.
- [ ] The judge baseline is captured, or the Godot build is still present and
      runnable.

The second is the one that matters. The rest are how it is reached.
