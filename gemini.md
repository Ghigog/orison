# Gemini Agent Guide

This document assists AI agents (like Antigravity) in navigating the Orison codebase, finding relevant features, and keeping development modular.

## Agent Guidelines

1. **Keep Code Modular**: Keep scripts focused on specific systems (e.g., Parser, State Manager, UI). Avoid monolithic files.
2. **Document Feature Locations**: If a feature is implemented inside a shared script rather than having its own dedicated script, add or update its exact line range in the **Feature Map** below.
3. **Keep this File Updated**: Every time you implement, refactor, or delete a feature, update the Feature Map and/or files list below.
4. **Follow the Ticket System**: Check [docs/in_progress.md](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/docs/in_progress.md) for current tasks. Move tickets between tracking files (`backlog.md` -> `in_progress.md` -> `done.md`) as status changes.

---

## Feature Map

This table maps specific functional features of Orison to their implementation scripts and line numbers.

| Feature ID | Feature Name | Target Platform | Script / File Path | Line Range / Class | Status | Description |
|---|---|---|---|---|---|---|
| *SYS001* | *Example Feature* | *All* | *res://example.gd* | *L10-L25* | *Placeholder* | *An example description of a feature* |

---

## Codebase Map

### Core Systems

*Placeholder for core engine systems structure once directories are created.*

- **Parser Engine**: Location of scripts parsing Obsidian markdown archives.
- **Story State Manager**: Handles visual novel state, choices, inventory/attributes, and variable persistence.
- **UI & Layout**: Custom controls, screen managers, scaling, and responsive design for PC, Mac, and Mobile.
- **Tabletop/DND Ruleset**: Handles dice rolling, character stats, and rules checks if defined by markdown vaults.
