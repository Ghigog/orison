# res://src/core/VaultCompiler.gd
extends RefCounted
class_name VaultCompiler


## Recursively scans a vault directory and compiles character notes, locations, and connections
static func compile_vault(vault_path: String, custom_mappings: Dictionary = {}) -> Dictionary:
	var compiled_data = {
		"characters": {},
		"knowledge_graph": {
			"nodes": {},
			"edges": []
		},
		"scenes": {},
		"writing_style": ""
	}
	
	if not DirAccess.dir_exists_absolute(vault_path):
		printerr("Vault directory does not exist: ", vault_path)
		return compiled_data
		
	var file_list: Array[String] = []
	var image_list: Array[String] = []
	_scan_dir_recursive(vault_path, file_list, image_list)
	
	# Keep track of location names to match connections later
	var location_ids = []
	var location_name_to_id = {}
	var node_name_to_id = {}
	
	# First pass: Parse files and extract nodes
	for file_path in file_list:
		var result = MarkdownParser.parse_file(file_path)
		var fm = result.get("frontmatter", {})
		var body = result.get("body", "")
		var file_basename = file_path.get_file().get_basename()
		var node_id = str(fm.get("id", file_basename)).to_lower().replace(" ", "_")
		
		var relative_path = file_path.substr(vault_path.length()).lstrip("/")
		var relative_folder = relative_path.get_base_dir()
		
		var type = _get_type_safe(fm)
		var custom_folders = custom_mappings.get("folders", {})
		var is_mapped = custom_folders.has(relative_folder)
		if is_mapped:
			type = custom_folders[relative_folder]
			
		var file_path_lower = file_path.to_lower()
		
		if not is_mapped:
			var is_scene_path = (
				file_path_lower.contains("/scene") or 
				file_path_lower.contains("/story") or 
				file_path_lower.contains("/chapter") or
				file_path_lower.contains("/event") or
				file_path_lower.contains("/quest") or
				file_path_lower.contains("/plot")
			)
			var is_scene_name = file_basename.to_lower() in ["start", "beginning", "intro", "introduction", "scene_1", "scene1", "chapter_1", "chapter1"]
			
			var relative_folder_lower = relative_folder.to_lower()
			var last_folder = relative_folder_lower.get_file()
			var is_location_path = false
			var is_character_path = false
			
			# Prioritize matching deepest directory component first
			if (
				last_folder.contains("location") or 
				last_folder.contains("environment") or 
				last_folder.contains("world") or 
				last_folder.contains("env") or 
				last_folder.contains("map") or
				last_folder.contains("place") or
				last_folder.contains("setting")
			):
				is_location_path = true
			elif (
				last_folder.contains("character") or 
				last_folder.contains("npc") or 
				last_folder.contains("entity") or
				last_folder.contains("person") or
				last_folder.contains("people")
			):
				is_character_path = true
			else:
				# Fall back to matching anywhere in the folder/file path
				if (
					relative_folder_lower.contains("location") or 
					relative_folder_lower.contains("environment") or 
					relative_folder_lower.contains("world") or 
					relative_folder_lower.contains("env") or 
					relative_folder_lower.contains("map") or
					relative_folder_lower.contains("place") or
					relative_folder_lower.contains("setting") or
					file_path_lower.contains("/location") or
					file_path_lower.contains("/environment") or
					file_path_lower.contains("/world") or
					file_path_lower.contains("/env") or
					file_path_lower.contains("/map") or
					file_path_lower.contains("/place") or
					file_path_lower.contains("/setting")
				):
					is_location_path = true
				elif (
					relative_folder_lower.contains("character") or 
					relative_folder_lower.contains("npc") or 
					relative_folder_lower.contains("entity") or
					relative_folder_lower.contains("person") or
					relative_folder_lower.contains("people") or
					file_path_lower.contains("/character") or
					file_path_lower.contains("/npc") or
					file_path_lower.contains("/entity") or
					file_path_lower.contains("/person") or
					file_path_lower.contains("/people")
				):
					is_character_path = true
			
			if type == "scene" or type == "story" or type == "event" or type == "quest" or is_scene_path or is_scene_name:
				if type != "scene" and type != "story":
					type = "scene"
			elif type == "location" or is_location_path:
				type = "location"
			elif type in ["character", "npc"] or is_character_path:
				type = "character"
		else:
			if type in ["scene", "story", "event", "quest"]:
				type = "scene"
				
		var label = str(fm.get("name", fm.get("title", file_basename)))
		var body_limit = 4000 if (type == "character" or type == "npc") else 500
		var desc = str(fm.get("description", fm.get("desc", body.substr(0, body_limit).strip_edges())))
		
		# Store as knowledge graph node
		compiled_data.knowledge_graph.nodes[node_id] = {
			"label": label,
			"type": type,
			"desc": desc,
			"properties": fm
		}
		
		# Map label and ID universally
		node_name_to_id[label.to_lower()] = node_id
		node_name_to_id[node_id] = node_id
		
		# Map aliases universally
		var aliases_list: Array = []
		var aliases_val = fm.get("aliases", fm.get("alias", []))
		if aliases_val is Array:
			aliases_list = aliases_val
		elif aliases_val is String:
			aliases_list = [aliases_val]
		for alias in aliases_list:
			node_name_to_id[str(alias).to_lower().strip_edges()] = node_id
		
		# Map based on specific types
		match type:
			"character", "npc":
				var writing_style = _extract_character_writing_style(node_id, label, body, fm, file_list)
				
				# Locate character image reference
				var character_image_path = ""
				
				# 1. Frontmatter keys
				var img_keys = ["image", "avatar", "portrait", "sprite", "picture", "cover image", "cover_image", "cover"]
				for key in img_keys:
					if fm.has(key) and fm[key] is String and not fm[key].strip_edges().is_empty():
						var val = fm[key].strip_edges()
						if val.begins_with("[[") and val.ends_with("]]"):
							val = val.substr(2, val.length() - 4).strip_edges()
						var val_base = val.get_file().to_lower()
						for img_path in image_list:
							if img_path.get_file().to_lower() == val_base or img_path.to_lower().ends_with(val.to_lower()):
								character_image_path = img_path
								break
						if not character_image_path.is_empty():
							break
				
				# 2. Body embeds / links
				if character_image_path.is_empty():
					# Check Obsidian ![[image.png]]
					var search_start = 0
					while true:
						var idx = body.find("![[", search_start)
						if idx == -1:
							break
						var end_idx = body.find("]]", idx + 3)
						if end_idx == -1:
							break
						var img_ref = body.substr(idx + 3, end_idx - (idx + 3)).strip_edges()
						if img_ref.contains("|"):
							img_ref = img_ref.split("|")[0].strip_edges()
						var ext = img_ref.get_extension().to_lower()
						if ext in ["png", "jpg", "jpeg"]:
							var val_base = img_ref.get_file().to_lower()
							for img_path in image_list:
								if img_path.get_file().to_lower() == val_base or img_path.to_lower().ends_with(img_ref.to_lower()):
									character_image_path = img_path
									break
						if not character_image_path.is_empty():
							break
						search_start = end_idx + 2
				
				if character_image_path.is_empty():
					# Check Markdown ![alt](image.png)
					var search_start = 0
					while true:
						var idx = body.find("![", search_start)
						if idx == -1:
							break
						var close_bracket = body.find("]", idx + 2)
						if close_bracket == -1:
							break
						if body.substr(close_bracket + 1, 1) == "(":
							var close_paren = body.find(")", close_bracket + 2)
							if close_paren == -1:
								break
							var img_ref = body.substr(close_bracket + 2, close_paren - (close_bracket + 2)).strip_edges()
							var ext = img_ref.get_extension().to_lower()
							if ext in ["png", "jpg", "jpeg"]:
								var val_base = img_ref.get_file().to_lower()
								for img_path in image_list:
									if img_path.get_file().to_lower() == val_base or img_path.to_lower().ends_with(img_ref.to_lower()):
										character_image_path = img_path
										break
							if not character_image_path.is_empty():
								break
							search_start = close_paren + 1
						else:
							search_start = close_bracket + 1
							
				# 2.5 Local App Overrides (for files that shouldn't be edited directly)
				if character_image_path.is_empty():
					var override_map = {
						"marcello": "nameless.jpeg",
						"fik": "nameless.jpeg"
					}
					var node_id_lower = node_id.to_lower()
					if override_map.has(node_id_lower):
						var target_filename = override_map[node_id_lower]
						for img_path in image_list:
							if img_path.get_file().to_lower() == target_filename:
								character_image_path = img_path
								break

				# 3. Fallback: Name/ID Match
				if character_image_path.is_empty():
					var node_id_lower = node_id.to_lower()
					var label_lower = label.to_lower()
					
					# Step 3a: Exact name/id match
					for img_path in image_list:
						var img_basename_lower = img_path.get_file().get_basename().to_lower()
						if img_basename_lower == node_id_lower or img_basename_lower == label_lower:
							character_image_path = img_path
							break
							
					# Step 3b: Starts with name/id match (e.g. marcello_silhouette.png)
					if character_image_path.is_empty():
						for img_path in image_list:
							var img_basename_lower = img_path.get_file().get_basename().to_lower()
							if img_basename_lower.begins_with(node_id_lower) or img_basename_lower.begins_with(label_lower):
								character_image_path = img_path
								break
								
					# Step 3c: Substring match (e.g. contains "marcello")
					if character_image_path.is_empty():
						for img_path in image_list:
							var img_basename_lower = img_path.get_file().get_basename().to_lower()
							if img_basename_lower.contains(node_id_lower) or img_basename_lower.contains(label_lower):
								character_image_path = img_path
								break
							
				# Copy image if found
				var target_avatar_path = ""
				if not character_image_path.is_empty():
					var ext = character_image_path.get_extension().to_lower()
					var target_dir = "user://assets/characters"
					var dir = DirAccess.open("user://")
					if dir:
						if not dir.dir_exists("assets/characters"):
							dir.make_dir_recursive("assets/characters")
					
					var target_path = target_dir.path_join("%s.%s" % [node_id, ext])
					var copy_err = DirAccess.copy_absolute(character_image_path, target_path)
					if copy_err == OK:
						print("[VaultCompiler] Successfully copied image for ", node_id, " to ", target_path)
						target_avatar_path = target_path
					else:
						printerr("[VaultCompiler] Failed to copy image for ", node_id, " error: ", copy_err)
				
				compiled_data.characters[node_id] = {
					"name": label,
					"biography": desc,
					"affinity": float(fm.get("base_affinity", fm.get("affinity", 0.0))),
					"base_emotion": str(fm.get("base_emotion", "")),
					"base_intensity": float(fm.get("base_emotion_intensity", fm.get("base_intensity", -1.0))),
					"inventory": [],
					"emotions": [],
					"writing_style": writing_style,
					"avatar": target_avatar_path
				}
				
				# Also update the knowledge graph node
				if compiled_data.knowledge_graph.nodes.has(node_id):
					compiled_data.knowledge_graph.nodes[node_id]["avatar"] = target_avatar_path
					compiled_data.knowledge_graph.nodes[node_id]["properties"]["avatar"] = target_avatar_path
			"location":
				location_ids.append(node_id)
				location_name_to_id[label.to_lower()] = node_id
				location_name_to_id[node_id] = node_id
			"scene", "story":
				compiled_data.scenes[node_id] = {
					"title": label,
					"body": body,
					"properties": fm
				}
				
	# If no scenes were found by type or path/name heuristics, find a fallback scene
	if compiled_data.scenes.is_empty():
		for file_path in file_list:
			var file_basename = file_path.get_file().get_basename()
			var file_basename_lower = file_basename.to_lower()
			if file_basename_lower in ["writing_style", "style", "readme"]:
				continue
			var result = MarkdownParser.parse_file(file_path)
			var body = result.get("body", "")
			if body.strip_edges().is_empty():
				continue
				
			var fm = result.get("frontmatter", {})
			var type = _get_type_safe(fm)
			var file_path_lower = file_path.to_lower()
			
			# Ignore known non-scene directories and types
			if (
				type in ["character", "npc", "location", "concept", "lore", "flora", "fauna", "item", "gate", "rule", "environment"] or
				file_path_lower.contains("/concepts/") or
				file_path_lower.contains("/systems/") or
				file_path_lower.contains("/gates/") or
				file_path_lower.contains("/rules/") or
				file_path_lower.contains("/templates/") or
				file_path_lower.contains("/meta/")
			):
				continue
				
			var node_id = str(fm.get("id", file_basename)).to_lower().replace(" ", "_")
			var label = str(fm.get("name", fm.get("title", file_basename)))
			compiled_data.scenes[node_id] = {
				"title": label,
				"body": body,
				"properties": fm
			}
			# Also update its type in the knowledge graph if it exists there
			if compiled_data.knowledge_graph.nodes.has(node_id):
				compiled_data.knowledge_graph.nodes[node_id]["type"] = "scene"
			break
				
	# Second pass: Process connections and relations to build graph edges
	for file_path in file_list:
		var result = MarkdownParser.parse_file(file_path)
		var fm = result.get("frontmatter", {})
		var file_basename = file_path.get_file().get_basename()
		var node_id = str(fm.get("id", file_basename)).to_lower().replace(" ", "_")
		
		# Parse location connections
		if fm.has("connections"):
			var connections = fm.get("connections")
			var connection_list: Array = []
			if connections is Array:
				connection_list = connections
			elif connections is String:
				connection_list = [connections]
				
			for conn in connection_list:
				var conn_str = str(conn).to_lower().strip_edges()
				var target_id = conn_str.replace(" ", "_")
				# Match by name or ID
				if location_name_to_id.has(conn_str):
					target_id = location_name_to_id[conn_str]
					
				if compiled_data.knowledge_graph.nodes.has(target_id):
					compiled_data.knowledge_graph.edges.append({
						"from": node_id,
						"to": target_id,
						"relation": "connected_to",
						"weight": 1.0
					})
					
		# Parse character relationships
		if fm.has("relationships"):
			var relations = fm.get("relationships")
			if relations is Dictionary:
				for rel_target in relations.keys():
					var target_id = str(rel_target).to_lower().replace(" ", "_")
					var relation_desc = str(relations[rel_target])
					compiled_data.knowledge_graph.edges.append({
						"from": node_id,
						"to": target_id,
						"relation": relation_desc,
						"weight": 0.8
					})
					
		# Parse connections from tags (frontmatter)
		var tags_list: Array = []
		var tags_val = fm.get("tags", fm.get("tag", []))
		if tags_val is Array:
			tags_list = tags_val
		elif tags_val is String:
			tags_list = [tags_val]
			
		for tag in tags_list:
			var tag_str = str(tag).to_lower().strip_edges()
			var target_id = tag_str.replace(" ", "_")
			if location_name_to_id.has(tag_str):
				target_id = location_name_to_id[tag_str]
			if location_ids.has(target_id) and target_id != node_id:
				var edge_exists = false
				for edge in compiled_data.knowledge_graph.edges:
					if edge.from == node_id and edge.to == target_id and edge.relation == "associated_with":
						edge_exists = true
						break
				if not edge_exists:
					compiled_data.knowledge_graph.edges.append({
						"from": node_id,
						"to": target_id,
						"relation": "associated_with",
						"weight": 1.0
					})

		# Parse connections from wiki-links in the body
		var body = result.get("body", "")
		var search_start = 0
		while true:
			var idx = body.find("[[", search_start)
			if idx == -1:
				break
			var end_idx = body.find("]]", idx + 2)
			if end_idx == -1:
				break
			var link_content = body.substr(idx + 2, end_idx - (idx + 2)).strip_edges()
			search_start = end_idx + 2
			
			if link_content.is_empty():
				continue
				
			if link_content.contains("|"):
				link_content = link_content.split("|")[0].strip_edges()
				
			var target_str = link_content.to_lower()
			var target_id = target_str.replace(" ", "_")
			if node_name_to_id.has(target_str):
				target_id = node_name_to_id[target_str]
				
			if compiled_data.knowledge_graph.nodes.has(target_id) and target_id != node_id:
				var target_node = compiled_data.knowledge_graph.nodes[target_id]
				var relation = "links_to"
				if target_node.get("type") == "location":
					relation = "associated_with"
					
				var edge_exists = false
				for edge in compiled_data.knowledge_graph.edges:
					if edge.from == node_id and edge.to == target_id and edge.relation == relation:
						edge_exists = true
						break
				if not edge_exists:
					compiled_data.knowledge_graph.edges.append({
						"from": node_id,
						"to": target_id,
						"relation": relation,
						"weight": 1.0
					})
					
	# Extract campaign-wide writing style
	var campaign_writing_style = ""
	
	# Look for writing_style.md or style.md first
	for file_path in file_list:
		var file_basename = file_path.get_file().get_basename().to_lower()
		if file_basename == "writing_style" or file_basename == "style":
			var result = MarkdownParser.parse_file(file_path)
			campaign_writing_style = result.get("body", "").strip_edges()
			break
			
	# If still empty, check if any scene/lore frontmatter has writing_style
	if campaign_writing_style.is_empty():
		for file_path in file_list:
			var result = MarkdownParser.parse_file(file_path)
			var fm = result.get("frontmatter", {})
			var type = _get_type_safe(fm)
			if type != "character" and type != "npc":
				var style = fm.get("writing_style", fm.get("style", ""))
				if style is String and not style.strip_edges().is_empty():
					campaign_writing_style = style.strip_edges()
					break
				
	# If still empty, gather snippets from scene note bodies
	if campaign_writing_style.is_empty():
		var scene_snippets: Array[String] = []
		for file_path in file_list:
			var result = MarkdownParser.parse_file(file_path)
			var fm = result.get("frontmatter", {})
			var type = _get_type_safe(fm)
			if type == "scene" or type == "story":
				var body = result.get("body", "").strip_edges()
				if not body.is_empty():
					var snippet = body.substr(0, 250)
					if body.length() > 250:
						snippet += "..."
					scene_snippets.append(snippet)
					if scene_snippets.size() >= 2:
						break
		if not scene_snippets.is_empty():
			campaign_writing_style = "Representative Prose Style:\n" + "\n---\n".join(scene_snippets)
			
	compiled_data["writing_style"] = campaign_writing_style
	

		
	# Re-order starting character if explicitly selected
	var starting_char_id = custom_mappings.get("starting_character_id", "")
	if not starting_char_id.is_empty() and compiled_data.characters.has(starting_char_id):
		var target_char = compiled_data.characters[starting_char_id]
		compiled_data.characters.erase(starting_char_id)
		var new_chars = {starting_char_id: target_char}
		for k in compiled_data.characters.keys():
			new_chars[k] = compiled_data.characters[k]
		compiled_data.characters = new_chars
		
	return compiled_data

## Helper method to extract character-specific writing style from the vault files
static func _extract_character_writing_style(char_id: String, char_name: String, char_body: String, fm: Dictionary, file_list: Array[String]) -> String:
	var traits_prefix = ""
	var traits_val = fm.get("traits", fm.get("personality", fm.get("voice", fm.get("tone", ""))))
	if traits_val != null:
		var traits_str = ""
		if traits_val is Array:
			traits_str = ", ".join(traits_val)
		else:
			traits_str = str(traits_val)
		if not traits_str.strip_edges().is_empty():
			traits_prefix = "Personality Traits/Tone: " + traits_str.strip_edges() + "\n"

	var style = fm.get("writing_style", fm.get("dialogue_style", fm.get("style", "")))
	if style is String and not style.strip_edges().is_empty():
		return traits_prefix + style.strip_edges()
		
	var snippets: Array[String] = []
	var body_lines = char_body.split("\n")
	
	# 1. Extract from body under headers like ## Writing Style, ## Dialogue Style, ## Dialogue, ## Quotes, ## Personality, ## Voice
	var headers = [
		"## writing style", "## dialogue style", "## dialogue", "## quotes", "## personality", "## voice",
		"### writing style", "### dialogue style", "### dialogue", "### quotes", "### personality", "### voice"
	]
	var in_header_section = false
	var header_content: Array[String] = []
	for line in body_lines:
		var trimmed = line.strip_edges()
		if trimmed.begins_with("#"):
			if in_header_section:
				break
			var lower_trimmed = trimmed.to_lower()
			for h in headers:
				if lower_trimmed.begins_with(h):
					in_header_section = true
					break
			continue
		if in_header_section:
			header_content.append(line)
			
	if not header_content.is_empty():
		var extracted = "\n".join(header_content).strip_edges()
		if not extracted.is_empty():
			return traits_prefix + extracted
			
	# 2. Extract blockquotes from the character note body
	for line in body_lines:
		var trimmed = line.strip_edges()
		if trimmed.begins_with(">"):
			var quote = trimmed.substr(1).strip_edges()
			quote = MarkdownParser._strip_quotes(quote)
			if not quote.is_empty():
				snippets.append(quote)
				
	# 3. Scan all other files in the vault for spoken lines by this character
	var id_lower = char_id.to_lower()
	for file_path in file_list:
		var base_lower = file_path.get_file().get_basename().to_lower()
		if base_lower == id_lower or base_lower.contains(id_lower) or base_lower.contains(char_name.to_lower()):
			continue
			
		var res = MarkdownParser.parse_file(file_path)
		var f_body = res.get("body", "")
		var f_lines = f_body.split("\n")
		for line in f_lines:
			var trimmed = line.strip_edges()
			var is_match = false
			var dialog_text = ""
			
			var prefix_matches = [
				char_name + ":",
				"**" + char_name + "**:",
				"*" + char_name + "*:",
				char_id.capitalize() + ":",
				"**" + char_id.capitalize() + "**:"
			]
			for pm in prefix_matches:
				if trimmed.begins_with(pm):
					is_match = true
					dialog_text = trimmed.substr(pm.length()).strip_edges()
					break
					
			if not is_match:
				var says_patterns = [
					char_name + " says, \"",
					char_name + " said, \"",
					char_name + " says \"",
					char_name + " said \""
				]
				for sp in says_patterns:
					var idx = trimmed.findn(sp)
					if idx != -1:
						is_match = true
						dialog_text = trimmed.substr(idx + sp.length()).strip_edges()
						if dialog_text.ends_with("\""):
							dialog_text = dialog_text.substr(0, dialog_text.length() - 1)
						break
						
			if is_match and not dialog_text.is_empty():
				dialog_text = MarkdownParser._strip_quotes(dialog_text)
				if not snippets.has(dialog_text):
					snippets.append(dialog_text)
					if snippets.size() >= 5:
						break
		if snippets.size() >= 5:
			break
			
	if not snippets.is_empty():
		var joined = ""
		for snippet in snippets:
			joined += "- \"%s\"\n" % snippet
		return traits_prefix + joined.strip_edges()
		
	return traits_prefix.strip_edges()

## Recursive directory helper
static func _scan_dir_recursive(dir_path: String, file_list: Array[String], image_list: Array[String]) -> void:
	var dir = DirAccess.open(dir_path)
	if not dir:
		return
		
	dir.list_dir_begin()
	var file_name = dir.get_next()
	while file_name != "":
		if dir.current_is_dir():
			if file_name != "." and file_name != "..":
				_scan_dir_recursive(dir_path.path_join(file_name), file_list, image_list)
		else:
			var ext = file_name.get_extension().to_lower()
			if ext == "md":
				file_list.append(dir_path.path_join(file_name))
			elif ext in ["png", "jpg", "jpeg"]:
				image_list.append(dir_path.path_join(file_name))
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
