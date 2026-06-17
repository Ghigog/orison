# res://scripts/state/SaveManager.gd
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
					list.append({
						"id": file_name.get_basename(),
						"title": data.adventure_meta.get("title", file_name.get_basename()),
						"last_played": data.adventure_meta.get("last_played", ""),
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
	return _read_json_file(path)

## Saves the campaign state as a structured JSON file
static func save_campaign(campaign_id: String, state_data: Dictionary) -> Error:
	if not DirAccess.dir_exists_absolute(SAVE_DIR):
		DirAccess.make_dir_absolute(SAVE_DIR)
		
	var path = SAVE_DIR + campaign_id + ".json"
	
	# Update timestamps
	if state_data.has("adventure_meta"):
		state_data.adventure_meta["last_played"] = Time.get_datetime_string_from_system(true)
		
	var file = FileAccess.open(path, FileAccess.WRITE)
	if not file:
		var err = FileAccess.get_open_error()
		printerr("Failed to write save file: ", path, " (Error: ", err, ")")
		return err
		
	var json_string = JSON.stringify(state_data, "\t")
	file.store_string(json_string)
	file.close()
	return OK

## Initializes a new campaign save state file with default schemas
static func create_campaign(campaign_id: String, title: String) -> Dictionary:
	var default_state = {
		"adventure_meta": {
			"campaign_id": campaign_id,
			"title": title,
			"created_at": Time.get_datetime_string_from_system(true),
			"last_played": Time.get_datetime_string_from_system(true),
			"active_scene": "",
			"active_location": "",
			"active_character": ""
		},
		"plot_states": {},
		"characters": {},
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

