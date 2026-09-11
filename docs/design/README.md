# Phase 6 design canvas

Source artboards for the Phase 6 UI. Two rounds so far; the current direction
is round 2.

## Round 2 — the paper direction (current)

One file, `Orison.dc.html`, covers all nine screens as states of a single
component (a `screen` prop switches views; there is no separate `canvas.json`
layout for this round). Built against the screen inventory in
[../handoff_phase6.md](../handoff_phase6.md) and the fixtures under
`fixtures/vaults/minimal`.

| Screen | Built from |
|---|---|
| Play / Play · small frame | `crates/orison-cli/src/shell.rs`, `../handoff_phase6.md`, `../first_run.md` |
| Interrupted | `crates/orison-cli/src/error.rs`, `../handoff_phase6.md` (B-15) |
| Elara Voss (character) | `fixtures/vaults/minimal/characters/elara_voss.md`, `../emotions.md` |
| What it knows (graph) | `fixtures/vaults/minimal/locations/*.md`, `../../README.md` |
| Campaigns | `crates/orison-cli/src/main.rs`, `../first_run.md` |
| Compile a vault | `crates/orison-cli/src/main.rs` (`print_import`), `../handoff_phase6.md` §import |
| Models | `crates/orison-cli/src/config.rs`, `../../README.md`, `../first_run.md` |
| Settings | `../../README.md` (privacy pillar), `../proposal.md` |

**This drops the round 1 palette.** `design_philosophy.md` and round 1
(below) are dark-mode-first: `#0A0712` background, glassmorphism, a Solar
Flare accent. Round 2 is a warm, light "paper on a desk" surface — Spectral
serif body text, IBM Plex Mono for anything instrumented, no blur, no glow.
A theme switcher is built into the canvas itself (Lamp / E-ink, with a
Nightwatch dark theme shown but not yet wired), rather than the five
runtime-editable color tokens `design_philosophy.md` §Visual Settings
describes. If round 2 stands, `design_philosophy.md`'s palette section needs
rewriting to match; that hasn't been done yet.

### Decisions this round makes

- **Verbosity is a setting, not a fixed choice.** Settings offers
  Instrumented (retrieval, Director state, tok/s, cache reuse),
  Narrative only ("Bram is thinking", just the prose), and Quiet (text
  only, `i` opens the panel). Round 1 hard-coded the narrative framing;
  round 2 lets the player pick, including the raw-numbers option round 1
  argued against.
- **The knowledge graph is the retrieval graph, not a world map.** "What it
  knows" draws only the edges the engine actually traverses when it
  assembles a prompt — wiki-links and folder structure — and marks
  dangling links (named, never written) as kept rather than pruned.
- **Character sheets quote the vault verbatim.** Biography, personality,
  appearance and goals are shown as the player wrote them, not summarized,
  next to a rapport scale (Nemesis −1 to Devoted +1) and a per-event log of
  what moved it and why, tied to a specific turn.
- **Vault import surfaces data-quality problems with a fix path**, not just
  a type-assignment table: frontmatter/prose gender mismatches, dangling
  links, and the percent of content that lands in overflow (retrievable,
  but not on the card the model reads first) each link to where to fix
  them. Untyped notes are called out in the accent color as needing the
  player, not silently guessed.
- **Interrupted gives three exits, not two.** Retry the turn, check the
  models, or keep what already streamed — because a turn is only committed
  to disk on completion, so a retry re-asks rather than repeating.
- **Models are named by role, not by filename.** "The Actor" (fast, speaks
  in character) and "The Director" (background, decides what the scene
  owes next) each show reachability, install size and measured throughput
  independently, plus a single endpoint field and a tokenizer warning: an
  unset tokenizer means word-counting runs about a third light and can
  silently truncate a long note out of the prompt.
- **The small-frame layout re-proportions rather than hides.** Scene plate
  keeps the top third, the sheet takes the bottom two-thirds; speaker names
  move above the line instead of into a left gutter (88px is too much
  measure to give up on a phone); the composer is a fixed 56px and the
  transcript scrolls under it.
- **Settings restates the privacy pillar in the UI**, not just in docs: "What
  leaves this machine" / "Nothing", with the one endpoint the player typed
  and the local database path, reveal-in-file-browser included.

## Round 1 — glassmorphic dark canvas (superseded)

Six artboards plus a canvas layout, drawn against the measured turn latency
in [../eval_baseline.md](../eval_baseline.md) and the tokens in
[../../design_philosophy.md](../../design_philosophy.md). Superseded by round
2 above but kept for reference; nothing here was carried into round 2.

Published canvas: https://claude.ai/code/artifact/b741854c-f3aa-4264-84a5-947638de745f

| File | What it settles |
|---|---|
| `Main.dc.html` | Gameplay viewport, running on the real clock. First token 3.2 s, stream ends 13.0 s, `TurnCompleted` 14.1 s, Director `Ready` at 19.5 s |
| `Waiting.dc.html` | The 0-3.2 s gap, the one moment in a turn with nothing to stream |
| `Director.dc.html` | One row per `DirectorState`, with the exact copy each state shows |
| `Failure.dc.html` | Every reportable `FailureKind` in the transcript, with its action. B-15 |
| `VaultImport.dc.html` | Import review built around the 48%-untyped problem (testing_backlog §5) |
| `Tokens.dc.html` | The token set, marking the part that never reached Godot |
| `canvas.json` | Layout, pages and the sticky notes on the canvas |

Decisions round 1 made:

- **The wait is filled with provenance, not motion.** Retrieval finishes by
  2.4 s, so the panel names the notes it pulled rather than spinning. The
  titles are true, already in hand, and the player's own writing.
- **The Director strip names the character, not the system.** "Bram is
  thinking about what happens next" is anticipation; "Director: composing" is
  a progress bar with extra steps. `Idle` draws nothing at all.
- **Solar Flare appears on `Ready` alone.** It is the one state that is about
  to change the scene.
- **Nothing blocks.** The composer stays typable through a turn, Esc cancels
  through `cancel_current_turn`, and the sidebar and map stay live.
- **No estimate is ever shown.** The engine does not have one, and inventing
  one is the apology the product premise cannot afford.
- **A failure never becomes a toast, and never becomes dialogue.** Toasts
  expire; the missing turn does not.

Two deliberate departures from `design_philosophy.md`: the dialogue panel is
372 px rather than the specified fixed 200 px, because a transcript, a
Director strip and a composer do not fit in 200; and the viewport is
animated rather than static, because a still frame cannot argue about time.

## Rebuilding a canvas

The published page is a seeded copy of the Claude Design canvas editor and is
**not** checked in (2 MB, and `.gitignore`d), and neither is `support.js` —
it's the editor's runtime shim, regenerated when a `.dc.html` is reseeded.
Edit the `.dc.html` files, then re-seed and republish. Anything saved in the
canvas editor is read back with the helper's `--extract` before editing, so
GUI edits are not lost.
