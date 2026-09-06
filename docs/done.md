# Completed Tickets

This file contains the archive of completed tickets for Orison.

---

## Done Tickets

### OBD002 : Onboarding Flow & Async Vault Compilation Fixes (done)
- Fixed compilation and onboarding race conditions by resolving a GDScript 2.0 limitation where `-> void` async helper functions in `VaultCompiler.gd` returned prematurely when awaited, causing the compilation to report completion with incomplete data.
- Reset the compiler state variable `_background_compilation_done` and `_signal_emitted` flag to `false` in `_on_llm_config_completed()` to prevent stale states when restarting or retrying campaign creation.
- Prevented double emissions of the `background_setup_completed` signal by introducing a one-shot `_signal_emitted` boolean guard inside `_update_background_progress_status()`.
- Fixed a race condition/hang in `_on_character_created()` by checking `_background_compile_completed` immediately before awaiting `background_setup_completed` in case compilation completed synchronously.
- Cancelled in-flight hook generation when backing out from the review screen by adding a `_cancel_hook_generation()` helper that resets hook generator states and calls `LLMClient.cancel()`.
- Implemented a unique generation ID counter (`_hook_gen_id`) to discard outdated callbacks from prior generation runs, preventing array corruption on retry or back/next transitions.
- Added validation for empty `location_id` and `character_id` values in `_on_review_next_pressed()`, falling back to the first available nodes in the compiled Knowledge Graph.
- Removed the redundant second `compile_vault()` call on hook selection in `_on_review_next_pressed()`, performing the starting character dictionary reordering in-place on the compiled data and introducing a starting adventure loading overlay.
- Guarded `LLMClient.cancel()` on `location_changed` events so that background compilation queues are not cancelled if `location_changed` is emitted during onboarding.
- Verified that all 45/45 tests pass successfully.

### RAG009 : Agentic Director — ReAct Tool Calls Before Narrative Generation (done)
- Implemented a pre-narration research loop implementing the ReAct pattern for the Director model inside `GameLoopController.gd` (`_run_director_react_loop`), performing up to 5 iterations of reasoning and tool calling before generating narrative beats.
- Registered and exposed four core knowledge graph query tools for the loop: `search_knowledge_graph`, `get_character_profile`, `get_location_detail`, and `get_relationship`.
- Created the `ReActSignalCarrier` inner class to safely wrap asynchronous LLM requests in awaitable co-routines, using `call_deferred` to prevent race conditions during synchronous mock execution in tests.
- Implemented robust node ID normalization and case-insensitive/fuzzy matching fallback inside `_find_node_id_by_name` to map dynamic name strings from model calls to exact node IDs.
- Appended the compiled ReAct research findings log directly to the Director prompt as an `=== AGENTIC RESEARCH FINDINGS ===` block, grounding subsequent narrative generation.
- Added a new unit test `test_react_loop` inside `TestRunnerNode.gd` validating that the Director successfully performs multiple thought/action steps, executes tools, records observations, and halts upon completion.
- Verified that all 45/45 tests pass successfully.

### RAG008 : RAPTOR-Inspired Hierarchical Summary Nodes (done)
- Implemented a deterministic K-Means clustering algorithm in `VaultCompiler.gd` based on semantic embedding similarity (using `nomic-embed-text` vectors in `EmbeddingStore`). Included a robust modulo-based sequential partition fallback when embeddings are missing or disabled.
- Implemented community narrative theme detection during compilation: clustering raw nodes (Level 0) into `max(3, ...)` groups and querying the Director LLM model in JSON format to generate descriptive titles and connection summaries.
- Saved Level 1 community summaries as graph nodes of `type: "summary", level: 1`, generated their embeddings, and registered them in the vector database.
- Grouped Level 1 summary nodes and queried the Director LLM model in JSON format to synthesize Level 2 overarching campaign arcs.
- Saved Level 2 campaign arcs as graph nodes of `type: "summary", level: 2`, generated their embeddings, and registered them in the vector database.
- Modified `KnowledgeGraphManager.retrieve_context()` to accept a `target_level` parameter. Added pre-retrieval node filtering based on target level: Level 2 summaries for the Director, and Level 0 (raw) nodes for the Character Agent.
- Disabled neighbor node expansion when retrieving summary nodes (`target_level != 0`) to prevent level bleeding.
- Implemented a safe fallback in `retrieve_context()` for Level 2 queries: returning all available Level 2 summary nodes if no exact keyword or semantic match was found, ensuring the Director is never starved of campaign setting context.
- Configured `PromptBuilder.gd` to fetch Level 2 summaries (`target_level = 2`) for the Director's world builder prompt, and Level 0 nodes (`target_level = 0`) for the Character Agent prompt.
- Added mock handlers in `tests/TestRunnerNode.gd` to intercept Level 1 and Level 2 JSON-mode LLM prompts and return valid mock JSON objects.
- Wrote a comprehensive unit/integration test `test_raptor_summaries` in `tests/TestRunnerNode.gd` validating that the compiler generates at least 3 Level 1 summary nodes and 1 Level 2 summary node, and that prompt assembly retrieves only the appropriate summary levels for each model.
- Verified that all 44/44 tests pass headlessly in Godot.

### RAG007 : Semantic Embedding Retrieval for KnowledgeGraphManager (done)
- Created the new `EmbeddingStore.gd` autoload class implementing local-first memory/file vector storage, JSON serialization per campaign, and high-performance Cosine Similarity calculations for KNN querying.
- Implemented `get_embedding(text)` in `LLMClient.gd` to POST to Ollama's `/api/embeddings` endpoint with a robust fallback to `/api/embed`. Exposed a `mock_embedding_handler` hook to support offline, deterministic unit testing.
- Added `is_embedding_model_available()` in `LLMClient.gd` and hooked it into onboarding/settings validation warnings in `LLMSettingsPanel.gd` and compile-time status updates.
- Integrated asynchronous vector generation in `VaultCompiler.gd` during the final compilation phase, embedding all knowledge graph node descriptions and storing/saving them to the campaign's vector database file.
- Redesigned `retrieve_context()` in `KnowledgeGraphManager.gd` as an `async` function implementing hybrid retrieval: executing both substring keyword matching (ranked by specificity) and KNN semantic matching (top 10), then merging candidate nodes using Reciprocal Rank Fusion (RRF, k=60).
- Updated the game loop controller, prompt builder, and test runner to cleanly await the updated asynchronous prompt compilation and retrieval calls.
- Created `test_semantic_retrieval()` in `TestRunnerNode.gd` validating semantic querying, orthogonality exclusion, and keyword search fallback without regressions.
- Verified that all 43/43 tests pass headlessly in Godot.

### RAG006 : Emotion No-Op Delta Skip (done)
- Added checks in `EmotionEngine.process_response_tags()` to retrieve the character's last emotion event from the `CampaignState` data.
- Compared the newly parsed emotion and intensity against the previous event's values, skipping the `character_visual_update_requested` signal emission and logging an `[EmotionEngine] No emotional delta for {char_id} — skipping visual update` message if the emotion matches exactly and the intensity change is less than `0.05`.
- Updated `test_emotion_engine_calculations` in `tests/TestRunnerNode.gd` with robust test cases validating the no-op skip conditions, close intensity delta skips, and larger intensity delta emissions.
- Confirmed that all 42/42 tests pass headlessly.

### RAG005 : Lower Director Threshold and Inject Narrative Beat (done)
- Changed Director fallback triggering threshold from `turns_since_last_director >= 6` to `turns_since_last_director >= 2` in `GameLoopController.gd`.
- Saved the pending scene's narration to `CampaignState.last_director_beat` inside `consume_pending_scene()`.
- Modified `PromptBuilder.build_prompt()` to extract `last_director_beat`, limit/truncate it to 500 characters, clear it, and prepend it to the dialogue history section as `[Narrative Context]: {beat}`.
- Added properties persistence and compatibility checks for `last_director_beat` in `CampaignState.gd`.
- Created an integration test `test_director_threshold_and_narration_beat` in `tests/TestRunnerNode.gd` verifying all state transitions, persistence, and prompt formatting/truncation rules.
- Verified that all 42/42 tests pass headlessly in Godot.

### RAG004 : Add Gender/Pronouns Field to Knowledge Graph (done)
- Added gender fallback keys `he/him`, `she/her`, and `they/them` to `VaultCompiler.gd` fallback logic (handling both booleans and strings) if LLM extraction returns empty.
- Updated Character Agent prompt instructions in `SystemPrompts.gd` with the pronoun inference rule: *"If Gender/Pronouns is unknown, infer from the character's title (e.g., King, Queen, Prince, Lord, Lady) and biography context. Never default to a pronoun based on the character's name alone."*
- Updated creative writer reaction description prompt in `CharacterVisuals.gd` to fetch the character's gender property from CampaignState and inject it into the prompt.
- Added a robust unit test `test_gender_fallbacks_and_persistence` in `tests/TestRunnerNode.gd` covering frontmatter compilation fallbacks, system prompt injection, and campaign save/load persistence.
- Verified that all 41/41 engine tests pass successfully headlessly.

### RAG003 : Debounce Physical Reaction Generation (done)
- Added `_reaction_pending` flag to `CharacterVisuals.gd` to prevent concurrent/redundant LLM physical reaction description queries.
- Guarded `generate_physical_reaction()` in `CharacterVisuals.gd` to return early if a query is already in progress, and reset the pending flag in both success and error callbacks.
- Removed the automatic call to `generate_physical_reaction()` inside `apply_emotion()`.
- Removed the `turn_started` connection to `_hide_reaction()` in `CharacterVisuals.gd` to ensure the reaction text remains visible until the next NPC turn completes.
- Defined a new signal `character_reaction_requested(char_id, emotion)` in `GameLoopController.gd` and emitted it once at the end of `_on_npc_stream_completed()` after emotion tags are processed.
- Connected `character_reaction_requested` in `MainViewport.gd` and delegated it to the active character visuals.

### RAG002 : Expand Character Fields in PromptBuilder and SystemPrompts (done)
- Updated `PromptBuilder.get_character_agent_prompt_for_id()` to fetch new character nodes properties (`personality`, `appearance`, `gender`, `goals`) and pass them to `SystemPrompts`.
- Updated `SystemPrompts.get_character_agent_prompt()` to accept these new parameters and format them in the CHARACTER PROFILE block.
- Implemented the gender fallback rule to infer from title (King/Queen/Lord/Lady) if unknown, preventing name-only assumptions.
- Raised `context_limit` to `8192` and scaled the lore budget ratio to 15% (1228 tokens) in `PromptBuilder.build_prompt()`.
- Verified character attributes are omitted from character profile if empty string.
- Ran automated test suite and verified all 40/40 tests pass successfully.

### RAG001 : Replace VaultCompiler Section Parsing with LLM Extraction (done)
- Replaced the regex and section heading based character trait parsing logic in `VaultCompiler.gd` with an asynchronous LLM extraction loop (`_extract_character_data_via_llm`).
- Added a `dm_model` getter/setter alias in `LLMClient.gd` mapping to the Director model, and supported `json_mode` in the `send_custom_request()` queue execution and raw request parameters.
- Configured LLM prompt to query character details structured as JSON fields: `biography`, `personality`, `appearance`, `gender`, and `goals`.
- Implemented character frontmatter fallback checks for `gender`, `pronouns`, and `sex` when the LLM response is empty.
- Updated node properties storage in the knowledge graph to save the extracted attributes.
- Expanded the compiled characters dictionary structure to include compatibility keys for `personality`, `gender`, and `goals`.
- Created a robust default LLM response mockup method in `TestRunnerNode.gd` that intercepts compilation queries during unit tests and returns target attributes JSON.
- Verified that all 26/26 engine tests compile and pass successfully.

### Bugfix : Remove Border Radii / Set Sharp Corners (done)
- Set all border radii variables (`radius_sm`, `radius_md`, `radius_lg`, `radius_pill`) to `0.0` in `ThemeManager.gd` to globally enforce sharp corners on all UI controls (buttons, containers, line edits, dialogs, progress bars, window panels, etc.).
- Replaced hardcoded corner radii values in `DrawThingsTutorial.gd` with design system token references (`ThemeManager.radius_lg` and `ThemeManager.radius_md`), which resolve to `0.0`.
- Updated expected theme design tokens in `TestRunnerNode.gd` to assert that all border radius tokens are `0.0`.
- Verified that all 40/40 tests compile and pass successfully.


### Bugfix : Fix Modal Screen Overflow and Bounding Sizes (done)
- Added `ScrollContainer` wrappers around the `CenterContainer` in `CharacterDetailModal.tscn`, `SettingsModal.tscn`, `SaveAsModal.tscn`, `OnboardingFlow.tscn`, and `DrawThingsTutorial.tscn` to prevent screen overflow and ensure close/cancel buttons are always accessible.
- Modified the size flags of `CenterContainer` inside these modals to `Fill` and `Expand` for both vertical and horizontal directions, centering the modals when small and allowing vertical scrolling when exceeding screen height.
- Updated `CharacterDetailModal.gd`, `SettingsModal.gd`, `SaveAsModal.gd`, `OnboardingFlow.gd`, and `DrawThingsTutorial.gd` to use correct unique names (`%CardPanel` / `%ModalCard`) or updated paths for node references.
- Verified that all 40/40 tests compile and pass successfully.


### TKT070 : Implement Demand-Driven Manual Image Generation (done)
- Modified `ImageGenManager.gd`'s `get_image_or_fallback()` to return `null` on missing avatars and location scenery instead of triggering background AI generation automatically, and disabled automatic emotion-variant generation.
- Added explicit manual trigger methods `generate_character_portrait(char_id)` and `generate_scene_background(location_id)` in `ImageGenManager.gd` using the heavy LLM (world builder model) to extract Stable Diffusion visual prompts from character profiles and location descriptions.
- Modified `MainViewport.gd` to handle null background textures gracefully and updated the Snapshot button handler to use `ImageGenManager.generate_scene_background(active_location)`.
- Modified `CharacterDetailModal.tscn` and `CharacterDetailModal.gd` to display a "🎨 Generate" button and support an `AssetStatusOverlay` spinner when a character's portrait is missing, allowing manual AI generation of character portraits.
- Updated unit test assertions in `test_procedural_image_fallback` of `tests/TestRunnerNode.gd` to verify the manual, demand-driven portrait generation pipeline with LLM and Stable Diffusion mocking.
- Verified that all 40/40 tests compile and pass successfully.

### Bugfix : Fix Mind Map Modal Layout Overflow and Keyboard Close (done)
- Replaced the horizontal `HBoxContainer` in `CampaignGraphView.tscn`'s toolbar with an auto-wrapping `HFlowContainer` and removed the expanding spacer node to prevent the layout from stretching the window limits on smaller screen dimensions.
- Reduced the initial HSplitContainer `split_offset` from `750` to `500` to prevent forcing a massive left pane.
- Added `_unhandled_input` handler to `MindMapModal.gd` to listen for the `Escape` key to safely close and save the mind map graph.
- Verified that all components compile and integrate successfully.

### Bugfix : Fix Invalid cancel() Call on HTTPRequest in LLMClient (done)
- Fixed runtime crash when cancelling LLM requests by changing `.cancel()` to `.cancel_request()` for `HTTPRequest` instances.
- Modified `_cancel_active_low_priority_request()` and `cancel()` inside `src/autoload/LLMClient.gd` to use the correct `cancel_request()` method.
- Verified that all 40/40 tests compile and pass successfully.

### Bugfix : Fix Character Creator Avatar Preloading in Onboarding (done)
- Fixed player avatar preloading in the character creator when starting a new campaign onboarding flow.
- Modified `OnboardingFlow.gd` to delete `user://temp_pc_avatar.png` on disk when resetting character creator data.
- Refactored `CharacterCreator.gd` to clear the avatar preview and set the preview texture to `null` if no avatar path is explicitly defined or found.
- Verified that all 40/40 tests compile and pass successfully.

### SYS052 : Serialized Background Image Generation and Onboarding Guards (done)
- Blocked all campaign background and character avatar generations if `CampaignState.campaign_id` is empty (during onboarding). Allowed manual player avatar generation.
- Implemented an internal queue `_image_request_queue` and a processing state flag `_is_generating` in `ImageGenManager.gd` to serialize Stable Diffusion image generation.
- Configured queue processing to check `LLMClient.is_busy()` and run only when the LLM is idle, preventing hardware contention on Apple Silicon.
- Disabled generation of character emotion variants, loading the default portrait path in `CharacterVisuals.gd` while preserving local tween animations, floating emojis, and LLM physical reaction captions.
- Added guards to `NearbyCharacterList.gd` to return early on `refresh()` if `campaign_id` is empty, preventing premature list rendering and image generation.
- Verified that all 40/40 tests compile and pass successfully.

### SYS051 : AI Art Generation Status Indicators (done)
- Removed all procedural art generation and saving (such as vector silhouettes, initials, and gradient landscape fallbacks) for characters and scenes.
- Created `AssetStatusOverlay` scene and script which dynamically overlays asset views (scenery background, character list items, character visuals, character creator preview).
- Added `asset_generation_started`, `asset_generation_completed`, and `asset_generation_failed` signals, along with state tracking and path helper utilities, to `ImageGenManager.gd`.
- Wired the status overlays to listen to these signals, displaying a spinning loading circle during AI generation and a detailed error panel (or tiny warning exclamation `⚠` inside list items/small views) if generation fails or is disabled.
- Updated `test_procedural_image_fallback` and `test_transparent_avatars_and_emotions` test cases in `TestRunnerNode.gd` to verify new asynchronous status transitions and base fallback assertions.
- Verified that all 40/40 tests compile and pass successfully.

### UI001 : UI Audit and .tscn Refactoring (done)
- Performed a full UI audit of Orison to locate programmatically injected UI controls in GDScript.
- Created reusable scene files `CampaignListItem.tscn`, `ConnectionListItem.tscn`, `FloatingEmoji.tscn`, and `ToastMessage.tscn` with companion scripts.
- Pre-placed `ToastContainer` and `LoadingIndicator` (with internal spinner/label) inside the overlay in `MainViewport.tscn`.
- Pre-placed `OverwriteConfirmDialog` and `LoadingSpinner` inside `OnboardingFlow.tscn`.
- Pre-placed `AvatarLoadingSpinner` and `ImageFileDialog` in `CharacterCreator.tscn`.
- Pre-placed `SetupFileDialog` in `SetupWizard.tscn`.
- Removed dynamic `.new()` control instantiations in `LoadScreen.gd`, `CampaignGraphView.gd`, `CharacterVisuals.gd`, `MainViewport.gd`, `OnboardingFlow.gd`, `CharacterCreator.gd`, and `SetupWizard.gd`.
- Updated guidelines in `gemini.md` and created `.agents/AGENTS.md` to ensure future agents prefer scene files and enforce responsive layouts.
- Verified that all 40/40 tests compile and pass successfully.

### Bugfix : ImageGenClient Mocking in Unit Tests (done)
- Introduced a `mock_handler: Callable` to `ImageGenClient.gd` to intercept all image generation requests (test_connection, send_txt2img_request, send_img2img_request).
- Registered a global mock handler in `TestRunnerNode.gd` during `_ready()` that intercepts all requests and responds instantly with success and valid mock images.
- Prevented unit tests from sending real Stable Diffusion HTTP requests to the local Draw Things application on startup/test runs, resolving the issue where test queries (like "Pilferwift") were queued repeatedly in Draw Things.
- Verified that all 40/40 tests continue to pass successfully.

### Bugfix : Character Creator Avatar Loading Spinner & Cache Invalidation (done)
- Added `LoadingSpinner` programmatically to the player avatar preview area in `CharacterCreator.gd`, styling it with the active theme's accent color.
- Updated `CharacterCreator.gd` to show the spinner and clear the preview image when starting an AI generation request to avoid displaying old avatars or mockups.
- Refactored the generation callback in `CharacterCreator.gd` to only update the preview and hide the spinner when the generation process is fully complete (not a placeholder).
- Fixed a bug in `ImageGenManager.gd` where the cache for the generated asset was not invalidated at its output path (`output_path`), causing the UI to show the old/fallback texture instead of the newly refined image.
- Updated the Stable Diffusion txt2img callback in `ImageGenManager.gd` to always emit completion (`asset_generated(output_path, false)`) on generation or file-saving failure, preventing the loading spinner from hanging indefinitely.
- Verified all 40/40 tests pass successfully.

### TKT068 : Refactor Stream and Request Timeouts (done)
- Extended default stream request timeout duration by 5x (from 60 seconds to 300 seconds) in both `LLMStreamRequest.gd` and `LLMClient.gd` to accommodate slower local LLM generation.
- Scaled all other connection, warmup, request, and stream timeouts by 5x in `LLMClient.gd` (e.g. connection test to 25s, warmup to 300s, standard custom request to 1500s, stream timeout to 150s) and `LLMStreamRequest.gd` (stalled stream timeout to 900s).
- Refactored `LLMStreamRequest.gd` timeout logic in the `_process()` loop to check `_chunk_count` and bypass the elapsed time timeout if the stream is actively returning text chunks.
- Added a dedicated unit test `test_llm_stream_request_timeout_prevented_by_chunks()` in `TestRunnerNode.gd` to verify that stream timeouts are successfully prevented when chunks are received, and registered it in the test runner suite.

### VN003 : Drag-and-Drop Floating Chat Window with Sidebar Toolbar (done)
- Added configuration variables `chat_box_position_x` and `chat_box_position_y` to `LLMClient.gd` to store and load chat panel positioning coordinates.
- Extended `ResizablePanel.gd` to handle drag-to-move input states on the empty panel background, margins, and the new sidebar drag handle.
- Implemented automatic layout centering on the first run, safe clamped position restoring, and absolute window positioning coordinates.
- Restructured `MainViewport.tscn` to place a `HBoxContainer` inside the dialogue margins, organizing the layout into a main dialogue vbox (expanded on the left) and a side toolbar vbox (docked on the right).
- Moved `SnapshotButton` (📷) to the right toolbar container.
- Added `DragHandle` (✥), `ChatCharacterSheetButton` (👤), and `ChatSettingsButton` (⚙️) to the side toolbar.
- Bound the Settings and Profile buttons in `MainViewport.gd` to trigger the settings modal and character details modal respectively.
- Created unit tests verifying configuration persistence, layout conversion, and position clamping, verifying all 39 tests pass.
- Fixed border resizing by intercepting input events in `_input(event)` before child containers consume them, and corrected absolute resizing math when dragging top or left edges.
- Resolved 200+ layout warning messages in the Godot logs by programmatically clearing anchors on instantiated chat row nodes in `VirtualScrollContainer.gd`.

### VN002 : Transparent Character Portraits with Dynamic Emotion Expressions (done)
- Added dropdown selector for Campaign Art Style presets ("Digital Anime Art", "Watercolor Fantasy", "Realistic Concept Art", "Pixel Art Portrait") to the onboarding `CharacterCreator.tscn` and settings `ImageGenSettingsPanel.tscn`.
- Stored the campaign art style preset under `CampaignState.state.adventure_meta["art_style"]` for campaign save metadata, with backward-compatibility default initialization.
- Enabled transparent background rendering in `ProceduralArtEngine.gd` by setting `SubViewport.transparent_bg = true` and removing solid gradients from procedural avatar fallbacks and initials-based placeholders (rendering them as circles instead of squares).
- Extended `ImageGenManager.gd` with a Plutchik emotion prompt modifier map, style modifier map, and a corner-detection chroma keying algorithm (`make_background_transparent()`) to dynamically remove solid dark/light backgrounds from generated Stable Diffusion portraits.
- Configured `ImageGenManager.gd` to asynchronously generate missing emotion variations (e.g. `character_{char_id}_{emotion}.png`) in the background while recursively falling back to the base avatar during gameplay.
- Updated `CharacterVisuals.gd` to reload the character portrait with the specific emotion texture in `apply_emotion()`, and hooked into the `asset_generated` signal to dynamically hot-reload the texture once background generation finishes.
- Wrote automated test suite `test_transparent_avatars_and_emotions` in `TestRunnerNode.gd` and verified all 38 tests pass.

### VN001 : Full-Screen Campaign Background & Environmental Snapshot Generator (done)
- Moved `BackgroundTextureRect` out of `SplitStageHBox` to be a direct full-rect child of `Stage` at the lowest visual layer.
- Removed the obsolete `SplitStageHBox` container and reparented `CharacterVisuals` directly under `Stage` to expand and center character portraits dynamically on top of the background.
- Added a `SnapshotButton` (📷) next to the chat input in `InputHBox`.
- Wired the Snapshot button to `_on_snapshot_pressed()` in `MainViewport.gd` to manually trigger `ImageGenManager.generate_asset` scenery generation for the active location and display a "Generating background artwork..." task indicator.
- Updated `gemini.md` feature map to reflect the visual novel layout change (SYS028) and added the manual environmental snapshot generator feature (SYS049).
- Cleaned up tracking files by moving the ticket from `backlog.md` through `in_progress.md` to `done.md`.

### Bugfix : Intelligent Location Description & LLM Scenery Prompt Summarization (done)
- Increased `body_limit` in the first pass compiler (`_process_nodes_first_pass()`) from 500 to 4000 characters for `location` nodes to preserve complete document context (geography, history, structures) in the knowledge graph.
- Added frontmatter `summary` key to the fallback chain for node descriptions in `VaultCompiler.gd`.
- Integrated an asynchronous background LLM summarization pass in `ImageGenManager.gd` using the world builder model (`LLMClient.world_builder_model`) to parse the full location description and synthesize visual keywords/tags.
- Passed the visual prompt to the Stable Diffusion txt2img pipeline, avoiding narrative phrases or brainstorm lists.
- Added comprehensive unit tests in `TestRunnerNode.gd` to validate description limits, summary extraction, and the LLM prompt generator.

### TKT028 : Redesign UI Layout, Resizable Chat Panel, Physical Emotion Reactions, and Split Viewport (done)
- Restructured `MainViewport.tscn` to place the chat panel (`DialoguePanel`) directly under the overlay (`UIOverlay`) and attached `ResizablePanel.gd` to enable drag-to-resize operations.
- Persisted the chat box width and height to `client_config.json` via `LLMClient.gd` load/save configuration.
- Hid the redundant `SpeakerNameLabel` ("Narrator") at the top of the chat box.
- Restructured the `Stage` layout with `SplitStageHBox` to show the scenery background (left 60%) and character portraits/visuals (right 40%) side-by-side concurrently, removing their mutual fade-out exclusivity.
- Implemented `generate_physical_reaction` asynchronously in `CharacterVisuals.gd` using a custom LLM prompt to generate single-sentence descriptions of the character's body language/expression based on their biography and emotional reason.
- Displayed the reaction text in a sleek frosted panel caption (`ReactionPanel`) at the bottom of the character portrait container, which automatically hides when a new turn begins or a location changes.

### TKT002 : Implement LLM Tag-Based Emotion & Rapport System (done)
- Moved from backlog. Fully implemented under the `AU004` (and related) tickets by consolidating emotion logic inside `EmotionEngine.gd`, parsing structured emotion/rapport JSON tags, and updating the character affinity and emotion log arrays.

### TKT003 : Develop Read-Only Markdown Vault Compiler (done)
- Moved from backlog. Fully implemented under `AU005` by developing a read-only compilation pipeline (`VaultCompiler.gd`) and a robust `MarkdownParser.gd` with wiki-link extraction, hashtags, tables, callout blocks, and an in-memory cache to prevent redundant disk operations.

### TKT004 : Develop JSON Knowledge Graph & Save Engine (done)
- Moved from backlog. Fully implemented under `AU010` and `AU022` by creating `SaveManager.gd`, making `KnowledgeGraphManager.gd` the authoritative backend, and adding thread-safe mutex guards to CampaignState state changes.

### TKT024 : Dynamic Starting Point Dropdown Re-population (obsolete)
- Moved from backlog. Marked as obsolete/superseded because the onboarding flow was refactored in `AU001` and `AU008` to use a dynamic LLM-generated story hook card interface (`ReviewScreen.gd` / `SYS008`) instead of dropdown selections.

### TKT005 : Create Media Asset Handler & Stub Generator (done)
- Created `MediaManager.gd` autoload coordinator managing background audio crossfades, voice snippet playback, and generator hooks.
- Updated `VaultScanner.gd` to recursively scan and compile campaign audio files (`.ogg`, `.mp3`, `.wav`) under `audio_list`.
- Updated `VaultCompiler.gd` to automatically copy location background music and character voices to user asset directories (`user://assets/audio` and `user://assets/voices`), injecting properties (`bgm_path`, `voice_path`) into knowledge graph nodes.
- Registered `MediaManager` autoload in `project.godot` to load it at engine startup.
- Updated `EventBus.gd` with a `character_speaking(character_id: String)` signal to decouple speech trigger events.
- Updated `GameLoopController.gd` to emit `EventBus.character_speaking` when streaming a character response.
- Implemented fallback procedural speech blips (using dynamically generated sine waves) and image generation stubs when external APIs/custom generators are unregistered.
- Added comprehensive unit test `test_media_manager_and_copying` in `TestRunnerNode.gd` validating compiling, copying, BGM location transitions, custom generator registry overrides, and sine wave stream generation.

### AU022 : Async Safety — CampaignState Mutex & Save Metadata (done)
- Added thread-safe access guards using a `Mutex` to `CampaignState.gd` to protect state modifications during concurrent async tasks.
- Routed all mutating operations through a single unified `apply_state_change(change: Dictionary) -> Variant` handler.
- Upgraded getter methods to acquire the lock during reads to guarantee thread consistency.
- Refactored direct state writes in `GameLoopController.gd`, `MemoryManager.gd`, `MindMapModal.gd`, and `TestRunnerNode.gd` to use thread-safe getter/setter wrapper methods.
- Enriched the save schema and `SaveManager.gd` with a `"metadata"` dictionary containing `"saved_at"`, `"playtime_seconds"`, `"engine_version"`, and `"thumbnail"`.
- Implemented automatic migration of legacy save formats to the new metadata schema.
- Added playtime accumulation via `_process(delta)` in `CampaignState.gd`.
- Implemented rotating auto-saves (`autosave_1.json`, `autosave_2.json`, `autosave_3.json`) triggered every N turns (configurable via SpinBox in the LLM settings panel) or on location change.
- Upgraded `LoadScreen.gd` to format/display playtime and timestamps, and decode/render base64 thumbnails next to save slots.
- Added a comprehensive unit test `test_campaign_state_mutex_and_autosave` in `TestRunnerNode.gd` and verified all 35 tests pass.

### AU021 : Keyboard Shortcuts, Accessibility Foundations & Developer QoL (done)
- Added global keyboard shortcuts handling via `_unhandled_input` in `MainViewport.gd` for quick save (`Ctrl+S`/`Cmd+S`), save as (`Ctrl+Shift+S`), escape closing modals or toggling settings (`Escape`), focusing chat input (`/` or `Enter`), toggling mind map (`Ctrl+M`/`Cmd+M`), and navigating sidebar character focus (`Tab`/`Shift+Tab`).
- Implemented `CharacterDetailModal` as a scene-first `.tscn` modal popup showing the character portrait, name, biography/description, affinity bar/score, and emotional states, with a button to initiate a direct conversation.
- Implemented `SaveAsModal` as a scene-first `.tscn` modal popup for copying/duplicating campaigns with alphanumeric validation.
- Added programmatically recursive focus mode assignment setting `focus_mode = FOCUS_ALL` on all interactive buttons and input fields at startup.
- Configured WCAG AA contrast ratio validation checking `color_text` vs `color_bg` (limit >= 4.5:1) in `ThemeManager.gd` for preset and custom themes.
- Added descriptive accessibility tooltips for all icon/symbol buttons in the main view and onboarding screens.
- Enhanced `JsonRepair.gd` to return descriptive error messages in its fallback dictionary under the `"error"` key on failure.
- Implemented a configurable `timeout_seconds` property in `LLMStreamRequest.gd` defaulting to 60s, aborting stalled connections.
- Added a `scan_progress(current, total)` signal to `EventBus.gd` and emitted it from `VaultScanner.gd` to show real-time progress.
- Wrote four new unit tests in `TestRunnerNode.gd` verifying JsonRepair diagnostics, LLMStreamRequest timeout, VaultScanner progress, and theme contrast checks.

### AU020 : CampaignGraphView Search, Filtering & Scaling (done)
- Added a horizontal toolbar at the top of the graph view containing `SearchInput`, checkable filter buttons (`ToggleLocation`, `ToggleLore`, `ToggleScene`, `ToggleCharacter`), Zoom buttons (`ZoomInBtn`, `ZoomOutBtn`, `ZoomResetBtn`), and `NodeCountLabel`.
- Enabled the interactive minimap of `GraphEdit` to display the viewport bounding box and support fast viewport navigation.
- Implemented `_apply_filters()` in `CampaignGraphView.gd` to hide category-disabled nodes and dim non-matching search results (case-insensitive substring match).
- Connected filter toggles and search inputs to call `_apply_filters()` dynamically on change.
- Implemented a dynamic connection updating routine to hide connections pointing to invisible nodes by clearing and re-rendering visible connections only.
- Added explicit zoom buttons programmatically adjusting `graph_edit.zoom` and clamping it between `zoom_min` and `zoom_max`.
- Added a `_process()` hook to synchronize the zoom reset button label text percentage automatically (supporting mouse-wheel zooming).
- Added the `test_campaign_graph_view_filtering` unit test in `TestRunnerNode.gd` to verify rendering, filtering, count labeling, edge disconnection, and zoom behaviors.

### AU019 : Fully Implement Design Philosophy System in ThemeManager (done)
- Added spacing scale tokens (`spacing_xs`, `spacing_sm`, `spacing_md`, `spacing_lg`, `spacing_xl`) matching `design_philosophy.md` specifications to `ThemeManager.gd`.
- Added border radius tokens (`radius_sm`, `radius_md`, `radius_lg`, `radius_pill`) to `ThemeManager.gd`.
- Added success/danger color tokens (`color_success`, `color_danger`) to `ThemeManager.gd`.
- Added animation duration and easing tokens (`duration_fast`, `duration_normal`, `duration_slow`, `ease_default`, `trans_default`) to `ThemeManager.gd`.
- Initialized and dynamically updated shadow/glow presets (`shadow_subtle`, `shadow_medium`, `shadow_elevated` StyleBoxFlat templates) in `ThemeManager.gd` according to theme luminance.
- Extended the `_update_sb()` helper method in `ThemeManager.gd` to apply dynamic spacing, border radii, and shadows directly to theme StyleBoxes in `apply_active_theme()`.
- Centralized `get_emotion_color(emotion)` logic inside `ThemeManager.gd` matching the emotional glow mappings.
- Refactored `OnboardingFlow.gd` to use animation duration and easing tokens for screen transitions.
- Refactored `MainViewport.gd` to use spacing scale, border radii, and animation/transition tokens for sidebars, backgrounds, toast notifications, and loading indicators.
- Refactored `SettingsModal.gd` to use animation duration and easing tokens for card transitions.
- Refactored `CharacterListItem.gd` to use success/danger color tokens and delegate emotion colors to `ThemeManager.get_emotion_color()`.
- Expanded `test_theme_manager_and_font_scaling` unit tests in `TestRunnerNode.gd` to assert the correct presence and values of the new design tokens and shadow templates.

### AU018 : Deduplicate Settings UI Between Onboarding & In-Game (done)
- Created shared settings panel sub-scenes (`ThemeSettingsPanel.tscn`, `LLMSettingsPanel.tscn`, `ImageGenSettingsPanel.tscn`) and their respective controller scripts under `scenes/ui/settings/` and `src/ui/settings/`.
- Decoupled and extracted theme configuration logic from `SettingsModal.gd` and `SettingsModal.tscn` into `ThemeSettingsPanel`.
- Decoupled and extracted LLM and image generation configuration/connection testing/probe logic from `LLMConfig.gd` and `LLMConfig.tscn` into `LLMSettingsPanel` and `ImageGenSettingsPanel`.
- Refactored `SettingsModal` to act as a slim container shell that embeds all 3 shared settings panels as tabs, enabling players to configure and test LLM and Image Gen settings in-game for the first time.
- Refactored `LLMConfig` to embed instances of `LLMSettingsPanel` and `ImageGenSettingsPanel` inside its onboarding ScrollContainer, delegating save/test logic.
- Wired up a dynamic `show_apply_button` setter on the LLM and Image Gen panels to selectively show the "Apply" button inside the in-game SettingsModal context, while keeping them hidden within the onboarding wizard.

### AU017 : ProceduralArtEngine as Universal Image Fallback (done)
- Added `generate_avatar_placeholder(char_name, width, height)` to `ProceduralArtEngine.gd` rendering a solid color background (hashed from char name) with centered initials and outline for readability.
- Added `generate_scene_placeholder(description, width, height)` to `ProceduralArtEngine.gd` rendering a three-stop diagonal gradient with colors hashed from the description.
- Implemented centralized `get_image_or_fallback(entity_id, entity_type, size)` in `ImageGenManager.gd` that resolves real images from disk or generates and caches procedural placeholders.
- Added `_image_cache` dictionary to `ImageGenManager.gd` for texture caching with compound keys (`entity_id_entity_type_WxH`).
- Added `invalidate_cache(entity_id, entity_type)` to `ImageGenManager.gd` to clear stale cache entries when new images are generated.
- Integrated cache invalidation into `generate_asset()` at both the start and on successful Stable Diffusion completion.
- Refactored `CampaignState.get_character_avatar()` to delegate to `ImageGenManager.get_image_or_fallback()`.
- Updated `CharacterListItem.gd` to load avatars via `ImageGenManager.get_image_or_fallback()`.
- Updated `CharacterVisuals.gd` to load character portraits via `ImageGenManager.get_image_or_fallback()`.
- Updated `MainViewport.gd` to immediately display a procedural fallback background on location change, then optionally trigger AI generation only if `image_gen_enabled`.
- Updated `MainViewport._on_background_generated()` to invalidate cache and reload via centralized helper.
- Updated `CharacterCreator.gd` (onboarding) to route all three avatar loading paths (initial setup, file selection, AI generation callback) through `ImageGenManager.get_image_or_fallback()`.
- Updated `test_texture_caching` in `TestRunnerNode.gd` to use async `ImageGenManager.get_image_or_fallback()` and verify cache invalidation.
- Added new `test_procedural_image_fallback` test verifying avatar/scene fallback generation, caching, and invalidation.

### AU016 : Implement Chat History Virtualization for Long Sessions (done)
- Created `ChatMessageRow.tscn` and `ChatMessageRow.gd` to represent individual chat rows, encapsulating sender styling, emotion-color mappings, and link actions.
- Created `VirtualScrollContainer.gd` to virtualize scrolling inside the dialogue interface.
- Implemented character/line-count-based height estimation for messages combined with immediate post-render height measurement, adjusting subsequent message positions dynamically.
- Implemented a node recycling pool to reuse offscreen `ChatMessageRow` nodes, maintaining visual stability and rendering efficiency.
- Refactored `MainViewport.tscn` and `MainViewport.gd` to replace the monolithic `RichTextLabel` with the custom `VirtualScrollContainer`.
- Added a full integration test covering layout estimation, scrolling updates, node recycling, and history clearing.
- Verified that all unit tests pass successfully (28/28 tests).

### AU015 : Add Player Input Validation & Character Creation Guardrails (done)
- Implemented robust regex-based character name validation allowing only alphanumeric characters, spaces, hyphens, and apostrophes (1-50 characters).
- Designed inline error labels under the character name LineEdit to display precise validation errors.
- Created character counters for physical description, personality, and backstory TextEdit fields.
- Implemented a custom `TextHistory` inner class to manage character limits and typing history.
- Prevented further input past the 2000-character limit, turning counters red and cleanly truncating inputted or pasted text while maintaining correct cursor placement.
- Provided custom undo/redo key mappings (`Ctrl+Z` / `Cmd+Z`, `Ctrl+Y` / `Cmd+Y`, `Ctrl+Shift+Z`) routing events to the independent `TextHistory` stack for each TextEdit.
- Added comprehensive unit tests validating name formats, sanitization behavior, character limits truncation, and undo/redo stacks.
- Verified that all unit tests pass successfully.

### AU014 : Add Section Name Aliases & Fuzzy Matching to VaultCompiler (done)
- Implemented a default character property alias dictionary in `VaultCompiler.gd` mapping backstory, appearance, and personality keys to common alternative names.
- Added support for custom alias dictionary extension via `vault_config.json` at the vault root.
- Added support for file-specific alias dictionary overrides using the flat frontmatter keys (`alias_appearance`, etc.) parsing prefix.
- Implemented a character body section parser that splits the markdown file body into sections and normalizes section/alias names (removing spaces, underscores, and hyphens) for case-insensitive fuzzy matching.
- Populated backstory, personality, and appearance values directly into graph node properties and compiled return dictionaries, with a fallback biography mapping.
- Logged unrecognized section warnings in format `[VaultCompiler] Warning: Unrecognized section "X" in file Y — skipping`.
- Added comprehensive unit tests in `TestRunnerNode.gd` and registered it as a new automated test.

### AU013 : Clarify PromptBuilder / SystemPrompts Boundary & Consolidate Responsibilities (done)
- Defined a clear separation of concerns boundary: `SystemPrompts.gd` holds strictly state-free static prompt templates and persona rules; `PromptBuilder.gd` is responsible for runtime dynamic state querying (CampaignState, Graph) and token budgeting.
- Added comprehensive file header docstrings to `SystemPrompts.gd` and `PromptBuilder.gd` documenting the boundary contract.
- Moved `get_character_agent_prompt_for_id` and `get_emotion_reflection_prompt_for_id` from `SystemPrompts.gd` to `PromptBuilder.gd`.
- Introduced pure static template method `get_emotion_reflection_prompt` in `SystemPrompts.gd`.
- Moved the static stalling text block out of `PromptBuilder.gd` into `SystemPrompts.get_director_busy_stalling_prompt()`.
- Updated `EmotionEngine.gd` and test cases in `TestRunnerNode.gd` to invoke the new static `PromptBuilder` methods.
- Updated `ARCHITECTURE.md` with prompt boundary clarifications and a Mermaid prompt pipeline data flow diagram.
- Updated `gemini.md` Feature Map mappings.

### AU012 : Add Prompt Injection Sanitization & Guardrails (done)

- Developed `PlayerInputParser.sanitize_input(input_text)` to strip line-beginning prompt injection keywords (such as `SYSTEM:`, `USER:`, `[INST]`, `<<SYS>>`) case-insensitively and escape `<player_message>` delimiters.
- Integrated `PlayerInputParser.sanitize_input` inside `PromptBuilder.gd`'s `build_prompt` and `build_world_builder_prompt` calls.
- Enclosed player inputs inside `<player_message>...</player_message>` XML-like tags in prompt builders.
- Updated system prompts in `SystemPrompts.gd` (Rule 10 for World Builder, Rule 11 for Character Agent) instructing the models to treat content within `<player_message>` tags strictly as dialogue/in-character actions and ignore any meta-directives.
- Integrated input sanitization inside `GameLoopController.gd`'s `send_player_input` before it gets added to history logs.
- Wrote and validated comprehensive test assertions in `TestRunnerNode.gd`.

### AU011 : Implement Medium & Long-Term Memory Summarization Pipeline (done)
- Implemented character-specific memory properties (`medium_term_memories`, `long_term_memory`, `turns_since_last_summary`) inside CampaignState with lazy backward-compatible initialization.
- Added active character ID stamping onto conversation log entries, enabling character-specific history isolation and retrieval.
- Developed a new, modular `MemoryManager` class handling sliding-window compaction, session summarization, and long-term memory distillation.
- Integrated background summarization and distillation calls into the sequential LLM request queue via `LLMClient.send_custom_request`.
- Updated `PromptBuilder` to format and inject character-specific long-term and medium-term memories into NPC prompts and World Builder prompts.
- Configured the gameplay loop (`GameLoopController.gd`) to track turns, trigger history compaction, and run session summaries.
- Modified the sidebar display in `MainViewport.gd` to show character-specific memory sections dynamically when a character is selected.
- Wrote and validated a comprehensive automated test `test_memory_summarization_pipeline` in `TestRunnerNode.gd` with mock LLM integration.

### AU010 : Make KnowledgeGraphManager Authoritative & Add Graph Traversal (done)
- Refactored `KnowledgeGraphManager.gd` to support optional custom graph targets, enabling local instantiation within compiler contexts and tests.
- Implemented `get_nodes_by_type(type)` to query entities of a specific category from the graph.
- Implemented `get_entities_connected_to(entity_id, relation, max_depth)` supporting multi-hop, bidirectional graph traversal.
- Removed redundant `characters` storage from `CampaignState.state`, `SaveManager.gd`, and `VaultCompiler.gd`.
- Programmed a seamless save state migration in `SaveManager._upgrade_save_state` to convert legacy `characters` configurations into knowledge graph nodes on-the-fly and clean up the JSON save files.
- Routed all runtime character lookups and mutations through `CampaignState.get_character(id)`, editing properties inside character nodes inside the knowledge graph.
- Added comprehensive unit tests in `TestRunnerNode.gd` validating bidirectional traversal queries and upgrade migration rules, passing the entire test suite.

### AU009 : Documentation, Configuration, and Ticket System Reconciliation (done)
- Replaced all occurrences of `res://scripts/` with `res://src/` in backlog files (`docs/backlog.md`), `design_philosophy.md`, and core files (`JsonRepair.gd`, `MarkdownParser.gd`, `SaveManager.gd`).
- Corrected broken cross-references to `ARCHITECTURE.md` in `design_philosophy.md` by pointing to specific heading anchors.
- Updated `README.md` to reference `resources/assets/` instead of `assets/`.
- Renamed all duplicate ticket ID collisions in `docs/done.md` (`TKT025`, `TKT026`, `TKT027`, `TKT030`, `TKT040`, `TKT059`, `TKT060`) with unique suffixes (`B`, `C`, `D`).
- Moved completed tickets (`TKT022`, `TKT023`, `TKT037`, `TKT038`, `TKT039`) by deleting them from `docs/backlog.md` (their completed logs were already recorded in `docs/done.md`).
- Removed `3d/physics_engine="Jolt Physics"` from `project.godot` to clean up unused settings.
- Removed SQLite database references from `docs/research.md` in favor of `CampaignState` JSON state.
- Synced `ARCHITECTURE.md` to document missing subsystems (`ImageGenClient`, `ImageGenManager`, `ProceduralArtEngine`, `LLMStreamRequest`, `PlayerInputParser`, `ThemeManager`, `OnboardingFlow`) and updated the Mermaid turn flow diagram to represent `GameLoopController` orchestration.
- Verified test suite with all 22/22 unit tests passing.

### AU008 : Expand Test Coverage for Critical Business Logic (Emotions, Streamers, JSON Repair, Parser) (done)
- Refactored `tests/TestRunnerNode.gd` `_ready()` to dynamically discover and run all `test_` methods via `get_method_list()`, mapping test functions to user-friendly descriptions and alphabetizing the execution order.
- Enhanced `src/core/JsonRepair.gd` with single-quote conversions and structural balancing (auto-closing strings, arrays, and objects) to repair and parse truncated LLM responses.
- Added `test_emotion_engine_calculations` covering valid, invalid, and boundary tag updates, out-of-bound `intensity` and `rapport_delta` clamping, invalid emotion fallback, and empty/missing arguments.
- Added `test_json_repair_edge_cases` covering single-quoted JSON conversion, truncated/missing brackets JSON recovery, and completely unparseable fallback structures.
- Added `test_llm_stream_parsing` covering chunked bytes ingestion/callbacks in `LLMStreamRequest` and zone transitions/character-streaming events in `LLMStreamParser`.
- Added `test_markdown_parser_edge_cases` covering multiline and inline lists in frontmatter, wiki-links (with and without display labels), and markdown callouts.
- Verified all 22/22 tests pass successfully in headless execution.

### AU007 : Implement Stream Request Cancellation, Robust Error Handlers, and UI Smoothness (done)
- Added `cancel()` support to `LLMStreamRequest.gd` and `LLMClient.gd` to immediately abort active HTTP client streams.
- Connected `EventBus.location_changed` to automatically cancel active streams during location transitions.
- Connected text-changed input listeners to cancel streams dynamically when the player starts typing.
- Added a circular procedural `LoadingSpinner.gd` component to show rotating progress indicators on the screen without using flat image files.
- Added a glassmorphic Toast notification system to the viewport to present clear errors (stalls, offline connection, bad models).
- Addressed JSON parsing failures by logging clean warning alerts with interactive `[url=retry]Click to retry.[/url]` links in the chat.
- Restructured `NearbyCharacterList.gd` refreshing logic to map and reuse existing list item nodes in place, completely eliminating visual flickering.

### AU006 : Fix Autoload Configuration Conflicts, Secure Save Manager, and Decouple via EventBus (done)
- Split configuration storage into separate config files (`user://theme_config.json` and `user://client_config.json`) to prevent write concurrency clashes.
- Programmed a fallback migration to read, copy, and write legacy settings from `user://config.json` upon first startup.
- Refactored `SaveManager.gd` to perform atomic writes (writing to `.tmp` files before renaming to `.json` upon success) to shield campaign files from write-time crash corruption.
- Added save versioning (`schema_version: 1.0.0` at root, `version: 1.0.0` in metadata) and mapped schema upgrade logic to automatically restore missing fields/tables for older campaigns.
- Added validation checks to reject corrupted JSON inputs safely.
- Implemented character avatar texture caching in `CampaignState.gd` to prevent repetitive disk reads on frame updates.
- Expanded `EventBus.gd` with 6 missing signals (`location_changed`, `campaign_saved`, `turn_started`, `turn_completed`, `director_prompt_generated`, `character_state_updated`) and wired them up to cleanly decouple viewport/lists from controller internals.
- Added Test 15, Test 16, Test 17, and Test 18 to the test suite (`TestRunnerNode.gd`) covering configs migration, atomic saving/upgrading, avatar caching, and EventBus signal notifications, verifying all 18/18 tests pass successfully.

### AU005 : Develop Robust Markdown Parser (Wiki-Links, Tags, Tables) & Cache-Enabled Compiler (done)
- Upgraded `MarkdownParser.gd` to parse wiki-links (`[[Target Entity]]` and `[[Target Entity|Display Label]]`), hierarchical hashtags (`#tag/subtag`), callout blocks (`> [!info]`), and markdown tables.
- Refactored `VaultCompiler.gd` from a single static method into a compiler class instance with clearly partitioned private methods (`_scan_vault_dir`, `_parse_all_files`, `_process_nodes_first_pass`, `_process_edges_second_pass`, etc.), retaining a backwards-compatible static entry point wrapper.
- Implemented a cache-enabled single-pass compiler logic in `VaultCompiler.gd` that pre-parses all markdown files exactly once at the beginning, replacing all redundant disk read/parse calls with fast cache lookups.
- Resolved entity relationship edges directly from parsed wiki-links (`wiki_links`) in node bodies, rather than naive text substring searches.
- Unified frontmatter tags and body hashtags into a single list in the node properties, automatically splitting hierarchical tags (e.g. assigning the tag `"shadows"` when `#faction/shadows` is found).
- Fixed a scanner-compiler mismatch by correcting the hidden folder checks to ignore any folders starting with `.`.
- Removed the hardcoded `"marcello"` and `"fik"` image overrides from the compiler logic.
- Expanded the engine test runner (`TestRunnerNode.gd`) with test cases validating robust markdown parsing and compiler-side tag and edge generation.

### AU004 : Consolidate Emotion Processing and Relationship Simulation in EmotionEngine.gd (done)
- Consolidated all emotion and relationship simulation logic from `GameLoopController.gd` to `EmotionEngine.gd`, including base emotion deduction and reflection coordination.
- Standardized the starting baseline emotion intensity to `0.5` across `PromptBuilder`, `SystemPrompts`, and the engine to ensure consistency.
- Implemented value clamping: clamped primary/secondary emotion intensity to `[0.0, 1.0]` and `rapport_delta` to `[-0.2, 0.2]` to prevent overflow.
- Implemented emotional decay towards `0.0` at a configurable `decay_rate` (default `0.05`) triggered on turn progression and location changes.
- Consolidated the duplicated relationship label helper `_get_relationship_label()` into `CharacterProfile.get_relationship_label()`.
- Added a `rapport_delta` field to `EmotionEvent` and tracked the delta history in `CampaignState` emotions log.
- Wrote unit tests verifying clamping, relationship mapping, and decay logic.

### AU003 : Centralized LLM Request Queue & Context Window Budgeting (done)
- Created a centralized request queue inside `LLMClient.gd` to process all LLM requests (custom requests, stream requests, and model warmups) sequentially with a concurrency limit of 1.
- Implemented a character-based token count estimation heuristic (`1 token ≈ 4 characters`) across the engine.
- Configured context window budgeting partition limits inside `PromptBuilder.gd` (30% system instructions, 30% NPC identity, 30% recent chat history, 10% lore context).
- Programmed a progressive context truncation pipeline: first dropping distant lore neighbors and connections in `KnowledgeGraphManager.gd`'s `retrieve_context()`, then trimming older history logs, and compressing older history turns by stripping parenthetical actions and clamping length.
- Added comprehensive unit tests in `TestRunnerNode.gd` validating request serialization order, token estimation correctness, history compression, and lore budgeting truncation.

### AU002 : Refactor MainViewport.gd Monolith & Gameplay UI Decoupling (done)
- Extracted business/narrative orchestration from `MainViewport.gd` into a dedicated `GameLoopController.gd` (pure script, no UI).
- Moved the inline streaming JSON token parser from `_on_ai_response_chunk_received()` and `_process_stream_characters()` into a reusable `src/core/LLMStreamParser.gd` class that parses chunk bytes/chars and emits structured key-value updates.
- Extracted the sidebar list logic (`_refresh_character_list`, `_get_nearby_character_ids()`) and biography parsing into a child node/script `src/ui/NearbyCharacterList.gd`.
- Fixed the `LLMClient.response_received` signal race condition by replacing global signal listening with target-specific request objects and callbacks.
- Cleaned up `_remove_last_system_message()` to not rely on naive `"Thinking..."` string match, but rather track UI Message node references.
- Implemented `_exit_tree()` in MainViewport to disconnect all autoload signals (`EventBus`, `ImageGenManager`, etc.) to prevent memory leaks on scene transitions.

### AU001 : Refactor OnboardingFlow.gd Mega-Monolith & UI Architecture (done)
- Decomposed the programmatic onboarding UI into smaller, modular sub-scenes: `WelcomeScreen.tscn`, `SetupWizard.tscn`, `LLMConfig.tscn`, `CharacterCreator.tscn`, `LoadScreen.tscn`, `ReviewScreen.tscn`, and `DrawThingsTutorial.tscn`.
- Created controller scripts for each sub-scene under `src/ui/onboarding/` to handle local events, input validation, and connection testing.
- Refactored `OnboardingFlow.gd` to act strictly as a screen orchestrator and state coordinator, simplifying visibility transitions to a clean data-driven mapping.
- Replaced the busy-wait polling loop in `_on_pc_next_pressed()` with a proper signal-based event await on `background_setup_completed`.
- Resolved the magic wand portrait generator signal connection leak on `ImageGenManager.asset_generated` by tracking active callables and disconnecting them safely.
- Confirmed that the refactored wizard layouts look identical, transitions function smoothly, and the Orison Engine Test Suite passes 11/11 tests.

### TKT067 : Refactor Model Thinking Output and Display Story Intro on Load (done)
- Changed character thinking message from "[Character] is thinking..." to "Thinking..." to accurately reflect that the model is processing, maintaining immersion.
- Added compatibility check in `CampaignState.gd` for `intro_narration` campaign metadata.
- Modified `MainViewport.gd` to save the campaign introduction narration (generated or fallback) to campaign metadata `intro_narration`.
- Updated `load_existing_campaign()` in `MainViewport.gd` to prepend the story introduction narration to the dialogue chat view on load if it is not already present in the loaded history, including a visual divider if messages in-between were omitted.
- Added validation checks for the `intro_narration` metadata functions in `tests/TestRunnerNode.gd` and confirmed all tests pass.

### TKT066 : Implement Mutual Exclusivity for Character and Background Images (done)
- Modified `CharacterVisuals.gd` to add `fade_out()` and `fade_in()` methods, animating the portrait `modulate:a` property smoothly using tweens.
- Updated `load_character()` and `apply_emotion()` in `CharacterVisuals.gd` to smoothly fade in the character portrait if it was previously hidden/faded out.
- Added `_bg_fade_tween` and `_fade_out_background()` to `MainViewport.gd` to manage the environment background's opacity.
- Configured `_fade_in_background()` in `MainViewport.gd` to call `character_visuals_rect.fade_out()`, ensuring the background photo replaces the character portrait when the environment finishes loading.
- Configured `_select_character()`, `_on_character_emotion_updated()`, `send_player_input()`, and the base emotion deduction callback in `MainViewport.gd` to trigger `_fade_out_background()`, ensuring the character portrait replaces the background photo when the NPC becomes active or speaks.

### TKT065 : Optimise Physical Description Generation Performance (done)
- Reverted the portrait-to-description vision request in `OnboardingFlow.gd` to use `LLMClient.world_builder_model` (which supports vision/multimodal input) instead of the fast `LLMClient.character_model`.
- Updated the character portrait analysis prompt in `OnboardingFlow.gd` to request a concise, single-paragraph physical description limited to 2-3 sentences max.

### TKT064 : Fix Eavesdropping Dialogue and First-Person Narrator Bug (done)
- Modified `SystemPrompts.gd` to instruct the Character Agent to write its `narration` field in the objective third-person from a narrator's perspective.
- Refactored prompt rules in `SystemPrompts.gd` so first-person instructions apply strictly to the `dialogue` field.
- Added a new Rule 7 in `SystemPrompts.gd` enforcing narrative progression: Character Agents must always move the story forward and avoid repetitiveness/stalling.
- Added a new Rule 8 in `SystemPrompts.gd` for handling eavesdropping/stealth: if the player is hidden, the NPC speaks aloud to itself or other characters in the room instead of staying silent.
- Updated JSON response schema descriptions in `SystemPrompts.gd` to reinforce these constraints.

### TKT063 : Implement Two-Pass Hook Generation Pipeline (done)
- Modified `OnboardingFlow.gd`'s `_build_connected_starting_clusters()` to pass through structured frontmatter `properties` metadata for locations, candidate characters, and connections.
- Implemented static helper functions `_build_compact_profile()` and `_get_first_sentence()` in `SystemPrompts.gd` to programmatically condense character and location details.
- Split the starters generation prompt in `SystemPrompts.gd` into a selection pass (`get_starters_selection_prompt()`) and a narration pass (`get_starter_narration_prompt()`).
- Replaced the single-pass hook generation flow in `OnboardingFlow.gd` with a two-pass sequence: first querying the smarter `world_builder_model` for selection, and then sequentially querying the faster `character_model` to generate narration for each of the 3 chosen hooks.
- Updated the loading screen progress messages and dots animations in `OnboardingFlow.gd` to show detailed statuses for both Pass 1 ("Analyzing World...") and Pass 2 ("Writing Narrative (X/3)...").
- Added dynamic context window token estimation in `LLMClient.gd`'s `send_custom_stream_request()` to prevent silent context truncation.
- Added test coverage verifying two-pass selection and narration prompts in `TestRunnerNode.gd` and ran the Godot test runner to confirm all 11/11 tests pass successfully.

### TKT062 : Implement Symmetric Magic Wands for Player Character Creation (done)
- Wrapped the character creator's `AvatarLabel` in a horizontal header `AvatarHeaderHBox` and added the new `PcAvatarMagicWandButton` (`%PcAvatarMagicWandButton`) next to it.
- Connected the `pc_description_input.text_changed` and wand buttons pressed signals in `OnboardingFlow.gd` to dynamically manage the wands' active/disabled states.
- Implemented `_on_pc_avatar_magic_wand_pressed()` to trigger the `ImageGenManager.generate_asset` pipeline using the character's physical description.
- Simplified `_on_pc_magic_wand_pressed()` to focus exclusively on vision analysis (description extraction) of the loaded/generated avatar.
- Implemented `_update_avatar_magic_wand_state()` (enabled only if the physical description is not empty) and `_update_magic_wand_button_state()` (enabled only if a valid avatar file exists on disk) and connected them to UI state initialization and update hooks.

### TKT061 : Use Native OS File Browser for Directory and File Dialogs (done)
- Configured campaign directory selection (`_on_browse_pressed`) to check for native OS dialog support via `DisplayServer.has_feature` and open a native macOS/Windows folder picker via `DisplayServer.file_dialog_show`.
- Configured character avatar selection (`_on_pc_avatar_browse_pressed`) to check for native OS dialog support and open a native macOS/Windows file picker via `DisplayServer.file_dialog_show` with image file filters (`*.png,*.jpg,*.jpeg;Images`).
- Added native selection callback handlers (`_on_native_dir_selected` and `_on_native_image_selected`) in `OnboardingFlow.gd` to cleanly forward user choices to downstream systems.
- Maintained a themed `FileDialog` fallback configuration for platforms where native dialogs are unsupported (e.g. Linux configurations or debug environments).

### TKT060 : Implement Background Onboarding Compilation & LLM Warmup (done)
- Added `warmup_model(model_name: String) -> void` helper in `LLMClient.gd` to load Ollama models asynchronously using the `/api/generate` endpoint with `keep_alive = -1`.
- Integrated background setup tracking variables (`_background_compile_started`, `_background_compile_completed`) and `pc_bg_progress_label` in `OnboardingFlow.gd`.
- Implemented `_start_background_setup()` in `OnboardingFlow.gd` to compile the vault and warmup LLM models asynchronously, updating the status label in the character panel at each step.
- Programmed `_on_start_campaign_pressed()` to trigger background compilation immediately when entering character creation.
- Modified `_on_pc_next_pressed()` to wait for background compilation if the user submits the form before it finishes, otherwise skipping compilation entirely and proceeding straight to hook generation.
- Configured `_on_config_back_pressed()` to reset compilation states if the user goes back to modify setup parameters.

### TKT059 : Implement Dynamic Speaking Character Selection & Narrative-Action Loop (done)
- Programmed `VaultCompiler.gd` and `CampaignState.gd` to detect character suitability flags (`is_creature`, `can_speak`, `humanoid`) during compilation using path checks (e.g. `/fauna/`, `/flora/`, `/creature`) and frontmatter checks, saving them to the character save state.
- Updated `OnboardingFlow.gd` starter hook generator to filter candidate characters for starting location clusters to only include humanoid speaking characters, preventing non-speaking creatures from being hardcoded as conversational targets.
- Refactored `SystemPrompts.gd` starters generation prompt to list all candidate speaking characters for each location, letting the DM model dynamically select the most logical character.
- Modified `get_character_agent_prompt` to introduce a mandatory `"thinking"` field in the JSON schema and explicitly instruct creature characters to not speak but react non-verbally in parentheses.
- Refactored the UI streaming parser in `MainViewport.gd` into a generalized `_stream_zone` sequential parser that dynamically hides the model's reasoning phase and handles both narration and dialogue sections.
- Verified that all unit tests compile and run successfully in Godot headless mode.

### TKT058 : Implement Local Offline Image Generation Pipeline & UI Settings (done)
- Created `ProceduralArtEngine.gd` to programmatically render landscape scenes, character avatars, and item icons offline inside Godot using `SubViewport` canvas nodes, styled using deterministic color palettes hashed from seed texts.
- Developed `ImageGenClient.gd` to handle HTTP POST requests targeting local Stable Diffusion endpoints (`/sdapi/v1/txt2img` and `/sdapi/v1/img2img`) for image refinement.
- Created `ImageGenManager.gd` to coordinate the generation loop, first saving a fast procedural placeholder image and then asynchronously refining it using the local SD API (img2img).
- Dynamically integrated the Stable Diffusion settings and a collapsible setup guide inside the LLM Config screen in `OnboardingFlow.gd`, utilizing a `ScrollContainer` constraint to prevent modal screen overflow.
- Themed all dynamically generated configuration controls to match text and accent styling dynamically from the user's active theme.
- Configured character creation "Magic Wand" to generate dynamic player avatars if no avatar path is set, or analyze selected pictures if they exist.
- Connected main viewport scenery backgrounds to automatically generate, save, and cross-fade background images on location transitions.
- Integrated comprehensive unit tests verifying procedural generation dimensions and Base64 conversion helpers, with headless mocks to prevent RenderingServer hang-ups.

### TKT040 : Enhance NPC Identity Prompting (done)
- Modified `get_character_agent_prompt()` in `SystemPrompts.gd` to change the instruction from `"You are acting as the NPC character..."` to `"You ARE the character..."`, explicitly telling the model to write in the first person.
- Updated SystemPrompts rules (Rule 1 & 4) and the JSON dialogue schema description to require first-person speech and actions (e.g. changing the parenthetical action example from third-person `(she sighs...)` to first-person `(I sigh...)`), and strictly forbidding speaker prefixes and third-person narrator descriptions.
- Enhanced `get_emotion_reflection_prompt_for_id()` to also tell the character they ARE the NPC and instruct them to reflect from their perspective.
- Adjusted the prompt ending suffix in `PromptBuilder.gd` from `"Assistant: "` to `"Response (JSON): "` to prevent the local LLM from generating responses in a generic AI assistant persona.

### TKT057 : Add HTTP Request and Stream Timeouts to LLM Client (done)
- Configured robust timeouts on all temporary `HTTPRequest` helper instances in `LLMClient.gd` (5 seconds for connection test, 90 seconds for custom text requests, 30 seconds for custom vision requests).
- Implemented a delta-based stream timeout check (30 seconds) in the `_process()` loop of `LLMClient.gd` to prevent infinite hangs if connection to the local streaming Ollama instance fails silently.
- Ensured failed requests cleanly trigger callbacks with error parameters, allowing the onboarding flow to fallback gracefully rather than freezing.
- Created and dynamically positioned a warning label inside the onboarding starting point selector to display details about LLM timeouts or malformed JSON formats if offline fallbacks are triggered.




### TKT056 : Support Dynamic NPC Base Emotion Deduction (done)
- Programmed `VaultCompiler.gd` and `CampaignState.gd` to leave the base emotion uninitialized if not explicitly configured in Obsidian frontmatter.
- Created `get_deduce_base_emotion_prompt` static method in `SystemPrompts.gd` to prompt the fast/character LLM to evaluate character biographies/profiles and return deduced base emotions and intensities as JSON.
- Implemented `_deduce_base_emotions_if_needed` in `MainViewport.gd` to asynchronously query the fast model.
- Refactored base emotion deduction to run lazily on-demand (only when a character is selected/activated) rather than querying the LLM for every character in bulk during campaign loading, resolving a massive bottleneck.
- Optimized character batch import in `MainViewport.gd` by adding an optional `affinity` parameter to `CampaignState.init_character()` to avoid 42 redundant sidebar refreshes (900+ instantiated items) during campaign import, reducing UI updates to a single call.
- Fixed a GDScript compile error in the player input parser and restored preloading to ensure clean project compilation.
- Wrote integration test cases in `TestRunnerNode.gd` verifying deduction prompt generation and fallback structures.


### TKT040B : Support Visual Novel Syntax Parsing for Player Inputs (done)
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

### TKT040C : Integrate Mind Map Button to Gameplay Sidebar (done)
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

### TKT030B : Fix Active Character Loading and Regional Nearby Filtering (done)
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

### TKT025B : Implement Dynamic Location Connections, Scene-Filtered Sidebar, and Avatars (done)
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


### TKT025C : Universal Scaling and High-DPI UI Legibility (done)
- Configured canvas-items based window stretch settings inside `project.godot` to enable responsive scaling across different window/desktop sizes.
- Added "Huge (+8px)" and "Gigantic (+12px)" font size modifier settings inside `SettingsModal.gd` dropdown selector.
- Updated dropdown mapping selectors to properly update the active theme and persist the larger scale values to user configuration.


### TKT026B : Dynamic Size Tweening for Menu Transitions (done)
- Bound `card_panel` and `card_vbox` variables in `OnboardingFlow.gd`.
- Refactored `_transition_to` in `OnboardingFlow.gd` to temporarily evaluate the target size of the card container using `get_combined_minimum_size()`, lock the card container's minimum size, fade out the current panel, swap visibility, and smoothly tween the card's `custom_minimum_size` to the target size while fading in the target panel.
- Added custom fade transitions in `OnboardingFlow.gd` to fade the entire `CardPanel` in or out when entering or exiting the full-screen mind map editor to prevent visual overlap.

### TKT027B : Player Character Creation (done)
- Designed and implemented a dedicated character creation panel (`CharacterPanel`) within the onboarding flow.
- Added support to `LLMClient.gd` to fetch and send profile images to Ollama via the `/api/generate` vision endpoint.
- Programmed a magic wand button that invokes the vision LLM to automatically summarize the physical features of the selected avatar image.
- Programmed the avatar selector file dialog, copying selected profile pictures to local user directory asset storage (`user://assets/characters/player.[ext]`).
- Passed the player character details to the starting adventure hooks generator (`SystemPrompts.gd`) and updated prompt instructions to weave the protagonist into the hooks.
- Injected player character context into character agent prompts and DM world builder prompts (`PromptBuilder.gd`).
- Integrated player character state initialization and excluded `"player"` from the active NPC sidebar list in `MainViewport.gd`.

### TKT059B : Standardize UI Guide Font Styling and Scaling (done)
- Configured the base theme resource `resources/themes/orison_ui.tres` to map all `RichTextSmall` style variations (normal, bold, italics, bold italics, mono) to the default sans-serif font (`SystemFont_sans`), eliminating mixed serif and sans-serif styling in system interfaces.
- Updated the autoload `ThemeManager.gd`'s `apply_active_theme()` function to dynamically assign the active theme's default sans-serif font to all `RichTextSmall` styles, supporting custom theme presets and font modifications consistently.
- Refactored `OnboardingFlow.gd` to remove hardcoded `[font_size=12]` and `[/font_size]` BBCode wraps from the Image Generator Guide text, letting it inherit size dynamically and scale with the user's global font size setting.
- Cleaned up redundant manual font override checks and logic from `OnboardingFlow.gd` since typography is now fully managed via global theme variation settings.
- Restored visual and typographical consistency to both the onboarding help guides and the collapsible sidebar memory labels, ensuring analytical/system text renders cleanly in sans-serif.

### TKT060B : Fix Premature LLM Stream Request Completion (done)
- Fixed a timing bug in `LLMStreamRequest.gd` and `LLMClient.gd` where the `HTTPClient` streaming status was evaluated as `STATUS_CONNECTED` (pre-request status) on the very first frame after issuing the request, causing the request to complete immediately with an empty response.
- Introduced `_has_entered_requesting` (in `LLMStreamRequest.gd`) and `_stream_has_entered_requesting` (in `LLMClient.gd`) tracking variables that ensure the stream completion logic only triggers if the status has actually transitioned to `STATUS_REQUESTING` or `STATUS_BODY` first.
- Added immediate `_client.poll()` execution on the connection frame to advance the Godot low-level HTTP state machine.
- Resolved the issue where local LLM hook generation in the onboarding flow was failing with empty responses and falling back to offline defaults.


### TKT040D : Convert Image Generation to txt2img with Custom Aspect Ratios (done)
- Changed the local AI image generation pipeline in `ImageGenManager.gd` from image-to-image (`img2img`) refinement to pure text-to-image (`txt2img`) generation.
- Configured category-specific target dimensions: scenery backgrounds (`scene`) generate at 768x512 (landscape format), while character portraits (`avatar`) and item icons (`item`) generate at 512x512 (square format).
- Updated procedural fallback generator calls inside `ImageGenManager.gd` to use these same custom dimensions, ensuring the temporary placeholders match the final aspect ratios of the AI-generated images.
- Added new automated tests to `TestRunnerNode.gd` to verify that `ProceduralArtEngine` correctly supports and generates custom sizes for both square and landscape aspect ratios.

### BUG001 : Fix Empty Streaming Character Dialogue & Add Player Console Logging (done)
- Printed the player's submitted chat inputs directly to the console (`[PLAYER] <input_text>`) within `GameLoopController.gd` to improve log detailing.
- Marked the empty dialogue message spawned at the start of LLM streaming as temporary (`is_temporary = true`) in `MainViewport.gd`.
- Bypassed the conditional `stream_zone == "none"` check at stream completion in `GameLoopController.gd` to always emit `message_logged` for the final narration and dialogue.
- Ensured any discrepancy, stall, or incomplete text from real-time stream chunking is cleanly cleaned up and overwritten by the final, authoritative parsed JSON response.

### BUG002 : Prevent Redundant Location Changed Events and Background Regens (done)
- Modified `consume_pending_scene()` in `GameLoopController.gd` to check if `active_location != _last_location_id` before emitting `EventBus.location_changed`.
- Prevented redundant location update triggers and image generation API calls on every game turn when the location has not actually changed.

