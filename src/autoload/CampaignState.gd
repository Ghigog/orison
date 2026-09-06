# res://src/autoload/CampaignState.gd
extends Node

const SaveManager = preload("res://src/core/SaveManager.gd")

var campaign_id: String = ""
var state: Dictionary = {}
var graph_manager: KnowledgeGraphManager

var _mutex: Mutex = Mutex.new()
var playtime_seconds: float = 0.0
var next_autosave_index: int = 1
var _turns_since_last_autosave: int = 0
var _last_autosave_location: String = ""

func _ready() -> void:
	graph_manager = KnowledgeGraphManager.new()
	# Connect to EventBus signals for autosaving
	EventBus.turn_completed.connect(_on_turn_completed)
	EventBus.location_changed.connect(_on_location_changed)

func _process(delta: float) -> void:
	if not campaign_id.is_empty():
		playtime_seconds += delta

## Initializes the manager with a campaign ID and its save data
func initialize(id: String, save_data: Dictionary) -> void:
	apply_state_change({
		"type": "initialize",
		"id": id,
		"save_data": save_data
	})
	EventBus.campaign_loaded.emit(id)

# ==============================================================================
# Unified Mutex Serialization & Mutation Handler
# ==============================================================================

func apply_state_change(change: Dictionary) -> Variant:
	_mutex.lock()
	
	# Pre-mutation checks (read values before mutation occurs)
	var old_location = ""
	if state.has("adventure_meta"):
		old_location = state["adventure_meta"].get("active_location", "")
		
	var old_affinity = 0.0
	if change.get("type") == "adjust_affinity":
		var char_id = change.get("char_id", "")
		var character = _get_character_internal(char_id)
		if not character.is_empty():
			old_affinity = character.get("affinity", 0.0)

	var result = _apply_change_internal(change)
	_mutex.unlock()
	
	# Synchronous signal emissions outside of the locked mutex (safe from deadlocks)
	var type = change.get("type", "")
	match type:
		"set_campaign_meta":
			var key = change["key"]
			var value = change["value"]
			if key == "active_location" and old_location != value:
				EventBus.location_changed.emit(value)
		"init_character":
			var char_id = change["char_id"]
			EventBus.character_state_updated.emit(char_id)
			var base_emotion = change.get("base_emotion", "")
			if not base_emotion.is_empty():
				var emotion = base_emotion.to_lower().strip_edges()
				EventBus.emotion_updated.emit(char_id, emotion, change.get("affinity", 0.0))
		"adjust_affinity":
			var char_id = change["char_id"]
			EventBus.emotion_updated.emit(char_id, "", result)
			EventBus.character_state_updated.emit(char_id)
		"add_emotion_event":
			var char_id = change["char_id"]
			var current_affinity = get_character(char_id).get("affinity", 0.0)
			EventBus.emotion_updated.emit(char_id, change["emotion"], current_affinity)
			EventBus.character_state_updated.emit(char_id)
		"add_to_inventory", "remove_from_inventory", "update_character_properties", "add_graph_node", "remove_graph_node", "add_graph_edge":
			if change.has("char_id"):
				EventBus.character_state_updated.emit(change["char_id"])
		"add_history_log":
			EventBus.dialogue_streamed.emit(change["role"], change["content"])
			
	return result

func _apply_change_internal(change: Dictionary) -> Variant:
	var type = change.get("type", "")
	match type:
		"initialize":
			campaign_id = change["id"]
			state = change["save_data"]
			
			# Backwards compatibility / sanity checks
			if not state.has("metadata"):
				state["metadata"] = {}
			playtime_seconds = state["metadata"].get("playtime_seconds", 0.0)
			
			if not state.has("adventure_meta"):
				state["adventure_meta"] = {"campaign_id": campaign_id, "title": campaign_id}
			if not state["adventure_meta"].has("writing_style"):
				state["adventure_meta"]["writing_style"] = ""
			if not state["adventure_meta"].has("active_location"):
				state["adventure_meta"]["active_location"] = ""
			if not state["adventure_meta"].has("active_character"):
				state["adventure_meta"]["active_character"] = ""
			if not state["adventure_meta"].has("intro_narration"):
				state["adventure_meta"]["intro_narration"] = ""
			if not state["adventure_meta"].has("art_style"):
				state["adventure_meta"]["art_style"] = "Digital Anime Art"
			if not state.has("plot_states"):
				state["plot_states"] = {}
			if not state.has("history_logs"):
				state["history_logs"] = []
			if not state.has("knowledge_graph"):
				state["knowledge_graph"] = {"nodes": {}, "edges": []}
			if not state.has("memory"):
				state["memory"] = {
					"short_term": "Adventure started.",
					"medium_term": "Starting scene initialized.",
					"long_term": "A new campaign begins."
				}
			if not state.has("pending_scene"):
				state["pending_scene"] = {}
			if not state.has("turns_since_last_director"):
				state["turns_since_last_director"] = 0
			if not state.has("director_cooldown"):
				state["director_cooldown"] = 0
			if not state.has("last_director_beat"):
				state["last_director_beat"] = ""
				
			_turns_since_last_autosave = 0
			_last_autosave_location = state["adventure_meta"].get("active_location", "")
			
			var title = state["adventure_meta"].get("title", campaign_id)
			print("[CampaignState] Campaign '", title, "' save structure initialized (Playtime: ", playtime_seconds, "s).")
			return null
			
		"set_campaign_meta":
			var key = change["key"]
			var value = change["value"]
			state.adventure_meta[key] = value
			return null
			
		"set_plot_state":
			state.plot_states[change["key"]] = change["value"]
			return null
			
		"init_character":
			var char_id = change["char_id"]
			var name = change["name"]
			var biography = change["biography"]
			var writing_style = change["writing_style"]
			var avatar = change["avatar"]
			var base_emotion = change["base_emotion"]
			var base_intensity = change["base_intensity"]
			var affinity = change["affinity"]
			var is_creature = change["is_creature"]
			var can_speak = change["can_speak"]
			var humanoid = change["humanoid"]
			
			if not graph_manager.has_node(char_id):
				var properties = {
					"name": name,
					"biography": biography,
					"affinity": affinity,
					"inventory": [],
					"emotions": [],
					"writing_style": writing_style,
					"avatar": avatar,
					"base_emotion": base_emotion,
					"base_intensity": base_intensity,
					"is_creature": is_creature,
					"can_speak": can_speak,
					"humanoid": humanoid,
					"personality": "",
					"appearance": "",
					"gender": "",
					"goals": ""
				}
				graph_manager._add_node_internal(char_id, name, "character", biography, properties)
				if not base_emotion.is_empty():
					var valid_emotions = ["serenity", "joy", "sadness", "anger", "fear", "trust", "disgust", "surprise"]
					var emotion = base_emotion.to_lower().strip_edges()
					if not valid_emotions.has(emotion):
						emotion = "serenity"
					_add_emotion_event_internal(char_id, emotion, base_intensity if base_intensity >= 0.0 else 0.5, "player", "Initial state.")
			else:
				var properties = graph_manager.get_node(char_id).get("properties", {})
				if not properties.is_empty():
					properties["writing_style"] = writing_style
					properties["is_creature"] = is_creature
					properties["can_speak"] = can_speak
					properties["humanoid"] = humanoid
					if not avatar.is_empty():
						properties["avatar"] = avatar
					
					var node = graph_manager.get_node(char_id)
					node["label"] = name
					node["desc"] = biography
			return null
			
		"adjust_affinity":
			var char_id = change["char_id"]
			var delta = change["delta"]
			var character = _get_character_internal(char_id)
			if character.is_empty():
				return 0.0
				
			var current_affinity = character.get("affinity", 0.0)
			var new_affinity = clamp(current_affinity + delta, -1.0, 1.0)
			character["affinity"] = new_affinity
			return new_affinity
			
		"add_emotion_event":
			var char_id = change["char_id"]
			var emotion = change["emotion"]
			var intensity = change["intensity"]
			var target = change["target"]
			var context = change["context"]
			var rapport_delta = change["rapport_delta"]
			_add_emotion_event_internal(char_id, emotion, intensity, target, context, rapport_delta)
			return null
			
		"add_to_inventory":
			var char_id = change["char_id"]
			var item_name = change["item_name"]
			var quantity = change["quantity"]
			var properties = change["properties"]
			
			var character = _get_character_internal(char_id)
			if character.is_empty():
				return null
				
			var inventory: Array = character.get("inventory", [])
			var found = false
			for item in inventory:
				if item is Dictionary and item.get("item") == item_name:
					item["quantity"] = item.get("quantity", 0) + quantity
					found = true
					break
					
			if not found:
				inventory.append({
					"item": item_name,
					"quantity": quantity,
					"properties": properties
				})
				
			character["inventory"] = inventory
			return null
			
		"remove_from_inventory":
			var char_id = change["char_id"]
			var item_name = change["item_name"]
			var quantity = change["quantity"]
			
			var character = _get_character_internal(char_id)
			if character.is_empty():
				return false
				
			var inventory: Array = character.get("inventory", [])
			var index_to_remove = -1
			var success = false
			
			for i in range(inventory.size()):
				var item = inventory[i]
				if item is Dictionary and item.get("item") == item_name:
					var current_qty = item.get("quantity", 0)
					if current_qty >= quantity:
						item["quantity"] = current_qty - quantity
						success = true
						if item["quantity"] <= 0:
							index_to_remove = i
						break
						
			if index_to_remove != -1:
				inventory.remove_at(index_to_remove)
				
			character["inventory"] = inventory
			return success
			
		"add_history_log":
			var role = change["role"]
			var content = change["content"]
			var sender = change["sender"]
			var active_char = change["active_char"]
			
			var logs: Array = state.get("history_logs", [])
			var entry = {
				"role": role,
				"content": content,
				"timestamp": Time.get_datetime_string_from_system(true)
			}
			if not sender.is_empty():
				entry["sender"] = sender
				
			var char_id = active_char
			if char_id.is_empty():
				char_id = state.adventure_meta.get("active_character", "")
			if not char_id.is_empty():
				entry["active_character"] = char_id
				
			logs.append(entry)
			if logs.size() > 100:
				logs.remove_at(0)
				
			state["history_logs"] = logs
			return null
			
		"set_player_character":
			state["player_character"] = change["pc"]
			return null
			
		"set_memory":
			state["memory"] = change["memory"]
			return null
			
		"set_pending_scene":
			state["pending_scene"] = change["scene"]
			return null
			
		"set_turns_since_last_director":
			state["turns_since_last_director"] = change["turns"]
			return null
			
		"set_last_director_beat":
			state["last_director_beat"] = change["beat"]
			return null
			
		"set_director_cooldown":
			state["director_cooldown"] = change["cooldown"]
			return null
			
		"set_history_logs":
			state["history_logs"] = change["logs"]
			return null
			
		"set_knowledge_graph_data":
			state["knowledge_graph"]["nodes"] = change["nodes"]
			state["knowledge_graph"]["edges"] = change["edges"]
			return null
			
		"update_character_properties":
			var char_id = change["char_id"]
			var props_to_update = change["properties"]
			var character = _get_character_internal(char_id)
			if not character.is_empty():
				for k in props_to_update.keys():
					character[k] = props_to_update[k]
			return null
			
		"add_graph_node":
			graph_manager._add_node_internal(
				change["node_id"],
				change["label"],
				change["node_type"],
				change["description"],
				change["properties"]
			)
			return null
			
		"remove_graph_node":
			graph_manager._remove_node_internal(change["node_id"])
			return null
			
		"add_graph_edge":
			graph_manager._add_edge_internal(
				change["from_node"],
				change["to_node"],
				change["relation"],
				change["weight"]
			)
			return null
			
	return null

func _get_character_internal(char_id: String) -> Dictionary:
	var node = graph_manager.get_node(char_id)
	if node.is_empty():
		return {}
	if node.get("type") != "character" and node.get("type") != "npc":
		return {}
	var props = node.get("properties", {})
	if not props.has("medium_term_memories"):
		props["medium_term_memories"] = []
	if not props.has("long_term_memory"):
		props["long_term_memory"] = ""
	if not props.has("turns_since_last_summary"):
		props["turns_since_last_summary"] = 0
	return props

func _add_emotion_event_internal(char_id: String, emotion: String, intensity: float, target: String, context: String, rapport_delta: float = 0.0) -> void:
	var character = _get_character_internal(char_id)
	if character.is_empty():
		return
		
	var events: Array = character.get("emotions", [])
	events.append({
		"timestamp": Time.get_datetime_string_from_system(true),
		"emotion": emotion,
		"intensity": clamp(intensity, 0.0, 1.0),
		"target": target,
		"context": context,
		"rapport_delta": clamp(rapport_delta, -0.2, 0.2)
	})
	
	if events.size() > 20:
		events.remove_at(0)
		
	character["emotions"] = events

# ==============================================================================
# Saves the current dynamic state to the save file
# ==============================================================================
func save() -> Error:
	if campaign_id.is_empty():
		printerr("Cannot save: campaign_id is empty")
		return ERR_UNCONFIGURED
		
	_mutex.lock()
	if not state.has("metadata"):
		state["metadata"] = {}
	state["metadata"]["playtime_seconds"] = playtime_seconds
	state["metadata"]["engine_version"] = ProjectSettings.get_setting("application/config/version", "1.0.0")
	state["metadata"]["thumbnail"] = _get_thumbnail_base64()
	
	var state_copy = state.duplicate(true)
	_mutex.unlock()
	
	var err = SaveManager.save_campaign(campaign_id, state_copy)
	if err == OK:
		EventBus.campaign_saved.emit()
	return err

func trigger_autosave() -> Error:
	if campaign_id.is_empty():
		return ERR_UNCONFIGURED
		
	# Rotate autosave slot (1 to 3)
	var autosave_id = "autosave_" + str(next_autosave_index)
	next_autosave_index = (next_autosave_index % 3) + 1
	
	_mutex.lock()
	if not state.has("metadata"):
		state["metadata"] = {}
	state["metadata"]["playtime_seconds"] = playtime_seconds
	state["metadata"]["engine_version"] = ProjectSettings.get_setting("application/config/version", "1.0.0")
	state["metadata"]["thumbnail"] = _get_thumbnail_base64()
	
	var state_copy = state.duplicate(true)
	_mutex.unlock()
	
	print("[CampaignState] Rotating autosave triggered. Saving to: ", autosave_id)
	var err = SaveManager.save_campaign(autosave_id, state_copy)
	return err

func _get_thumbnail_base64() -> Variant:
	if DisplayServer.get_name() == "headless":
		return null
	var scene_tree = Engine.get_main_loop() as SceneTree
	if not scene_tree:
		return null
	var root = scene_tree.root
	if not root:
		return null
	var viewport = root.get_viewport()
	if not viewport:
		return null
	var texture = viewport.get_texture()
	if not texture:
		return null
	var img = texture.get_image()
	if not img or img.is_empty():
		return null
		
	img.resize(160, 90, Image.INTERPOLATE_LANCZOS)
	var buffer = img.save_png_to_buffer()
	if buffer.is_empty():
		return null
	return Marshalls.raw_to_base64(buffer)

# ==============================================================================
# Adventure Meta APIs
# ==============================================================================

func get_campaign_meta(key: String, default = "") -> Variant:
	_mutex.lock()
	var val = state.adventure_meta.get(key, default)
	_mutex.unlock()
	return val

func set_campaign_meta(key: String, value: Variant) -> void:
	apply_state_change({
		"type": "set_campaign_meta",
		"key": key,
		"value": value
	})

# ==============================================================================
# Plot State APIs
# ==============================================================================

func get_plot_state(key: String, default = null) -> Variant:
	_mutex.lock()
	var val = state.plot_states.get(key, default)
	_mutex.unlock()
	return val

func set_plot_state(key: String, value: Variant) -> void:
	apply_state_change({
		"type": "set_plot_state",
		"key": key,
		"value": value
	})

# ==============================================================================
# Character APIs
# ==============================================================================

func get_character(char_id: String) -> Dictionary:
	_mutex.lock()
	var character_props = _get_character_internal(char_id)
	_mutex.unlock()
	return character_props

func has_character(char_id: String) -> bool:
	return not get_character(char_id).is_empty()

func get_character_ids() -> Array:
	_mutex.lock()
	var ids = graph_manager.get_nodes_by_type("character").keys()
	_mutex.unlock()
	return ids

func get_character_profile(char_id: String) -> CharacterProfile:
	var char_data = get_character(char_id)
	if char_data.is_empty():
		return null
	return CharacterProfile.from_dict(char_id, char_data)

func init_character(char_id: String, name: String, biography: String = "", writing_style: String = "", avatar: String = "", base_emotion: String = "", base_intensity: float = -1.0, affinity: float = 0.0, is_creature: bool = false, can_speak: bool = true, humanoid: bool = true) -> void:
	apply_state_change({
		"type": "init_character",
		"char_id": char_id,
		"name": name,
		"biography": biography,
		"writing_style": writing_style,
		"avatar": avatar,
		"base_emotion": base_emotion,
		"base_intensity": base_intensity,
		"affinity": affinity,
		"is_creature": is_creature,
		"can_speak": can_speak,
		"humanoid": humanoid
	})

func get_character_avatar(char_id: String) -> Texture2D:
	return await ImageGenManager.get_image_or_fallback(char_id, "avatar")

func adjust_affinity(char_id: String, delta: float) -> float:
	return apply_state_change({
		"type": "adjust_affinity",
		"char_id": char_id,
		"delta": delta
	})

func add_emotion_event(char_id: String, emotion: String, intensity: float, target: String, context: String, rapport_delta: float = 0.0) -> void:
	apply_state_change({
		"type": "add_emotion_event",
		"char_id": char_id,
		"emotion": emotion,
		"intensity": intensity,
		"target": target,
		"context": context,
		"rapport_delta": rapport_delta
	})

# ==============================================================================
# Inventory APIs
# ==============================================================================

func get_inventory(char_id: String) -> Array:
	_mutex.lock()
	var character = _get_character_internal(char_id)
	if character.is_empty():
		_mutex.unlock()
		return []
	var inv = character.get("inventory", [])
	_mutex.unlock()
	return inv

func add_to_inventory(char_id: String, item_name: String, quantity: int, properties: Dictionary = {}) -> void:
	apply_state_change({
		"type": "add_to_inventory",
		"char_id": char_id,
		"item_name": item_name,
		"quantity": quantity,
		"properties": properties
	})

func remove_from_inventory(char_id: String, item_name: String, quantity: int) -> bool:
	return apply_state_change({
		"type": "remove_from_inventory",
		"char_id": char_id,
		"item_name": item_name,
		"quantity": quantity
	})

# ==============================================================================
# Conversation History APIs
# ==============================================================================

func add_history_log(role: String, content: String, sender: String = "", active_char: String = "") -> void:
	apply_state_change({
		"type": "add_history_log",
		"role": role,
		"content": content,
		"sender": sender,
		"active_char": active_char
	})

func get_recent_history(limit: int) -> Array:
	_mutex.lock()
	var logs: Array = state.get("history_logs", [])
	if logs.is_empty():
		_mutex.unlock()
		return []
	var start = max(0, logs.size() - limit)
	var slice = logs.slice(start)
	_mutex.unlock()
	return slice

func log_state_summary() -> void:
	_mutex.lock()
	var title = state.adventure_meta.get("title", campaign_id)
	var char_count = graph_manager.get_nodes_by_type("character").keys().size()
	var node_count = state.knowledge_graph.get("nodes", {}).size()
	var edge_count = state.knowledge_graph.get("edges", []).size()
	_mutex.unlock()
	print("[CampaignState] Campaign state active: '", title, "'. Loaded setup details: Characters: ", char_count, " | Graph Nodes: ", node_count, " | Graph Edges: ", edge_count)

# ==============================================================================
# Thread-safe direct write surrogates
# ==============================================================================

func set_player_character(pc: Dictionary) -> void:
	apply_state_change({
		"type": "set_player_character",
		"pc": pc
	})

func set_memory(memory: Dictionary) -> void:
	apply_state_change({
		"type": "set_memory",
		"memory": memory
	})

func set_pending_scene(scene: Dictionary) -> void:
	apply_state_change({
		"type": "set_pending_scene",
		"scene": scene
	})

func set_turns_since_last_director(turns: int) -> void:
	apply_state_change({
		"type": "set_turns_since_last_director",
		"turns": turns
	})

func set_director_cooldown(cooldown: int) -> void:
	apply_state_change({
		"type": "set_director_cooldown",
		"cooldown": cooldown
	})

func set_history_logs(logs: Array) -> void:
	apply_state_change({
		"type": "set_history_logs",
		"logs": logs
	})

func set_knowledge_graph_data(nodes: Dictionary, edges: Array) -> void:
	apply_state_change({
		"type": "set_knowledge_graph_data",
		"nodes": nodes,
		"edges": edges
	})

func update_character_properties(char_id: String, properties: Dictionary) -> void:
	apply_state_change({
		"type": "update_character_properties",
		"char_id": char_id,
		"properties": properties
	})

func get_turns_since_last_director() -> int:
	_mutex.lock()
	var val = int(state.get("turns_since_last_director", 0))
	_mutex.unlock()
	return val

func get_director_cooldown() -> int:
	_mutex.lock()
	var val = int(state.get("director_cooldown", 0))
	_mutex.unlock()
	return val

func get_pending_scene() -> Dictionary:
	_mutex.lock()
	var val = state.get("pending_scene", {})
	_mutex.unlock()
	return val

func get_memory() -> Dictionary:
	_mutex.lock()
	var val = state.get("memory", {})
	_mutex.unlock()
	return val

func get_last_director_beat() -> String:
	_mutex.lock()
	var val = state.get("last_director_beat", "")
	_mutex.unlock()
	return val

func set_last_director_beat(beat: String) -> void:
	apply_state_change({
		"type": "set_last_director_beat",
		"beat": beat
	})

# ==============================================================================
# Signal Connect handlers for autosave triggers
# ==============================================================================

func _on_turn_completed() -> void:
	if campaign_id.is_empty():
		return
	_turns_since_last_autosave += 1
	var interval = LLMClient.autosave_interval
	if _turns_since_last_autosave >= interval:
		_turns_since_last_autosave = 0
		trigger_autosave()

func _on_location_changed(new_location: String) -> void:
	if campaign_id.is_empty():
		return
	if _last_autosave_location.is_empty():
		_last_autosave_location = new_location
		return
	if new_location != _last_autosave_location:
		_last_autosave_location = new_location
		print("[CampaignState] Location changed from old to '", new_location, "'. Triggering autosave.")
		trigger_autosave()
