# Project Backlog

This file contains the list of planned tickets for Orison. Once a ticket is ready to be worked on, move it to [in_progress.md](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/docs/in_progress.md).

---

## Backlog Tickets


### TKT002 : Implement LLM Tag-Based Emotion & Rapport System (backlog)

**User story:**
* **As a** writer / designer
* **I'd Like to** have character emotional profiles update based on structured response tags returned by the LLM
* **So that** emotional states represent narrative event memories rather than backend metrics.

**Context:**
- Replaces the Courage/Wisdom/Power telemetry model with emotional tags embedded in the LLM's conversational JSON responses.

**Description:**
- Write dialogue prompts requiring JSON responses containing dialogue text and emotion/rapport deltas. Read these updates to write logs directly to the character's JSON memory.

**Requirements:**
- Update `res://scripts/emotions/EmotionEngine.gd` to process response tags.
- Implement memory updates to `affinity` and `emotions` array within the save JSON.

**Acceptance criteria:**
* **Given** a character response contains an emotion tag `"anger"` and rapport delta `-0.1`
* **When** parsed by the engine
* **Then** the character's memory adds an anger event log, and their affinity score decreases by 0.1.

---

### TKT003 : Develop Read-Only Markdown Vault Compiler (backlog)

**User story:**
* **As an** adventure author
* **I'd Like to** import a folder of Markdown files in bulk, having them compiled into internal JSON structures without the engine ever modifying my original vault files
* **So that** my source files remain safe and untainted by the game.

**Context:**
- Orison must protect the user's Obsidian notes, acting strictly as a compiler that reads vault definitions and generates a runtime JSON save-state file.

**Description:**
- Create a Markdown folder compiler that reads frontmatter schemas for characters and scenes, storing them in memory and the JSON save engine.

**Requirements:**
- Develop `res://scripts/parser/VaultCompiler.gd`.
- Strictly enforce write-protection by never opening files in `FileAccess.WRITE` mode within the user's vault path.

**Acceptance criteria:**
* **Given** a directory of Markdown files is imported
* **When** compiled
* **Then** the engine generates a local game save file in `user://adventures/` and leaves the source vault files untouched.

---

### TKT004 : Develop JSON Knowledge Graph & Save Engine (backlog)

**User story:**
* **As a** player
* **I'd Like to** have my choices, inventory, character affinity, and dynamic knowledge graph saved in native Godot JSON files
* **So that** the game loads and saves state instantly on any platform without requiring compiled database libraries.

**Context:**
- Replacing SQLite with a native JSON serialization model to ensure cross-platform compatibility and zero build friction.

**Description:**
- Design a save/load state manager that writes dynamic game data to a structured JSON file, including node-edge tables representing character memory graphs.

**Requirements:**
- Create `res://scripts/state/SaveManager.gd`.
- Define Graph node and edge schemas inside the JSON state document.

**Acceptance criteria:**
* **Given** a game session with dynamic inventory updates and relationship changes
* **When** saving the game
* **Then** a valid JSON document is saved to `user://adventures/campaign_name.json` containing the updated values.

---

### TKT005 : Create Media Asset Handler & Stub Generator (backlog)

**User story:**
* **As a** player
* **I'd Like to** experience visual novels utilizing pre-rendered visual assets and audio loops, with on-the-fly media generation stubbed out for future phases
* **So that** game performance is smooth and lacks hardware lag.

**Context:**
- Push local generative image and TTS sound models to a future release to avoid the hardware bottleneck (VRAM Wall) of running concurrent local models.

**Description:**
- Create a media coordinator that maps character voice IDs and avatar paths to existing file directories, stubbing out API calls to Stable Diffusion/TTS for future integration.

**Requirements:**
- Implement the media player structures using Godot's `AudioStreamPlayer` and texture loaders.
- Set up a fallback pipeline for dynamically linking generator stubs.

**Acceptance criteria:**
* **Given** a scene references a character sprite and background audio
* **When** loaded
* **Then** the engine loads the assets locally from the import path and prepares interface hooks for future AI generation layers.

---

### TKT022 : Implement Interactive Onboarding Mapping Review (done)

**User story:**
* **As a** campaign author
* **I'd Like to** review and override the folder classifications and starting points of my campaign before final compilation
* **So that** my custom folder structures are correctly mapped without compilation errors.

**Description:**
* Scan directories in a selected vault for markdown contents and compile potential scene/character nodes.
* Add an interactive review panel during onboarding displaying detected folders with option dropdowns and select boxes.
* Update compiler and main viewport to receive and utilize these mapping configuration overrides.

---

### TKT023 : Invert Onboarding Mapping Review to Category-Grouped Layout (done)

**User story:**
* **As a** campaign author
* **I'd Like to** see folders grouped by category (Scenes, Characters, Locations, Lore) in the onboarding review panel
* **So that** I can easily review what folders are included in each category and adjust them naturally.

**Description:**
* Modify onboarding review UI to list categories as section headers and group folders under them.
* Re-sort folders reactively when dropdown type classifications are changed.
* Read configurations and starting point dropdowns directly from dynamic state references instead of UI hierarchy traversal.

---

### TKT024 : Dynamic Starting Point Dropdown Re-population (backlog)

**User story:**
* **As a** campaign author
* **I'd Like to** have starting scene and starting character options refresh dynamically when folders are reclassified
* **So that** folders I categorize later as scenes/characters immediately display their files in the starting selectors.

**Description:**
* Modify `VaultScanner.gd` to return a list of all parsed markdown files.
* Update `OnboardingFlow.gd` to dynamically rebuild the starting scene and starting character dropdown lists using the live categories state.

---

### TKT037 : Simplify Onboarding Setup UI (backlog)

**User story:**
* **As a** new player / DM
* **I'd Like to** skip manual folder classification review and setup-phase mind map graph steps
* **So that** I can start playing my campaign immediately with minimal setup friction.

**Description:**
* Remove the folder review list (`ScrollContainer` / `%FolderListVBox`) in the onboarding Setup UI.
- Replace it with a scan summary label detailing how many entities were found.
- Remove the full-screen `MindMapPanel` container from the onboarding scene tree.

---

### TKT038 : Update Onboarding Setup Flow Logic (backlog)

**User story:**
* **As an** adventure author
* **I'd Like to** have onboarding automatically classify and compile my notes under the hood
* **So that** the transition from vault path selection to character starting point and play is completely seamless.

**Description:**
- Simplify `OnboardingFlow.gd` logic to bypass the folder-categorization list and the mind map screen step.
- Update mapping configuration payload to use auto-detected scanner classifications.
- Direct-transition from Setup screen to Starting Point, then directly to LLM Config.

---

### TKT039 : Create In-Game Mind Map Modal (backlog)

**User story:**
* **As a** player / DM
* **I'd Like to** open the Mind Map in-game at any time to inspect characters, locations, and lore connections
* **So that** I have full visual confidence in how Orison maps the campaign world and can override classifications on the fly.

**Description:**
- Develop a custom popup/overlay modal scene (`scenes/ui/MindMapModal.tscn` / `src/ui/MindMapModal.gd`) hosting the `CampaignGraphView` component.
- Save modified node type and edge modifications back to the active `CampaignState` JSON save.
- Implement premium fade and scale animation transitions when showing/hiding the modal.
