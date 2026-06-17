# res://src/ui/MainViewport.gd
extends Control
class_name MainViewport

const CharacterListItemScene = preload("res://scenes/ui/CharacterListItem.tscn")
const OnboardingFlowScene = preload("res://scenes/ui/OnboardingFlow.tscn")

# Subsystem Managers (Local to view or instantiated helpers)
var graph_manager: KnowledgeGraphManager
var emotion_prompt_builder: EmotionPromptBuilder
var prompt_builder: PromptBuilder
var emotion_engine: EmotionEngine

# Turn Phase state machine
enum TurnState {
	IDLE,
	WORLD_BUILDER_THINKING,
	CHARACTER_THINKING
}
var _current_turn_state: TurnState = TurnState.IDLE
var _last_player_input: String = ""

# Dynamic State
var active_character_id: String = ""
var _is_generating_beginning: bool = false
var _fallback_beginning_text: String = ""
var _beginning_scene_title: String = ""

# Background Director & Streaming State
var _is_director_running: bool = false
var _stream_sender: String = ""
var _stream_is_first_chunk: bool = true
var _stream_in_dialogue_zone: bool = false
var _stream_dialogue_buffer: String = ""

# Sidebar collapse/expand animation state
var _is_sidebar_collapsed: bool = false
var _sidebar_tween: Tween

# Bound UI Nodes via @onready
@onready var dialogue_label: RichTextLabel = %DialogueLabel
@onready var input_field: LineEdit = %InputField
@onready var send_button: Button = %SendButton
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
	send_button.pressed.connect(_on_send_pressed)
	toggle_sidebar_button.pressed.connect(_on_toggle_sidebar_pressed)
	settings_button.pressed.connect(_on_settings_pressed)
	mind_map_button.pressed.connect(_on_mind_map_pressed)
	
	ThemeManager.theme_changed.connect(_on_theme_changed)
	_on_theme_changed()
	
	dialogue_label.text = ""
	dialogue_label.scroll_following = true
	
	# 2. Instantiate core local helpers
	graph_manager = KnowledgeGraphManager.new()
	emotion_prompt_builder = EmotionPromptBuilder.new()
	prompt_builder = PromptBuilder.new(graph_manager, emotion_prompt_builder)
	emotion_engine = EmotionEngine.new()
	
	# 3. Connect global Autoload signals
	LLMClient.response_received.connect(_on_ai_response_received)
	LLMClient.request_failed.connect(_on_ai_request_failed)
	LLMClient.response_chunk_received.connect(_on_ai_response_chunk_received)
	EventBus.emotion_updated.connect(_on_character_emotion_updated)
	
	# 4. Initialize Onboarding Flow and hide sidebar initially
	sidebar_container.visible = false
	if onboarding_flow:
		onboarding_flow.adventure_started.connect(start_new_campaign)
		onboarding_flow.adventure_loaded.connect(load_existing_campaign)
	
	# Default message
	_display_system_message("Welcome to Orison! Import an Obsidian vault folder to begin your adventure.")
	print("[SYSTEM] MainViewport loaded. Renderer: ", ProjectSettings.get_setting("rendering/renderer/rendering_method"))

# ==============================================================================
# Game Setup & Import APIs
# ==============================================================================

func start_new_campaign(campaign_id: String, title: String, vault_path: String, custom_mappings: Dictionary = {}) -> void:
	dialogue_label.text = ""
	_display_system_message("Compiling Vault: " + vault_path + "...")
	_is_sidebar_collapsed = false
	sidebar_container.offset_left = -350.0
	sidebar_container.offset_right = 0.0
	toggle_sidebar_button.text = "⟫"
	sidebar_container.visible = true
	
	# 1. Compile Markdown vault directory or use pre-compiled data from onboarding
	var compiled
	if custom_mappings.has("compiled_data") and custom_mappings["compiled_data"] != null:
		compiled = custom_mappings["compiled_data"]
		print("[MainViewport] Using pre-compiled campaign data from onboarding mind map.")
	else:
		compiled = VaultCompiler.compile_vault(vault_path, custom_mappings)
	
	# 2. Initialize new Campaign JSON save document
	var initial_state = SaveManager.create_campaign(campaign_id, title)
	if initial_state.is_empty():
		_display_error_message("Failed to create save document.")
		return
		
	# 3. Instantiate dynamic state data
	CampaignState.initialize(campaign_id, initial_state)
	CampaignState.state["adventure_meta"]["writing_style"] = compiled.get("writing_style", "")
	_update_memory_ui()
	
	# Dynamically handle all missing starter data
	# A. Character fallback
	if compiled.characters.is_empty():
		var fallback_char_id = "companion"
		compiled.characters[fallback_char_id] = {
			"name": "Companion",
			"biography": "A local companion assisting with navigation and instructions.",
			"affinity": 0.0,
			"writing_style": "",
			"avatar": "",
			"base_emotion": "serenity",
			"base_intensity": 0.5
		}
		compiled.knowledge_graph.nodes[fallback_char_id] = {
			"label": "Companion",
			"type": "character",
			"desc": "A local companion assisting with navigation and instructions.",
			"properties": {}
		}
		
	# B. Location fallback
	var has_location = false
	for node_id in compiled.knowledge_graph.nodes.keys():
		if compiled.knowledge_graph.nodes[node_id].get("type") == "location":
			has_location = true
			break
			
	if not has_location:
		var fallback_desc = "A mysterious location where the adventure begins."
		var fallback_label = "Starting Area"
		
		# Look for any lore/scene node to borrow description/name from
		for node_id in compiled.knowledge_graph.nodes.keys():
			var node = compiled.knowledge_graph.nodes[node_id]
			if node.get("type") in ["lore", "scene", "story"] and not node.desc.is_empty():
				fallback_label = node.label
				fallback_desc = node.desc
				break
				
		var fallback_loc_id = "starting_location"
		compiled.knowledge_graph.nodes[fallback_loc_id] = {
			"label": fallback_label,
			"type": "location",
			"desc": fallback_desc,
			"properties": {}
		}
		
	# C. Lore/Scene fallback (if graph is completely empty)
	if compiled.knowledge_graph.nodes.is_empty():
		var fallback_story_id = "intro_scene"
		compiled.knowledge_graph.nodes[fallback_story_id] = {
			"label": "Beginning",
			"type": "scene",
			"desc": "The story begins in a quiet corner of the universe.",
			"properties": {}
		}
	
	# 4. Merge compiled entities (characters, notes)
	for char_key in compiled.characters.keys():
		var char_data = compiled.characters[char_key]
		CampaignState.init_character(
			char_key, 
			char_data.name, 
			char_data.biography, 
			char_data.get("writing_style", ""),
			char_data.get("avatar", ""),
			char_data.get("base_emotion", ""),
			char_data.get("base_intensity", -1.0)
		)
		CampaignState.adjust_affinity(char_key, char_data.affinity)
		_deduce_base_emotions_if_needed(char_key, char_data)
		
	# 4.5. Initialize player character if present
	if custom_mappings != null and custom_mappings.has("player_character") and custom_mappings["player_character"] != null:
		var pc = custom_mappings["player_character"]
		CampaignState.init_character(
			"player",
			pc.get("name", "Player"),
			pc.get("physical_description", ""),
			"", # writing style
			pc.get("avatar", "")
		)
		CampaignState.state["player_character"] = pc
		CampaignState.state.characters["player"]["physical_description"] = pc.get("physical_description", "")
		CampaignState.state.characters["player"]["personality"] = pc.get("personality", "")
		CampaignState.state.characters["player"]["backstory"] = pc.get("backstory", "")
		
	# Import knowledge graph structure
	var graph = compiled.knowledge_graph
	for node_id in graph.nodes.keys():
		var n = graph.nodes[node_id]
		graph_manager.add_node(node_id, n.label, n.type, n.desc, n.properties)
		
	for edge in graph.edges:
		graph_manager.add_edge(edge.from, edge.to, edge.relation, edge.weight)
		
	CampaignState.save()
	CampaignState.log_state_summary()
	
	# 6. Determine starting location and set CampaignState active_location
	var starting_location_id = ""
	if custom_mappings != null and custom_mappings.has("starting_location_id") and custom_mappings["starting_location_id"] != null:
		starting_location_id = str(custom_mappings["starting_location_id"])
		
	if starting_location_id.is_empty():
		for node_id in compiled.knowledge_graph.nodes.keys():
			if compiled.knowledge_graph.nodes[node_id].get("type") == "location":
				starting_location_id = node_id
				break
				
	if not starting_location_id.is_empty():
		CampaignState.set_campaign_meta("active_location", starting_location_id)
		
	# 7. Auto-select starting character
	var starting_char_id = ""
	if custom_mappings != null and custom_mappings.has("starting_character_id") and custom_mappings["starting_character_id"] != null:
		starting_char_id = str(custom_mappings["starting_character_id"])
		
	if not starting_char_id.is_empty() and CampaignState.state.characters.has(starting_char_id):
		_select_character(starting_char_id)
	else:
		var nearby_ids = _get_nearby_character_ids()
		if not nearby_ids.is_empty():
			_select_character(nearby_ids[0])
		elif not CampaignState.state.characters.is_empty():
			_select_character(CampaignState.state.characters.keys()[0])
			
	# 8. Refresh Character List in UI
	_refresh_character_list()
	_display_system_message("Import Successful! Campaign '" + title + "' initialized.")
	
	# 9. Display location intro if available
	if not starting_location_id.is_empty() and compiled.knowledge_graph.nodes.has(starting_location_id):
		var location_node = compiled.knowledge_graph.nodes[starting_location_id]
		_beginning_scene_title = location_node.label
		_fallback_beginning_text = location_node.desc
		
		_display_system_message("Starting Location: " + location_node.label)
		
		var intro_narration = custom_mappings.get("intro_narration", "")
		if not intro_narration.is_empty():
			_is_generating_beginning = false
			_current_turn_state = TurnState.IDLE
			_append_to_dialogue_display("narrator", intro_narration)
			CampaignState.add_history_log("assistant", intro_narration, "narrator")
			CampaignState.save()
			_set_input_disabled(false)
			_trigger_emotion_reflection(intro_narration)
			return
			
		_is_generating_beginning = true
		_set_input_disabled(true)
		_append_to_dialogue_display("system", "Generating creative introduction narration...")
		
		# Gather characters and locations in compiled state for context
		var chars_list: Array = []
		var locs_list: Array = []
		
		locs_list.append({"name": location_node.label, "description": location_node.desc})
		
		for node_id in compiled.knowledge_graph.nodes.keys():
			if node_id == starting_location_id:
				continue
			var node = compiled.knowledge_graph.nodes[node_id]
			
			if node.type in ["character", "npc"]:
				var is_associated = false
				for edge in compiled.knowledge_graph.edges:
					var f = edge.from
					var t = edge.to
					var rel = edge.relation
					if (f == node_id and t == starting_location_id) or (t == node_id and f == starting_location_id):
						if rel in ["associated_with", "connected_to"]:
							is_associated = true
							break
				if not is_associated:
					var desc_lower = node.desc.to_lower()
					var label_lower = location_node.label.to_lower()
					if desc_lower.contains(label_lower) or desc_lower.contains(starting_location_id.to_lower().replace("_", " ")):
						is_associated = true
						
				if is_associated:
					chars_list.append({"name": node.label, "biography": node.desc})
			elif node.type == "location":
				var is_connected = false
				for edge in compiled.knowledge_graph.edges:
					var f = edge.from
					var t = edge.to
					var rel = edge.relation
					if (f == node_id and t == starting_location_id) or (t == node_id and f == starting_location_id):
						if rel in ["connected_to", "associated_with"]:
							is_connected = true
							break
				if is_connected:
					locs_list.append({"name": node.label, "description": node.desc})
					
		var campaign_writing_style = compiled.get("writing_style", "")
		var beginning_prompt = SystemPrompts.get_beginning_generation_prompt(
			title,
			location_node.label,
			location_node.desc,
			chars_list,
			locs_list,
			campaign_writing_style
		)
		
		LLMClient.send_prompt(beginning_prompt, LLMClient.world_builder_model)

func load_existing_campaign(campaign_id: String) -> void:
	_display_system_message("Loading Save: " + campaign_id + "...")
	_is_sidebar_collapsed = false
	sidebar_container.offset_left = -350.0
	sidebar_container.offset_right = 0.0
	toggle_sidebar_button.text = "⟫"
	sidebar_container.visible = true
	var data = SaveManager.load_campaign(campaign_id)
	if data.is_empty():
		_display_error_message("Save game not found or corrupted.")
		return
		
	CampaignState.initialize(campaign_id, data)
	_update_memory_ui()
	
	# Re-import knowledge graph structure from loaded state
	var graph = CampaignState.state.get("knowledge_graph", {"nodes": {}, "edges": []})
	var first_scene_key = ""
	for node_id in graph.get("nodes", {}).keys():
		var n = graph.nodes[node_id]
		graph_manager.add_node(node_id, n.label, n.type, n.desc, n.properties)
		if n.type in ["scene", "story"] and first_scene_key.is_empty():
			first_scene_key = node_id
		
	for edge in graph.get("edges", []):
		graph_manager.add_edge(edge.from, edge.to, edge.relation, edge.weight)
		
	var current_active_scene = CampaignState.get_campaign_meta("active_scene", "")
	if current_active_scene.is_empty() and not first_scene_key.is_empty():
		CampaignState.set_campaign_meta("active_scene", first_scene_key)
		
	var current_active_location = CampaignState.get_campaign_meta("active_location", "")
	if current_active_location.is_empty():
		for node_id in graph.get("nodes", {}).keys():
			if graph.nodes[node_id].get("type") == "location":
				current_active_location = node_id
				CampaignState.set_campaign_meta("active_location", current_active_location)
				break
				
	# Auto-select character on load based on metadata, nearby, or campaign characters list
	var saved_active_char = CampaignState.get_campaign_meta("active_character", "")
	if not saved_active_char.is_empty() and CampaignState.state.characters.has(saved_active_char):
		_select_character(saved_active_char)
	else:
		var nearby_ids = _get_nearby_character_ids()
		if not nearby_ids.is_empty():
			_select_character(nearby_ids[0])
		elif not CampaignState.state.characters.is_empty():
			_select_character(CampaignState.state.characters.keys()[0])
			
	_refresh_character_list()
	CampaignState.log_state_summary()
	
	# Print last conversation snippet if available
	var history = CampaignState.get_recent_history(5)
	if not history.is_empty():
		dialogue_label.text = ""
		for entry in history:
			var sender = entry.get("sender", entry.role)
			_append_to_dialogue_display(sender, entry.content)
	else:
		_display_system_message("Loaded campaign '" + CampaignState.get_campaign_meta("title") + "'. Ready.")

# ==============================================================================
# UI Interaction Flow
# ==============================================================================

func _consume_pending_scene() -> void:
	var pending = CampaignState.state.get("pending_scene", {})
	if pending.is_empty() or pending.get("narration", "").is_empty():
		return
		
	var narration_text = pending.get("narration", "")
	_append_to_dialogue_display("narrator", narration_text)
	CampaignState.add_history_log("assistant", narration_text, "narrator")
	
	var memory_updates = pending.get("memory_updates", {})
	if not memory_updates.is_empty():
		var current_mem = CampaignState.state.get("memory", {})
		for key in ["short_term", "medium_term", "long_term"]:
			if memory_updates.has(key) and not str(memory_updates[key]).is_empty():
				current_mem[key] = str(memory_updates[key])
		CampaignState.state["memory"] = current_mem
		_update_memory_ui()
		
	var plot_updates = pending.get("plot_updates", {})
	if plot_updates is Dictionary:
		for flag in plot_updates.keys():
			CampaignState.set_plot_state(flag, plot_updates[flag])
			
	var inventory_updates = pending.get("inventory_updates", [])
	if inventory_updates is Array:
		for update in inventory_updates:
			if update is Dictionary:
				var item_id = update.get("item_id", "")
				var action = update.get("action", "add")
				var qty = int(update.get("quantity", 1))
				if not item_id.is_empty():
					if action == "add":
						CampaignState.add_to_inventory(active_character_id, item_id, qty)
						_display_system_message("Added to Inventory: %s x%d" % [item_id.capitalize(), qty])
					elif action == "remove":
						CampaignState.remove_from_inventory(active_character_id, item_id, qty)
						_display_system_message("Removed from Inventory: %s x%d" % [item_id.capitalize(), qty])
		
	CampaignState.state["pending_scene"] = {}
	CampaignState.save()
	_trigger_emotion_reflection(narration_text)

func send_player_input(input_text: String) -> void:
	if input_text.strip_edges().is_empty():
		return
		
	if active_character_id.is_empty():
		_display_warning_message("Please select a character in the sidebar to talk to first.")
		return
		
	# 1. Update save logs for player input
	CampaignState.add_history_log("user", input_text, "player")
	_append_to_dialogue_display("player", input_text)
	
	_last_player_input = input_text
	_set_input_disabled(true)
	
	# Check if a pending scene is queued
	_consume_pending_scene()
	
	# Update turn tracking
	var turns_since = int(CampaignState.state.get("turns_since_last_director", 0)) + 1
	CampaignState.state["turns_since_last_director"] = turns_since
	
	var cooldown = int(CampaignState.state.get("director_cooldown", 0))
	if cooldown > 0:
		CampaignState.state["director_cooldown"] = cooldown - 1
		
	CampaignState.save()
	
	# Fire NPC immediately
	_current_turn_state = TurnState.CHARACTER_THINKING
	var char_name = active_character_id.capitalize()
	var character = CampaignState.get_character(active_character_id)
	if not character.is_empty():
		char_name = character.get("name", char_name)
	_display_system_message(char_name + " is thinking...")
	
	# Start stream variables
	_stream_sender = active_character_id
	_stream_is_first_chunk = true
	_stream_in_dialogue_zone = false
	_stream_dialogue_buffer = ""
	
	var npc_prompt = prompt_builder.build_prompt(active_character_id, input_text, _is_director_running)
	LLMClient.send_prompt(npc_prompt, LLMClient.character_model, "", true)

func _on_ai_response_received(raw_response: String) -> void:
	_remove_last_system_message()
	
	var parsed = JsonRepair.extract_json(raw_response)
	
	if _is_generating_beginning:
		_is_generating_beginning = false
		_current_turn_state = TurnState.IDLE
		var text_response = parsed.get("response", parsed.get("narration", raw_response))
		
		var clean_response = text_response.strip_edges().replace("`", "")
		if clean_response.is_empty() or text_response.contains("```") or (text_response == raw_response and not raw_response.contains("{")):
			text_response = _fallback_beginning_text
			
		_append_to_dialogue_display("narrator", text_response)
		CampaignState.add_history_log("assistant", text_response, "narrator")
		CampaignState.save()
		_set_input_disabled(false)
		_trigger_emotion_reflection(text_response)
		return
		
	if _current_turn_state == TurnState.CHARACTER_THINKING:
		var text_response = parsed.get("dialogue", parsed.get("response", parsed.get("narration", raw_response)))
		var emotional_update = parsed.get("emotional_update", {})
		var escalation = str(parsed.get("escalation_signal", "none")).to_lower()
		
		# If streaming failed or JSON parser found different keys, output dialogue
		if _stream_is_first_chunk:
			_append_to_dialogue_display(active_character_id, text_response)
		else:
			dialogue_label.text += "\n"
			
		# Record response history
		CampaignState.add_history_log("assistant", text_response, active_character_id)
		
		# Update character emotional state logs & relationship affinity
		emotion_engine.process_response_tags(active_character_id, emotional_update)
		
		# Display the emotional update in logs
		if not emotional_update.is_empty():
			var character = CampaignState.get_character(active_character_id)
			var friendly_char_name = character.get("name", active_character_id.capitalize()) if not character.is_empty() else active_character_id.capitalize()
			
			var emotion_name = str(emotional_update.get("emotion", "serenity")).capitalize()
			var intensity_val = float(emotional_update.get("intensity", 0.5))
			var delta_val = float(emotional_update.get("rapport_delta", emotional_update.get("affinity_delta", 0.0)))
			var reason_str = str(emotional_update.get("reason", ""))
			
			var delta_str = ""
			if delta_val > 0:
				delta_str = "+%.2f" % delta_val
			elif delta_val < 0:
				delta_str = "%.2f" % delta_val
			else:
				delta_str = "0.0"
				
			var msg = "%s feels: %s (intensity: %.1f) | Rapport: %s" % [
				friendly_char_name, emotion_name, intensity_val, delta_str
			]
			if not reason_str.is_empty():
				msg += "\nReason: %s" % reason_str
			print("[SYSTEM] " + msg)
			
		# Save updated campaign state
		CampaignState.save()
		
		# Consume pending scene immediately after NPC dialogue is finalized
		_consume_pending_scene()
		
		# Check if we should trigger the background Director
		var turns_since = int(CampaignState.state.get("turns_since_last_director", 0))
		var cooldown = int(CampaignState.state.get("director_cooldown", 0))
		var pending = CampaignState.state.get("pending_scene", {})
		
		var needs_escalation = (escalation != "none" and escalation != "")
		var fallback_reached = (turns_since >= 6)
		
		var should_trigger = false
		if needs_escalation:
			should_trigger = true
		elif fallback_reached and cooldown <= 0:
			should_trigger = true
			
		if should_trigger and pending.is_empty() and not _is_director_running:
			_trigger_background_director()
			
		# Reset turn state to Idle and re-enable player controls
		_current_turn_state = TurnState.IDLE
		_set_input_disabled(false)
		return

func _on_ai_request_failed(error_msg: String) -> void:
	_remove_last_system_message()
	
	if _is_generating_beginning:
		_is_generating_beginning = false
		_current_turn_state = TurnState.IDLE
		_display_warning_message("LLM connection failed. Showing default introduction.")
		_append_to_dialogue_display("narrator", _fallback_beginning_text)
		CampaignState.add_history_log("assistant", _fallback_beginning_text, "narrator")
		CampaignState.save()
		_set_input_disabled(false)
		_trigger_emotion_reflection(_fallback_beginning_text)
		return
		
	_display_error_message("Error from LLM client: " + error_msg)
	_current_turn_state = TurnState.IDLE
	_set_input_disabled(false)

# ==============================================================================
# Helper Methods
# ==============================================================================

func _set_input_disabled(disabled: bool) -> void:
	input_field.editable = not disabled
	send_button.disabled = disabled

func _append_to_dialogue_display(sender: String, message: String) -> void:
	var friendly_name = sender.capitalize()
	
	if sender == "user" or sender == "player":
		friendly_name = "Player"
	elif sender == "system":
		friendly_name = "System"
	elif sender == "narrator":
		friendly_name = "Narrator"
	else:
		var character = CampaignState.get_character(sender)
		if not character.is_empty():
			friendly_name = character.get("name", sender.capitalize())
			
	speaker_name_label.text = friendly_name
	_update_nameplate_color(sender)
	
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	var sender_color = _get_adjusted_sender_color(sender, is_light)
	dialogue_label.text += "[color=%s]%s[/color]: %s\n" % [sender_color, friendly_name, message]

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
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	var system_tag_color = "#2563EB" if is_light else "#60A5FA"
	dialogue_label.text += "[color=%s]ℹ️ [SYSTEM][/color]: %s\n" % [system_tag_color, msg]

func _display_warning_message(msg: String) -> void:
	push_warning(msg)
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	var warning_tag_color = "#D97706" if is_light else "#FBBF24"
	dialogue_label.text += "[color=%s]⚠️ [WARNING][/color]: %s\n" % [warning_tag_color, msg]

func _display_error_message(msg: String) -> void:
	push_error(msg)
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	var error_color = "#DC2626" if is_light else "#F87171"
	dialogue_label.text += "[color=%s]❌ [ERROR][/color]: [color=%s][b]%s[/b][/color]\n" % [error_color, error_color, msg]

func _remove_last_system_message() -> void:
	# Strip trailing thinking lines
	var lines = dialogue_label.text.split("\n")
	var new_lines = []
	for line in lines:
		if not line.contains("Thinking..."):
			new_lines.append(line)
	dialogue_label.text = "\n".join(new_lines)

func _refresh_character_list() -> void:
	# Clear old list
	for child in character_list_container.get_children():
		child.queue_free()
		
	var nearby_ids = _get_nearby_character_ids()
	var characters = CampaignState.state.get("characters", {})
	for char_key in nearby_ids:
		if not characters.has(char_key):
			continue
		var char_data = characters[char_key]
		var item = CharacterListItemScene.instantiate()
		character_list_container.add_child(item)
		item.setup(char_key, char_data.get("name", char_key), char_data.get("affinity", 0.0))
		item.selected.connect(_select_character)

func _get_nearby_character_ids() -> Array[String]:
	var nearby_ids: Array[String] = []
	
	if not active_character_id.is_empty():
		nearby_ids.append(active_character_id)
		
	var active_location = CampaignState.get_campaign_meta("active_location", "")
	if active_location.is_empty():
		var graph = CampaignState.state.get("knowledge_graph", {"nodes": {}, "edges": []})
		var nodes = graph.get("nodes", {})
		for node_id in nodes.keys():
			if nodes[node_id].get("type") == "location":
				active_location = node_id
				CampaignState.set_campaign_meta("active_location", active_location)
				break
				
	if active_location.is_empty():
		var characters = CampaignState.state.get("characters", {})
		for char_id in characters.keys():
			if char_id == "player":
				continue
			if not nearby_ids.has(char_id):
				nearby_ids.append(char_id)
		return nearby_ids
		
	var loc_node = graph_manager.get_node(active_location)
	var loc_label = loc_node.get("label", active_location).to_lower()
	var characters = CampaignState.state.get("characters", {})
	
	for char_id in characters.keys():
		if char_id == "player":
			continue
		if nearby_ids.has(char_id):
			continue
			
		var char_data = characters[char_id]
		var is_nearby = false
		
		# 1. Check direct or regional edges in the knowledge graph
		var edges = graph_manager.get_connected_edges(char_id)
		var active_neighbors: Array[String] = []
		for neighbor_id in graph_manager.get_neighbors(active_location):
			var neighbor_node = graph_manager.get_node(neighbor_id)
			if neighbor_node.get("type", "") in ["location", "environment", "gate"]:
				active_neighbors.append(neighbor_id)
				
		for edge in edges:
			var f = edge.get("from", "")
			var t = edge.get("to", "")
			var rel = edge.get("relation", "")
			if rel in ["associated_with", "connected_to"]:
				if f == active_location or t == active_location:
					is_nearby = true
					break
				if f in active_neighbors or t in active_neighbors:
					is_nearby = true
					break
				
		# 2. Check biography mentions
		if not is_nearby:
			var bio = char_data.get("biography", "").to_lower()
			if bio.contains(loc_label) or bio.contains(active_location.to_lower().replace("_", " ")):
				is_nearby = true
				
		# 3. Check frontmatter properties connections
		if not is_nearby:
			var char_node = graph_manager.get_node(char_id)
			var fm = char_node.get("properties", {})
			var connections = fm.get("connections", [])
			if connections is Array:
				for conn in connections:
					var conn_str = str(conn).to_lower().replace(" ", "_")
					if conn_str == active_location or conn_str == loc_label.replace(" ", "_"):
						is_nearby = true
						break
			elif connections is String:
				var conn_str = connections.to_lower().replace(" ", "_")
				if conn_str == active_location or conn_str == loc_label.replace(" ", "_"):
					is_nearby = true
					
		if is_nearby:
			nearby_ids.append(char_id)
			
	return nearby_ids

func _select_character(char_id: String) -> void:
	active_character_id = char_id
	CampaignState.set_campaign_meta("active_character", char_id)
	CampaignState.save()
	var character = CampaignState.get_character(char_id)
	character_visuals_rect.load_character(char_id)
	_display_system_message("Selected conversation target: " + character.get("name", char_id))
	
	# Fetch last emotions if available to display active visual state
	var emotions = character.get("emotions", [])
	if not emotions.is_empty():
		var last = emotions[-1]
		character_visuals_rect.apply_emotion(last.get("emotion", "serenity"), character.get("affinity", 0.0))

func _on_character_emotion_updated(char_id: String, emotion: String, affinity: float) -> void:
	if char_id == active_character_id:
		# If the update has no defined emotion label (e.g. just raw affinity shift), fetch active one
		var current_emotion = emotion
		if current_emotion.is_empty():
			var character = CampaignState.get_character(char_id)
			var emotions = character.get("emotions", [])
			current_emotion = emotions[-1].get("emotion", "serenity") if not emotions.is_empty() else "serenity"
			
		character_visuals_rect.apply_emotion(current_emotion, affinity)
		
	# Always refresh the character list to keep the sidebar updated for all characters
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
		flow.adventure_started.connect(start_new_campaign)
		flow.adventure_loaded.connect(load_existing_campaign)
		
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
		# Expand
		target_offset_left = -350.0
		target_offset_right = 0.0
		button_text = "⟫"
		_is_sidebar_collapsed = false
	else:
		# Collapse
		target_offset_left = -30.0
		target_offset_right = 320.0
		button_text = "⟪"
		_is_sidebar_collapsed = true
		
	_sidebar_tween.tween_property(sidebar_container, "offset_left", target_offset_left, 0.35).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	_sidebar_tween.tween_property(sidebar_container, "offset_right", target_offset_right, 0.35).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	
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
		medium_term_memory_label.text = "[color=%s][b]Medium-Term:[/b][/color] " % yellow_color + memory.get("medium_term", "None")
	if long_term_memory_label:
		long_term_memory_label.text = "[color=%s][b]Long-Term:[/b][/color] " % red_color + memory.get("long_term", "None")

func _on_settings_pressed() -> void:
	var modal_scene = load("res://scenes/ui/SettingsModal.tscn")
	if modal_scene:
		var modal = modal_scene.instantiate()
		# Override the static .tres reference so the modal uses the live active_theme.
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

func _on_theme_changed() -> void:
	# Rebind the root node to active_theme so all children inherit updates
	# when active_theme.emit_changed() fires on subsequent theme switches.
	self.theme = ThemeManager.active_theme
	# ColorRect nodes cannot use theme variations — update explicitly.
	if bg_color_rect:
		bg_color_rect.color = ThemeManager.color_bg
	_update_memory_ui()

func _on_ai_response_chunk_received(chunk: String) -> void:
	if not _stream_in_dialogue_zone:
		_stream_dialogue_buffer += chunk
		var start_tag = "\"dialogue\":"
		var idx = _stream_dialogue_buffer.find(start_tag)
		if idx == -1:
			return
			
		var val_after = _stream_dialogue_buffer.substr(idx + start_tag.length()).strip_edges()
		if val_after.begins_with("\""):
			_stream_in_dialogue_zone = true
			var content = val_after.substr(1)
			_stream_dialogue_buffer = content
			if not content.is_empty():
				_append_stream_chunk(content)
	else:
		# Process character by character to handle quotes properly
		for i in range(chunk.length()):
			var char_val = chunk[i]
			# If we find a closing quote that is not escaped
			if char_val == "\"" and (i == 0 or chunk[i-1] != "\\"):
				_stream_in_dialogue_zone = false
				break
			else:
				_append_stream_chunk(char_val)

func _append_stream_chunk(text: String) -> void:
	if _stream_is_first_chunk:
		_stream_is_first_chunk = false
		_remove_last_system_message() # Remove "thinking" block immediately when stream starts
		
		var friendly_name = _stream_sender.capitalize()
		var character = CampaignState.get_character(_stream_sender)
		if not character.is_empty():
			friendly_name = character.get("name", _stream_sender.capitalize())
			
		speaker_name_label.text = friendly_name
		_update_nameplate_color(_stream_sender)
		
		var is_light = ThemeManager.color_bg.get_luminance() > 0.5
		var sender_color = _get_adjusted_sender_color(_stream_sender, is_light)
		dialogue_label.text += "[color=%s]%s[/color]: " % [sender_color, friendly_name]
		
	dialogue_label.text += text

func _trigger_background_director() -> void:
	print("[SYSTEM] Triggering background Director model...")
	_is_director_running = true
	
	# Build the DM prompt
	var dm_prompt = prompt_builder.build_world_builder_prompt(_last_player_input, active_character_id)
	
	LLMClient.send_custom_request(dm_prompt, LLMClient.world_builder_model, _on_background_director_completed)

func _on_background_director_completed(success: bool, response_text: String, error_msg: String) -> void:
	_is_director_running = false
	
	if not success:
		print("[SYSTEM] Background Director model failed: ", error_msg)
		return
		
	var parsed = JsonRepair.extract_json(response_text)
	var narration_text = parsed.get("narration", response_text)
	var clean_narration = narration_text.strip_edges().replace("`", "")
	
	# Skip if malformed response or empty
	if clean_narration.is_empty() or narration_text.contains("```") or (narration_text == response_text and not response_text.contains("{")):
		print("[SYSTEM] Background Director model response was invalid/malformed.")
		return
		
	print("[SYSTEM] Background Director model generated next scene beat successfully.")
	
	# Save to pending_scene queue
	var pending = {
		"narration": narration_text,
		"memory_updates": parsed.get("memory_updates", {}),
		"plot_updates": parsed.get("plot_updates", {}),
		"inventory_updates": parsed.get("inventory_updates", []),
		"choices": parsed.get("choices", [])
	}
	
	CampaignState.state["pending_scene"] = pending
	CampaignState.state["turns_since_last_director"] = 0
	CampaignState.state["director_cooldown"] = 3 # Cooldown of 3 turns
	CampaignState.save()
	
	# If the user is currently idle, consume the scene immediately so they see the result of the plot transition
	if _current_turn_state == TurnState.IDLE:
		_consume_pending_scene()

func _trigger_emotion_reflection(narration_text: String) -> void:
	if active_character_id.is_empty():
		return
		
	var char_id = active_character_id
	var prompt = SystemPrompts.get_emotion_reflection_prompt_for_id(char_id, narration_text)
	if prompt.is_empty():
		return
		
	print("[SYSTEM] Character %s is reflecting on the narrative beat..." % char_id)
	
	LLMClient.send_custom_request(prompt, LLMClient.character_model, func(success: bool, response_text: String, error_msg: String):
		if not success:
			print("[SYSTEM] Emotion reflection failed for %s: %s" % [char_id, error_msg])
			return
			
		var parsed = JsonRepair.extract_json(response_text)
		var emotional_update = parsed.get("emotional_update", {})
		if not emotional_update.is_empty():
			emotion_engine.process_response_tags(char_id, emotional_update)
			
			# Log it
			var character = CampaignState.get_character(char_id)
			var friendly_char_name = character.get("name", char_id.capitalize()) if not character.is_empty() else char_id.capitalize()
			
			var emotion_name = str(emotional_update.get("emotion", "serenity")).capitalize()
			var intensity_val = float(emotional_update.get("intensity", 0.5))
			var delta_val = float(emotional_update.get("rapport_delta", emotional_update.get("affinity_delta", 0.0)))
			var reason_str = str(emotional_update.get("reason", ""))
			
			var delta_str = ""
			if delta_val > 0:
				delta_str = "+%.2f" % delta_val
			elif delta_val < 0:
				delta_str = "%.2f" % delta_val
			else:
				delta_str = "0.0"
				
			var msg = "%s reflects on environment: %s (intensity: %.1f) | Rapport: %s" % [
				friendly_char_name, emotion_name, intensity_val, delta_str
			]
			if not reason_str.is_empty():
				msg += "\nReason: %s" % reason_str
			print("[SYSTEM] " + msg)
			CampaignState.save()
	)

func _deduce_base_emotions_if_needed(char_id: String, char_data: Dictionary) -> void:
	var base_emo = char_data.get("base_emotion", "")
	var base_int = float(char_data.get("base_intensity", -1.0))
	
	if not base_emo.is_empty() and base_int >= 0.0:
		# Already explicitly specified in the campaign/frontmatter
		return
		
	var char_name = char_data.get("name", char_id.capitalize())
	var biography = char_data.get("biography", "")
	
	if biography.strip_edges().is_empty():
		# No biography to deduce from, fallback to serenity
		CampaignState.add_emotion_event(char_id, "serenity", 0.5, "player", "Default baseline (no biography provided).")
		return
		
	var prompt = SystemPrompts.get_deduce_base_emotion_prompt(char_name, biography)
	print("[SYSTEM] Deducing base emotion for character: %s..." % char_id)
	
	# Quick non-blocking call to character (fast) model
	LLMClient.send_custom_request(prompt, LLMClient.character_model, func(success: bool, response_text: String, error_msg: String):
		var deduced_emo = "serenity"
		var deduced_int = 0.5
		var reason = "Default fallback (analysis failed)."
		
		if success:
			var parsed = JsonRepair.extract_json(response_text)
			if parsed.has("base_emotion") and not str(parsed["base_emotion"]).is_empty():
				var valid_emotions = ["serenity", "joy", "sadness", "anger", "fear", "trust", "disgust", "surprise"]
				var emotion = str(parsed["base_emotion"]).to_lower().strip_edges()
				if valid_emotions.has(emotion):
					deduced_emo = emotion
					deduced_int = clamp(float(parsed.get("base_intensity", 0.5)), 0.0, 1.0)
					reason = "Deduced from biography: %s" % deduced_emo.capitalize()
					
		# Save deduced stats on character
		var character = CampaignState.get_character(char_id)
		if not character.is_empty():
			character["base_emotion"] = deduced_emo
			character["base_intensity"] = deduced_int
			
		CampaignState.add_emotion_event(char_id, deduced_emo, deduced_int, "player", reason)
		CampaignState.save()
		
		# Refresh UI to show deduced emotion if this is the active character
		if char_id == active_character_id:
			character_visuals_rect.apply_emotion(deduced_emo, CampaignState.get_character(char_id).get("affinity", 0.0))
		_refresh_character_list()
	)
