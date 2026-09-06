# res://src/ui/MainViewport.gd
extends Control
class_name MainViewport

const CharacterListItemScene = preload("res://scenes/ui/CharacterListItem.tscn")
const OnboardingFlowScene = preload("res://scenes/ui/OnboardingFlow.tscn")
const NearbyCharacterListScript = preload("res://src/ui/NearbyCharacterList.gd")
const LoadingSpinnerScript = preload("res://src/ui/LoadingSpinner.gd")

# Toast & Loading indicators
@onready var _toast_container: VBoxContainer = %ToastContainer
@onready var _loading_indicator: PanelContainer = %LoadingIndicator
@onready var _loading_label: Label = %LoadingIndicatorLabel
@onready var _loading_spinner: LoadingSpinner = %LoadingIndicatorSpinner
var _active_tasks: Dictionary = {}

# Subsystem Managers / References
var game_loop_controller: Node

# UI Display State
var _displayed_messages: Array[Dictionary] = []
var _stream_is_first_chunk: bool = true

# Sidebar collapse/expand animation state
var _is_sidebar_collapsed: bool = false
var _sidebar_tween: Tween

# Background image generation state
@onready var bg_texture_rect: TextureRect = %BackgroundTextureRect
@onready var bg_status_overlay: AssetStatusOverlay = %BgStatusOverlay
var _last_rendered_location: String = ""
var _bg_fade_tween: Tween


# Bound UI Nodes via @onready
@onready var dialogue_label: ScrollContainer = %DialogueLabel
@onready var input_field: LineEdit = %InputField
@onready var send_button: Button = %SendButton
@onready var snapshot_button: Button = %SnapshotButton
@onready var chat_settings_button: Button = %ChatSettingsButton
@onready var chat_character_sheet_button: Button = %ChatCharacterSheetButton
@onready var character_list_container: VBoxContainer = %CharacterListContainer
@onready var character_visuals_rect: CharacterVisuals = %CharacterVisuals
@onready var import_vault_button: Button = %ImportVaultButton
@onready var load_game_button: Button = %LoadGameButton
@onready var mind_map_button: Button = %MindMapButton
@onready var settings_button: Button = %SettingsButton
@onready var onboarding_flow: OnboardingFlow = %OnboardingFlow
@onready var sidebar_container: Control = %SidebarContainer
@onready var toggle_sidebar_button: Button = %ToggleSidebarButton
@onready var speaker_name_label: Label = %SpeakerNameLabel
@onready var short_term_memory_label: RichTextLabel = %ShortTermMemoryLabel
@onready var medium_term_memory_label: RichTextLabel = %MediumTermMemoryLabel
@onready var long_term_memory_label: RichTextLabel = %LongTermMemoryLabel
@onready var bg_color_rect: ColorRect = $BGColor

func _ready() -> void:
	# 1. Connect scene buttons
	import_vault_button.pressed.connect(_on_import_pressed)
	load_game_button.pressed.connect(_on_load_pressed)
	input_field.text_submitted.connect(_on_input_submitted)
	input_field.text_changed.connect(_on_input_text_changed)
	dialogue_label.meta_clicked.connect(_on_dialogue_meta_clicked)
	send_button.pressed.connect(_on_send_pressed)
	snapshot_button.pressed.connect(_on_snapshot_pressed)
	chat_settings_button.pressed.connect(_on_settings_pressed)
	chat_character_sheet_button.pressed.connect(_on_chat_character_sheet_pressed)
	toggle_sidebar_button.pressed.connect(_on_toggle_sidebar_pressed)
	settings_button.pressed.connect(_on_settings_pressed)
	mind_map_button.pressed.connect(_on_mind_map_pressed)
	
	ThemeManager.theme_changed.connect(_on_theme_changed)
	_on_theme_changed()
	
	# Set tooltips for icon/action buttons
	toggle_sidebar_button.tooltip_text = "Toggle Character & Logs Sidebar"
	settings_button.tooltip_text = "Open settings panel"
	mind_map_button.tooltip_text = "Open campaign connection mind map"
	import_vault_button.tooltip_text = "Import a new campaign vault folder"
	load_game_button.tooltip_text = "Load an existing save file"
	snapshot_button.tooltip_text = "Generate scenery snapshot"
	chat_settings_button.tooltip_text = "Open settings panel"
	chat_character_sheet_button.tooltip_text = "View active character details"
	
	dialogue_label.clear_messages()
	dialogue_label.scroll_following = true
	
	# 2. Instantiate logic controller
	game_loop_controller = GameLoopController.new()
	add_child(game_loop_controller)
	
	# 3. Connect controller signals
	game_loop_controller.campaign_started.connect(_on_controller_campaign_started)
	game_loop_controller.campaign_loaded.connect(_on_controller_campaign_loaded)
	game_loop_controller.system_message_logged.connect(_display_system_message)
	game_loop_controller.warning_message_logged.connect(_display_warning_message)
	game_loop_controller.error_message_logged.connect(_display_error_message)
	game_loop_controller.message_logged.connect(_on_controller_message_logged)
	game_loop_controller.stream_chunk_logged.connect(_on_controller_stream_chunk_logged)
	game_loop_controller.stream_started.connect(_on_controller_stream_started)
	game_loop_controller.stream_zone_ended.connect(_on_controller_stream_zone_ended)
	game_loop_controller.turn_state_changed.connect(_on_controller_turn_state_changed)
	game_loop_controller.active_character_changed.connect(_on_controller_active_character_changed)
	game_loop_controller.character_visual_update_requested.connect(_on_controller_character_visual_update_requested)
	game_loop_controller.character_reaction_requested.connect(_on_controller_character_reaction_requested)
	game_loop_controller.sidebar_refresh_requested.connect(_refresh_character_list)
	game_loop_controller.memory_updated.connect(_update_memory_ui)
	game_loop_controller.input_disabled_changed.connect(_set_input_disabled)
	
	# 4. Setup NearbyCharacterList UI node
	character_list_container.setup(game_loop_controller.graph_manager)
	character_list_container.character_selected.connect(_on_character_selected)
	
	# 5. Connect global Autoload signals
	EventBus.emotion_updated.connect(_on_character_emotion_updated)
	EventBus.location_changed.connect(_on_controller_background_update_requested)
	
	# Initialize Toast & Loading systems
	_init_loading_indicator()
	LLMClient.request_failed.connect(_on_llm_request_failed)
	ImageGenManager.asset_generated.connect(_on_background_generated)
	
	# 6. Initialize Onboarding Flow and hide sidebar initially
	sidebar_container.visible = false
	if onboarding_flow:
		onboarding_flow.adventure_started.connect(game_loop_controller.start_new_campaign)
		onboarding_flow.adventure_loaded.connect(game_loop_controller.load_existing_campaign)
	
	# Default message
	_display_system_message("Welcome to Orison! Import an Obsidian vault folder to begin your adventure.")
	
	_ensure_focus_mode(self)
	
	print("[SYSTEM] MainViewport loaded. Renderer: ", ProjectSettings.get_setting("rendering/renderer/rendering_method"))
 
func _exit_tree() -> void:
	# Disconnect all autoload signals to prevent memory leaks
	if ThemeManager.theme_changed.is_connected(_on_theme_changed):
		ThemeManager.theme_changed.disconnect(_on_theme_changed)
	if EventBus.emotion_updated.is_connected(_on_character_emotion_updated):
		EventBus.emotion_updated.disconnect(_on_character_emotion_updated)
	if EventBus.location_changed.is_connected(_on_controller_background_update_requested):
		EventBus.location_changed.disconnect(_on_controller_background_update_requested)
	if ImageGenManager.asset_generated.is_connected(_on_background_generated):
		ImageGenManager.asset_generated.disconnect(_on_background_generated)

# ==============================================================================
# UI Interaction Flow
# ==============================================================================

func send_player_input(input_text: String) -> void:
	_fade_out_background()
	if character_visuals_rect:
		character_visuals_rect.fade_in()
	game_loop_controller.send_player_input(input_text)

# ==============================================================================
# UI Update Receivers (from GameLoopController signals)
# ==============================================================================

func _on_controller_campaign_started(title: String, active_location_id: String) -> void:
	_displayed_messages.clear()
	_is_sidebar_collapsed = false
	sidebar_container.offset_left = -350.0
	sidebar_container.offset_right = 0.0
	toggle_sidebar_button.text = "⟫"
	sidebar_container.visible = true
	_update_memory_ui()
	_refresh_character_list()

func _on_controller_campaign_loaded(campaign_id: String) -> void:
	_is_sidebar_collapsed = false
	sidebar_container.offset_left = -350.0
	sidebar_container.offset_right = 0.0
	toggle_sidebar_button.text = "⟫"
	sidebar_container.visible = true
	_update_memory_ui()
	_refresh_character_list()
	_populate_chat_from_history()

func _on_controller_message_logged(sender: String, message: String) -> void:
	_append_message(sender, message, "chat")

func _on_controller_stream_started(sender_id: String) -> void:
	_remove_last_system_message()
	_stream_is_first_chunk = true
	
	# Add empty streaming message entry
	_append_message(sender_id, "", "chat", true)

func _on_controller_stream_chunk_logged(sender: String, word: String) -> void:
	if _stream_is_first_chunk:
		_stream_is_first_chunk = false
		var sender_id = "narrator" if game_loop_controller.stream_parser.stream_zone == "narration" else sender
		var friendly_name = "Narrator"
		if sender_id != "narrator":
			friendly_name = sender_id.capitalize()
			var character = CampaignState.get_character(sender_id)
			if not character.is_empty():
				friendly_name = character.get("name", sender_id.capitalize())
		speaker_name_label.text = friendly_name
		_update_nameplate_color(sender_id)
		
	if not _displayed_messages.is_empty():
		_displayed_messages[-1]["text"] += word
		_rebuild_dialogue_text()

func _on_controller_stream_zone_ended() -> void:
	_stream_is_first_chunk = true

func _on_controller_turn_state_changed(state: int) -> void:
	if state == GameLoopController.TurnState.WORLD_BUILDER_THINKING:
		register_task("llm_thinking", "World Builder is thinking...")
	elif state == GameLoopController.TurnState.CHARACTER_THINKING:
		var char_name = "Character"
		if game_loop_controller:
			var character = CampaignState.get_character(game_loop_controller.active_character_id)
			if not character.is_empty():
				char_name = character.get("name", char_name)
		register_task("llm_thinking", "%s is thinking..." % char_name)
	else:
		unregister_task("llm_thinking")
		_remove_last_system_message()
		# Clean up any trailing empty message
		if not _displayed_messages.is_empty():
			var last_msg = _displayed_messages[-1]
			if last_msg.get("text", "").strip_edges().is_empty() and last_msg.get("type") == "chat":
				_displayed_messages.remove_at(_displayed_messages.size() - 1)
				_rebuild_dialogue_text()
				
	if state == GameLoopController.TurnState.WORLD_BUILDER_THINKING or state == GameLoopController.TurnState.CHARACTER_THINKING:
		var already_thinking = false
		for msg in _displayed_messages:
			if msg.get("is_temporary", false) and msg.get("text") == "Thinking...":
				already_thinking = true
				break
		if not already_thinking:
			_append_message("system", "Thinking...", "system", true)
	else:
		_remove_last_system_message()

func _on_controller_active_character_changed(char_id: String) -> void:
	# _fade_out_background()
	character_visuals_rect.load_character(char_id)
	_update_memory_ui()

func _on_controller_background_update_requested(active_location: String) -> void:
	if active_location == _last_rendered_location:
		return
		
	_last_rendered_location = active_location
	
	var loc_node = game_loop_controller.graph_manager.get_node(active_location)
	if not loc_node:
		return
		
	# Determine target path and setup status overlay
	var target_path = ImageGenManager.get_scene_path(active_location)
	if bg_status_overlay:
		bg_status_overlay.setup(target_path)
		
	# Try loading existing image — does NOT trigger generation
	var tex = await ImageGenManager.get_image_or_fallback(active_location, "scene")
	if tex:
		_fade_in_background(tex)
	else:
		if bg_texture_rect:
			bg_texture_rect.texture = null
	
	# If a generation is already underway (e.g. triggered manually), show task indicator
	if LLMClient.image_gen_enabled:
		var state = ImageGenManager.get_asset_state(target_path)
		if state.status == "generating":
			register_task("image_gen", "Generating background artwork...")


func _on_controller_character_visual_update_requested(char_id: String, emotion: String, affinity: float) -> void:
	if char_id == game_loop_controller.active_character_id:
		# _fade_out_background()
		character_visuals_rect.apply_emotion(emotion, affinity)

func _on_controller_character_reaction_requested(char_id: String, emotion: String) -> void:
	if char_id == game_loop_controller.active_character_id:
		character_visuals_rect.generate_physical_reaction(char_id, emotion)

# ==============================================================================
# Helper Methods
# ==============================================================================

func _set_input_disabled(disabled: bool) -> void:
	var can_cancel = false
	if game_loop_controller:
		var state = game_loop_controller.get_current_turn_state()
		if state == GameLoopController.TurnState.CHARACTER_THINKING or state == GameLoopController.TurnState.WORLD_BUILDER_THINKING:
			can_cancel = true
			
	input_field.editable = (not disabled) or can_cancel
	send_button.disabled = disabled

func _append_message(sender: String, text: String, type: String = "chat", is_temporary: bool = false) -> void:
	_displayed_messages.append({
		"sender": sender,
		"text": text,
		"type": type,
		"is_temporary": is_temporary
	})
	_rebuild_dialogue_text()

func _remove_last_system_message() -> void:
	# Filter out temporary/thinking messages
	for i in range(_displayed_messages.size() - 1, -1, -1):
		if _displayed_messages[i].get("is_temporary", false):
			_displayed_messages.remove_at(i)
	_rebuild_dialogue_text()

func _rebuild_dialogue_text() -> void:
	if not dialogue_label:
		return
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	dialogue_label.set_messages(_displayed_messages, is_light)

func _populate_chat_from_history() -> void:
	_displayed_messages.clear()
	
	var intro_narration = CampaignState.get_campaign_meta("intro_narration", "")
	if intro_narration.is_empty():
		var logs = CampaignState.state.get("history_logs", [])
		for log_entry in logs:
			if log_entry.get("role") == "assistant" and log_entry.get("sender") == "narrator":
				intro_narration = log_entry.content
				CampaignState.set_campaign_meta("intro_narration", intro_narration)
				CampaignState.save()
				break
		if intro_narration.is_empty() and not logs.is_empty():
			intro_narration = logs[0].content
			CampaignState.set_campaign_meta("intro_narration", intro_narration)
			CampaignState.save()
			
	var history = CampaignState.get_recent_history(5)
	if not history.is_empty():
		var first_entry_is_intro = false
		if not intro_narration.is_empty():
			var first_entry = history[0]
			if first_entry.content == intro_narration:
				first_entry_is_intro = true
				
		if not intro_narration.is_empty() and not first_entry_is_intro:
			_append_message("narrator", intro_narration, "chat")
			
			var logs = CampaignState.state.get("history_logs", [])
			var intro_index = -1
			for i in range(logs.size()):
				if logs[i].content == intro_narration:
					intro_index = i
					break
			
			var history_start_index = -1
			for i in range(logs.size()):
				if logs[i].content == history[0].content and logs[i].timestamp == history[0].timestamp:
					history_start_index = i
					break
					
			if intro_index != -1 and history_start_index != -1 and history_start_index > intro_index + 1:
				_append_message("system", "─── Earlier Messages Omitted ───", "system")
				
		for entry in history:
			var sender = entry.get("sender", entry.role)
			_append_message(sender, entry.content, "chat")
	else:
		_append_message("system", "Loaded campaign '" + CampaignState.get_campaign_meta("title") + "'. Ready.", "system")

func _update_nameplate_color(sender: String) -> void:
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	var target_color = Color(_get_adjusted_sender_color(sender, is_light))
	speaker_name_label.add_theme_color_override("font_color", target_color)

func _get_adjusted_sender_color(sender: String, is_light: bool) -> String:
	if sender == "user" or sender == "player":
		return "#EA580C" if is_light else "#FF5F38"
	elif sender == "system":
		return "#4B5563" if is_light else "#A59EBF"
	elif sender == "narrator":
		return "#4A3F35" if is_light else "#FFF8F2"
	else:
		var character = CampaignState.get_character(sender)
		if not character.is_empty():
			var emotions = character.get("emotions", [])
			var last_emotion = emotions[-1].get("emotion", "serenity") if not emotions.is_empty() else "serenity"
			return _get_emotion_hex_color(last_emotion, is_light)
		return "#BE123C" if is_light else "#F43F5E"

func _get_emotion_hex_color(emotion: String, is_light: bool) -> String:
	match emotion.to_lower():
		"joy": return "#D97706" if is_light else "#F59E0B"
		"anger": return "#B91C1C" if is_light else "#DC2626"
		"sadness": return "#1D4ED8" if is_light else "#3B82F6"
		"fear": return "#6D28D9" if is_light else "#7C3AED"
		"trust": return "#047857" if is_light else "#059669"
		"disgust": return "#4D7C0F" if is_light else "#65A30D"
		"surprise": return "#0891B2" if is_light else "#06B6D4"
		"serenity": return "#4B5563" if is_light else "#D1D5DB"
		_: return "#BE123C" if is_light else "#F43F5E"

func _display_system_message(msg: String) -> void:
	print("[SYSTEM] ", msg)
	_append_message("system", msg, "system")

func _display_warning_message(msg: String) -> void:
	push_warning(msg)
	_append_message("system", msg, "warning")

func _display_error_message(msg: String) -> void:
	push_error(msg)
	_append_message("system", msg, "error")
	
	var clean_msg = msg
	if msg.begins_with("Error from LLM client: "):
		clean_msg = msg.replace("Error from LLM client: ", "")
		
	if "cancelled" in clean_msg.to_lower():
		return
		
	var friendly_msg = clean_msg
	if "connect" in clean_msg.to_lower() or "status: 0" in clean_msg.to_lower() or "cant_connect" in clean_msg.to_lower():
		friendly_msg = "Ollama offline on port 11434. Make sure Ollama is running."
	elif "404" in clean_msg or "not found" in clean_msg.to_lower():
		friendly_msg = "LLM model not found. Check model configuration."
	elif "stalled" in clean_msg.to_lower() or "timeout" in clean_msg.to_lower() or "interrupted" in clean_msg.to_lower():
		friendly_msg = "LLM stream connection was interrupted."
		
	show_toast(friendly_msg, true)

func _refresh_character_list() -> void:
	if game_loop_controller.is_initializing:
		return
	character_list_container.refresh(game_loop_controller.active_character_id)

func _on_character_emotion_updated(char_id: String, emotion: String, affinity: float) -> void:
	if char_id == game_loop_controller.active_character_id:
		var current_emotion = emotion
		if current_emotion.is_empty():
			var character = CampaignState.get_character(char_id)
			var emotions = character.get("emotions", [])
			current_emotion = emotions[-1].get("emotion", "serenity") if not emotions.is_empty() else "serenity"
			
		# _fade_out_background()
		character_visuals_rect.apply_emotion(current_emotion, affinity)
		
	_refresh_character_list()

func _on_input_submitted(new_text: String) -> void:
	input_field.text = ""
	send_player_input(new_text)

func _on_send_pressed() -> void:
	var text = input_field.text
	input_field.text = ""
	send_player_input(text)

func _on_import_pressed() -> void:
	_open_onboarding_to("setup")

func _on_load_pressed() -> void:
	_open_onboarding_to("load")

func _open_onboarding_to(screen: String) -> void:
	var flow = get_node_or_null("%OnboardingFlow")
	if not flow:
		flow = OnboardingFlowScene.instantiate()
		add_child(flow)
		flow.adventure_started.connect(game_loop_controller.start_new_campaign)
		flow.adventure_loaded.connect(game_loop_controller.load_existing_campaign)
		
	sidebar_container.visible = false
	flow.show_screen(screen)

func _on_toggle_sidebar_pressed() -> void:
	if _sidebar_tween and _sidebar_tween.is_valid():
		_sidebar_tween.kill()
		
	_sidebar_tween = create_tween().set_parallel(true)
	
	var target_offset_left: float
	var target_offset_right: float
	var button_text: String
	
	if _is_sidebar_collapsed:
		target_offset_left = -350.0
		target_offset_right = 0.0
		button_text = "⟫"
		_is_sidebar_collapsed = false
	else:
		target_offset_left = -30.0
		target_offset_right = 320.0
		button_text = "⟪"
		_is_sidebar_collapsed = true
		
	_sidebar_tween.tween_property(sidebar_container, "offset_left", target_offset_left, ThemeManager.duration_normal).set_trans(ThemeManager.trans_default).set_ease(ThemeManager.ease_default)
	_sidebar_tween.tween_property(sidebar_container, "offset_right", target_offset_right, ThemeManager.duration_normal).set_trans(ThemeManager.trans_default).set_ease(ThemeManager.ease_default)
	
	toggle_sidebar_button.text = button_text

func _update_memory_ui() -> void:
	var memory = CampaignState.state.get("memory", {})
	
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	var green_color = "#047857" if is_light else "#A7F3D0"
	var yellow_color = "#B45309" if is_light else "#FDE68A"
	var red_color = "#B91C1C" if is_light else "#F87171"
	
	if short_term_memory_label:
		short_term_memory_label.text = "[color=%s][b]Short-Term:[/b][/color] " % green_color + memory.get("short_term", "None")
		
	if medium_term_memory_label:
		var mt_text = "[color=%s][b]Medium-Term:[/b][/color] " % yellow_color + memory.get("medium_term", "None")
		if game_loop_controller and not game_loop_controller.active_character_id.is_empty():
			var character = CampaignState.get_character(game_loop_controller.active_character_id)
			if not character.is_empty():
				var char_name = character.get("name", game_loop_controller.active_character_id.capitalize())
				var char_mt_list = character.get("medium_term_memories", [])
				if not char_mt_list.is_empty():
					mt_text += " | [color=%s][b]%s Memories:[/b][/color]" % [yellow_color, char_name]
					for mt in char_mt_list:
						mt_text += " %s;" % mt
		medium_term_memory_label.text = mt_text
		
	if long_term_memory_label:
		var lt_text = "[color=%s][b]Long-Term:[/b][/color] " % red_color + memory.get("long_term", "None")
		if game_loop_controller and not game_loop_controller.active_character_id.is_empty():
			var character = CampaignState.get_character(game_loop_controller.active_character_id)
			if not character.is_empty():
				var char_name = character.get("name", game_loop_controller.active_character_id.capitalize())
				var char_lt = character.get("long_term_memory", "")
				if not char_lt.is_empty():
					lt_text += " | [color=%s][b]%s LTM:[/b][/color] %s" % [red_color, char_name, char_lt]
		long_term_memory_label.text = lt_text

func _on_settings_pressed() -> void:
	var modal_scene = load("res://scenes/ui/SettingsModal.tscn")
	if modal_scene:
		var modal = modal_scene.instantiate()
		modal.theme = ThemeManager.active_theme
		add_child(modal)

func _on_mind_map_pressed() -> void:
	var modal_scene = load("res://scenes/ui/MindMapModal.tscn")
	if modal_scene:
		var modal = modal_scene.instantiate()
		modal.theme = ThemeManager.active_theme
		add_child(modal)
		modal.closed.connect(func():
			_refresh_character_list()
		)

func _on_snapshot_pressed() -> void:
	if _last_rendered_location.is_empty():
		show_toast("No active location to snapshot.", true)
		return
		
	var active_location = _last_rendered_location
	print("[MainViewport] Snapshot requested for location: ", active_location)
	
	register_task("image_gen", "Generating background artwork...")
	ImageGenManager.generate_scene_background(active_location)

func _on_theme_changed() -> void:
	self.theme = ThemeManager.active_theme
	if bg_color_rect:
		bg_color_rect.color = ThemeManager.color_bg
	_rebuild_dialogue_text()
	_update_memory_ui()

func _on_background_generated(output_path: String, is_placeholder: bool) -> void:
	unregister_task("image_gen")
	var campaign_id = CampaignState.state.get("adventure_meta", {}).get("campaign_id", "default")
	var filename = _last_rendered_location.to_lower().replace(" ", "_")
	var expected_path = "user://adventures/%s/generated_assets/scene_%s.png" % [campaign_id, filename]
	
	if output_path == expected_path or output_path.get_file() == expected_path.get_file():
		var state = ImageGenManager.get_asset_state(output_path)
		if state.status == "success":
			ImageGenManager.invalidate_cache(_last_rendered_location, "scene")
			var tex = await ImageGenManager.get_image_or_fallback(_last_rendered_location, "scene")
			if tex:
				_fade_in_background(tex)
		else:
			if bg_texture_rect:
				bg_texture_rect.texture = null

func _fade_in_background(new_texture: Texture2D) -> void:
	if not bg_texture_rect:
		return
		
	# if character_visuals_rect:
	# 	character_visuals_rect.fade_out()
		
	if _bg_fade_tween and _bg_fade_tween.is_valid():
		_bg_fade_tween.kill()
		
	_bg_fade_tween = create_tween()
	_bg_fade_tween.tween_property(bg_texture_rect, "modulate:a", 0.0, ThemeManager.duration_normal)\
		.set_trans(ThemeManager.trans_default)\
		.set_ease(ThemeManager.ease_default)
	_bg_fade_tween.tween_callback(func():
		bg_texture_rect.texture = new_texture
	)
	_bg_fade_tween.tween_property(bg_texture_rect, "modulate:a", 1.0, ThemeManager.duration_slow)\
		.set_trans(ThemeManager.trans_default)\
		.set_ease(ThemeManager.ease_default)

func _on_input_text_changed(new_text: String) -> void:
	if new_text.is_empty():
		return
	if game_loop_controller:
		var state = game_loop_controller.get_current_turn_state()
		if state == GameLoopController.TurnState.CHARACTER_THINKING or state == GameLoopController.TurnState.WORLD_BUILDER_THINKING:
			print("[MainViewport] Player started typing, cancelling active LLM stream request.")
			LLMClient.cancel()

func _on_dialogue_meta_clicked(meta) -> void:
	if str(meta) == "retry":
		print("[MainViewport] User clicked retry.")
		if game_loop_controller:
			for i in range(_displayed_messages.size() - 1, -1, -1):
				var msg = _displayed_messages[i]
				if msg.get("type") == "error" and "unable to format" in msg.get("text", ""):
					_displayed_messages.remove_at(i)
					break
			_rebuild_dialogue_text()
			game_loop_controller.retry_last_input()

func _fade_out_background(duration: float = ThemeManager.duration_normal) -> void:
	if not bg_texture_rect:
		return
	if bg_texture_rect.modulate.a == 0.0:
		return
		
	if _bg_fade_tween and _bg_fade_tween.is_valid():
		_bg_fade_tween.kill()
		
	_bg_fade_tween = create_tween()
	_bg_fade_tween.tween_property(bg_texture_rect, "modulate:a", 0.0, duration)\
		.set_trans(ThemeManager.trans_default)\
		.set_ease(ThemeManager.ease_default)

# ==============================================================================
# Toast & Loading Indicator Helpers & Callbacks
# ==============================================================================

func _init_loading_indicator() -> void:
	var sb = StyleBoxFlat.new()
	sb.bg_color = Color(0.05, 0.04, 0.08, 0.75)
	sb.border_width_left = 1
	sb.border_width_right = 1
	sb.border_width_top = 1
	sb.border_width_bottom = 1
	sb.border_color = Color(0.4, 0.3, 0.5, 0.3)
	sb.corner_radius_top_left = ThemeManager.radius_lg
	sb.corner_radius_top_right = ThemeManager.radius_lg
	sb.corner_radius_bottom_left = ThemeManager.radius_lg
	sb.corner_radius_bottom_right = ThemeManager.radius_lg
	sb.content_margin_left = ThemeManager.spacing_sm
	sb.content_margin_right = ThemeManager.spacing_md
	sb.content_margin_top = ThemeManager.spacing_sm
	sb.content_margin_bottom = ThemeManager.spacing_sm
	_loading_indicator.add_theme_stylebox_override("panel", sb)
	
	_loading_spinner.radius = ThemeManager.radius_md
	_loading_spinner.line_width = 2.0
	_loading_spinner.custom_minimum_size = Vector2(20, 20)

func show_toast(message: String, is_error: bool = false, duration: float = 4.0) -> void:
	if not _toast_container:
		return
		
	var toast_scene = preload("res://scenes/ui/ToastMessage.tscn")
	var toast = toast_scene.instantiate()
	_toast_container.add_child(toast)
	toast.setup(message, is_error)
	
	var tween = create_tween().set_parallel(true)
	tween.tween_property(toast, "modulate:a", 1.0, ThemeManager.duration_normal)
	
	toast.position.x += 50
	tween.tween_property(toast, "position:x", toast.position.x - 50, ThemeManager.duration_normal).set_trans(ThemeManager.trans_default).set_ease(ThemeManager.ease_default)
	
	get_tree().create_timer(duration).timeout.connect(func():
		if is_instance_valid(toast):
			var fade_tween = create_tween()
			fade_tween.tween_property(toast, "modulate:a", 0.0, ThemeManager.duration_normal)
			fade_tween.finished.connect(func():
				if is_instance_valid(toast):
					toast.queue_free()
			)
	)

func register_task(task_id: String, task_name: String) -> void:
	_active_tasks[task_id] = task_name
	_update_loading_indicator()

func unregister_task(task_id: String) -> void:
	_active_tasks.erase(task_id)
	_update_loading_indicator()

func _update_loading_indicator() -> void:
	if not _loading_indicator or not _loading_label:
		return
		
	if _active_tasks.is_empty():
		if _loading_indicator.visible:
			var tween = create_tween()
			tween.tween_property(_loading_indicator, "modulate:a", 0.0, ThemeManager.duration_normal)
			tween.finished.connect(func():
				if _active_tasks.is_empty() and is_instance_valid(_loading_indicator):
					_loading_indicator.visible = false
			)
	else:
		var task_keys = _active_tasks.keys()
		_loading_label.text = _active_tasks[task_keys[-1]]
		
		if not _loading_indicator.visible:
			_loading_indicator.visible = true
			_loading_indicator.modulate.a = 0.0
			var tween = create_tween()
			tween.tween_property(_loading_indicator, "modulate:a", 1.0, ThemeManager.duration_normal)

func _on_llm_request_failed(err_msg: String) -> void:
	var friendly_msg = err_msg
	if "connect" in err_msg.to_lower() or "unreachable" in err_msg.to_lower():
		friendly_msg = "Ollama offline on port 11434. Make sure Ollama is running."
	elif "404" in err_msg or "not found" in err_msg.to_lower():
		friendly_msg = "Model not found. Verify model name."
	elif "cancel" in err_msg.to_lower():
		return
	show_toast(friendly_msg, true)

func _unhandled_input(event: InputEvent) -> void:
	if not event is InputEventKey or not event.pressed:
		return
		
	var is_cmd_or_ctrl = event.is_command_or_control_pressed()
	
	# 1. Escape: Close active modal / toggle settings
	if event.keycode == KEY_ESCAPE:
		get_viewport().set_input_as_handled()
		_close_or_toggle_settings()
		return
		
	# 2. Ctrl+S / Cmd+S: Quick save
	if is_cmd_or_ctrl and event.keycode == KEY_S and not event.shift_pressed:
		get_viewport().set_input_as_handled()
		_quick_save()
		return
		
	# 3. Ctrl+Shift+S: Save as
	if is_cmd_or_ctrl and event.keycode == KEY_S and event.shift_pressed:
		get_viewport().set_input_as_handled()
		_open_save_as()
		return
		
	# 4. Ctrl+M / Cmd+M: Toggle mind map
	if is_cmd_or_ctrl and event.keycode == KEY_M:
		get_viewport().set_input_as_handled()
		_on_mind_map_pressed()
		return
		
	# 5. / or Enter: Focus chat input
	if (event.keycode == KEY_SLASH or event.keycode == KEY_ENTER or event.keycode == KEY_KP_ENTER) and not is_cmd_or_ctrl:
		var focus_owner = get_viewport().get_focus_owner()
		if focus_owner != input_field:
			get_viewport().set_input_as_handled()
			input_field.grab_focus()
			if event.keycode == KEY_SLASH:
				input_field.text = ""
		return
		
	# 6. Tab / Shift+Tab: Navigate sidebar character list
	if event.keycode == KEY_TAB and not is_cmd_or_ctrl:
		get_viewport().set_input_as_handled()
		_navigate_character_list(event.shift_pressed)
		return

func _on_character_selected(char_id: String) -> void:
	var modal_scene = load("res://scenes/ui/CharacterDetailModal.tscn")
	if modal_scene:
		var modal = modal_scene.instantiate() as CharacterDetailModal
		modal.theme = ThemeManager.active_theme
		add_child(modal)
		modal.initialize(char_id)
		modal.talk_requested.connect(func(id: String):
			game_loop_controller.select_character(id)
		)

func _ensure_focus_mode(node: Node) -> void:
	if node is Button or node is LineEdit or node is TextEdit or node is OptionButton or node is TabContainer:
		if node.focus_mode == Control.FOCUS_NONE:
			node.focus_mode = Control.FOCUS_ALL
	for child in node.get_children():
		_ensure_focus_mode(child)

func _close_or_toggle_settings() -> void:
	# Close topmost modal with _on_close_pressed
	var children = get_children()
	for i in range(children.size() - 1, -1, -1):
		var child = children[i]
		if child.name.ends_with("Modal") or child is SettingsModal or child is MindMapModal or child is CharacterDetailModal or child is SaveAsModal:
			if child.has_method("_on_close_pressed"):
				child._on_close_pressed()
				return
			elif child.has_method("close"):
				child.close()
				return
				
	# If no modals open, open settings
	_on_settings_pressed()

func _quick_save() -> void:
	if CampaignState.campaign_id.is_empty():
		show_toast("No active campaign to save.", true)
		return
	var err = CampaignState.save()
	if err == OK:
		show_toast("Campaign quick saved successfully!")
	else:
		show_toast("Failed to save campaign: Error %d" % err, true)

func _open_save_as() -> void:
	if CampaignState.campaign_id.is_empty():
		show_toast("No active campaign to save as.", true)
		return
		
	var modal_scene = load("res://scenes/ui/SaveAsModal.tscn")
	if modal_scene:
		var modal = modal_scene.instantiate() as SaveAsModal
		modal.theme = ThemeManager.active_theme
		add_child(modal)
		modal.saved.connect(func(new_id: String):
			show_toast("Duplicated campaign saved as: " + new_id)
			_refresh_character_list()
		)

func _navigate_character_list(reverse: bool = false) -> void:
	var items = []
	for child in character_list_container.get_children():
		if child is CharacterListItem:
			items.append(child)
	if items.is_empty():
		return
		
	var current_focus = get_viewport().get_focus_owner()
	var current_idx = items.find(current_focus)
	
	if current_idx == -1:
		items[0].grab_focus()
	else:
		var next_idx = (current_idx + (-1 if reverse else 1)) % items.size()
		if next_idx < 0:
			next_idx += items.size()
		items[next_idx].grab_focus()

func _on_chat_character_sheet_pressed() -> void:
	if game_loop_controller and not game_loop_controller.active_character_id.is_empty():
		_on_character_selected(game_loop_controller.active_character_id)
	else:
		show_toast("No active character to view details.", true)
