# res://src/core/GameLoopController.gd
extends Node
class_name GameLoopController

const LLMStreamParserScript = preload("res://src/core/LLMStreamParser.gd")
const MemoryManager = preload("res://src/core/MemoryManager.gd")
const PlayerInputParser = preload("res://src/core/PlayerInputParser.gd")


# Turn Phase state machine
enum TurnState {
	IDLE,
	WORLD_BUILDER_THINKING,
	CHARACTER_THINKING
}

# Decoupled Signals for UI communication
signal campaign_started(title: String, active_location_id: String)
signal campaign_loaded(campaign_id: String)
signal system_message_logged(message: String)
signal warning_message_logged(message: String)
signal error_message_logged(message: String)
signal message_logged(sender: String, message: String)
signal stream_chunk_logged(sender: String, word: String)
signal stream_started(sender_id: String)
signal stream_zone_ended()
signal turn_state_changed(state: TurnState)
signal active_character_changed(char_id: String)
signal character_visual_update_requested(char_id: String, emotion: String, affinity: float)
signal character_reaction_requested(char_id: String, emotion: String)
signal sidebar_refresh_requested()
signal memory_updated()
signal input_disabled_changed(disabled: bool)

# Subsystem Managers (pure logic / helpers)
var graph_manager: KnowledgeGraphManager
var emotion_prompt_builder: EmotionPromptBuilder
var prompt_builder: PromptBuilder
var emotion_engine: EmotionEngine
var stream_parser: RefCounted
var memory_manager: MemoryManager

# Controller State
var active_character_id: String = ""
var is_initializing: bool = false
var _current_turn_state: TurnState = TurnState.IDLE
var _last_player_input: String = ""
var _is_generating_beginning: bool = false
var _fallback_beginning_text: String = ""
var _beginning_scene_title: String = ""
var _is_director_running: bool = false
var _last_location_id: String = ""

func _ready() -> void:
	# 1. Instantiate core local helpers
	graph_manager = KnowledgeGraphManager.new()
	emotion_prompt_builder = EmotionPromptBuilder.new()
	prompt_builder = PromptBuilder.new(graph_manager, emotion_prompt_builder)
	emotion_engine = EmotionEngine.new()
	stream_parser = LLMStreamParserScript.new()
	memory_manager = MemoryManager.new(self)
	
	# 2. Connect stream parser and emotion engine signals
	stream_parser.zone_started.connect(_on_stream_zone_started)
	stream_parser.char_received.connect(_on_stream_char_received)
	stream_parser.zone_ended.connect(_on_stream_zone_ended)
	
	emotion_engine.character_visual_update_requested.connect(func(c_id, emo, aff):
		character_visual_update_requested.emit(c_id, emo, aff)
	)
	emotion_engine.sidebar_refresh_requested.connect(func():
		sidebar_refresh_requested.emit()
	)

# ==============================================================================
# Game Setup & Import APIs
# ==============================================================================

func start_new_campaign(campaign_id: String, title: String, vault_path: String, custom_mappings: Dictionary = {}) -> void:
	is_initializing = true
	system_message_logged.emit("Compiling Vault: " + vault_path + "...")
	input_disabled_changed.emit(true)
	
	# 1. Compile Markdown vault directory or use pre-compiled data from onboarding
	var compiled
	if custom_mappings.has("compiled_data") and custom_mappings["compiled_data"] != null:
		compiled = custom_mappings["compiled_data"]
		print("[GameLoopController] Using pre-compiled campaign data from onboarding mind map.")
	else:
		compiled = await VaultCompiler.compile_vault(vault_path, custom_mappings)
	
	# 2. Initialize new Campaign JSON save document
	var initial_state = SaveManager.create_campaign(campaign_id, title)
	if initial_state.is_empty():
		is_initializing = false
		error_message_logged.emit("Failed to create save document.")
		input_disabled_changed.emit(false)
		return
		
	# 3. Instantiate dynamic state data
	CampaignState.initialize(campaign_id, initial_state)
	CampaignState.set_campaign_meta("writing_style", compiled.get("writing_style", ""))
	if custom_mappings.has("art_style"):
		CampaignState.set_campaign_meta("art_style", custom_mappings["art_style"])
	memory_updated.emit()
	
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
			char_data.get("base_intensity", -1.0),
			char_data.affinity,
			char_data.get("is_creature", false),
			char_data.get("can_speak", true),
			char_data.get("humanoid", true)
		)

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
		CampaignState.set_player_character(pc)
		CampaignState.update_character_properties("player", {
			"physical_description": pc.get("physical_description", ""),
			"personality": pc.get("personality", ""),
			"backstory": pc.get("backstory", "")
		})
		
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
		EventBus.location_changed.emit(starting_location_id)
		
	# 7. Auto-select starting character
	var starting_char_id = ""
	if custom_mappings != null and custom_mappings.has("starting_character_id") and custom_mappings["starting_character_id"] != null:
		starting_char_id = str(custom_mappings["starting_character_id"])
		
	if not starting_char_id.is_empty() and CampaignState.has_character(starting_char_id):
		select_character(starting_char_id)
	else:
		var nearby_ids = get_nearby_character_ids()
		if not nearby_ids.is_empty():
			select_character(nearby_ids[0])
		elif not CampaignState.get_character_ids().is_empty():
			select_character(CampaignState.get_character_ids()[0])
			
	# 8. Refresh Character List in UI
	sidebar_refresh_requested.emit()
	_last_location_id = starting_location_id
	system_message_logged.emit("Import Successful! Campaign '" + title + "' initialized.")
	
	# 9. Display location intro if available
	if not starting_location_id.is_empty() and compiled.knowledge_graph.nodes.has(starting_location_id):
		var location_node = compiled.knowledge_graph.nodes[starting_location_id]
		_beginning_scene_title = location_node.label
		_fallback_beginning_text = location_node.desc
		
		system_message_logged.emit("Starting Location: " + location_node.label)
		
		var intro_narration = custom_mappings.get("intro_narration", "")
		if not intro_narration.is_empty():
			_is_generating_beginning = false
			_current_turn_state = TurnState.IDLE
			turn_state_changed.emit(_current_turn_state)
			message_logged.emit("narrator", intro_narration)
			CampaignState.set_campaign_meta("intro_narration", intro_narration)
			CampaignState.add_history_log("assistant", intro_narration, "narrator")
			CampaignState.save()
			input_disabled_changed.emit(false)
			emotion_engine.trigger_emotion_reflection(active_character_id, intro_narration)
			
			# End initialization early since intro narration is pre-provided
			is_initializing = false
			var starting_location_id_meta = CampaignState.get_campaign_meta("active_location", "")
			if not starting_location_id_meta.is_empty():
				EventBus.location_changed.emit(starting_location_id_meta)
			if not active_character_id.is_empty():
				select_character(active_character_id)
			return
			
		_is_generating_beginning = true
		input_disabled_changed.emit(true)
		message_logged.emit("system", "Generating creative introduction narration...")
		
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
		
		_current_turn_state = TurnState.WORLD_BUILDER_THINKING
		turn_state_changed.emit(_current_turn_state)
		# Beginning prompt is HIGH priority as player has selected hook and is waiting
		LLMClient.send_custom_request(beginning_prompt, LLMClient.world_builder_model, _on_beginning_generation_completed, 1500.0, LLMClient.RequestPriority.HIGH)

func load_existing_campaign(campaign_id: String) -> void:
	is_initializing = true
	if not CampaignState.campaign_id.is_empty():
		trigger_all_pending_summaries()
		
	system_message_logged.emit("Loading Save: " + campaign_id + "...")
	input_disabled_changed.emit(true)
	var data = SaveManager.load_campaign(campaign_id)
	if data.is_empty():
		is_initializing = false
		error_message_logged.emit("Save game not found or corrupted.")
		input_disabled_changed.emit(false)
		return
		
	CampaignState.initialize(campaign_id, data)
	memory_updated.emit()
	
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
	if not saved_active_char.is_empty() and CampaignState.has_character(saved_active_char):
		select_character(saved_active_char)
	else:
		var nearby_ids = get_nearby_character_ids()
		if not nearby_ids.is_empty():
			select_character(nearby_ids[0])
		elif not CampaignState.get_character_ids().is_empty():
			select_character(CampaignState.get_character_ids()[0])
			
	# End initialization and trigger background scenery and character load
	is_initializing = false
	sidebar_refresh_requested.emit()
	CampaignState.log_state_summary()
	if not current_active_location.is_empty():
		_last_location_id = current_active_location
		EventBus.location_changed.emit(current_active_location)
		
	campaign_loaded.emit(campaign_id)
	input_disabled_changed.emit(false)

func _on_beginning_generation_completed(success: bool, response_text: String, error_msg: String) -> void:
	_is_generating_beginning = false
	_current_turn_state = TurnState.IDLE
	turn_state_changed.emit(_current_turn_state)
	
	if not success:
		warning_message_logged.emit("LLM connection failed. Showing default introduction.")
		message_logged.emit("narrator", _fallback_beginning_text)
		CampaignState.set_campaign_meta("intro_narration", _fallback_beginning_text)
		CampaignState.add_history_log("assistant", _fallback_beginning_text, "narrator")
		CampaignState.save()
		input_disabled_changed.emit(false)
		emotion_engine.trigger_emotion_reflection(active_character_id, _fallback_beginning_text)
		
		# End initialization and trigger background scenery and character load
		is_initializing = false
		var starting_location_id = CampaignState.get_campaign_meta("active_location", "")
		if not starting_location_id.is_empty():
			EventBus.location_changed.emit(starting_location_id)
		if not active_character_id.is_empty():
			select_character(active_character_id)
		return
		
	var parsed = JsonRepair.extract_json(response_text)
	var text_response = parsed.get("response", parsed.get("narration", response_text))
	
	var clean_response = text_response.strip_edges().replace("`", "")
	if clean_response.is_empty() or text_response.contains("```") or (text_response == response_text and not response_text.contains("{")):
		text_response = _fallback_beginning_text
		
	message_logged.emit("narrator", text_response)
	CampaignState.set_campaign_meta("intro_narration", text_response)
	CampaignState.add_history_log("assistant", text_response, "narrator")
	CampaignState.save()
	input_disabled_changed.emit(false)
	emotion_engine.trigger_emotion_reflection(active_character_id, text_response)
	
	# End initialization and trigger background scenery and character load
	is_initializing = false
	var starting_location_id_meta = CampaignState.get_campaign_meta("active_location", "")
	if not starting_location_id_meta.is_empty():
		EventBus.location_changed.emit(starting_location_id_meta)
	if not active_character_id.is_empty():
		select_character(active_character_id)

# ==============================================================================
# Game Loop & Input APIs
# ==============================================================================

func send_player_input(input_text: String) -> void:
	if input_text.strip_edges().is_empty():
		return
		
	if active_character_id.is_empty():
		warning_message_logged.emit("Please select a character in the sidebar to talk to first.")
		return
		
	var sanitized_text = PlayerInputParser.sanitize_input(input_text)
		
	# 1. Update save logs for player input
	CampaignState.add_history_log("user", sanitized_text, "player")
	print("[PLAYER] %s" % input_text)
	message_logged.emit("player", sanitized_text)
	
	_last_player_input = sanitized_text

	input_disabled_changed.emit(true)
	EventBus.turn_started.emit()
	
	# Check if a pending scene is queued
	consume_pending_scene()
	
	# Update turn tracking
	var turns_since = CampaignState.get_turns_since_last_director() + 1
	CampaignState.set_turns_since_last_director(turns_since)
	
	# Decay inactive characters' emotions
	emotion_engine.decay_emotions(active_character_id, false)
	
	CampaignState.save()
	
	# Fire NPC immediately
	_current_turn_state = TurnState.CHARACTER_THINKING
	turn_state_changed.emit(_current_turn_state)
	system_message_logged.emit("Thinking...")
	
	# Reset streaming state
	stream_parser.reset()
	
	var npc_prompt = await prompt_builder.build_prompt(active_character_id, input_text, _is_director_running)
	LLMClient.send_custom_stream_request(
		npc_prompt,
		LLMClient.character_model,
		func(chunk: String):
			stream_parser.ingest_chunk(chunk),
		_on_npc_stream_completed,
		_on_npc_stream_failed,
		300.0,
		false,
		LLMClient.RequestPriority.HIGH,
		LLMClient.ROLE_CHARACTER
	)

func _on_npc_stream_completed(full_response: String) -> void:
	var parsed = JsonRepair.extract_json(full_response)
	
	if parsed.get("parsing_failed", false):
		error_message_logged.emit("The DM was unable to format this response. [url=retry]Click to retry.[/url]")
		_current_turn_state = TurnState.IDLE
		turn_state_changed.emit(_current_turn_state)
		input_disabled_changed.emit(false)
		EventBus.turn_completed.emit()
		return
		
	var thinking_text = parsed.get("thinking", "")
	var narration_text = parsed.get("narration", "")
	var dialogue_text = parsed.get("dialogue", parsed.get("response", ""))
	
	if dialogue_text == full_response and not full_response.contains("{"):
		dialogue_text = full_response
		
	var emotional_update = parsed.get("emotional_update", {})
	var escalation = str(parsed.get("escalation_signal", "none")).to_lower()
	
	# Always log the final message to the UI to ensure any streaming discrepancies/stalls are resolved
	if not narration_text.strip_edges().is_empty():
		message_logged.emit("narrator", narration_text)
		CampaignState.add_history_log("assistant", narration_text, "narrator")
	if not dialogue_text.strip_edges().is_empty():
		message_logged.emit(active_character_id, dialogue_text)
		CampaignState.add_history_log("assistant", dialogue_text, active_character_id)
			
	# Update character emotional state logs & relationship affinity and emit updates
	emotion_engine.process_response_tags(active_character_id, emotional_update)
	
	# Trigger physical reaction generation for the active character
	if not active_character_id.is_empty():
		var character = CampaignState.get_character(active_character_id)
		if not character.is_empty():
			var emotions = character.get("emotions", [])
			var current_emotion = emotions[-1].get("emotion", "serenity") if not emotions.is_empty() else "serenity"
			character_reaction_requested.emit(active_character_id, current_emotion)
	
	# Display the emotional update in system logs
	if not emotional_update.is_empty():
		var character = CampaignState.get_character(active_character_id)
		var friendly_char_name = character.get("name", active_character_id.capitalize()) if not character.is_empty() else active_character_id.capitalize()
		var emotion_name = str(emotional_update.get("emotion", "serenity")).capitalize()
		var intensity_val = clamp(float(emotional_update.get("intensity", 0.5)), 0.0, 1.0)
		var delta_val = clamp(float(emotional_update.get("rapport_delta", emotional_update.get("affinity_delta", 0.0))), -0.2, 0.2)
		var reason_str = str(emotional_update.get("reason", ""))
		
		var delta_str = "+%.2f" % delta_val if delta_val > 0 else ("%.2f" % delta_val if delta_val < 0 else "0.0")
		var msg = "%s feels: %s (intensity: %.1f) | Rapport: %s" % [friendly_char_name, emotion_name, intensity_val, delta_str]
		if not reason_str.is_empty():
			msg += "\nReason: %s" % reason_str
		print("[SYSTEM] " + msg)
		
	# Memory tracking and compaction check
	if not active_character_id.is_empty():
		var character = CampaignState.get_character(active_character_id)
		if not character.is_empty():
			var turns = int(character.get("turns_since_last_summary", 0)) + 1
			character["turns_since_last_summary"] = turns
			
			var char_logs = memory_manager.get_character_history(active_character_id)
			if char_logs.size() > MemoryManager.COMPACTION_THRESHOLD:
				memory_manager.check_and_compact_history(active_character_id)
			elif turns >= 15:
				memory_manager.summarize_session_for_character(active_character_id)
				
	CampaignState.save()
	
	# Consume pending scene immediately after NPC dialogue is finalized
	consume_pending_scene()
	
	# Check if we should trigger the background Director
	var turns_since = int(CampaignState.state.get("turns_since_last_director", 0))
	var cooldown = int(CampaignState.state.get("director_cooldown", 0))
	var pending = CampaignState.state.get("pending_scene", {})
	
	var needs_escalation = (escalation != "none" and escalation != "")
	var fallback_reached = (turns_since >= 2)
	
	var should_trigger = false
	if needs_escalation:
		should_trigger = true
	elif fallback_reached and cooldown <= 0:
		should_trigger = true
		
	if should_trigger and pending.is_empty() and not _is_director_running:
		_trigger_background_director()
		
	# Reset turn state to Idle and re-enable player controls
	_current_turn_state = TurnState.IDLE
	turn_state_changed.emit(_current_turn_state)
	input_disabled_changed.emit(false)
	EventBus.turn_completed.emit()

func _on_npc_stream_failed(error_msg: String) -> void:
	if error_msg == "Stream cancelled by user" or error_msg == "Request cancelled by user":
		# Handle silently
		_current_turn_state = TurnState.IDLE
		turn_state_changed.emit(_current_turn_state)
		input_disabled_changed.emit(false)
		EventBus.turn_completed.emit()
		return
		
	error_message_logged.emit("Error from LLM client: " + error_msg)
	_current_turn_state = TurnState.IDLE
	turn_state_changed.emit(_current_turn_state)
	input_disabled_changed.emit(false)
	EventBus.turn_completed.emit()

# ==============================================================================
# Helper Methods
# ==============================================================================

func get_current_turn_state() -> TurnState:
	return _current_turn_state

func get_last_player_input() -> String:
	return _last_player_input

func retry_last_input() -> void:
	if active_character_id.is_empty() or _last_player_input.is_empty():
		return
		
	# Transition state to character thinking
	_current_turn_state = TurnState.CHARACTER_THINKING
	turn_state_changed.emit(_current_turn_state)
	system_message_logged.emit("Thinking...")
	
	# Reset streaming state
	stream_parser.reset()
	
	# Re-build prompt and run request
	var npc_prompt = await prompt_builder.build_prompt(active_character_id, _last_player_input, _is_director_running)
	input_disabled_changed.emit(true)
	EventBus.turn_started.emit()
	
	LLMClient.send_custom_stream_request(
		npc_prompt,
		LLMClient.character_model,
		func(chunk: String):
			stream_parser.ingest_chunk(chunk),
		_on_npc_stream_completed,
		_on_npc_stream_failed,
		300.0,
		false,
		LLMClient.RequestPriority.HIGH,
		LLMClient.ROLE_CHARACTER
	)

func select_character(char_id: String) -> void:
	if char_id.is_empty():
		return
	active_character_id = char_id
	CampaignState.set_campaign_meta("active_character", char_id)
	CampaignState.save()
	var character = CampaignState.get_character(char_id)
	
	# Lazily deduce base emotion if it has not been done yet
	emotion_engine.deduce_base_emotion_if_needed(char_id, character)
	
	active_character_changed.emit(char_id)
	
	# Fetch last emotions if available to display active visual state
	var emotions = character.get("emotions", [])
	var current_emotion = emotions[-1].get("emotion", "serenity") if not emotions.is_empty() else "serenity"
	character_visual_update_requested.emit(char_id, current_emotion, character.get("affinity", 0.0))
	system_message_logged.emit("Selected conversation target: " + character.get("name", char_id))

func consume_pending_scene() -> void:
	var pending = CampaignState.get_pending_scene()
	if pending.is_empty() or pending.get("narration", "").is_empty():
		return
		
	var narration_text = pending.get("narration", "")
	message_logged.emit("narrator", narration_text)
	CampaignState.add_history_log("assistant", narration_text, "narrator")
	
	CampaignState.set_last_director_beat(narration_text)
	
	var memory_updates = pending.get("memory_updates", {})
	if not memory_updates.is_empty():
		var current_mem = CampaignState.get_memory()
		for key in ["short_term", "medium_term", "long_term"]:
			if memory_updates.has(key) and not str(memory_updates[key]).is_empty():
				current_mem[key] = str(memory_updates[key])
		CampaignState.set_memory(current_mem)
		memory_updated.emit()
		
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
						system_message_logged.emit("Added to Inventory: %s x%d" % [item_id.capitalize(), qty])
					elif action == "remove":
						CampaignState.remove_from_inventory(active_character_id, item_id, qty)
						system_message_logged.emit("Removed from Inventory: %s x%d" % [item_id.capitalize(), qty])
		
	CampaignState.set_pending_scene({})
	CampaignState.save()
	
	var active_location = CampaignState.get_campaign_meta("active_location", "")
	if not active_location.is_empty():
		if _last_location_id == "" or active_location != _last_location_id:
			if _last_location_id != "" and active_location != _last_location_id:
				emotion_engine.decay_emotions(active_character_id, true)
			_last_location_id = active_location
			EventBus.location_changed.emit(active_location)
		
	emotion_engine.trigger_emotion_reflection(active_character_id, narration_text)

func _trigger_background_director() -> void:
	print("[SYSTEM] Triggering background Director model...")
	_is_director_running = true
	
	# 1. Run ReAct research loop
	var research_findings = await _run_director_react_loop(_last_player_input, active_character_id)
	
	# 2. Build the DM prompt with findings injected
	var dm_prompt = await prompt_builder.build_world_builder_prompt(_last_player_input, active_character_id, research_findings)
	EventBus.director_prompt_generated.emit(dm_prompt)
	LLMClient.send_custom_request(dm_prompt, LLMClient.world_builder_model, _on_background_director_completed, 1500.0, LLMClient.RequestPriority.LOW, false, LLMClient.ROLE_WORLD_BUILDER)

# Inner class carrier to wrap custom requests in an awaitable co-routine
class ReActSignalCarrier extends RefCounted:
	signal completed(success: bool, response_text: String, error_msg: String)
	func emit_completed(success: bool, response_text: String, error_msg: String) -> void:
		_emit_completed_deferred.call_deferred(success, response_text, error_msg)
		
	func _emit_completed_deferred(success: bool, response_text: String, error_msg: String) -> void:
		completed.emit(success, response_text, error_msg)


# Awaitable wrapper for LLM client request
func _send_llm_request_async(prompt: String, model: String, json_mode: bool = false) -> Array:
	var carrier = ReActSignalCarrier.new()
	LLMClient.send_custom_request(
		prompt,
		model,
		func(success: bool, response_text: String, error_msg: String):
			carrier.emit_completed(success, response_text, error_msg),
		120.0,
		LLMClient.RequestPriority.LOW,
		json_mode
	)
	var res = await carrier.completed
	return res

# Normalizes dynamic search keys/labels to existing KG node IDs
func _find_node_id_by_name(name_str: String) -> String:
	var norm = name_str.to_lower().strip_edges()
	var id_guess = norm.replace(" ", "_")
	if graph_manager.has_node(id_guess):
		return id_guess
	# Search by label
	var graph = graph_manager._get_graph()
	var nodes = graph.get("nodes", {})
	for node_id in nodes.keys():
		var node = nodes[node_id]
		if node.get("label", "").to_lower() == norm:
			return node_id
	# Partial match fallback
	for node_id in nodes.keys():
		var label = nodes[node_id].get("label", "").to_lower()
		if norm in label or norm in node_id:
			return node_id
	return id_guess

# Pre-narration ReAct research loop
func _run_director_react_loop(user_prompt: String, active_char_id: String) -> String:
	print("[SYSTEM] Starting Director ReAct research loop...")
	var start_time = Time.get_ticks_msec()
	var research_history: Array = []
	var max_iterations = 5
	
	for i in range(max_iterations):
		var elapsed = (Time.get_ticks_msec() - start_time) / 1000.0
		if elapsed >= 60.0:
			print("[SYSTEM] Director ReAct loop timed out after %.1fs" % elapsed)
			break
			
		var prompt = prompt_builder.build_react_prompt(user_prompt, active_char_id, research_history)
		
		# Await LLM request
		var res = await _send_llm_request_async(prompt, LLMClient.world_builder_model, true)
		var success = res[0]
		var response_text = res[1]
		var error_msg = res[2]
		
		if not success:
			print("[SYSTEM] LLM request failed in ReAct loop: ", error_msg)
			break
			
		var parsed = JsonRepair.extract_json(response_text)
		if parsed.get("parsing_failed", false):
			print("[SYSTEM] JSON parsing failed in ReAct loop response: ", response_text)
			break
			
		var thought = parsed.get("thought", "")
		var action = parsed.get("action", "")
		var args = parsed.get("args", {})
		var final_signal = parsed.get("final", false)
		
		if final_signal:
			print("[SYSTEM] Director concluded research loop: ", thought)
			break
			
		if action.is_empty():
			print("[SYSTEM] ReAct response had no action and was not final. Concluding.")
			break
			
		# Execute action
		var observation = ""
		print("[SYSTEM] ReAct Action: %s (%s)" % [action, str(args)])
		match action:
			"search_knowledge_graph":
				var query = str(args.get("query", ""))
				if not query.is_empty():
					observation = await graph_manager.retrieve_context(query, 500, 0)
					if observation.strip_edges().is_empty():
						observation = "No relevant entities or lore found in the knowledge graph for query: " + query
				else:
					observation = "Error: search_knowledge_graph requires a 'query' argument."
			"get_character_profile":
				var character_name = str(args.get("character_name", ""))
				if not character_name.is_empty():
					var char_id = _find_node_id_by_name(character_name)
					var character = CampaignState.get_character(char_id)
					if character.is_empty():
						observation = "Character not found: " + character_name
					else:
						observation = "Character Profile for " + character.get("name", char_id.capitalize()) + ":\n"
						observation += "- Biography: " + character.get("biography", "") + "\n"
						if not character.get("personality", "").is_empty():
							observation += "- Personality: " + character.get("personality", "") + "\n"
						if not character.get("appearance", "").is_empty():
							observation += "- Appearance: " + character.get("appearance", "") + "\n"
						if not character.get("goals", "").is_empty():
							observation += "- Goals: " + character.get("goals", "") + "\n"
						if not character.get("gender", "").is_empty():
							observation += "- Gender/Pronouns: " + character.get("gender", "") + "\n"
						observation += "- Relationship: %s (Affinity: %.2f)\n" % [CharacterProfile.get_relationship_label(character.get("affinity", 0.0)), character.get("affinity", 0.0)]
				else:
					observation = "Error: get_character_profile requires a 'character_name' argument."
			"get_location_detail":
				var location_name = str(args.get("location_name", ""))
				if not location_name.is_empty():
					var loc_id = _find_node_id_by_name(location_name)
					var node = graph_manager.get_node(loc_id)
					if node.is_empty():
						observation = "Location not found: " + location_name
					else:
						observation = "Location Details for " + node.get("label", loc_id.capitalize()) + " (" + node.get("type", "location") + "):\n"
						observation += "- Description: " + node.get("desc", "No description.") + "\n"
						var props = node.get("properties", {})
						if not props.is_empty():
							observation += "- Properties:\n"
							for key in props.keys():
								observation += "  * " + key + ": " + str(props[key]) + "\n"
				else:
					observation = "Error: get_location_detail requires a 'location_name' argument."
			"get_relationship":
				var entity_a = str(args.get("entity_a", ""))
				var entity_b = str(args.get("entity_b", ""))
				if not entity_a.is_empty() and not entity_b.is_empty():
					var id_a = _find_node_id_by_name(entity_a)
					var id_b = _find_node_id_by_name(entity_b)
					var graph = graph_manager._get_graph()
					var edges: Array = graph.get("edges", [])
					var found_edges: Array = []
					for edge in edges:
						var f = edge.get("from", "")
						var t = edge.get("to", "")
						if (f == id_a and t == id_b) or (f == id_b and t == id_a):
							found_edges.append(edge)
					if found_edges.is_empty():
						observation = "No relationship edges found between " + entity_a + " and " + entity_b
					else:
						observation = "Relationships between " + entity_a + " and " + entity_b + ":\n"
						for edge in found_edges:
							var from_lbl = graph_manager.get_node(edge.get("from", "")).get("label", edge.get("from", ""))
							var to_lbl = graph_manager.get_node(edge.get("to", "")).get("label", edge.get("to", ""))
							observation += "- " + from_lbl + " is [" + edge.get("relation", "") + "] -> " + to_lbl + " (Weight: " + str(edge.get("weight", 1.0)) + ")\n"
				else:
					observation = "Error: get_relationship requires 'entity_a' and 'entity_b' arguments."
			_:
				observation = "Error: Unknown action: " + action
		research_history.append({
			"thought": thought,
			"action": action,
			"args": args,
			"observation": observation
		})
		
	if research_history.is_empty():
		return ""
		
	var log_str = "\n=== AGENTIC RESEARCH FINDINGS ===\n"
	for idx in range(research_history.size()):
		var step = research_history[idx]
		log_str += "Research Step %d:\n" % (idx + 1)
		log_str += "- Thought: %s\n" % step["thought"]
		log_str += "- Action: %s(%s)\n" % [step["action"], JSON.stringify(step["args"])]
		log_str += "- Observation:\n%s\n\n" % step["observation"]
	return log_str


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
	
	CampaignState.set_pending_scene(pending)
	CampaignState.set_turns_since_last_director(0)
	CampaignState.set_director_cooldown(3) # Cooldown of 3 turns
	CampaignState.save()
	
	# If the user is currently idle, consume the scene immediately so they see the result of the plot transition
	if _current_turn_state == TurnState.IDLE:
		consume_pending_scene()


func get_nearby_character_ids() -> Array[String]:
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
		var char_ids = CampaignState.get_character_ids()
		for char_id in char_ids:
			if char_id == "player":
				continue
			var char_data = CampaignState.get_character(char_id)
			if char_data.get("is_creature", false) or not char_data.get("can_speak", true):
				continue
			if not nearby_ids.has(char_id):
				nearby_ids.append(char_id)
		return nearby_ids
		
	var loc_node = graph_manager.get_node(active_location)
	if not loc_node:
		return nearby_ids
		
	var loc_label = loc_node.get("label", active_location).to_lower()
	var char_ids = CampaignState.get_character_ids()
	
	for char_id in char_ids:
		if char_id == "player":
			continue
		if nearby_ids.has(char_id):
			continue
			
		var char_data = CampaignState.get_character(char_id)
		if char_data.get("is_creature", false) or not char_data.get("can_speak", true):
			continue
			
		var is_nearby = false
		
		# 1. Check direct or regional edges in the knowledge graph
		var edges = graph_manager.get_connected_edges(char_id)
		var active_neighbors: Array[String] = []
		for neighbor_id in graph_manager.get_neighbors(active_location):
			var neighbor_node = graph_manager.get_node(neighbor_id)
			if neighbor_node and neighbor_node.get("type", "") in ["location", "environment", "gate"]:
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
			if char_node:
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

# ==============================================================================
# Stream Parser Signal Receivers
# ==============================================================================

func _on_stream_zone_started(zone_name: String) -> void:
	var sender_id = "narrator" if zone_name == "narration" else active_character_id
	stream_started.emit(sender_id)
	if sender_id != "narrator" and not sender_id.is_empty():
		EventBus.character_speaking.emit(sender_id)

func _on_stream_char_received(char_val: String) -> void:
	stream_chunk_logged.emit(active_character_id, char_val)

func _on_stream_zone_ended() -> void:
	stream_zone_ended.emit()

func _exit_tree() -> void:
	trigger_all_pending_summaries()

func trigger_all_pending_summaries() -> void:
	if CampaignState.campaign_id.is_empty() or memory_manager == null:
		return
	var char_ids = CampaignState.get_character_ids()
	for c_id in char_ids:
		var character = CampaignState.get_character(c_id)
		if not character.is_empty() and int(character.get("turns_since_last_summary", 0)) > 0:
			memory_manager.summarize_session_for_character(c_id)
