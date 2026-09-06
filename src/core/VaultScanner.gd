# res://src/core/VaultScanner.gd
extends RefCounted
class_name VaultScanner

## Scans a vault path to auto-detect markdown directories, potential scenes, and characters
static func scan_vault(vault_path: String) -> Dictionary:
	var results = {
		"folders": {},              # Directory path (relative) -> suggested type
		"potential_scenes": [],     # Array of Dictionary {"id": String, "label": String, "file_path": String}
		"potential_characters": [], # Array of Dictionary {"id": String, "label": String, "file_path": String}
		"all_files": [],            # Array of Dictionary {"id": String, "label": String, "file_path": String, "body": String}
		"audio_list": []            # Array of String file paths
	}
	
	if not DirAccess.dir_exists_absolute(vault_path):
		printerr("[VaultScanner] Directory does not exist: ", vault_path)
		return results
		
	var file_list: Array[String] = []
	var image_list: Array[String] = []
	var audio_list: Array[String] = []
	_scan_dir_recursive(vault_path, file_list, image_list, audio_list)
	results.audio_list = audio_list
	
	# Group files by their parent folder path (relative to vault_path)
	var folder_files = {} # relative_folder_path -> Array of absolute file_paths
	
	for file_path in file_list:
		var relative_path = file_path.substr(vault_path.length())
		relative_path = relative_path.lstrip("/")
		var folder_path = relative_path.get_base_dir()
		if not folder_files.has(folder_path):
			folder_files[folder_path] = []
		folder_files[folder_path].append(file_path)
		
	# Analyze each folder to auto-detect its type
	for folder in folder_files.keys():
		var files = folder_files[folder]
		var votes = {
			"character": 0,
			"location": 0,
			"scene": 0,
			"lore": 0
		}
		
		# Sample up to 5 files in this folder to vote on the type
		var sample_count = min(5, files.size())
		for i in range(sample_count):
			var file_path = files[i]
			var result = MarkdownParser.parse_file(file_path)
			var fm = result.get("frontmatter", {})
			
			# Only vote if the file explicitly defines a type or orison_type in frontmatter
			if fm.has("type") or fm.has("orison_type"):
				var type = _get_type_safe(fm)
				if type in ["character", "npc"]:
					votes["character"] += 1
				elif type == "location":
					votes["location"] += 1
				elif type in ["scene", "story", "event", "quest"]:
					votes["scene"] += 1
				else:
					votes["lore"] += 1
				
		# Folder name heuristics as a tie-breaker or priority.
		# We split the path into segments and match each against keyword sets.
		# This avoids substring false-negatives (e.g. "entities" does NOT contain "entity").
		var folder_lower = folder.to_lower()
		var heuristic_type = ""
		
		# Split folder path into individual word-level segments
		# e.g. "10_Entities/flora" -> ["10", "entities", "flora"]
		var segments: Array[String] = []
		for part in folder_lower.split("/", false):
			for sub in part.split("_", false):
				for word in sub.split("-", false):
					if not word.is_empty():
						segments.append(word)
		
		# Check deepest folder first (last segment), then full path
		var last_folder = folder_lower.get_file()
		var last_segments: Array[String] = []
		for sub in last_folder.split("_", false):
			for word in sub.split("-", false):
				if not word.is_empty():
					last_segments.append(word)
		
		# Try deepest folder first, then full path as fallback
		heuristic_type = _match_segments_to_type(last_segments)
		if heuristic_type.is_empty():
			heuristic_type = _match_segments_to_type(segments)
		
		print("[VaultScanner] Folder: '%s' | Segments: %s | LastSegments: %s | Heuristic: '%s'" % [folder, str(segments), str(last_segments), heuristic_type])
			
		# Determine winner
		var winner = "lore"
		var max_votes = 0
		for t in votes.keys():
			if votes[t] > max_votes:
				max_votes = votes[t]
				winner = t
				
		# Heuristic wins by default. Only explicit votes for a SPECIFIC type
		# (character/location/scene) can override it. "lore" votes are just
		# unrecognized frontmatter types and should NOT override folder names.
		if not heuristic_type.is_empty():
			if max_votes >= 2 and winner != "lore" and winner != heuristic_type:
				pass # strong explicit votes for a specific type override heuristic
			else:
				winner = heuristic_type
		elif max_votes == 0:
			winner = "lore"
		
		print("[VaultScanner]   -> Winner: '%s' (max_votes=%d)" % [winner, max_votes])
			
		results.folders[folder] = winner
		
	# Build lists of potential scenes and characters
	var total_files = file_list.size()
	var processed_count = 0
	
	for folder in folder_files.keys():
		var type = results.folders[folder]
		var files = folder_files[folder]
		for file_path in files:
			processed_count += 1
			EventBus.scan_progress.emit(processed_count, total_files)
			
			var file_basename = file_path.get_file().get_basename()
			var result = MarkdownParser.parse_file(file_path)
			var fm = result.get("frontmatter", {})
			var node_id = str(fm.get("id", file_basename)).to_lower().replace(" ", "_")
			var label = str(fm.get("name", fm.get("title", file_basename)))
			var body = result.get("body", "")
			
			var entry = {
				"id": node_id,
				"label": label,
				"file_path": file_path,
				"body": body
			}
			
			results.all_files.append(entry)
			
			# We consider a folder mapped to "scene" as containing scenes
			if type == "scene":
				if not body.strip_edges().is_empty():
					results.potential_scenes.append({
						"id": node_id,
						"label": label,
						"file_path": file_path
					})
			elif type == "character":
				results.potential_characters.append({
					"id": node_id,
					"label": label,
					"file_path": file_path
				})
				
	return results

## Recursive helper to list all markdown files and images
static func _scan_dir_recursive(dir_path: String, file_list: Array[String], image_list: Array[String], audio_list: Array[String] = []) -> void:
	var dir = DirAccess.open(dir_path)
	if not dir:
		return
		
	dir.list_dir_begin()
	var file_name = dir.get_next()
	while file_name != "":
		if dir.current_is_dir():
			if not file_name.begins_with("."):
				_scan_dir_recursive(dir_path.path_join(file_name), file_list, image_list, audio_list)
		else:
			var ext = file_name.get_extension().to_lower()
			if ext == "md":
				file_list.append(dir_path.path_join(file_name))
			elif ext in ["png", "jpg", "jpeg"]:
				image_list.append(dir_path.path_join(file_name))
			elif ext in ["ogg", "mp3", "wav"]:
				audio_list.append(dir_path.path_join(file_name))
		file_name = dir.get_next()
	dir.list_dir_end()

## Helper to safely extract a lowercase string type from frontmatter properties
static func _get_type_safe(fm: Dictionary) -> String:
	var type_val = fm.get("type", fm.get("orison_type", "lore"))
	if type_val == null:
		return "lore"
	if type_val is Array:
		if not type_val.is_empty():
			return str(type_val[0]).to_lower().strip_edges()
		return "lore"
	return str(type_val).to_lower().strip_edges()

## Matches an array of word segments against known keyword sets for each type.
## Returns "character", "location", "scene", or "" if no match.
## Uses exact word matching so "entities" must be listed explicitly (not substring).
static func _match_segments_to_type(segs: Array[String]) -> String:
	# Keywords include both singular AND plural forms explicitly.
	var location_keywords := [
		"location", "locations",
		"environment", "environments",
		"world", "worlds",
		"env", "envs",
		"map", "maps",
		"place", "places",
		"setting", "settings",
		"region", "regions",
		"area", "areas",
		"realm", "realms",
		"zone", "zones",
		"city", "cities",
		"town", "towns",
		"village", "villages",
		"dungeon", "dungeons",
		"biome", "biomes",
	]
	var character_keywords := [
		"character", "characters",
		"npc", "npcs",
		"person", "persons",
		"people", "peoples",
		"creature", "creatures",
		"faction", "factions",
		"monster", "monsters",
		"bestiary",
		"fauna",
		"flora",
	]
	var scene_keywords := [
		"scene", "scenes",
		"story", "stories",
		"chapter", "chapters",
		"event", "events",
		"quest", "quests",
		"plot", "plots",
		"arc", "arcs",
		"adventure", "adventures",
		"campaign", "campaigns",
		"session", "sessions",
		"encounter", "encounters",
	]
	var lore_keywords := [
		"lore",
		"system", "systems",
		"rule", "rules",
		"item", "items",
		"artifact", "artifacts",
		"concept", "concepts",
		"literature",
		"note", "notes",
		"template", "templates",
		"meta",
		"prompt", "prompts",
		"mechanic", "mechanics",
		"reference", "references",
		"appendix",
		"glossary",
		"index",
		"gate", "gates",
		"key", "keys",
	]
	
	for seg in segs:
		if seg in location_keywords:
			return "location"
		if seg in character_keywords:
			return "character"
		if seg in scene_keywords:
			return "scene"
		if seg in lore_keywords:
			return "lore"
	
	return ""

