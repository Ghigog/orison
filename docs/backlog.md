# Project Backlog

> **Moved to GitHub Issues.**
>
> Open tickets now live at
> [github.com/Ghigog/orison/issues](https://github.com/Ghigog/orison/issues).
> Filing them here was merge-conflict-prone and left no way to link work to
> discussion. This file is kept only as a pointer.
>
> Migrated in Phase 0.5 of the [migration plan](migration_plan.md):
>
> | Original ID | Issue |
> |---|---|
> | OBD001 | [#5](https://github.com/Ghigog/orison/issues/5) Onboarding tool requirements and model pull guide |
> | STT001 | [#6](https://github.com/Ghigog/orison/issues/6) Define stats schema and dynamic prompt injection |
> | STT002 | [#7](https://github.com/Ghigog/orison/issues/7) Extract character stats from vault frontmatter |
> | STT003 | [#8](https://github.com/Ghigog/orison/issues/8) Interactive stat allocator in character creator |
> | STT004 | [#9](https://github.com/Ghigog/orison/issues/9) Player character sheet UI |
> | STT005 | [#10](https://github.com/Ghigog/orison/issues/10) Emotion-stat dynamic modifier system |
> | STT006 | [#11](https://github.com/Ghigog/orison/issues/11) Dice roll resolution engine and UI |
> | STT007 | [#12](https://github.com/Ghigog/orison/issues/12) AI DC calibration and difficulty guardrails |
> | STT008 | [#13](https://github.com/Ghigog/orison/issues/13) Equipment and item stat modifiers |
> | STT009 | [#14](https://github.com/Ghigog/orison/issues/14) Audio and visual feedback polish for dice rolls |
> | TKT027 | [#15](https://github.com/Ghigog/orison/issues/15) Rich Markdown and BBCode rendering in chat rows |
>
> Every migrated issue carries a migration note saying how it is affected by the
> planned move off Godot. Several are better built after the port than before it.

Filed against `apps/desktop` after [#28](https://github.com/Ghigog/orison/pull/28)
(the Tauri shell scaffold) merged, in the order they're meant to be worked —
each sized for one session:

| # | Issue |
|---|---|
| 1 | [#29](https://github.com/Ghigog/orison/issues/29) Show real model reachability on the Models/connect screen |
| 2 | [#30](https://github.com/Ghigog/orison/issues/30) First manual playtest on real hardware |
| 3 | [#31](https://github.com/Ghigog/orison/issues/31) Wire slash commands and Esc-to-cancel in Play |
| 4 | [#32](https://github.com/Ghigog/orison/issues/32) Full design token set and a working Lamp/E-ink theme toggle |
| 5 | [#33](https://github.com/Ghigog/orison/issues/33) Character screen: current rapport/emotion |
| 6 | [#34](https://github.com/Ghigog/orison/issues/34) Map screen: real knowledge-graph edges |
| 7 | [#35](https://github.com/Ghigog/orison/issues/35) Persist shell settings across restarts |
| 8 | [#36](https://github.com/Ghigog/orison/issues/36) Import screen: real folder-type UI and data-quality findings |
| 9 | [#37](https://github.com/Ghigog/orison/issues/37) Play screen: small-frame (mobile) layout |
| 10 | [#38](https://github.com/Ghigog/orison/issues/38) Accessibility pass: contrast and keyboard navigation |

Known **defects** are not tracked here or in Issues. They live in one place: the
defect register in [migration_plan.md](migration_plan.md) Appendix B, scheduled
against the phase that resolves each one.
