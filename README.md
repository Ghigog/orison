# Orison

Orison is a multiplatform (PC, Mac, Mobile) game engine built in Godot 4.x. It parses a Markdown archive (such as an Obsidian vault) containing environments, stories, characters, and other notes, and turns it into an interactive DND or Visual Novel experience that players can play through and interact with.

## Documentation References

For details on the project design, developer/agent guides, and backlog tracking, see the following key documents:

- **[Architecture](ARCHITECTURE.md)**: Conceptual layout of the parser, state manager, and game renderer.
- **[Design Philosophy & Language](design_philosophy.md)**: Unified visual language, color tokens, typography pairing, responsive grids, and motion system.
- **[Gemini Agent Guide](gemini.md)**: Guide for AI agents to locate specific features, find codebase patterns, and maintain modular development.
- **[Research Case Studies](docs/research.md)**: Deep dive into market platforms, Nomi.AI memory system, and takeaways.
- **[Technical Architecture Proposal](docs/proposal.md)**: Details on SQLite databases, desktop passthrough, and local model mitigations.
- **[Emotion System](docs/emotions.md)**: Tri-Dimensional Emotion System logic, relevance flags, and visual feedback mapping.
- **[Backlog](docs/backlog.md)**: Feature roadmap and list of planned tickets.
- **[In Progress](docs/in_progress.md)**: Active tickets currently under development.
- **[Done](docs/done.md)**: Log of completed tickets.

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
│   ├── emotions.md          # Character/narrator emotional state engine
│   ├── in_progress.md       # Tickets currently being worked on
│   ├── proposal.md          # Technical design proposal and schema
│   ├── research.md          # Market case studies and memory systems
│   └── ticket_template.md   # Standard template for creating tickets
├── resources/               # Centralized style templates & themes
│   ├── assets/              # Static media assets (fonts, UI textures, sounds)
│   └── themes/              # Custom Theme resources (.theme)
├── scenes/                  # Visual scene trees (.tscn)
│   └── ui/                  # Full-screen and component scene views
├── src/                     # Game source scripts
│   ├── autoload/            # Global singletons (EventBus, CampaignState, LLMClient, MediaManager)
│   ├── core/                # Core engines (parser, compilers, logic modules)
│   ├── resources/           # Custom typed data models (extends Resource)
│   └── ui/                  # Controllers bound to visual scenes
├── tests/                   # Integration and unit test runner scenes
├── ARCHITECTURE.md          # Project architecture design document
├── design_philosophy.md     # Unified visual language and component styling
├── gemini.md                # Agent guide and modular feature map
└── project.godot            # Godot project settings file
```

## Godot Coding & Design Guidelines

To maintain Orison's modularity, clean styling, and cross-platform compatibility, all developers and AI agents must adhere to the following **Godot Best Practices**:

1. **Scene-First UI Layouts**: 
   * *Anti-pattern:* Assembling container grids, panel margins, or form structures dynamically in code using `.new()`.
   * *Best Practice:* Design UI hierarchies visually inside `.tscn` scene files. Bind nodes to scripts using `@onready` properties and unique name references.
2. **Decoupled Architecture (Singletons)**:
   * *Anti-pattern:* Writing core logic (e.g. database CRUD operations, network requests, state histories) inside UI presentation scripts.
   * *Best Practice:* Encapsulate core state and connections in Autoload Singletons (like `CampaignState` and `LLMClient`). UI nodes should merely listen to singleton events via `EventBus` signals.
3. **Autoload Class Naming Collision**:
   * *Anti-pattern:* Defining a script class using `class_name Name` if that script is registered as an Autoload Singleton named `Name`. This generates a naming clash in the compiler.
   * *Best Practice:* Omit the `class_name` declaration in autoloaded scripts; rely on the autoload name itself.
4. **Type-Safe Resource Modeling**:
   * *Anti-pattern:* Storing NPC profiles, inventory databases, or historical events in plain, untyped `Dictionary` and `Array` objects.
   * *Best Practice:* Implement custom `Resource` scripts (e.g. `extends Resource`) with `@export` typed properties, ensuring static IDE autocompletion and compiler validation.
5. **Autoload-Dependent Testing**:
   * *Anti-pattern:* Running unit tests that reference global Autoload variables using standalone scripts (`godot -s script.gd`), which bypasses autoload scene tree injection.
   * *Best Practice:* Run unit tests by booting a test scene (`TestRunner.tscn` containing `TestRunnerNode.gd`) to ensure autoload systems are fully compiled and registered in the test tree.


## Local AI Setup & Models Guide

Orison uses a **decoupled, two-model local inference architecture** to run adventures privately and efficiently. This separation of concerns distributes tasks between a structured world orchestrator and a creative roleplay writer, preventing token bloup and VRAM bottlenecks.

### 1. Model Roles & Recommendations

To guarantee that Orison runs smoothly on consumer laptops and entry-level graphics cards, we recommend the following baseline configuration:

| Model Role | Responsibility | Baseline Model (Low-VRAM Baseline) | Alternative Model (High-VRAM Desktop) |
| :--- | :--- | :--- | :--- |
| **World Builder & DM**<br>*(Story Architect)* | Narrates settings, manages inventory, updates quest/plot flags, checks rules, and returns structured JSON outputs. | **Llama 3.1 8B** (`llama3.1`) | **Gemma 2 9B** (`gemma2`) |
| **Character Agent**<br>*(Dialogue Writer)* | Speaks in character, tracks individual emotional reactions, adjusts player affinity, and writes expressive dialogue. | **Llama 3.2 3B** (`llama3.2`) | **Hermes 3 Llama 3.1 8B** (`hermes3`) |

### 2. Setting Up Ollama

Orison connects to local model endpoints managed by [Ollama](https://ollama.com/).

1. Download and install Ollama for your operating system (macOS, Windows, Linux).
2. Open your terminal and download the recommended baseline models:
   ```bash
   # Download the World Builder (DM) model
   ollama pull llama3.1
   
   # Download the Character Agent model
   ollama pull llama3.2
   ```
3. Verify the models are correctly downloaded and cached locally:
   ```bash
   ollama list
   ```
4. Ollama will run in the background, exposing its API at:
   - Base URL: `http://localhost:11434`
   - Generate Endpoint: `http://localhost:11434/api/generate`

### 3. API Payload Integration

The engine communicates with the models using HTTP requests.
- **World Builder Payload**: Generates environmental descriptions and sets choices using `llama3.1`.
- **Character Agent Payload**: Feeds the active character profile, emotions block, and history to `llama3.2` for character replies.

---

## Local AI Image Generation (Optional)

Orison can generate AI-painted portraits, scenes, and item artwork using a locally-running Stable Diffusion backend. This is **entirely optional** — if disabled, Orison uses its built-in procedural art engine to generate instant retro-style artwork automatically.

The image generation backend is expected to expose the standard `sdapi/v1` API endpoints on `http://127.0.0.1:7860`.

### Option A: Draw Things (Mac — Recommended)

**Draw Things** is a free, native macOS app optimised for Apple Silicon. It is the easiest way to run local image generation on a Mac without any Terminal setup.

1. Install **[Draw Things: AI Generation](https://apps.apple.com/app/draw-things-ai-generation/id6444050820)** from the Mac App Store (free).
2. Open Draw Things → **Settings (⚙️)** → find **API Server** → turn it **On** and set the port to `7860`.
3. When first opened, choose **"Download a model via Draw Things"** and select:
   - **DreamShaper 8** or **Stable Diffusion 1.5** — fast, works on any Mac with 8 GB+ RAM.
   - **Stable Diffusion XL** — higher quality, requires 16 GB+ RAM or an M-series Pro/Max chip.
   - Avoid Flux.1 unless you have a Mac Pro/Studio — it is very large and slow on standard Macs.
4. Keep Draw Things running in the background.
5. In Orison's settings panel, enable **Local Image Generation** — Orison will auto-detect Draw Things on port 7860.

### Option B: AUTOMATIC1111 / WebUI Forge (Cross-platform — Advanced)

WebUI Forge and AUTOMATIC1111 are developer-oriented WebUI frontends for Stable Diffusion. They require running commands in the Terminal and installing developer tools.

1. Install dependencies (macOS): `brew install cmake protobuf rust python@3.10 git wget`
2. Clone the repository: `git clone https://github.com/lllyasviel/stable-diffusion-webui-forge`
3. Edit `webui-user.sh`: set `export COMMANDLINE_ARGS="--api"` to expose the API.
4. Run: `./webui.sh` — on first launch it downloads models and dependencies automatically.
5. Once running, the API is available at `http://127.0.0.1:7860`.

> **Tip**: Use the **Test Models** button in Orison's LLM configuration screen to verify both your Ollama connection and your local image generation backend simultaneously.

