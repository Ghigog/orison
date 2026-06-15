# Orison

Orison is a multiplatform (PC, Mac, Mobile) game engine built in Godot 4.x. It parses a Markdown archive (such as an Obsidian vault) containing environments, stories, characters, and other notes, and turns it into an interactive DND or Visual Novel experience that players can play through and interact with.

## Documentation References

For details on the project design, developer/agent guides, and backlog tracking, see the following key documents:

- **[Architecture](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/ARCHITECTURE.md)**: Conceptual layout of the parser, state manager, and game renderer.
- **[Gemini Agent Guide](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/gemini.md)**: Guide for AI agents to locate specific features, find codebase patterns, and maintain modular development.
- **[Backlog](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/docs/backlog.md)**: Feature roadmap and list of planned tickets.
- **[In Progress](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/docs/in_progress.md)**: Active tickets currently under development.
- **[Done](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/docs/done.md)**: Log of completed tickets.

## Getting Started

### Prerequisites
- [Godot Engine 4.x](https://godotengine.org/) (Standard or Mono/C# version depending on final architecture).

### Setup
1. Clone the repository.
2. Open Godot Engine.
3. Import the project by selecting the `project.godot` file in this directory.

## Project Structure

```text
├── .godot/                  # Godot metadata (ignored)
├── docs/                    # Development documentation and ticket tracking
│   ├── backlog.md           # Queue of feature tickets
│   ├── done.md              # Log of completed tickets
│   ├── in_progress.md       # Tickets currently being worked on
│   └── ticket_template.md   # Standard template for creating tickets
├── ARCHITECTURE.md          # Project architecture design document
├── gemini.md                # Agent guide and modular feature map
└── project.godot            # Godot project file
```
