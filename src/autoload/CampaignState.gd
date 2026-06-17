# res://src/autoload/CampaignState.gd
extends Node

const SaveManager = preload("res://src/core/SaveManager.gd")

var campaign_id: String = ""
var state: Dictionary = {}

## Initializes the manager with a campaign ID and its save data
func initialize(id: String, save_data: Dictionary) -> void:
	print("[CampaignState] Initializing campaign: ", id)
	campaign_id = id
	state = save_data
	
	# Backwards compatibility / sanity checks
	if not state.has("adventure_meta"):
		state["adventure_meta"] = {"campaign_id": id, "title": id}
	if not state["adventure_meta"].has("writing_style"):
		state["adventure_meta"]["writing_style"] = ""
	if not state["adventure_meta"].has("active_location"):
		state["adventure_meta"]["active_location"] = ""
	if not state["adventure_meta"].has("active_character"):
		state["adventure_meta"]["active_character"] = ""
	if not state.has("plot_states"):
		state["plot_states"] = {}
	if not state.has("characters"):
		state["characters"] = {}
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
		
	var title = state.adventure_meta.get("title", id)
	print("[CampaignState] Campaign '", title, "' save structure initialized.")
	
	EventBus.campaign_loaded.emit(id)
	
# ==============================================================================
# Saves the current dynamic state to the save file
# ==============================================================================
func save() -> Error:
	if campaign_id.is_empty():
		printerr("Cannot save: campaign_id is empty")
		return ERR_UNCONFIGURED
	return SaveManager.save_campaign(campaign_id, state)

# ==============================================================================
# Adventure Meta APIs
# ==============================================================================

func get_campaign_meta(key: String, default = "") -> Variant:
	return state.adventure_meta.get(key, default)

func set_campaign_meta(key: String, value: Variant) -> void:
	state.adventure_meta[key] = value

# ==============================================================================
# Plot State APIs
# ==============================================================================

func get_plot_state(key: String, default = null) -> Variant:
	return state.plot_states.get(key, default)

func set_plot_state(key: String, value: Variant) -> void:
	state.plot_states[key] = value

# ==============================================================================
# Character APIs
# ==============================================================================

func get_character(char_id: String) -> Dictionary:
	return state.characters.get(char_id, {})

func get_character_profile(char_id: String) -> CharacterProfile:
	var char_data = get_character(char_id)
	if char_data.is_empty():
		return null
	return CharacterProfile.from_dict(char_id, char_data)

func init_character(char_id: String, name: String, biography: String = "", writing_style: String = "", avatar: String = "", base_emotion: String = "", base_intensity: float = -1.0) -> void:
	if not state.characters.has(char_id):
		state.characters[char_id] = {
			"name": name,
			"biography": biography,
			"affinity": 0.0,
			"inventory": [],
			"emotions": [],
			"writing_style": writing_style,
			"avatar": avatar,
			"base_emotion": base_emotion if not base_emotion.is_empty() else "serenity",
			"base_intensity": base_intensity if base_intensity >= 0.0 else 0.5
		}
		if not base_emotion.is_empty():
			var valid_emotions = ["serenity", "joy", "sadness", "anger", "fear", "trust", "disgust", "surprise"]
			var emotion = base_emotion.to_lower().strip_edges()
			if not valid_emotions.has(emotion):
				emotion = "serenity"
			add_emotion_event(char_id, emotion, base_intensity if base_intensity >= 0.0 else 0.5, "player", "Initial state.")
	else:
		state.characters[char_id]["writing_style"] = writing_style
		if not avatar.is_empty():
			state.characters[char_id]["avatar"] = avatar

func get_character_avatar(char_id: String) -> Texture2D:
	var character = get_character(char_id)
	var avatar_path = character.get("avatar", "")
	
	if avatar_path.is_empty() or not FileAccess.file_exists(avatar_path):
		avatar_path = ""
		for ext in ["png", "jpg", "jpeg"]:
			var test_path = "user://assets/characters/%s.%s" % [char_id, ext]
			if FileAccess.file_exists(test_path):
				avatar_path = test_path
				break
				
	var texture: Texture2D = null
	if not avatar_path.is_empty() and FileAccess.file_exists(avatar_path):
		var img = Image.load_from_file(avatar_path)
		if img:
			texture = ImageTexture.create_from_image(img)
			
	if not texture:
		texture = load("res://resources/assets/orisonlogo2.png")
		
	return texture

func adjust_affinity(char_id: String, delta: float) -> float:
	var character = get_character(char_id)
	if character.is_empty():
		return 0.0
		
	var current_affinity = character.get("affinity", 0.0)
	var new_affinity = clamp(current_affinity + delta, -1.0, 1.0)
	character["affinity"] = new_affinity
	
	EventBus.emotion_updated.emit(char_id, "", new_affinity)
	return new_affinity

func add_emotion_event(char_id: String, emotion: String, intensity: float, target: String, context: String) -> void:
	var character = get_character(char_id)
	if character.is_empty():
		return
		
	var events: Array = character.get("emotions", [])
	events.append({
		"timestamp": Time.get_datetime_string_from_system(true),
		"emotion": emotion,
		"intensity": clamp(intensity, 0.0, 1.0),
		"target": target,
		"context": context
	})
	
	# Limit emotional log memory to last 20 events to avoid document blowup
	if events.size() > 20:
		events.remove_at(0)
		
	character["emotions"] = events
	
	var current_affinity = character.get("affinity", 0.0)
	EventBus.emotion_updated.emit(char_id, emotion, current_affinity)

# ==============================================================================
# Inventory APIs
# ==============================================================================

func get_inventory(char_id: String) -> Array:
	var character = get_character(char_id)
	if character.is_empty():
		return []
	return character.get("inventory", [])

func add_to_inventory(char_id: String, item_name: String, quantity: int, properties: Dictionary = {}) -> void:
	var character = get_character(char_id)
	if character.is_empty():
		return
		
	var inventory: Array = character.get("inventory", [])
	
	# Search for existing stack
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

func remove_from_inventory(char_id: String, item_name: String, quantity: int) -> bool:
	var character = get_character(char_id)
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

# ==============================================================================
# Conversation History APIs
# ==============================================================================

func add_history_log(role: String, content: String, sender: String = "") -> void:
	var logs: Array = state.get("history_logs", [])
	var entry = {
		"role": role,
		"content": content,
		"timestamp": Time.get_datetime_string_from_system(true)
	}
	if not sender.is_empty():
		entry["sender"] = sender
	logs.append(entry)
	
	# Keep the log bounded (e.g. last 100 entries). Old logs can be archived
	# or summarized into memory graph nodes by background workers.
	if logs.size() > 100:
		logs.remove_at(0)
		
	state["history_logs"] = logs
	EventBus.dialogue_streamed.emit(role, content)

func get_recent_history(limit: int) -> Array:
	var logs: Array = state.get("history_logs", [])
	if logs.is_empty():
		return []
		
	var start = max(0, logs.size() - limit)
	return logs.slice(start)

func log_state_summary() -> void:
	var title = state.adventure_meta.get("title", campaign_id)
	var char_count = state.characters.size()
	var node_count = state.knowledge_graph.get("nodes", {}).size()
	var edge_count = state.knowledge_graph.get("edges", []).size()
	print("[CampaignState] Campaign state active: '", title, "'. Loaded setup details: Characters: ", char_count, " | Graph Nodes: ", node_count, " | Graph Edges: ", edge_count)
