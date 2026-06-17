# Completed Tickets

This file contains the archive of completed tickets for Orison.

---

## Done Tickets

### TKT056 : Support Dynamic NPC Base Emotion Deduction (done)
- Programmed `VaultCompiler.gd` and `CampaignState.gd` to leave the base emotion uninitialized if not explicitly configured in Obsidian frontmatter.
- Created `get_deduce_base_emotion_prompt` static method in `SystemPrompts.gd` to prompt the fast/character LLM to evaluate character biographies/profiles and return deduced base emotions and intensities as JSON.
- Implemented `_deduce_base_emotions_if_needed` in `MainViewport.gd` to asynchronously query the fast model during campaign loading, saving deduced emotional baselines and logging the initial emotion event log dynamically.
- Fixed a GDScript compile error in the player input parser and restored preloading to ensure clean project compilation.
- Wrote integration test cases in `TestRunnerNode.gd` verifying deduction prompt generation and fallback structures.

### TKT040 : Support Visual Novel Syntax Parsing for Player Inputs (done)
- Created `PlayerInputParser.gd` to parse player input for visual novel format cues (quotes for dialogue, asterisks or pronouns for actions/context).
- Programmed `_clean_action_remnants()` in the parser to strip dialogue punctuation residues (like commas and exclamation points) from the start and end of parsed action blocks.
- Integrated the parser in `PromptBuilder.gd` to inject raw text, parsed dialogue, parsed action/context, and syntax format metadata into character and world builder LLM prompts.
- Updated system instruction templates in `SystemPrompts.gd` with guidance rules explaining how the models should interpret and react to the structured input cues.
- Implemented `test_player_input_parser()` in `TestRunnerNode.gd` to verify visual novel syntax parse results.


### TKT055 : Implement NPC Default Emotions & Environmental Reflection (done)
- Parsed `base_emotion` and `base_intensity` from character Obsidian frontmatter files inside `VaultCompiler.gd`.
- Updated character initialization in `CampaignState.gd` to store base emotions and inject the initial emotion event log upon character creation.
- Implemented `get_emotion_reflection_prompt_for_id` static method in `SystemPrompts.gd` to prompt the character agent to reflect on environmental narrative beats.
- Programmed `_trigger_emotion_reflection` in `MainViewport.gd` using an asynchronous, non-blocking custom LLM request that updates the NPC's emotional state upon narration completion.
- Connected the emotion reflection trigger to all 4 narrative display points in `MainViewport.gd` (custom beginning narration, generated beginning narration, consumed director scene beats, and fallback narration).
- Updated the Orison Engine Test Suite to verify default emotion parsing and emotional reflection prompt generation.

### TKT054 : Fix Gameplay Loop Stalling and Delayed Narration (done)
- Implemented `_consume_pending_scene()` in `MainViewport.gd` to handle narrator display, inventory adjustments, plot updates, and memory modifications.
- Refactored `send_player_input()` to consume pending scenes cleanly via `_consume_pending_scene()`.
- Updated `_on_ai_response_received()` to call `_consume_pending_scene()` right after the NPC finishes speaking, ensuring immediate narration updates.
- Refactored the Director trigger condition in `_on_ai_response_received()` to bypass the cooldown check when an NPC explicitly requests an escalation.
- Updated `_on_background_director_completed()` to consume the pending scene immediately if the game loop is currently in the Idle state.

### TKT053 : Implement Director/Actor Split & LLM Performance Optimizations (done)
- Configured dynamic context windows (num_ctx: 4096/8192) in LLMClient.gd.
- Added prompt caching keep_alive (-1) for both models.
- Developed an asynchronous HTTPClient-based streaming parser in LLMClient.gd.
- Trimmed injected history limits in PromptBuilder.gd and added a "Director Busy" instruction to stall the player conversation.
- Initialized state tracking variables (pending_scene, turns_since_last_director, director_cooldown) in CampaignState.gd.
- Updated SystemPrompts.gd character schemas to request and include an escalation_signal.
- Refactored MainViewport.gd to stream character dialogues, run the DM model in parallel in the background, and consume pending scenes on transitions.

### TKT052 : Update Project Documentation & Registry (done)
- Synchronized Feature Map row SYS008 and updated codebase map.
- Updated docs tracking files backlog.md, in_progress.md, and done.md.

### TKT051 : Integrate new hook generator prompt building and display (done)
- Integrated the cluster builder into `_generate_adventure_hooks()`.
- Updated prompt execution to pass the pre-connected clusters to the prompt template.
- Connected the LLM response parsed JSON validator to map location and character properties back to the cluster's pre-connected nodes if LLM hallucinated invalid IDs.

### TKT050 : Implement knowledge-graph traversal and cluster selection (done)
- Developed `_build_connected_starting_clusters()` in `OnboardingFlow.gd` to traverse the compiled vault graph.
- Programmed location adjacency checking to scan characters and scenes/lore linked to each location node.
- Ranked starting locations by connection density and resolved connected characters and lore elements dynamically, using shuffled duplicates to vary starting points.
- Refactored `_generate_fallback_starters()` to consume the same pre-built clusters, ensuring fallback cards always utilize the correct, connected location and character pairings.

### TKT049 : Update get_starters_generation_prompt in SystemPrompts.gd (done)
- Updated static helper `get_starters_generation_prompt()` to consume pre-built starting clusters.
- Configured prompt formatting to present the pre-connected Location, Character, and Lore/Scene elements clearly for each cluster option.

### TKT048 : Update Project Documentation & Registry (done)
- Synchronized Feature Map and codebase mapping inside GEMINI.md.
- Updated docs tracking files backlog.md, in_progress.md, and done.md.

### TKT047 : Integrate pre-generated narration in MainViewport.gd (done)
- Programmed `MainViewport.gd` campaign launch sequence to check if a pre-generated intro narration is supplied.
- If present, it bypasses the launch-phase LLM call and displays the narration instantly.

### TKT046 : Implement Hook Generation, loading states, and fallback logic (done)
- Programmed `OnboardingFlow.gd` to compile the vault and execute the LLM hook generator request upon transitioning to the hook picker screen.
- Implemented a robust fallback starting hook generator if Ollama connection fails or JSON is malformed.
- Handled visual selection highlights on hook cards and mapped selected location/character properties to the campaign launch state.

### TKT045 : Create Hook Cards UI layout and Loading Overlay in OnboardingFlow.tscn (done)
- Bound `%StarterCard1/2/3`, `%LoadingOverlay`, and `%LoadingText` nodes in `OnboardingFlow.gd` ready configurations.
- Integrated a frosted twilight slate panel overlay with custom loading description text.

### TKT044 : Restructure Onboarding Flow screens sequence (done)
- Overhauled transitions flow to execute Welcome -> Setup -> LLM Config -> Hook Picker (ReviewPanel) -> Play.

### TKT043 : Implement auto-connection test on entering LLM Config screen (done)
- Programmed automatic LLM server connection testing upon entering the LLM Config screen to remove manual button clicking.

### TKT042 : Add send_custom_request in LLMClient and hook prompt in SystemPrompts (done)
- Created async HTTPRequest method `send_custom_request` inside `LLMClient.gd` to isolate onboarding LLM requests.
- Added starting hooks generation prompt builder `get_starters_generation_prompt` in `SystemPrompts.gd` returning structured JSON requirements.

### TKT041 : Update Project Documentation & Registry (done)
- Synchronized GEMINI.md feature items and codebase map with the simplified onboarding and in-game Mind Map modal features.
- Updated docs tracking files backlog.md, in_progress.md, and done.md.

### TKT040 : Integrate Mind Map Button to Gameplay Sidebar (done)
- Replaced sidebar control ButtonsHBox with a responsive 2x2 GridContainer inside MainViewport.tscn.
- Bound and connected the new Mind Map button (%MindMapButton) inside MainViewport.gd to show the MindMapModal on click.
- Configured nearby characters sidebar refresh on modal close.

### TKT039 : Create In-Game Mind Map Modal (done)
- Developed MindMapModal.tscn containing a centered PanelContainer overlay hosting the CampaignGraphView component and Close button.
- Programmed MindMapModal.gd to load current campaign graph structure from CampaignState, hide setup-phase starting parameter dropdowns, save modified node types and edge connections back to CampaignState when closed, and perform scale/fade entrance and exit transitions.

### TKT038 : Update Onboarding Setup Flow Logic (done)
- Simplified transition logic in OnboardingFlow.gd to bypass folder list editing and setup-phase mind map graph steps.
- Programmed custom mappings payload to automatically use scanned scanner classifications without manual user review.
- Updated scan counts evaluation to populate the new setup summary display.

### TKT037 : Simplify Onboarding Setup UI (ReviewPanel & Scan Summary) (done)
- Modified OnboardingFlow.tscn to rename ReviewPanel title to 'Choose Starting Point' and remove the folder list ScrollContainer.
- Added ScanSummaryLabel to display the count of scanned entities in the campaign.
- Removed the setup-phase MindMapPanel container node from the onboarding flow scene tree.

### TKT036 : Implement Interactive Campaign Setup Mind Map / Node Graph (done)
- Created `CampaignGraphView.gd` and `CampaignGraphView.tscn` using Godot's built-in `GraphEdit`/`GraphNode` controls.
- Integrated a Category-Clustered Radial layout to position scenes (coral), characters (pink), locations (orange), and lore (purple) nodes in distinct clusters to prevent overlapping.
- Built a right-sidebar InspectorPanel allowing users to select nodes, override their classification types, preview markdown summaries, and manage edge connections.
- Programmed dynamic port rendering matching category colors, allowing users to connect/disconnect nodes by dragging lines in the viewport or using the Inspector menu.
- Integrated the Mind Map panel as a new step in the onboarding flow (`OnboardingFlow.gd` & `OnboardingFlow.tscn`), positioned between the folder classification review and the LLM config panel.
- Refactored compiler timing to run `VaultCompiler.compile_vault` before the mind map step, enabling graph visualization of actual nodes and edges.
- Passed pre-compiled campaign data back to `MainViewport.gd` through the `custom_mappings` parameter (`"compiled_data"` key), avoiding redundant compiling on campaign launch.
- Improved auto-categorization folder heuristics in `VaultScanner.gd` and `VaultCompiler.gd`: ignored files without explicit type keys in frontmatter during folder voting to prevent skewed votes, prioritized folder name keywords (e.g. "environment", "world") over neutral file counts, and utilized singular versions of keywords (e.g. "location", "character", "scene") to match both singular and plural folder structures.

### TKT035 : Robust Campaign Import Folder heuristics, character file search filtering, and fallback synthesis (done)
- Added folder name classification heuristics when importing folders not explicitly mapped (supporting locations, environments, worlds, envs, maps, characters, npcs, entities).
- Implemented deepest-folder-first checks so nested locations (such as `"entities/environments"`) are prioritized over parent classifications (like `"entities"`).
- Updated character dialogue style snippet source file filter to skip files whose basename contains the character ID or name.
- Programmed automatic fallbacks to dynamically synthesize missing character, location, and scene/lore starter data during campaign initialization in `MainViewport.gd`, resolving the issue where campaigns without locations failed to generate an introduction.
- Resolved double logging of the campaign import success message.

### TKT034 : Remove Rounded Corners and Inherit Active Dialog Theme (done)
- Removed all `corner_radius_*` properties from StyleBoxFlat resources in the global stylesheet `resources/themes/orison_ui.tres`.
- Removed dynamic `corner_radius_*` configurations from popup panels, dialog panels, window panels, tree panels, and window borders in `ThemeManager.gd`.
- Updated dynamically instantiated Dialog nodes in `OnboardingFlow.gd` (`file_dialog`, `overwrite_dialog`, `delete_confirm_dialog`) to inherit `ThemeManager.active_theme` instead of loading the raw, un-instantiated stylesheet.
- Updated `%DeleteConfirmDialog` in `SettingsModal.gd` to use `ThemeManager.active_theme` on initialization.
- Centered the label text horizontally and vertically inside all of the above confirmation dialogs by adjusting their internal Label node alignments.

### TKT033 : Fix Startup Theme Parser Comments, Path UIDs, and Campaign Start Null Mappings Crash (done)
- Removed hash `#` comments from `orison_ui.tres` to fix Godot's parser combining comment strings into the `LabelTitle` type variation name.
- Replaced UID-based reference `"uid://chq6foc7hesjg"` with direct file paths `"res://resources/assets/orisonlogo2.png"` for `boot_splash/image` and `config/icon` in `project.godot` to prevent startup UID lookup and empty file load errors.
- Refactored `start_new_campaign` in `MainViewport.gd` to safely validate that `custom_mappings` and its keys are not null before calling `.is_empty()`, preventing crash on starting campaigns with omitted dropdown settings.

### TKT032 : Delete Saved Adventures from Onboarding Load Screen (done)
- Added `delete_campaign` static method to `SaveManager.gd` to safely remove the targeted campaign's JSON save file from the `user://adventures/` directory.
- Refactored `_refresh_campaign_list()` in `OnboardingFlow.gd` to wrap load items inside an `HBoxContainer`.
- Implemented a red trash icon button next to each load button, showing a custom confirmation modal styled with the current active theme, which triggers file deletion on confirmation and refreshes the load list dynamically.

### TKT031 : Clarify Narrator Spoken Dialogue and NPC Addressing Boundaries (done)
- Tightened the NPC Boundaries rules in the World Builder narrator system instructions in `SystemPrompts.gd` to prevent the Narrator from generating spoken dialogue for NPCs (especially the active partner) or answering direct questions intended for them.
- Updated `PromptBuilder.gd` `build_world_builder_prompt` to accept an optional `active_char_id` parameter and inject the active conversation partner's name as context.
- Modified `MainViewport.gd` to pass `active_character_id` to `build_world_builder_prompt` during player inputs, ensuring the Dungeon Master narrator is aware of who the player is directly talking to.

### TKT030 : Migrate Local Overrides to Theme Type Variations (done)
- Defined 7 Theme Type Variations (`LabelTitle`, `LabelMuted`, `LabelSubtle`, `LabelSmall`, `LabelAccent`, `RichTextSmall`, `ButtonAccent`) in `orison_ui.tres`, each with a `base_type` and seed colors/font sizes.
- Added `ButtonAccent` `StyleBoxFlat` sub-resource to `orison_ui.tres` for the accent CTA normal state.
- Extended `apply_active_theme()` in `ThemeManager.gd` to write colors and font sizes directly onto each variation — no tree traversal needed.
- Removed all `theme_override_colors` and `theme_override_font_sizes` from `MainViewport.tscn`, `OnboardingFlow.tscn`, `SettingsModal.tscn`, and `CharacterListItem.tscn`, replacing with `theme_type_variation` assignments.
- Removed the 3 inline `StyleBoxFlat` sub-resources from `OnboardingFlow.tscn`; reduced `load_steps` from 5 to 2.
- Replaced `ThemeManager.apply_theme_to_hierarchy(self)` calls in `MainViewport.gd`, `OnboardingFlow.gd`, and `SettingsModal.gd` with direct `ColorRect` update or a no-op; marked `apply_theme_to_hierarchy()` as deprecated in `ThemeManager.gd`.

### TKT030 : Fix Active Character Loading and Regional Nearby Filtering (done)
- Added active character (`active_character`) tracking to the campaign metadata system inside `CampaignState.gd`.
- Saved the selected active character to metadata on every selection inside `MainViewport.gd` and saved the campaign state dynamically.
- Restored the active character on load inside `MainViewport.gd` (`load_existing_campaign`), prioritizing it over general auto-selection.
- Modified the proximity filtering algorithm in `_get_nearby_character_ids()` to also check if characters are associated with neighbors (parents/sub-regions) of the active location, filtering neighbors to location/environment/gate types only.
- Added default values for `active_location` and `active_character` in the save state template in `SaveManager.gd` and loaded fallbacks in `CampaignState.gd`.

### TKT029 : Implement Universal Font Size Scaling and Legibility Readability Upgrades (done)
- Bound and connected the `%FontSizeDropdown` UI node in `SettingsModal.gd`, supporting Small (-2px), Normal, Large (+2px), and Extra Large (+4px) settings.
- Saved and loaded the font size setting from the global config file (`user://config.json`).
- Updated `apply_theme_to_hierarchy()` in `ThemeManager.gd` to recursively scale both generic Control overrides (`font_size`) and specific RichTextLabel overrides (`normal_font_size`, `bold_font_size`, etc.).
- Cached original font sizes using node metadata (`original_font_size` / `original_normal_font_size`) to ensure consecutive adjustments remain relative to base layout values.
- Refactored `MainViewport.gd` system/warning/error message logging and narrator nameplate colors to use high-contrast, theme-aware contrast styling instead of hardcoded colors, resolving legibility issues on the Dawn theme background.
- Refactored `CharacterListItem.gd` to dynamically adjust character emotions (e.g. serenity) and neutral rapport fill colors based on theme background luminance.
- Created Test 9 `ThemeManager & Font Scaling Engine` in `TestRunnerNode.gd` to assert modifier updates and dynamic label/rich text font size scaling relative to metadata caches.

### TKT025 : Implement Dawn-Inspired Design Language and Color Scheme and Settings Theme Customizer (done)
- Refactored the core color scheme in `design_philosophy.md` to represent **the Dawn** of AI-generated stories, replacing amethyst/obsidian with twilight obsidian, aurora slate, sunrise glow, sunbeam silk, and solar flare.
- Overhauled the global stylesheet `resources/themes/orison_ui.tres` by replacing all old slate, obsidian, and amethyst color values in disabled/hover/pressed/normal button styles, line edits, surface panels, and progress bars with the new dawn colors.
- Fixed the campaign load sequence bug in `load_existing_campaign()` where character list items and character visual sprites were missing due to incorrect sequence order.
- Created `ThemeManager.gd` autoload singleton to manage presets (Dawn, Ethereal Codex, Daybreak Meadow, Solstice Obsidian) and user custom color schemes, saving/loading them from `user://config.json` and dynamically modifying the Godot Theme resource in memory.
- Created `SettingsModal.tscn` and `SettingsModal.gd` visual settings window exposing 5 ColorPickerButtons to change theme colors dynamically, a dropdown to select themes, and a LineEdit to save custom configurations.
- Integrated Settings button inside the Onboarding screen and main gameplay sidebar to open the theme visual customizer instantly.
- Modified `scenes/ui/MainViewport.tscn` to use the new Twilight Obsidian color for the underlying background (`BGColor`) and the Dawn Rose color for the narrator/default speaker name plate font override.
- Modified `scenes/ui/OnboardingFlow.tscn` to use the new card background color, button accent styleboxes, and text overrides, and set `LogoLabel` to the Solar Flare color.
- Modified `scenes/ui/CharacterListItem.tscn` label font color overrides to use the new Horizon Grey color.
- Refactored `src/ui/MainViewport.gd` log and nameplate styling methods (`_append_to_dialogue_display` and `_update_nameplate_color`) to use the new Dawn theme hex strings for players, system messages, narrators, and default NPCs.
- Updated `src/ui/CharacterListItem.gd` to use Horizon Grey for neutral relationships.
- Updated `src/ui/OnboardingFlow.gd` folder scanning category badge colors to use Solar Flare, Dawn Rose, Sunbeam Amber, and Horizon Violet.
- Successfully ran the Godot test suite via the command line to verify that all 8 unit tests pass without regressions.

### TKT028 : Implement Narrative DM Memory Integration (done)
- Programmed sequential turn coordination in `MainViewport.gd` that runs the World Builder (Narrative DM) model and the Character Agent (NPC) model on every player turn using a state machine (`TurnState`).
- Added short-term, medium-term, and long-term memory structures to `CampaignState.gd` to store and persist the campaign memory path in save files.
- Programmed memory updates parsing in the World Builder response handler, updating campaign state memory keys dynamically and updating the UI sidebar.
- Modified `SystemPrompts.gd` to instruct the World Builder on memory tracking rules and require returning `memory_updates` in the response JSON payload.
- Added `build_world_builder_prompt` to `PromptBuilder.gd` to compile environment details, world flags, inventory state, memory graph context, history, and user input for the DM.
- Modified `PromptBuilder.gd` `build_prompt` to inject the active campaign memories into the NPC prompt to ensure conversational responses are contextually grounded.
- Integrated the Adventure Memory UI section with styled labels in the sidebar layout of `MainViewport.tscn` and connected it in `MainViewport.gd` to update dynamically.

### TKT027 : Warn User on Campaign Name Overwrite (done)
- Added `overwrite_dialog` property to `OnboardingFlow.gd` class.
- Dynamically instantiated and themed a `ConfirmationDialog` using the system stylesheet `orison_ui.tres` to maintain "The Ethereal Codex" aesthetic guidelines.
- Modified `_on_craft_pressed()` to scan existing campaigns via `SaveManager.get_campaign_list()`, showing the overwrite confirmation warning popup if the campaign ID matches a previous save.
- Refactored the setup page's next steps transition into a clean `_proceed_to_next_step()` helper.
- Added a new automated unit test assertion in `TestRunnerNode.gd` validating that `SaveManager.get_campaign_list()` correctly retrieves campaigns.

### TKT026 : Implement Location-First Campaign Startup and Location-Based Sidebar (done)
- Shifted onboarding campaign startup classification from Starting Scene to Starting Location to avoid literature text dump prompt overflow.
- Updated `OnboardingFlow.gd` to populate `%StartingSceneDropdown` (now "Starting Location") with markdown files located in folders classified under `"location"`, storing `"starting_location_id"` in custom mappings.
- Refactored `MainViewport.gd` `start_new_campaign` to load `"starting_location_id"` (falling back to first location node), set active location campaign metadata, and auto-select `"starting_character_id"` if valid.
- Rewrote `_get_nearby_character_ids()` in `MainViewport.gd` to filter the "Nearby Characters" sidebar list based on direct associated edges to the active location, location mentions in the biography, or frontmatter connections.
- Updated `SystemPrompts.gd` `get_beginning_generation_prompt()` terminology to reference starting location and description context.
- Modified test suite in `TestRunnerNode.gd` to create mock locations, pass `"starting_location_id"`, and verify location mapping compiler results successfully.

### TKT025 : Implement Dynamic Location Connections, Scene-Filtered Sidebar, and Avatars (done)
- Added location connection extraction in `VaultCompiler.gd` during the second pass, processing frontmatter `tags` and note body wiki-links `[[LocationName]]` to create `"associated_with"` edges to location nodes in the knowledge graph.
- Programmed a local avatar overrides map in `VaultCompiler.gd` to associate characters like Marcello/Fik with relevant images (e.g. `nameless.jpeg`) without modifying the read-only vault folder.
- Expanded the compiler's frontmatter image search to parse `cover image` keys (clearing Obsidian bracket wrappers `[[` and `]]`) and fallback name match suffixes/substrings.
- Implemented `get_character_avatar(char_id)` in `CampaignState.gd` to modularize texture loading with automatic file extensions fallback, and refactored `CharacterVisuals.gd` to utilize it.
- Added active scene tracking (`active_scene`) to the campaign save metadata on startup and loading.
- Programmed `_get_nearby_character_ids()` in `MainViewport.gd` to filter the "Nearby Characters" sidebar list based on scene text mentions, direct scene graph connections, and shared location edges.
- Added `AvatarRect` texture rect control to `scenes/ui/CharacterListItem.tscn` and updated `CharacterListItem.gd` to load and display character avatars next to their names.

### TKT001 : Implement Responsive Full-Screen Dialogue UI (done)
- Overhauled layout in `scenes/ui/MainViewport.tscn` to implement full-screen canvas backdrop, bottom-center floating dialogue box, and collapsible right-floating sidebar.
- Created `resources/themes/orison_ui.tres` defining custom StyleBoxFlat templates (Frosted Slate surface, Frosted Glass borders) and SystemFont bindings (Outfit for UI, Lora for Dialogue text).
- Programmed collapsible sidebar tweening in `src/ui/MainViewport.gd` utilizing cubic transitions.
- Integrated a dynamic nameplate label with colors mapped to character active emotions.
- Redesigned `CharacterListItem.tscn` and `CharacterListItem.gd` to include custom horizontal progress bars representing rapport affinity with dynamic green/red/gray coloration.
- Programmed tactile, smooth motion tweens for Character Emotions (Joy, Anger, Sadness, Fear, Trust, Disgust, Surprise) in `src/ui/CharacterVisuals.gd`.


### TKT006 : Refactor UI Layout to Scene-First TSCN Node Hierarchy (done)
- Migrated dynamic layout initialization in `MainViewport.gd` into a visual hierarchy in `scenes/ui/MainViewport.tscn`.
- Fixed a parenting typo in `scenes/ui/MainViewport.tscn` where `InputHBox` was recursively parented to itself, causing `InputField` and `SendButton` to not be found at runtime.

### TKT007 : Implement Global Theme System and Typography Styles (done)
- Created centralized `resources/themes/orison_ui.tres` stylesheet asset.

### TKT008 : Decouple Game State and Networking into Autoload Singletons (done)
- Registered `CampaignState` and `LLMClient` autoloads, decoupling core game logic from UI screen lifecycles.

### TKT009 : Implement Type-Safe Data Modeling with Custom Resources (done)
- Designed type-safe resources `InventoryItem`, `EmotionEvent`, and `CharacterProfile`.

### TKT010 : Establish a Centralized Event Bus for Decoupled Signaling (done)
- Created the global `EventBus` autoload to handle decoupled state notifications.

### TKT011 : Standardize Directory Structure and project.godot Settings (done)
- Reorganized files into standard Godot directories (`src/`, `scenes/`, `assets/`, `resources/`, `tests/`) and registered entry configurations.

### TKT012 : Establish System Prompts and Local Model Architecture (done)
- Created `SystemPrompts.gd` with static prompts and integration helpers for the decoupled two-model local inference architecture (World Builder and Character Agent).
- Updated the main `README.md` with recommended local models (Llama 3.1 8B and Llama 3.2 3B) and configuration instructions using Ollama.
- Registered the prompt system in the codebase feature map (`gemini.md`) and added comprehensive verification unit tests in `TestRunnerNode.gd`.

### TKT013 : Implement In-game Onboarding Flow & Sample Adventure Creation (done)
- Created `OnboardingFlow.gd` script and `OnboardingFlow.tscn` visual scene, designing a premium glassmorphic overlay following the Design Philosophy guidelines.
- Integrated onboarding logic into `MainViewport.gd` and `MainViewport.tscn`, hiding the RPG sidebar during setup and automatically showing/enabling it when campaign launches.
- Added a sample adventure creator to generate standard mock Obsidian markdown vault directories in `user://sample_vault/` dynamically to support immediate play.
- Extended scene loading to auto-select the first imported character, print the first scene's introductory text, and restore knowledge graph states on load.

### TKT014 : Implement Onboarding LLM Config and Dynamic Beginning Generation (done)
- Added an LLM configuration step in the onboarding flow with connection verification against the local Ollama instance.
- Enhanced World Builder and Character Agent prompts with literary guidelines (sensory description, "Show, Don't Tell", varied pacing) and refactored the prompt system to leverage them.
- Enabled creative dynamic beginning generation on campaign start using the compiled campaign details and scene text, with a robust fallback to static markdown scene body on connection failure.

### TKT015 : Resolve Startup Freeze, Logging, and Text Selection Issues (done)
- Fixed a 2-second machine freeze on initialization by changing the rendering method in `project.godot` to `gl_compatibility` and removing the unused `Jolt Physics` 3D engine.
- Configured Godot's built-in file logging under `[debug]` settings in `project.godot` to log all runtime output to `user://logs/godot.log`.
- Added detailed startup and campaign initialization diagnostic logging in `MainViewport.gd`, `LLMClient.gd`, and `CampaignState.gd`.
- Enabled text selection on the main log display (`DialogueLabel` in `MainViewport.tscn`) to allow players to copy-paste the text content.
- Enriched log output with BBCode styling (e.g. system, warning, error colors/symbols) to make logs pleasing and readable on the screen.

### TKT016 : Fix Dialogue Clearing, JSON Key Processing, and Thinking Log Output (done)
- Fixed text entry bug in `MainViewport.gd` that cleared the entire scrollable dialogue display history on user text submission, keeping conversations persistent.
- Enabled automatic scroll-following on the main `DialogueLabel` so new entries automatically scroll the viewport down.
- Resolved raw JSON character output dump by parsing the `"dialogue"` JSON key (in addition to `"response"` and `"narration"` keys) in LLM responses correctly.
- Added thinking steps/reasoning to logs and UI by parsing character emotional updates and formatting them as rich text system messages in the dialogue log.
- Added full prompt and model response console/stdout print-outs in `LLMClient.gd` to enable transparent tracking of the agent thinking process.

### TKT017 : Display Character Feelings and Reasons in Sidebar (done)
- Added `EmotionLabel` and `ReasonLabel` controls in `scenes/ui/CharacterListItem.tscn` to display the active emotion and reason next to their names.
- Configured the reason text to autowrap cleanly within the sidebar layout boundaries.
- Modified `src/ui/CharacterListItem.gd` to retrieve current emotional metrics and reasons from `CampaignState`, color emotion names using design system HSL mapping, and dynamically adjust parent button heights when layout widths are resolved.
- Refactored `src/ui/MainViewport.gd` to always refresh the sidebar character list on any character's emotional updates, and redirect verbose feeling logs from the main chat viewport to standard output console to protect player narrative immersion.
- Fixed speech author preservation across load/save states by updating `CampaignState.add_history_log` to accept and persist an optional `sender` identifier, and refactoring `MainViewport.gd` to pass specific sender identities and resolve them during campaign loads.

### TKT018 : Implement Writing Style Extraction and Prompt Injection (done)
- Modified `CharacterProfile.gd` and `CampaignState.gd` to support serialization and tracking of character-specific and campaign-wide writing styles in save state JSON files.
- Programmed campaign-wide style compiler logic in `VaultCompiler.gd` supporting checking for a central `writing_style.md` file, scene note frontmatter, or extracting descriptive scene prose snippets.
- Implemented character dialogue style compiler logic in `VaultCompiler.gd` scanning character frontmatter, markdown header sections, blockquotes, and scanning scene files for character spoken dialogue.
- Integrated style snippets inside `SystemPrompts.gd` templates to automatically inject context-grounded style guidelines for the World Builder narrator and Character Agents, with built-in fallbacks.
- Updated the onboarding sample adventure generator in `OnboardingFlow.gd` with sample writing styles and scene dialogue lines to verify extraction end-to-end.
- Extended the unit test suite in `TestRunnerNode.gd` to cover save state serialization, vault compilation style extraction, and correct prompt formatting.


### TKT019 : Fix to_lower() Array Type Crash in VaultCompiler (done)
- Fixed an `Invalid call: Nonexistent function 'to_lower' in base 'Array'` error when vault files contain YAML frontmatter values (e.g. `type`, `orison_type`, or `id`) represented as arrays/lists or other non-string types.
- Introduced `_get_type_safe()` helper utility in `VaultCompiler.gd` to parse and normalize frontmatter types robustly.
- Normalized ID node parsing to safely convert any numeric or non-string frontmatter ID fields to string before lowercase formatting.


### TKT020 : Support Character Image Extraction & Dynamic Loading (done)
- Modified `VaultCompiler.gd` to recursively scan vault directories for media files (`.png`, `.jpg`, `.jpeg`).
- Implemented character image reference lookup by searching character note frontmatter keys (`image`, `avatar`, `portrait`, `sprite`, `picture`), parsing Obsidian embeds (`![[image.png]]`), parsing Markdown image syntax (`![caption](image.png)`), and falling back to matching filenames against character IDs/names.
- Integrated file copying behavior during vault compilation to copy resolved character images to `user://assets/characters/`.
- Updated `CharacterVisuals.gd` to dynamically search for `.png`, `.jpg`, and `.jpeg` file extensions in the local user assets folder before rendering the default fallback.
- Added comprehensive unit tests in `TestRunnerNode.gd` validating image asset extraction and copy mechanisms, and successfully verified them via headless test execution.


### TKT021 : Fix Empty Introduction Narration and Prompt Overflow (done)
- Expanded scene heuristics in `VaultCompiler.gd` to recognize directories like `/events/` or `/quests/` and types like `"event"` or `"quest"` as scenes, ensuring campaign event files are imported correctly instead of empty system gate files.
- Refined fallback scene selection in `VaultCompiler.gd` to skip empty notes and non-scene folders (e.g. `/gates/`, `/systems/`, `/concepts/`).
- Added prompt filtering in `MainViewport.gd` to only include characters and locations mentioned in the scene or the active conversation partner, reducing beginning generation prompts from 65k characters to a small relevant snippet.
- Configured `"num_ctx": 8192` in `LLMClient.gd` to increase Ollama's context window limit and prevent truncation of prompts.


### TKT022 : Implement Interactive Onboarding Mapping Review (done)
- Designed and implemented `VaultScanner.gd` to recursively identify markdown folders, auto-detect default classifications (Character, Location, Scene, Lore), and gather potential starting scenes and character candidates.
- Created an interactive `ReviewPanel` inside `OnboardingFlow.tscn` incorporating custom-styled scroll containers and dynamic option dropdown list selectors.
- Programmed user review UI logic in `OnboardingFlow.gd` enabling real-time dropdown updates for starting points and generating custom mapping config payloads.
- Modified `VaultCompiler.gd` to consume custom mappings, overriding default type heuristics and dynamically re-ordering parsed dictionaries to guarantee the selected starting scene and starting character load first.
- Integrated mapping overrides within `MainViewport.gd` and added comprehensive integration unit tests in `TestRunnerNode.gd`.

### TKT023 : Invert Onboarding Mapping Review to Category-Grouped Layout (done)
- Refactored `OnboardingFlow.gd` to organize folders by category (`scene`, `character`, `location`, `lore`) on the review screen.
- Added custom headers for each category, styled with distinct left accent indicator bars matching the HSL values of the design system.
- Hooked option button events to reactively re-classify folder state and repaint the category groupings in the UI.
- Decoupled `OnboardingFlow.gd` starting dropdown and compilation config logic from UI tree iteration, reading mappings directly from scanned state metadata.

### TKT024 : Dynamic Starting Point Dropdown Re-population (done)
- Modified `VaultScanner.gd` to return the complete array of scanned markdown files (`all_files` key containing parsed file ID, title, path, and body).
- Refactored `_update_starting_dropdowns()` in `OnboardingFlow.gd` to dynamically search, filter, and rebuild the list of starting scenes and starting conversation partners from the scanned files based on live folder categories.
- Fixed a bug where files inside folders re-classified by the user as scenes or characters were not selectable as starting points.


### TKT025 : Universal Scaling and High-DPI UI Legibility (done)
- Configured canvas-items based window stretch settings inside `project.godot` to enable responsive scaling across different window/desktop sizes.
- Added "Huge (+8px)" and "Gigantic (+12px)" font size modifier settings inside `SettingsModal.gd` dropdown selector.
- Updated dropdown mapping selectors to properly update the active theme and persist the larger scale values to user configuration.


### TKT026 : Dynamic Size Tweening for Menu Transitions (done)
- Bound `card_panel` and `card_vbox` variables in `OnboardingFlow.gd`.
- Refactored `_transition_to` in `OnboardingFlow.gd` to temporarily evaluate the target size of the card container using `get_combined_minimum_size()`, lock the card container's minimum size, fade out the current panel, swap visibility, and smoothly tween the card's `custom_minimum_size` to the target size while fading in the target panel.
- Added custom fade transitions in `OnboardingFlow.gd` to fade the entire `CardPanel` in or out when entering or exiting the full-screen mind map editor to prevent visual overlap.

### TKT027 : Player Character Creation (done)
- Designed and implemented a dedicated character creation panel (`CharacterPanel`) within the onboarding flow.
- Added support to `LLMClient.gd` to fetch and send profile images to Ollama via the `/api/generate` vision endpoint.
- Programmed a magic wand button that invokes the vision LLM to automatically summarize the physical features of the selected avatar image.
- Programmed the avatar selector file dialog, copying selected profile pictures to local user directory asset storage (`user://assets/characters/player.[ext]`).
- Passed the player character details to the starting adventure hooks generator (`SystemPrompts.gd`) and updated prompt instructions to weave the protagonist into the hooks.
- Injected player character context into character agent prompts and DM world builder prompts (`PromptBuilder.gd`).
- Integrated player character state initialization and excluded `"player"` from the active NPC sidebar list in `MainViewport.gd`.

