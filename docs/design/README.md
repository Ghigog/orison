# Phase 6 design canvas

Source artboards for the Phase 6 UI, drawn against the measured turn latency in
[../eval_baseline.md](../eval_baseline.md) and the tokens in
[../../design_philosophy.md](../../design_philosophy.md).

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

## Decisions these artboards make

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
372 px rather than the specified fixed 200 px, because a transcript, a Director
strip and a composer do not fit in 200; and the viewport is animated rather
than static, because a still frame cannot argue about time.

## Rebuilding the canvas

The published page is a seeded copy of the Claude Design canvas editor and is
**not** checked in (2 MB, and `.gitignore`d). Edit the `.dc.html` files, then
re-seed and republish to the same URL. Anything saved in the canvas editor is
read back with the helper's `--extract` before editing, so GUI edits are not
lost.
