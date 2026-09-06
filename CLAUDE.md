# CLAUDE.md

See [AGENTS.md](AGENTS.md). It is the canonical guidance for this repository and
applies in full here; nothing is duplicated in this file so the two cannot drift.

Two things worth knowing before you start:

- The Godot build is under **feature freeze**. Bug fixes only. See
  [docs/migration_plan.md](docs/migration_plan.md).
- Known defects are catalogued in that plan's Appendix B and scheduled by phase.
  Several are deliberately left unfixed until the port. Check the register before
  "fixing" anything in the inference or retrieval layers.
