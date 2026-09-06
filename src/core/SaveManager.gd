# res://src/core/SaveManager.gd
extends RefCounted
class_name SaveManager

const SAVE_DIR = "user://adventures/"

## Returns a list of all saved campaigns in the user://adventures/ directory
static func get_campaign_list() -> Array[Dictionary]:
	var list: Array[Dictionary] = []
	
	# Ensure the save directory exists
	if not DirAccess.dir_exists_absolute(SAVE_DIR):
		DirAccess.make_dir_absolute(SAVE_DIR)
		return list
		
	var dir = DirAccess.open(SAVE_DIR)
	if dir:
		dir.list_dir_begin()
		var file_name = dir.get_next()
		while file_name != "":
			if not dir.current_is_dir() and file_name.ends_with(".json"):
				var full_path = SAVE_DIR + file_name
				var data = _read_json_file(full_path)
				if not data.is_empty() and data.has("adventure_meta"):
					var metadata = data.get("metadata", {})
					var title = data.adventure_meta.get("title", file_name.get_basename())
					var basename = file_name.get_basename()
					if basename.begins_with("autosave_"):
						var num = basename.substr("autosave_".length())
						title = "Auto-Save " + num + " (" + title + ")"
					list.append({
						"id": basename,
						"title": title,
						"last_played": metadata.get("saved_at", data.adventure_meta.get("last_played", "")),
						"playtime_seconds": float(metadata.get("playtime_seconds", 0.0)),
						"engine_version": metadata.get("engine_version", ""),
						"thumbnail": metadata.get("thumbnail", null),
						"active_scene": data.adventure_meta.get("active_scene", "")
					})
			file_name = dir.get_next()
		dir.list_dir_end()
	return list

## Loads a campaign's full JSON save state
static func load_campaign(campaign_id: String) -> Dictionary:
	var path = SAVE_DIR + campaign_id + ".json"
	if not FileAccess.file_exists(path):
		printerr("Save file does not exist: ", path)
		return {}
		
	var data = _read_json_file(path)
	if data.is_empty():
		printerr("Corrupted or empty save file: ", path)
		return {}
		
	# Validation checks to prevent crash on corrupted JSON inputs
	if not data.has("adventure_meta") or not (data["adventure_meta"] is Dictionary):
		printerr("Invalid save schema in file: ", path, " (missing adventure_meta)")
		return {}
		
	# Upgrade mapping if needed
	return _upgrade_save_state(data)

## Saves the campaign state as a structured JSON file
static func save_campaign(campaign_id: String, state_data: Dictionary) -> Error:
	if not DirAccess.dir_exists_absolute(SAVE_DIR):
		DirAccess.make_dir_absolute(SAVE_DIR)
		
	var tmp_path = SAVE_DIR + campaign_id + ".tmp"
	var final_path = SAVE_DIR + campaign_id + ".json"
	
	# Update timestamps
	if state_data.has("adventure_meta"):
		state_data.adventure_meta["last_played"] = Time.get_datetime_string_from_system(true)
		
	# Update metadata
	if not state_data.has("metadata"):
		state_data["metadata"] = {}
	state_data["metadata"]["saved_at"] = Time.get_datetime_string_from_system(true)
	if not state_data["metadata"].has("playtime_seconds"):
		state_data["metadata"]["playtime_seconds"] = 0.0
	if not state_data["metadata"].has("engine_version"):
		state_data["metadata"]["engine_version"] = "1.0.0"
	if not state_data["metadata"].has("thumbnail"):
		state_data["metadata"]["thumbnail"] = null
		
	var file = FileAccess.open(tmp_path, FileAccess.WRITE)
	if not file:
		var err = FileAccess.get_open_error()
		printerr("Failed to write temporary save file: ", tmp_path, " (Error: ", err, ")")
		return err
		
	var json_string = JSON.stringify(state_data, "\t")
	file.store_string(json_string)
	file.close()
	
	# Atomically replace final file
	var dir = DirAccess.open(SAVE_DIR)
	if not dir:
		printerr("Failed to open save directory for atomic rename: ", SAVE_DIR)
		return FAILED
		
	if dir.file_exists(campaign_id + ".json"):
		var err = dir.remove(campaign_id + ".json")
		if err != OK:
			printerr("Failed to remove old save file: ", final_path, " (Error: ", err, ")")
			return err
			
	var err = dir.rename(campaign_id + ".tmp", campaign_id + ".json")
	if err != OK:
		printerr("Failed to rename temporary save file to final: ", final_path, " (Error: ", err, ")")
		return err
		
	return OK

## Initializes a new campaign save state file with default schemas
static func create_campaign(campaign_id: String, title: String) -> Dictionary:
	var default_state = {
		"schema_version": "1.0.0",
		"adventure_meta": {
			"campaign_id": campaign_id,
			"title": title,
			"created_at": Time.get_datetime_string_from_system(true),
			"last_played": Time.get_datetime_string_from_system(true),
			"active_scene": "",
			"active_location": "",
			"active_character": "",
			"art_style": "Digital Anime Art",
			"version": "1.0.0"
		},
		"metadata": {
			"saved_at": Time.get_datetime_string_from_system(true),
			"playtime_seconds": 0.0,
			"engine_version": "1.0.0",
			"thumbnail": null
		},
		"plot_states": {},
		"history_logs": [],
		"knowledge_graph": {
			"nodes": {},
			"edges": []
		}
	}
	var err = save_campaign(campaign_id, default_state)
	if err != OK:
		return {}
	return default_state

## Upgrades older save states to the current version
static func _upgrade_save_state(data: Dictionary) -> Dictionary:
	if not data.has("metadata"):
		var last_played = ""
		if data.has("adventure_meta"):
			last_played = data["adventure_meta"].get("last_played", "")
		if last_played.is_empty():
			last_played = Time.get_datetime_string_from_system(true)
		data["metadata"] = {
			"saved_at": last_played,
			"playtime_seconds": 0.0,
			"engine_version": "1.0.0",
			"thumbnail": null
		}
		
	var current_version = data.get("schema_version", "")
	if current_version.is_empty() and data.has("adventure_meta"):
		current_version = data["adventure_meta"].get("version", "0.0.0")
		
	if current_version == "0.0.0" or current_version.is_empty():
		print("[SaveManager] Upgrading campaign save from older version to 1.0.0")
		if not data.has("schema_version"):
			data["schema_version"] = "1.0.0"
		if not data.has("plot_states"):
			data["plot_states"] = {}
		if not data.has("history_logs"):
			data["history_logs"] = []
		if not data.has("knowledge_graph"):
			data["knowledge_graph"] = {"nodes": {}, "edges": []}
			
		# Migrate legacy characters to knowledge graph nodes
		if data.has("characters") and data["characters"] is Dictionary:
			var legacy_chars = data["characters"]
			var nodes = data["knowledge_graph"].get("nodes", {})
			for char_id in legacy_chars.keys():
				var char_data = legacy_chars[char_id]
				if not nodes.has(char_id):
					nodes[char_id] = {
						"label": char_data.get("name", char_id),
						"type": "character",
						"desc": char_data.get("biography", ""),
						"properties": char_data
					}
			data["knowledge_graph"]["nodes"] = nodes
			data.erase("characters")
			
		var meta = data.get("adventure_meta", {})
		if not meta.has("version"):
			meta["version"] = "1.0.0"
		if not meta.has("active_location"):
			meta["active_location"] = ""
		if not meta.has("active_character"):
			meta["active_character"] = ""
		if not meta.has("intro_narration"):
			meta["intro_narration"] = ""
		data["adventure_meta"] = meta
		
	# Also ensure legacy character migration runs if characters key exists regardless of version
	if data.has("characters"):
		if not data.has("knowledge_graph"):
			data["knowledge_graph"] = {"nodes": {}, "edges": []}
		var legacy_chars = data["characters"]
		if legacy_chars is Dictionary:
			var nodes = data["knowledge_graph"].get("nodes", {})
			for char_id in legacy_chars.keys():
				var char_data = legacy_chars[char_id]
				if not nodes.has(char_id):
					nodes[char_id] = {
						"label": char_data.get("name", char_id),
						"type": "character",
						"desc": char_data.get("biography", ""),
						"properties": char_data
					}
			data["knowledge_graph"]["nodes"] = nodes
		data.erase("characters")
		
	return data

## Helper to read and parse a JSON file
static func _read_json_file(path: String) -> Dictionary:
	var file = FileAccess.open(path, FileAccess.READ)
	if not file:
		printerr("Failed to open file for reading: ", path)
		return {}
		
	var content = file.get_as_text()
	file.close()
	
	var json = JSON.new()
	var err = json.parse(content)
	if err != OK:
		printerr("JSON Parse Error in save file: ", path, " (Line ", json.get_error_line(), ": ", json.get_error_message(), ")")
		return {}
		
	if json.data is Dictionary:
		return json.data
	return {}

## Deletes a campaign's JSON save state file
static func delete_campaign(campaign_id: String) -> Error:
	var path = SAVE_DIR + campaign_id + ".json"
	if not FileAccess.file_exists(path):
		printerr("Save file does not exist: ", path)
		return ERR_FILE_NOT_FOUND
	var dir = DirAccess.open(SAVE_DIR)
	if dir:
		var err = dir.remove(campaign_id + ".json")
		if err != OK:
			printerr("Failed to remove save file: ", path, " (Error: ", err, ")")
		return err
	return FAILED

