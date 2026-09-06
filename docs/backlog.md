# Project Backlog

This file contains the list of planned tickets for Orison. Once a ticket is ready to be worked on, move it to [in_progress.md](file:///Users/dylangrowcoot/Documents/Personal Apps/orison/docs/in_progress.md).

---

## Backlog Tickets

---

## Onboarding (OBD Series)

> These tickets refine the new-campaign onboarding flow to ensure players have all required tools correctly set up before their first game session.

---

### OBD001 : Onboarding Tool Requirements & Model Pull Guide (backlog)

**User story:**
* **As a** new player setting up Orison for the first time
* **I'd Like to** see a clear, actionable setup screen that verifies all required tools are ready (Ollama running, DM model, NPC model, `nomic-embed-text`)
* **So that** I can be confident the app will work correctly before I start a campaign.

**Context:**
The LLM config screen (`LLMConfig.gd` / `LLMSettingsPanel.gd`) already runs `test_connection()` on load and displays per-model warnings inline. However, there is no blocking gate preventing progression with a broken setup, and there is no actionable guidance on *how to fix* missing models. The `nomic-embed-text` embedding model is a soft dependency — without it, RAPTOR clustering (RAG008) uses round-robin partitioning and `retrieve_context()` (RAG007) falls back to keyword-only search. The app still works, but narrative quality drops noticeably. Players should be informed and unblocked.

**Description:**
- After `test_connection()` runs on the LLM config screen, render a colour-coded checklist with 4 rows:
  1. ✅/❌ **Ollama Server** — required
  2. ✅/❌ **DM (Director) Model** — required
  3. ✅/❌ **NPC (Character) Model** — required
  4. ⭐/⚠️ **nomic-embed-text** — *Optional (Recommended)*
- For each missing item, show the exact `ollama pull <model>` command in a clearly styled copyable Label.
- For `nomic-embed-text` specifically, add a collapsible "Why does this matter?" section explaining that it enables semantic scene retrieval and hierarchical narrative summaries (RAPTOR).
- The **"Start Campaign" button** is disabled only when Ollama is unreachable or either primary model is missing. It remains enabled when only `nomic-embed-text` is absent.
- Wire `LLMSettingsPanel.settings_changed` to re-trigger validation whenever any field is edited.

**Requirements:**
- Add a `CheckList` UI component to the LLM config onboarding scene (prefer `.tscn` scene approach per project rules).
- Add a collapsible info panel for `nomic-embed-text` with a plain-English explanation.
- Disable the "Start Campaign" button if Ollama is unreachable or either primary model is absent.
- Show the exact `ollama pull <model>` shell command for any missing model.
- Re-run validation on `settings_changed` signal from `LLMSettingsPanel`.

**Acceptance criteria:**
* **Given** a player opens the LLM config with Ollama running but `nomic-embed-text` not pulled
* **When** the connection test completes
* **Then** the checklist shows ✅ Ollama, ✅ DM Model, ✅ NPC Model, ⭐ nomic-embed-text (Optional — not found), the pull command is displayed, and "Start Campaign" is enabled.
* **And given** the DM model is also missing
* **Then** "Start Campaign" is disabled and the missing model's pull command is shown.

---

## Stats, Dice & Combat (STT Series)

> These tickets implement the Power/Courage/Wisdom stat system, dice rolls, and the player character sheet. Implement in order — STT001 and STT002 are foundational. All prompt injection goes through the updated `PromptBuilder.gd` / `SystemPrompts.gd` which now use an 8192-token budget with expanded character fields (RAG002) and Level 2 RAPTOR lore context (RAG008).

---

### STT001 : Define Stats Schema & Dynamic Prompt Injection (backlog)

**User story:**
* **As a** developer
* **I'd Like to** register Power, Courage, and Wisdom stats in the character state schema and system prompts
* **So that** both the Director World Builder and Character Agents are aware of player stats and can request appropriate checks.

**Context:**
The Director model now runs a ReAct research loop (RAG009) before each narrative beat and receives Level 2 RAPTOR campaign arc summaries via `retrieve_context(..., target_level=2)` (RAG008). The `dice_roll` response key belongs in the **Director (World Builder)** JSON schema — not the character agent — since it is the Director that decides when a challenge occurs. The Director's prompt is built by `PromptBuilder.build_world_builder_prompt()`.

**Description:**
- Add `power`, `courage`, `wisdom` keys to `CampaignState.state["player_character"]` with default value `0`.
- Update `PromptBuilder.build_world_builder_prompt()` to append Power/Courage/Wisdom to the Player Character Profile section.
- Update `SystemPrompts.get_world_builder_prompt()` to include a `dice_roll` JSON response block with `"ability"` restricted to `"power|courage|wisdom"`.
- Ensure `SaveManager` migrates legacy saves that lack these keys.

**Requirements:**
- Add stat keys to `player_character` in `CampaignState.gd` with defaults.
- Extend `PromptBuilder.build_world_builder_prompt()` to format stats into the Player Character Profile block.
- Add `dice_roll` to the Director response schema in `SystemPrompts.get_world_builder_prompt()`.
- Add `SaveManager` migration for the new keys.

**Acceptance criteria:**
* **Given** the world builder prompt is compiled with a player character having Power: +3, Courage: +2, Wisdom: +1
* **Then** the compiled Director prompt contains these scores and the schema restricts `"ability"` to `"power|courage|wisdom"`.

---

### STT002 : Extract Character Stats from Vault Frontmatter (backlog)

**User story:**
* **As a** campaign writer
* **I'd Like to** declare NPC stats directly in Markdown frontmatter
* **So that** custom NPCs have unique Power, Courage, and Wisdom ratings that inform dice challenge design.

**Context:**
`VaultCompiler.gd` now extracts character data via the Director LLM (`_extract_character_data_via_llm()`) which handles biography, personality, appearance, gender, and goals as prose. Stats are expected as structured frontmatter values, not prose — they should be read directly from the `fm` dict after LLM extraction completes, then stored on the KG node like all other properties.

**Description:**
- After LLM extraction in `VaultCompiler._process_nodes_first_pass()`, read `power`, `courage`, `wisdom` from the frontmatter dict (`fm`) case-insensitively.
- Store on `props["power"]`, `props["courage"]`, `props["wisdom"]`, defaulting to `0`.
- Expose in `_compiled_data["characters"][node_id]` so `CampaignState.get_character()` returns them.

**Requirements:**
- Read `power`, `courage`, `wisdom` from `fm` after LLM extraction, default `0`.
- Store on character node properties.
- Expose in the compatibility dict at the bottom of `compile()`.

**Acceptance criteria:**
* **Given** a character Markdown file with frontmatter `power: 2`, `courage: 3`, `wisdom: 1`
* **When** compiled by `VaultCompiler`
* **Then** `CampaignState.get_character(char_id)` returns `power: 2`, `courage: 3`, `wisdom: 1`.

---

### STT003 : Interactive Stat Allocator in Character Creator (backlog)

**User story:**
* **As a** player
* **I'd Like to** allocate points to my starting Power, Courage, and Wisdom stats during character creation
* **So that** I can customise my character before starting the campaign.

**Context:**
`CharacterCreator.gd` handles name validation, avatar picking, physical description with a 2K character limit, and a custom undo/redo stack. All new UI must use `.tscn` scenes (not code-injected nodes) per project rules, and must respect `ThemeManager.active_theme`. The "Next" button progression gate is already used for name validation — the same pattern should block progression until all 6 points are spent.

**Description:**
- Add a stat allocation panel to `CharacterCreator.tscn` with three rows (Power, Courage, Wisdom), each with a current value label and +/- buttons.
- A shared pool of 6 starting points is distributed across the three stats. Minimum per stat: 0, maximum per stat: 4.
- "Next" stays disabled until all 6 points are allocated.
- Save finalized stats to `CampaignState.state["player_character"]` as `power`, `courage`, `wisdom`.

**Requirements:**
- Add the stat panel to `CharacterCreator.tscn` using `HBoxContainer` / `VBoxContainer` controls.
- Validate that pool total == 6 before enabling "Next".
- Style with `ThemeManager.active_theme`.
- Persist stats to `player_character` in `CampaignState`.

**Acceptance criteria:**
* **Given** the character creator is open
* **When** the player distributes all 6 points across stats
* **Then** "Next" enables and the saved `player_character` has the correct values.
* **And given** points remain unspent
* **Then** "Next" remains disabled.

---

### STT004 : Player Character Sheet UI (backlog)

**User story:**
* **As a** player
* **I'd Like to** open a dedicated full-screen Player Character Sheet
* **So that** I can view my character's biography, base stats, active emotional modifiers, effective totals, and inventory.

**Context:**
The Director's ReAct loop (RAG009) can call `get_character_profile` to look up the active NPC. The player character profile is injected into both the Director prompt and the character agent prompt via `PromptBuilder`. The character sheet should reflect the same effective stats that the Director sees — base stats from `CampaignState` plus emotional modifiers from `EmotionEngine` plus item bonuses (STT008). The emotion no-op skip (RAG006) means visual updates only fire when emotions meaningfully change.

**Description:**
- Create `PlayerCharacterSheet.tscn` and `PlayerCharacterSheet.gd` as a full-screen overlay modal.
- Display: profile avatar, biography/backstory, base stats (Power/Courage/Wisdom), active emotional modifier row, effective totals, and inventory list.
- Wire a keyboard shortcut (`C` key) or a sidebar button in `MainViewport.gd` to toggle the sheet.

**Requirements:**
- Build `PlayerCharacterSheet.tscn` with `MarginContainer` / `ScrollContainer` for responsive layout.
- Stat grid: `Base + Emotional Modifier + Item Modifier = Effective`.
- Bind `C` shortcut via `MainViewport._unhandled_input()` (SYS042).
- Respect `ThemeManager.active_theme`.
- Update `GEMINI.md` Feature Map (SYS042) with the new shortcut.

**Acceptance criteria:**
* **Given** the player presses the character sheet shortcut in the main viewport
* **Then** the Character Sheet slides in showing biography, base stats, all modifier layers, effective totals, and current inventory.

---

### STT005 : Emotion-Stat Dynamic Modifier System (backlog)

**User story:**
* **As a** designer
* **I'd Like to** link the player's active Plutchik emotion to temporary stat modifiers
* **So that** emotional states dynamically affect dice roll outcomes.

**Context:**
`EmotionEngine.gd` manages NPC emotions with the no-op delta skip (RAG006). Player emotions will be set by the Director via a `player_emotional_update` field in its JSON response — the Director sees the player character profile in both its ReAct context and its main prompt, so it has full information to make this call. The effective stat calculation will be used by `DiceRollModal` (STT006) and `PlayerCharacterSheet` (STT004).

**Description:**
- Add `get_emotional_modifiers(emotion: String, intensity: float) -> Dictionary` to `EmotionEngine.gd` returning `{"power": int, "courage": int, "wisdom": int}`.
- Define a mapping for all 8 Plutchik emotions (e.g., Anger = +1 Power, -1 Wisdom; Fear = -2 Courage, +1 Wisdom; Joy = +1 Courage; etc.).
- Update `GameLoopController._on_background_director_completed()` to parse `player_emotional_update` from the Director JSON and apply it to `CampaignState.state["player_character"]`.
- Add `player_emotional_update` to the Director response schema in `SystemPrompts.get_world_builder_prompt()`.

**Requirements:**
- Implement `get_emotional_modifiers()` in `EmotionEngine.gd`.
- Parse `player_emotional_update` in `GameLoopController`.
- Add schema entry in `SystemPrompts.get_world_builder_prompt()`.

**Acceptance criteria:**
* **Given** the player's emotion is Fear at intensity 0.8
* **When** effective stats are calculated for a dice roll
* **Then** Courage receives -2 and Wisdom receives +1.

---

### STT006 : Dice Roll Resolution Engine & UI (backlog)

**User story:**
* **As a** player
* **I'd Like to** roll an interactive dice when the Director requests a challenge and see the outcome resolved using my effective stats
* **So that** risky actions are resolved with fair, visible random-chance rolling.

**Context:**
The Director now generates structured JSON responses and runs a ReAct loop before each beat. The `dice_roll` key is added to the Director response schema by STT001. The Director (not the character agent) decides when a challenge occurs. The dice roll flow halts the NPC game loop, presents the modal, then resumes by injecting an XML `<dice_roll_outcome>` block into the next Director/NPC turn.

**Flow:**
1. Director JSON includes `"dice_roll": { "ability": "courage", "dc": 12, "context": "..." }`
2. `GameLoopController` intercepts this in `_on_background_director_completed()`, halts the loop, and launches `DiceRollModal`.
3. Player rolls d20 + `get_effective_player_stats()[ability]`.
4. Outcome injected via `PromptBuilder` as `<dice_roll_outcome success="true" roll="14" dc="12">` in the next turn.

**Description:**
- Create `DiceRollModal.tscn` and `DiceRollModal.gd` with animated spinning number and result reveal.
- Add `DICE_ROLL_PENDING` to `GameLoopController.TurnState`.
- Detect `dice_roll` in Director response and launch modal.
- Update `PromptBuilder.build_prompt()` to accept and format a pending `dice_roll_outcome` block.

**Requirements:**
- `DiceRollModal.tscn` as a full-screen overlay scene using proper container controls.
- `DICE_ROLL_PENDING` state in `GameLoopController.TurnState`.
- Detection logic in `GameLoopController._on_background_director_completed()`.
- `PromptBuilder` extension for `<dice_roll_outcome>` XML injection.
- Depends on: **STT001**.

**Acceptance criteria:**
* **Given** the Director requests a Courage check of DC 12
* **When** the player rolls d20 and hits a total of 14
* **Then** "Success!" is displayed, and the game resumes by injecting the outcome into the next Ollama prompt.

---

### STT007 : AI DC Calibration & Difficulty Guardrails (backlog)

**User story:**
* **As a** developer
* **I'd Like to** calibrate Director prompts with a clear Difficulty Class scale and guardrails
* **So that** the Director generates balanced challenges and only calls for dice checks when the outcome is genuinely uncertain and narratively meaningful.

**Context:**
The Director runs a full ReAct research loop (RAG009) before each beat, consulting the knowledge graph for relevant world detail. It receives Level 2 RAPTOR campaign arc summaries (RAG008) for big-picture context. DC calibration instructions in `SystemPrompts.get_world_builder_prompt()` should reference the player character's effective stats so the Director can calibrate difficulty relative to actual capability.

**Description:**
- Add a DC scale and guardrail instruction block to `SystemPrompts.get_world_builder_prompt()`:
  - DC 5: Trivial (no roll unless dramatically interesting)
  - DC 8: Easy
  - DC 12: Moderate
  - DC 16: Hard
  - DC 20: Near-impossible
- Instruct the Director: only call for a roll when the outcome is uncertain AND failure has interesting narrative consequences.
- Instruct the Director: on failure, advance the story with a permanent complication — never repeat the same check.

**Requirements:**
- Add the DC scale and guardrail instructions to `SystemPrompts.get_world_builder_prompt()`.
- Specify that trivial actions must never trigger a `dice_roll` block.
- Specify that a failed check advances the narrative with a consequence.

**Acceptance criteria:**
* **Given** the player attempts a trivial action (e.g. reading a clearly labelled sign)
* **When** evaluated by the Director
* **Then** the Director narrates the result directly without generating a `dice_roll` block.

---

### STT008 : Equipment and Item Stat Modifiers (backlog)

**User story:**
* **As a** player
* **I'd Like to** equip items from my inventory that grant stat bonuses
* **So that** my equipment assists me in succeeding at dice challenges.

**Context:**
Character inventories are already tracked in `CampaignState` and displayed in the "Active World & Inventory State" section of the Director prompt. Item modifiers are consumed by `get_effective_player_stats()` (needed by `DiceRollModal` in STT006 and `PlayerCharacterSheet` in STT004).

**Description:**
- Add `get_effective_player_stats() -> Dictionary` to `CampaignState.gd`.
- This method reads base stats (`power`, `courage`, `wisdom`) from `player_character`, adds emotional modifiers from `EmotionEngine.get_emotional_modifiers()`, and iterates `player_character.inventory` for `power_bonus`, `courage_bonus`, `wisdom_bonus` metadata keys.
- Returns `{"power": int, "courage": int, "wisdom": int}`.

**Requirements:**
- Add `get_effective_player_stats()` to `CampaignState.gd`.
- `DiceRollModal` and `PlayerCharacterSheet` must call this method for all modifier displays.
- Depends on: **STT005** (emotional modifiers).

**Acceptance criteria:**
* **Given** a player has a "Scribe's Amulet" (wisdom_bonus: 1) in their inventory
* **When** a Wisdom check is requested
* **Then** the +1 amulet bonus is included in the effective Wisdom shown in the dice modal and character sheet.

---

### STT009 : Audio & Visual Feedback Polish for Dice Rolls (backlog)

**User story:**
* **As a** player
* **I'd Like to** experience satisfying visual and audio feedback when rolling dice
* **So that** completing challenges feels exciting and impactful.

**Context:**
`MediaManager.gd` (SYS045) already provides audio stubs (TTS, background music crossfades, voice clips). Visual tweens follow the same pattern as `CharacterVisuals.gd` emotion animations (SYS050). `DiceRollModal.gd` (STT006) must exist before this ticket can be implemented.

**Description:**
- Add screen shake tween to `DiceRollModal.gd` when the dice lands (using `Tween.tween_property()` on panel position).
- Add particle burst or scale pulse tween on Success/Failure result reveal.
- Add gold glow for natural 20 (critical success) and red pulse for natural 1 (critical failure).
- Call audio stubs in `MediaManager` for rolling click-clacks, success chime, and failure tone.

**Requirements:**
- Shake tween via `Tween.tween_property()` on panel position offset.
- Audio calls via `MediaManager` stubs.
- Distinct visual treatment for natural 20 and natural 1.
- Depends on: **STT006**.

**Acceptance criteria:**
* **Given** a player rolls a D20 in the modal
* **When** the roll completes
* **Then** the panel shakes on landing, the result animates in, and audio plays.

---

## Chat & Narrative Polish (TKT Series)

---

### TKT027 : Rich Markdown & BBCode Rendering in Chat Rows (backlog)

**User story:**
* **As a** player
* **I'd Like to** view dialogue rendered with rich Markdown typography and inline BBCode text effects
* **So that** storytelling feels immersive and character dialogue can use dynamic visual animations.

**Context:**
`ChatMessageRow.gd` currently renders plain text. `MarkdownParser.gd` (SYS031) already extracts bold, italic, callout blocks, tables, and wiki-links from vault content. `VirtualScrollContainer.gd` (SYS040) pools and recycles message nodes — switching to `RichTextLabel` requires verifying that the pool correctly measures variable-height rich text nodes.

The Character Agent prompt (`SystemPrompts.get_character_agent_prompt()`) already includes a writing style guideline block. The Director also generates narration text. Both should be allowed to use BBCode effects.

**Description:**
- Upgrade `ChatMessageRow.gd` to use `RichTextLabel` with `bbcode_enabled = true`.
- Add a `markdown_to_bbcode(text: String) -> String` helper in `MarkdownParser.gd` converting inline `**bold**`, `*italic*`, and `` `code` `` to BBCode equivalents.
- Render callout blocks and tables from `MarkdownParser` as styled child nodes within the chat row layout.
- Update `SystemPrompts.get_character_agent_prompt()` and `get_world_builder_prompt()` to document permitted inline BBCode effects (`[wave]`, `[color=...]`, `[b]`, `[i]`) in the CORE RULES section.
- Verify `VirtualScrollContainer` pool recycling handles `RichTextLabel` height measurement correctly — add a minimum height override if needed.

**Requirements:**
- Swap `Label` nodes in `ChatMessageRow.tscn` with `RichTextLabel` with `bbcode_enabled = true`.
- Add `markdown_to_bbcode()` to `MarkdownParser.gd`.
- Document permitted BBCode in both system prompts.
- Confirm `VirtualScrollContainer` height measurement works with `RichTextLabel`.

**Acceptance criteria:**
* **Given** a chat message containing `**bold**` and a `[wave]animated[/wave]` BBCode tag
* **When** rendered in the chat row
* **Then** bold text is visually styled and the wave text animates.
