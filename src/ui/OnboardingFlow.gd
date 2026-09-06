# res://src/ui/OnboardingFlow.gd
extends Control
class_name OnboardingFlow

signal adventure_started(campaign_id: String, title: String, vault_path: String, custom_mappings: Dictionary)
signal adventure_loaded(campaign_id: String)
signal background_setup_completed(success: bool)

const VaultScanner = preload("res://src/core/VaultScanner.gd")
const LoadingSpinnerScript = preload("res://src/ui/LoadingSpinner.gd")

# Bound UI child screen nodes
@onready var welcome_screen: VBoxContainer = %WelcomeScreen
@onready var setup_wizard: VBoxContainer = %SetupWizard
@onready var llm_config: VBoxContainer = %LLMConfig
@onready var character_creator: VBoxContainer = %CharacterCreator
@onready var review_screen: VBoxContainer = %ReviewScreen
@onready var load_screen: VBoxContainer = %LoadScreen
@onready var loading_overlay: PanelContainer = %LoadingOverlay
@onready var loading_text: Label = %LoadingText
@onready var card_panel: PanelContainer = %CardPanel

@onready var overwrite_dialog: ConfirmationDialog = %OverwriteConfirmDialog
@onready var loading_spinner = %LoadingSpinner
var _screens: Dictionary = {}
var _active_screen_name: String = ""

# Campaign setup and compilation state
var _scanned_results: Dictionary = {}
var _custom_mappings: Dictionary = {}
var _compiled_data: Dictionary = {}
var _player_character: Dictionary = {
	"name": "",
	"physical_description": "",
	"personality": "",
	"backstory": "",
	"avatar": ""
}

var _background_compile_started: bool = false
var _background_compile_completed: bool = false
var _background_compilation_done: bool = false
var _pending_warmups: int = 0
var _warmup_success: bool = true
var _signal_emitted: bool = false
var _background_setup_id: int = 0
var _hook_gen_id: int = 0


# Hook generation streaming state
var _is_generating_hooks: bool = false
var _loading_phase: String = "" # "compiling", "connecting", "loading_model", "generating"
var _loading_elapsed: float = 0.0
var _loading_dots_timer: float = 0.0
var _loading_dots_frame: int = 0
var _hook_text_received: String = ""

var _selected_starter_idx: int = -1
var _generated_starters: Array = []
var _selected_clusters: Array = []
var _pass1_selections: Array = []
var _current_hook_idx: int = 0

const CATEGORIES = [
	{
		"id": "scene",
		"label": "Scenes & Story",
		"icon": "🎬",
		"color": Color(1.0, 0.373, 0.220)
	},
	{
		"id": "character",
		"label": "Characters & NPCs",
		"icon": "👤",
		"color": Color(0.957, 0.247, 0.369)
	},
	{
		"id": "location",
		"label": "Locations & World",
		"icon": "📍",
		"color": Color(0.96, 0.62, 0.04)
	},
	{
		"id": "lore",
		"label": "Lore & Systems",
		"icon": "📚",
		"color": Color(0.624, 0.525, 0.753)
	}
]

func _ready() -> void:
	# 1. Map screen nodes
	_screens = {
		"welcome": welcome_screen,
		"setup": setup_wizard,
		"config": llm_config,
		"character": character_creator,
		"review": review_screen,
		"load": load_screen
	}
	
	# 2. Connect signals from children screens
	welcome_screen.start_new_pressed.connect(func(): show_screen("setup"))
	welcome_screen.load_pressed.connect(func(): show_screen("load"))
	welcome_screen.settings_pressed.connect(_on_settings_pressed)
	
	setup_wizard.back_pressed.connect(func(): show_screen("welcome"))
	setup_wizard.campaign_crafted.connect(_on_campaign_crafted)
	
	llm_config.back_pressed.connect(func(): show_screen("setup"))
	llm_config.start_campaign_pressed.connect(_on_llm_config_completed)
	
	character_creator.back_pressed.connect(func(): show_screen("config"))
	character_creator.character_created.connect(_on_character_created)
	
	review_screen.back_pressed.connect(func():
		_cancel_hook_generation()
		show_screen("character")
	)
	review_screen.next_pressed.connect(_on_review_next_pressed)
	
	load_screen.back_pressed.connect(func(): show_screen("welcome"))
	load_screen.campaign_selected.connect(_load_campaign)
	
	ThemeManager.theme_changed.connect(_on_theme_changed)
	_on_theme_changed()
	
	# Setup Overwrite Confirmation Dialog
	overwrite_dialog.confirmed.connect(_on_overwrite_confirmed)
	var overwrite_label = overwrite_dialog.get_label()
	if overwrite_label:
		overwrite_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		overwrite_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		
	if loading_spinner:
		loading_spinner.color = Color("#A59EBF")
		
	# Hide loading overlay initially
	loading_overlay.visible = false
	
	# Show initial welcome screen
	show_screen("welcome")

func show_screen(screen_name: String) -> void:
	if not _screens.has(screen_name):
		return
	
	# Fade/visibility transition
	for name in _screens.keys():
		var scr = _screens[name]
		if name == screen_name:
			scr.visible = true
			scr.modulate.a = 1.0
			_active_screen_name = screen_name
			
			# Trigger child screen setup logic if present
			if scr.has_method("on_screen_shown"):
				scr.on_screen_shown()
		else:
			scr.visible = false
			scr.modulate.a = 0.0
			
	# Update card panel container sizes
	card_panel.custom_minimum_size = Vector2(480, 0)
	await get_tree().process_frame
	card_panel.size = card_panel.get_combined_minimum_size()

func _on_theme_changed() -> void:
	self.theme = ThemeManager.active_theme
	if overwrite_dialog:
		overwrite_dialog.theme = ThemeManager.active_theme

func _on_settings_pressed() -> void:
	var modal_scene = load("res://scenes/ui/SettingsModal.tscn")
	if modal_scene:
		var modal = modal_scene.instantiate()
		modal.theme = ThemeManager.active_theme
		add_child(modal)

func _on_campaign_crafted(title: String, campaign_id: String, vault_path: String, use_sample: bool) -> void:
	# Keep a reference to campaign config in custom mappings
	_custom_mappings["campaign_title"] = title
	_custom_mappings["campaign_id"] = campaign_id
	_custom_mappings["vault_path"] = vault_path
	_custom_mappings["use_sample"] = use_sample
	
	# Check if campaign already exists on disk
	var campaign_exists = false
	var saves = SaveManager.get_campaign_list()
	for save in saves:
		if save.id == campaign_id:
			campaign_exists = true
			break
			
	if campaign_exists:
		overwrite_dialog.dialog_text = "An adventure with the ID '%s' already exists. Overwriting will delete all previous saves and progress for this adventure. Do you want to proceed?" % campaign_id
		overwrite_dialog.popup_centered()
	else:
		_proceed_to_next_step()

func _on_overwrite_confirmed() -> void:
	_proceed_to_next_step()

func _proceed_to_next_step() -> void:
	show_screen("config")

func _on_llm_config_completed() -> void:
	# Reset compile state for a new campaign run
	_background_compile_started = false
	_background_compile_completed = false
	_background_compilation_done = false
	_signal_emitted = false
	
	# Reset character creator data to blank state for this campaign
	_player_character = {
		"name": "",
		"physical_description": "",
		"personality": "",
		"backstory": "",
		"avatar": ""
	}
	
	# Clean up any stale temp avatar file from previous runs
	if FileAccess.file_exists("user://temp_pc_avatar.png"):
		DirAccess.remove_absolute("user://temp_pc_avatar.png")
		
	show_screen("character")
	character_creator.setup_character(_player_character)
	
	# Run compilation and model warmup in the background
	_start_background_setup()

# ==============================================================================
# Background Compiler & LLM Warmup
# ==============================================================================

func _start_background_setup() -> void:
	if _background_compile_started:
		return
	_background_compile_started = true
	_background_compile_completed = false
	_background_compilation_done = false
	_pending_warmups = 0
	_warmup_success = true
	
	_background_setup_id += 1
	var current_setup_id = _background_setup_id
	
	_update_background_progress_status(current_setup_id)
	
	# 1. Warm up the LLMs in the background
	var wb_model = LLMClient.world_builder_model
	var char_model = LLMClient.character_model
	
	var models_to_warm = []
	if not wb_model.is_empty():
		models_to_warm.append(wb_model)
	if not char_model.is_empty() and char_model != wb_model:
		models_to_warm.append(char_model)
		
	_pending_warmups = models_to_warm.size()
	
	var on_warmup_completed = func(success: bool):
		if current_setup_id != _background_setup_id:
			return
		_pending_warmups -= 1
		if not success:
			_warmup_success = false
		_update_background_progress_status(current_setup_id)
		
	for model in models_to_warm:
		LLMClient.warmup_model(model, on_warmup_completed)
		
	# 2. Run compilation asynchronously
	_update_background_progress_status(current_setup_id)
	
	var vault_path = _custom_mappings.get("vault_path", "")
	if _custom_mappings.get("use_sample", false):
		_generate_sample_vault(vault_path)
		await get_tree().process_frame
		if current_setup_id != _background_setup_id:
			return
		
	# Run vault scanner
	_update_background_progress_status(current_setup_id)
	await get_tree().process_frame
	if current_setup_id != _background_setup_id:
		return
	var on_progress = func(current: int, total: int):
		if current_setup_id != _background_setup_id:
			return
		if character_creator:
			character_creator.set_compilation_progress("⚙️ Scanning vault files (%d/%d)..." % [current, total], false, _warmup_success)
	EventBus.scan_progress.connect(on_progress)
	_scanned_results = VaultScanner.scan_vault(vault_path)
	EventBus.scan_progress.disconnect(on_progress)
	if current_setup_id != _background_setup_id:
		return

	
	# Setup initial custom mappings
	_custom_mappings["folders"] = {}
	_custom_mappings["starting_location_id"] = ""
	_custom_mappings["starting_character_id"] = ""
	
	var folders = _scanned_results.get("folders", {})
	for folder_path in folders.keys():
		_custom_mappings["folders"][folder_path] = folders[folder_path]
		
	_update_background_progress_status(current_setup_id)
	await get_tree().process_frame
	if current_setup_id != _background_setup_id:
		return
	
	# Compile vault
	var compiled = await VaultCompiler.compile_vault(vault_path, _custom_mappings, func(msg: String):
		if current_setup_id != _background_setup_id:
			return
		if character_creator:
			character_creator.set_compilation_progress("⚙️ " + msg, false, _warmup_success)
	)
	if current_setup_id != _background_setup_id:
		return
	_compiled_data = compiled
	_custom_mappings["compiled_data"] = _compiled_data
	
	_background_compilation_done = true
	_update_background_progress_status(current_setup_id)

func _update_background_progress_status(setup_id: int) -> void:
	if setup_id != _background_setup_id:
		return
	if not character_creator:
		return
		
	# Check if both compilation and warmups are completed
	if _background_compilation_done and _pending_warmups <= 0:
		if _signal_emitted:
			return
		_signal_emitted = true
		_background_compile_completed = true
		
		var status_text = ""
		if _warmup_success:
			status_text = "✨ Vault compiled & LLMs ready!"
		else:
			status_text = "⚠️ Vault compiled. LLM warmup failed or timed out."
			
		character_creator.set_compilation_progress(status_text, true, _warmup_success)
		background_setup_completed.emit(_warmup_success)
		return
		
	# Otherwise, display current status
	var status_text = ""
	if not _background_compilation_done:
		status_text = "⚙️ Compiling campaign vault..."
	elif _pending_warmups > 0:
		status_text = "🧠 Warming up LLM models (%d remaining)..." % _pending_warmups
		
	character_creator.set_compilation_progress(status_text, false, _warmup_success)

# ==============================================================================
# Character creation complete workflow
# ==============================================================================

func _on_character_created(pc_data: Dictionary) -> void:
	_player_character = pc_data
	
	# Show loading overlay while resolving background setup
	loading_overlay.visible = true
	var loading_title = loading_overlay.find_child("LoadingTitle")
	
	# Copy player avatar image to local user directory if selected
	var pc_avatar_source = _player_character.get("avatar", "")
	if not pc_avatar_source.is_empty() and FileAccess.file_exists(pc_avatar_source):
		var ext = pc_avatar_source.get_extension().to_lower()
		var target_dir = "user://assets/characters"
		var dir = DirAccess.open("user://")
		if dir:
			if not dir.dir_exists("assets/characters"):
				dir.make_dir_recursive("assets/characters")
		var target_path = target_dir.path_join("player.%s" % ext)
		if pc_avatar_source != target_path:
			var copy_err = DirAccess.copy_absolute(pc_avatar_source, target_path)
			if copy_err == OK:
				_player_character["avatar"] = target_path
				print("[OnboardingFlow] Copied player avatar to: ", target_path)
				
	_custom_mappings["player_character"] = _player_character
	_custom_mappings["art_style"] = pc_data.get("art_style", "Digital Anime Art")
	
	# Wait for background compilation to complete (Proper signal-based await)
	if not _background_compile_completed:
		if not _background_compile_started:
			_start_background_setup()
			
		if not _background_compile_completed:
			if not _background_compilation_done:
				if loading_title:
					loading_title.text = "COMPILING VAULT..."
				loading_text.text = "Completing background vault compilation..."
			else:
				if loading_title:
					loading_title.text = "WAKING UP LOCAL LLM..."
				loading_text.text = "Warming up local LLM models (%d remaining)..." % _pending_warmups
				
			# Wait for the signal instead of a polling while loop
			await background_setup_completed
			
	# Populate metrics summary on review panel
	_populate_review_panel()
	
	# Update loading message for LLM generation
	if loading_title:
		loading_title.text = "GENERATING HOOKS..."
	loading_text.text = "Drafting adventure hooks via local LLM..."
	await get_tree().process_frame
	
	# Transition to Hook Selection screen while keeping the overlay visible
	show_screen("review")
	
	# Trigger LLM Hook Generation
	_generate_adventure_hooks()

func _populate_review_panel() -> void:
	var folders = _scanned_results.get("folders", {})
	var all_files = _scanned_results.get("all_files", [])
	
	var locations_count = 0
	var characters_count = 0
	var scenes_count = 0
	var lore_count = 0
	
	var vault_path_clean = _custom_mappings.get("vault_path", "")
	for file_entry in all_files:
		var file_path = file_entry.file_path
		var rel_path = file_path.substr(vault_path_clean.length()).lstrip("/")
		var parent_dir = rel_path.get_base_dir()
		var type = folders.get(parent_dir, "lore")
		match type:
			"location": locations_count += 1
			"character": characters_count += 1
			"scene": scenes_count += 1
			"lore": lore_count += 1
			
	var summary = ""
	summary += "•  Locations / Regions: %d\n" % locations_count
	summary += "•  Characters / NPCs: %d\n" % characters_count
	summary += "•  Scenes / Quests: %d\n" % scenes_count
	summary += "•  Lore / Systems: %d" % lore_count
	
	review_screen.set_summary_text(summary)

# ==============================================================================
# Adventure Hook Generator (Two-Pass Pipeline)
# ==============================================================================

func _cancel_hook_generation() -> void:
	_hook_gen_id += 1
	if _is_generating_hooks:
		_is_generating_hooks = false
		loading_overlay.visible = false
		LLMClient.cancel()
		_generated_starters = []
		_pass1_selections = []
		_current_hook_idx = 0
		_hook_text_received = ""

func _generate_adventure_hooks() -> void:
	var campaign_title = _custom_mappings.get("campaign_title", "Custom Adventure")
	
	# Programmatically find pre-connected clusters using the Knowledge Graph
	_selected_clusters = _build_connected_starting_clusters()
	
	# Verify that we have any starting locations
	var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
	var has_locations = false
	for node_id in nodes.keys():
		if nodes[node_id].get("type") == "location":
			has_locations = true
			break
			
	if not has_locations:
		var fallback_starters = _generate_fallback_starters([])
		review_screen.display_starters(fallback_starters, _compiled_data)
		loading_overlay.visible = false
		return
		
	# Pass 1: Request starter selections/concepts using the World Builder model
	var prompt = SystemPrompts.get_starters_selection_prompt(campaign_title, _selected_clusters, _player_character)
	
	# Setup streaming state
	_hook_gen_id += 1
	var current_gen_id = _hook_gen_id
	_is_generating_hooks = true
	_loading_phase = "connecting"
	_loading_elapsed = 0.0
	_loading_dots_timer = 0.0
	_loading_dots_frame = 0
	_hook_text_received = ""
	_pass1_selections = []
	_current_hook_idx = 0
	_generated_starters = []
	
	var loading_title = loading_overlay.find_child("LoadingTitle")
	if loading_title:
		loading_title.text = "CONNECTING TO OLLAMA..."
	loading_text.text = "Connecting to server at " + LLMClient.api_url + "..."
	
	var active_model = LLMClient.world_builder_model
	if active_model.is_empty():
		active_model = "gemma4:e4b"
		
	var on_chunk = func(chunk: String):
		if current_gen_id != _hook_gen_id:
			return
		if _loading_phase != "generating_pass1":
			_loading_phase = "generating_pass1"
			if loading_title:
				loading_title.text = "ANALYZING WORLD..."
		_hook_text_received += chunk
		
	var on_completed = func(response_text: String):
		if current_gen_id != _hook_gen_id:
			return
		var parsed = _parse_json_response(response_text)
		var starters = parsed.get("starters", [])
		if not (starters is Array) or starters.is_empty():
			print("[OnboardingFlow] Pass 1 returned unusable response. Using fallbacks.")
			_on_hooks_generation_failed("Pass 1 (selection) failed to return a valid starters list. Response: " + response_text.left(200))
			return
			
		_pass1_selections = starters
		_current_hook_idx = 0
		_generate_next_narration()
		
	var on_failed = func(error_msg: String):
		if current_gen_id != _hook_gen_id:
			return
		print("[OnboardingFlow] Pass 1 hook selection FAILED. Error: %s" % error_msg)
		_on_hooks_generation_failed(error_msg)

	print("[OnboardingFlow] Requesting Pass 1 starter selection from model '%s' (prompt: %d chars)..." % [active_model, prompt.length()])
	LLMClient.send_custom_stream_request(prompt, active_model, on_chunk, on_completed, on_failed, 300.0, true)

func _generate_next_narration() -> void:
	if _current_hook_idx >= 3 or _current_hook_idx >= _pass1_selections.size():
		_finish_hook_generation()
		return
		
	var current_gen_id = _hook_gen_id
	var selection = _pass1_selections[_current_hook_idx]
	if not (selection is Dictionary):
		_handle_narration_failure("Invalid selection structure at index %d" % _current_hook_idx)
		return
		
	var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
	var loc_id = str(selection.get("location_id", "")).to_lower().replace(" ", "_")
	var char_id = str(selection.get("character_id", "")).to_lower().replace(" ", "_")
	
	# Validate and fallback to cluster defaults if the LLM returned invalid IDs
	var cluster = _selected_clusters[_current_hook_idx]
	if not nodes.has(loc_id):
		loc_id = cluster.location.id
	if not nodes.has(char_id):
		if cluster.has("candidate_characters") and not cluster.candidate_characters.is_empty():
			char_id = cluster.candidate_characters[0].id
		else:
			char_id = cluster.character.id
			
	var loc_node = nodes.get(loc_id, {"id": loc_id, "name": loc_id.capitalize(), "desc": "A starting location."})
	var char_node = nodes.get(char_id, {"id": char_id, "name": char_id.capitalize(), "desc": "A starting character."})
	
	var campaign_title = _custom_mappings.get("campaign_title", "Custom Adventure")
	var writing_style = _compiled_data.get("writing_style", "")
	
	var prompt = SystemPrompts.get_starter_narration_prompt(
		campaign_title,
		selection.get("title", "The Starter Hook"),
		selection.get("concept", "A mysterious event begins."),
		char_node,
		loc_node,
		writing_style,
		_player_character
	)
	
	_loading_phase = "connecting"
	_loading_elapsed = 0.0
	_hook_text_received = ""
	
	var loading_title = loading_overlay.find_child("LoadingTitle")
	if loading_title:
		loading_title.text = "CONNECTING FOR HOOK %d/3..." % (_current_hook_idx + 1)
	loading_text.text = "Connecting to server for Hook %d of 3..." % (_current_hook_idx + 1)
	
	var active_model = LLMClient.character_model
	if active_model.is_empty():
		active_model = "llama3.2:3b"
		
	var on_chunk = func(chunk: String):
		if current_gen_id != _hook_gen_id:
			return
		if _loading_phase != "generating_pass2":
			_loading_phase = "generating_pass2"
			if loading_title:
				loading_title.text = "WRITING NARRATIVE (%d/3)..." % (_current_hook_idx + 1)
		_hook_text_received += chunk
		
	var on_completed = func(response_text: String):
		if current_gen_id != _hook_gen_id:
			return
		var parsed = _parse_json_response(response_text)
		var narration_text = parsed.get("narration", "")
		if narration_text.is_empty():
			narration_text = "%s. %s is here at %s." % [selection.get("concept", "A strange event unfolds"), char_node.get("name", "Someone"), loc_node.get("name", "someplace")]
			
		var hook_title = parsed.get("title", selection.get("title", "Adventure Starter"))
		var hook_desc = parsed.get("description", selection.get("concept", ""))
		
		_generated_starters.append({
			"title": hook_title,
			"description": hook_desc,
			"narration": narration_text,
			"location_id": loc_id,
			"character_id": char_id
		})
		
		_current_hook_idx += 1
		_generate_next_narration()
		
	var on_failed = func(error_msg: String):
		if current_gen_id != _hook_gen_id:
			return
		print("[OnboardingFlow] Narration pass FAILED for hook %d. Error: %s" % [_current_hook_idx + 1, error_msg])
		_handle_narration_failure(error_msg)

	print("[OnboardingFlow] Requesting Pass 2 narration for hook %d/3 from model '%s' (prompt: %d chars)..." % [_current_hook_idx + 1, active_model, prompt.length()])
	LLMClient.send_custom_stream_request(prompt, active_model, on_chunk, on_completed, on_failed, 300.0, true)

func _handle_narration_failure(error_msg: String) -> void:
	var fallbacks = _generate_fallback_starters(_selected_clusters)
	if _current_hook_idx < fallbacks.size():
		_generated_starters.append(fallbacks[_current_hook_idx])
	else:
		_generated_starters.append(fallbacks[0].duplicate())
		
	_current_hook_idx += 1
	_generate_next_narration()

func _on_hooks_generation_failed(error_msg: String) -> void:
	_is_generating_hooks = false
	loading_overlay.visible = false
	
	var active_model = LLMClient.world_builder_model
	if active_model.is_empty():
		active_model = "unknown"
		
	# Display error status directly on review screen
	var err_label = review_screen.find_child("ScanSummaryLabel")
	if err_label:
		err_label.text = "⚠️ Hook generation failed (model: %s — %s).\nUsing offline fallbacks instead." % [active_model, error_msg]
		err_label.add_theme_color_override("font_color", Color(0.9, 0.45, 0.1))
		
	var fallback_starters = _generate_fallback_starters(_selected_clusters)
	_generated_starters = fallback_starters
	review_screen.display_starters(fallback_starters, _compiled_data)

func _finish_hook_generation() -> void:
	_is_generating_hooks = false
	loading_overlay.visible = false
	
	# Pad to exactly 3 starters if needed
	var clean_starters = _generated_starters.duplicate()
	if clean_starters.is_empty():
		clean_starters = _generate_fallback_starters(_selected_clusters)
		
	while clean_starters.size() < 3:
		var fallbacks = _generate_fallback_starters(_selected_clusters)
		var index = clean_starters.size()
		clean_starters.append(fallbacks[index])
		
	review_screen.display_starters(clean_starters, _compiled_data)

func _process(delta: float) -> void:
	if not _is_generating_hooks:
		return
		
	_loading_elapsed += delta
	_loading_dots_timer += delta
	
	if _loading_dots_timer >= 0.2:
		_loading_dots_timer = 0.0
		_loading_dots_frame += 1
		
	var dots_frame = _loading_dots_frame % 4
	var dots = ""
	match dots_frame:
		0: dots = ".  "
		1: dots = ".. "
		2: dots = "..."
		3: dots = "   "
		
	var loading_title = loading_overlay.find_child("LoadingTitle")
	
	if _loading_phase == "connecting" and _loading_elapsed > 1.5:
		_loading_phase = "loading_model"
		
	if _loading_phase == "connecting":
		if loading_title:
			loading_title.text = "CONNECTING" + dots
		var current_model = LLMClient.world_builder_model if _generated_starters.is_empty() else LLMClient.character_model
		if current_model.is_empty():
			current_model = "Ollama Model"
		loading_text.text = "Connecting to server at %s\nModel: %s (num_ctx: 8192)\n\n%s\nStatus: Initializing HTTP handshake\nElapsed time: %.1fs" % [
			LLMClient.api_url,
			current_model,
			_get_moving_dot_bar(_loading_dots_frame),
			_loading_elapsed
		]
	elif _loading_phase == "loading_model":
		var current_model = LLMClient.world_builder_model if _generated_starters.is_empty() else LLMClient.character_model
		if current_model.is_empty():
			current_model = "Ollama Model"
		if _warmup_success and _background_compile_completed:
			if loading_title:
				loading_title.text = "ANALYZING CAMPAIGN" + dots
			loading_text.text = "Evaluating notes and preparing adventure hooks...\nModel: %s (num_ctx: 8192)\n\n%s\nStatus: Performing prompt evaluation (pre-processing)\nElapsed time: %.1fs" % [
				current_model,
				_get_moving_dot_bar(_loading_dots_frame),
				_loading_elapsed
			]
		else:
			if loading_title:
				loading_title.text = "WAKING UP LOCAL LLM" + dots
			loading_text.text = "Loading '%s' into system memory\nModel: %s (num_ctx: 8192)\n\n%s\nStatus: Allocating VRAM/RAM (can take up to a minute)\nElapsed time: %.1fs" % [
				current_model,
				current_model,
				_get_moving_dot_bar(_loading_dots_frame),
				_loading_elapsed
			]
	elif _loading_phase == "generating_pass1":
		var word_count = _hook_text_received.split(" ", false).size()
		var percentage = clampf((float(word_count) / 100.0) * 100.0, 0.0, 100.0)
		var ascii_bar = _get_ascii_progress_bar(word_count, 100)
		
		if loading_title:
			loading_title.text = "ANALYZING WORLD" + dots
		loading_text.text = "Structuring starting options with %s...\n\n%s %d%%\nStatus: Planning starters (Pass 1 of 2)\nWords generated: %d\nElapsed time: %.1fs" % [
			LLMClient.world_builder_model,
			ascii_bar,
			int(percentage),
			word_count,
			_loading_elapsed
		]
	elif _loading_phase == "generating_pass2":
		var word_count = _hook_text_received.split(" ", false).size()
		var percentage = clampf((float(word_count) / 200.0) * 100.0, 0.0, 100.0)
		var ascii_bar = _get_ascii_progress_bar(word_count, 200)
		
		if loading_title:
			loading_title.text = "WRITING NARRATIVE (%d/3)" % (_current_hook_idx + 1) + dots
		loading_text.text = "Drafting Hook %d of 3 with %s...\n\n%s %d%%\nStatus: Writing story hooks (Pass 2 of 2)\nWords generated: %d\nElapsed time: %.1fs" % [
			(_current_hook_idx + 1),
			LLMClient.character_model,
			ascii_bar,
			int(percentage),
			word_count,
			_loading_elapsed
		]

func _get_moving_dot_bar(frame: int) -> String:
	var total_dots = 8
	var active_index = frame % (total_dots * 2 - 2)
	var pos = active_index
	if active_index >= total_dots:
		pos = (total_dots * 2 - 2) - active_index
		
	var bar = "[  "
	for i in range(total_dots):
		if i == pos:
			bar += "● "
		else:
			bar += "◌ "
	bar += " ]"
	return bar

func _get_ascii_progress_bar(current: int, target: int) -> String:
	var bar_length = 20
	var filled = clampi(int(float(current) / target * bar_length), 0, bar_length)
	var empty = bar_length - filled
	var bar_str = "["
	for i in range(filled):
		bar_str += "■"
	for i in range(empty):
		bar_str += "░"
	bar_str += "]"
	return bar_str

# ==============================================================================
# Helper structures and campaign starters fallback
# ==============================================================================

func _build_connected_starting_clusters() -> Array:
	var clusters = []
	var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
	var edges = _compiled_data.get("knowledge_graph", {}).get("edges", [])
	
	var locations = []
	var characters = []
	var lore_and_scenes = []
	
	for node_id in nodes.keys():
		var node = nodes[node_id]
		var type = node.get("type", "")
		var info = {
			"id": node_id,
			"name": node.get("label", node_id),
			"desc": node.get("desc", ""),
			"properties": node.get("properties", {})
		}
		if type == "location":
			locations.append(info)
		elif type == "character":
			characters.append(info)
		elif type in ["scene", "lore"]:
			lore_and_scenes.append(info)
			
	var loc_connections = {}
	for loc in locations:
		loc_connections[loc.id] = {
			"characters": [],
			"lore": []
		}
		
	for edge in edges:
		var f = edge.get("from", "")
		var t = edge.get("to", "")
		
		var f_node = nodes.get(f)
		var t_node = nodes.get(t)
		if not f_node or not t_node:
			continue
			
		var f_type = f_node.get("type")
		var t_type = t_node.get("type")
		
		var add_conn = func(loc_id: String, other_id: String, other_type: String):
			if not loc_connections.has(loc_id):
				return
			var other_info = {
				"id": other_id,
				"name": nodes[other_id].get("label", other_id),
				"desc": nodes[other_id].get("desc", ""),
				"properties": nodes[other_id].get("properties", {})
			}
			if other_type == "character":
				var exists = false
				for c in loc_connections[loc_id].characters:
					if c.id == other_id:
						exists = true
						break
				if not exists:
					loc_connections[loc_id].characters.append(other_info)
			elif other_type in ["scene", "lore"]:
				var exists = false
				for l in loc_connections[loc_id].lore:
					if l.id == other_id:
						exists = true
						break
				if not exists:
					loc_connections[loc_id].lore.append(other_info)
					
		if f_type == "location" and t_type == "character":
			add_conn.call(f, t, "character")
		elif t_type == "location" and f_type == "character":
			add_conn.call(t, f, "character")
		elif f_type == "location" and t_type in ["scene", "lore"]:
			add_conn.call(f, t, "lore")
		elif t_type == "location" and f_type in ["scene", "lore"]:
			add_conn.call(t, f, "lore")
			
	var loc_candidates = locations.duplicate()
	loc_candidates.sort_custom(func(a, b):
		var a_conns = loc_connections[a.id].characters.size() + loc_connections[a.id].lore.size()
		var b_conns = loc_connections[b.id].characters.size() + loc_connections[b.id].lore.size()
		return a_conns > b_conns
	)
	
	var selected_locs = []
	for loc in loc_candidates:
		if loc_connections[loc.id].characters.size() > 0:
			selected_locs.append(loc)
		if selected_locs.size() == 3:
			break
			
	if selected_locs.size() < 3:
		for loc in loc_candidates:
			if not selected_locs.has(loc):
				selected_locs.append(loc)
			if selected_locs.size() == 3:
				break
				
	while selected_locs.size() < 3:
		if selected_locs.size() > 0:
			selected_locs.append(selected_locs[0].duplicate())
		else:
			selected_locs.append({"id": "", "name": "Unknown Location", "desc": "A mysterious starting point."})
			
	for loc in selected_locs:
		var loc_id = loc.id
		var conns = loc_connections.get(loc_id, {"characters": [], "lore": []})
		
		var candidate_chars = []
		for c in conns.characters:
			var node = nodes.get(c.id, {})
			var props = node.get("properties", {})
			if props.get("can_speak", true) and not props.get("is_creature", false):
				candidate_chars.append(c)
				
		if candidate_chars.is_empty():
			for c in characters:
				var node = nodes.get(c.id, {})
				var props = node.get("properties", {})
				if props.get("can_speak", true) and not props.get("is_creature", false):
					candidate_chars.append(c)
					
		if candidate_chars.is_empty():
			candidate_chars.append({"id": "companion", "name": "Companion", "desc": "A quiet traveler assisting you."})
			
		var primary_char = candidate_chars[0]
		
		var selected_lore = null
		if conns.lore.size() > 0:
			var lore_pool = conns.lore.duplicate()
			lore_pool.shuffle()
			selected_lore = lore_pool[0]
		else:
			if lore_and_scenes.size() > 0:
				var lore_pool = lore_and_scenes.duplicate()
				lore_pool.shuffle()
				selected_lore = lore_pool[0]
			else:
				selected_lore = {"id": "", "name": "", "desc": ""}
				
		clusters.append({
			"location": loc,
			"character": primary_char,
			"candidate_characters": candidate_chars,
			"lore": selected_lore
		})
		
	return clusters

func _generate_fallback_starters(clusters: Array) -> Array:
	var starters = []
	for i in range(3):
		var cluster = null
		if i < clusters.size():
			cluster = clusters[i]
		else:
			cluster = {
				"location": {"id": "", "name": "Unknown Location"},
				"character": {"id": "", "name": "Unknown Character"}
			}
			
		var loc = cluster.get("location", {"id": "", "name": "Unknown Location"})
		var ch = cluster.get("character", {"id": "", "name": "Unknown Character"})
		
		var title = ""
		var desc = ""
		var narration = ""
		
		match i:
			0:
				title = "A Whisper in the Shadows"
				desc = "A sudden, freezing breeze sweeps through %s. A quiet whisper calls out your name." % loc.name
				narration = "The air grows thin and cold as you arrive at %s. A quiet, raspy whisper echoes from the shadows, calling your name. Standing nearby, %s notices the sudden drop in temperature, their hand instinctively reaching for their weapon as they look around in quiet alarm." % [loc.name, ch.name]
			1:
				title = "The Burning Sigil"
				desc = "A flash of crimson light erupts in %s, carving a glowing, hot sigil into the stone." % loc.name
				narration = "A sharp hiss echoes through %s as a brilliant crimson sigil begins to glow on a nearby stone surface. The rock cracks under the sudden intense heat. %s stares at the burning mark, face pale in the red light, whispering that this is a sign they had hoped never to see again." % [loc.name, ch.name]
			2:
				title = "The Shattered Mirror"
				desc = "A loud crack splits the air in %s, leaving behind a shimmering rift that distorts reality." % loc.name
				narration = "A deafening sound like shattering glass reverberates through %s. In its wake, a vertical shimmer hangs in the air, refracting light like a warped lens. %s takes a step back, warning you that the fabric of this place has just fractured, and whatever is on the other side is starting to look back." % [loc.name, ch.name]
				
		starters.append({
			"title": title,
			"description": desc,
			"location_id": loc.id,
			"character_id": ch.id,
			"narration": narration
		})
	return starters

func _parse_json_response(raw_text: String) -> Dictionary:
	var result = {}
	var text = raw_text.strip_edges()
	
	var first_brace = text.find("{")
	var last_brace = text.rfind("}")
	if first_brace != -1 and last_brace != -1 and last_brace > first_brace:
		text = text.substr(first_brace, last_brace - first_brace + 1)
		
	var json = JSON.new()
	if json.parse(text) == OK:
		if json.data is Dictionary:
			result = json.data
	return result

func _on_review_next_pressed() -> void:
	var selected_idx = review_screen.get_selected_starter_index()
	if selected_idx == -1 or selected_idx >= _generated_starters.size():
		return
		
	var selected_hook = _generated_starters[selected_idx]
	var loc_id = selected_hook.get("location_id", "")
	var char_id = selected_hook.get("character_id", "")
	
	# Validate/fallback empty starting location/character IDs (Bug J)
	if loc_id.is_empty():
		var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
		for nid in nodes:
			if nodes[nid].get("type") == "location":
				loc_id = nid
				break
	if char_id.is_empty():
		var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
		for nid in nodes:
			if nodes[nid].get("type") == "character":
				char_id = nid
				break
				
	_custom_mappings["starting_location_id"] = loc_id
	_custom_mappings["starting_character_id"] = char_id
	_custom_mappings["intro_narration"] = selected_hook.get("narration", "")
	
	# Perform the character reorder in-place on _compiled_data (Bug A)
	if not char_id.is_empty() and _compiled_data.has("characters") and _compiled_data["characters"].has(char_id):
		var target_char = _compiled_data["characters"][char_id]
		_compiled_data["characters"].erase(char_id)
		var new_chars = {char_id: target_char}
		for k in _compiled_data["characters"].keys():
			new_chars[k] = _compiled_data["characters"][k]
		_compiled_data["characters"] = new_chars
		
	_custom_mappings["compiled_data"] = _compiled_data
	
	var title = _custom_mappings.get("campaign_title", "Custom Adventure")
	var campaign_id = _custom_mappings.get("campaign_id", "custom_adventure")
	var vault_path = _custom_mappings.get("vault_path", "")
	
	# Show a brief loading overlay for UX feedback (Bug A)
	loading_overlay.visible = true
	var loading_title = loading_overlay.find_child("LoadingTitle")
	if loading_title:
		loading_title.text = "STARTING ADVENTURE..."
	loading_text.text = "Preparing your world..."
	await get_tree().process_frame
	
	# Animate closing overlay
	var tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, ThemeManager.duration_slow).set_trans(ThemeManager.trans_default).set_ease(ThemeManager.ease_default)
	tween.tween_callback(func():
		adventure_started.emit(campaign_id, title, vault_path, _custom_mappings)
		queue_free()
	)

func _load_campaign(campaign_id: String) -> void:
	var tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, ThemeManager.duration_slow).set_trans(ThemeManager.trans_default).set_ease(ThemeManager.ease_default)
	tween.tween_callback(func():
		adventure_loaded.emit(campaign_id)
		queue_free()
	)

func _generate_sample_vault(path: String) -> void:
	if not DirAccess.dir_exists_absolute(path):
		DirAccess.make_dir_recursive_absolute(path)
		
	# 1. Elara Character File
	_write_file(path.path_join("elara_the_wise.md"), """---
type: character
name: Elara the Wise
base_affinity: 0.2
connections: [phandalin_town_square, crypt_of_shadows]
relationships:
  valen_the_ranger: Friend
writing_style: "Speaks with formal, archaic elven grace. Often uses terms like 'indeed', 'observe', or references ancient lore."
---
Elara is an elven wizard who spends her days studying the ancient ley lines of the valley. She is searching for a lost spellbook hidden within the Crypt of Shadows. She appears calm but has a fierce intellectual curiosity.
""")

	# 2. Valen Character File
	_write_file(path.path_join("valen_the_ranger.md"), """---
type: character
name: Valen the Ranger
base_affinity: 0.1
connections: [phandalin_town_square]
relationships:
  elara_the_wise: Companion
---
Valen is a gruff but loyal ranger who guards the borderlands of Phandalin. He knows the dangers of the forest and helps travelers navigate the mountain passes. He is highly suspicious of magic.

## Dialogue Style
- Speaks gruffly, using short sentences and contractions.
- E.g., "Watch your step." or "Magic's trouble."
""")

	# 3. Phandalin Location File
	_write_file(path.path_join("phandalin_town_square.md"), """---
type: location
name: Phandalin Town Square
connections: [crypt_of_shadows]
---
The bustling center of the frontier town of Phandalin. Merchants set up wooden stalls around a stone well, and town guards watch the road. A dusty track leads north toward the mountains.
""")

	# 4. Crypt Location File
	_write_file(path.path_join("crypt_of_shadows.md"), """---
type: location
name: Crypt of Shadows
connections: [phandalin_town_square]
---
A dark, cold stone structure half-buried in the mountainside. Sigils of protection are carved into the heavy iron-reinforced wooden door, though some have been defaced. A chill draft escapes from the gaps.
""")

	# 5. Intro Scene File
	_write_file(path.path_join("intro_the_journey_begins.md"), """---
type: scene
name: The Journey Begins
---
The wind howls through the mountain passes of Phandalin as you arrive at the town square. Standing near the stone well is Elara the Wise, her robes fluttering in the cold air. She looks at you with anticipation, hoping you can help her recover the lost spellbook.

Valen says, "Hmph. More magic nonsense. We should stay clear of that crypt."
""")

	# 6. Global Writing Style File
	_write_file(path.path_join("writing_style.md"), """---
type: lore
name: Campaign Writing Style
---
A dark fantasy prose style, with atmospheric, descriptive sentences. Pacing is slow and measured.
""")

func _write_file(file_path: String, content: String) -> void:
	var file = FileAccess.open(file_path, FileAccess.WRITE)
	if file:
		file.store_string(content)
		file.close()
