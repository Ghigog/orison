# res://scripts/parser/MarkdownParser.gd
extends RefCounted
class_name MarkdownParser

## Parses a Markdown file, splitting YAML frontmatter from the body content
static func parse_file(path: String) -> Dictionary:
	var file = FileAccess.open(path, FileAccess.READ)
	if not file:
		printerr("Failed to open markdown file: ", path)
		return {"frontmatter": {}, "body": ""}
		
	var lines: Array[String] = []
	while not file.eof_reached():
		lines.append(file.get_line())
	file.close()
	
	var frontmatter_lines: Array[String] = []
	var body_lines: Array[String] = []
	var in_frontmatter = false
	var frontmatter_count = 0
	
	for i in range(lines.size()):
		var line = lines[i]
		var trimmed = line.strip_edges()
		
		if trimmed == "---":
			frontmatter_count += 1
			if frontmatter_count == 1:
				in_frontmatter = true
				continue
			elif frontmatter_count == 2:
				in_frontmatter = false
				continue
				
		if in_frontmatter:
			frontmatter_lines.append(line)
		else:
			# Skip empty leading lines after frontmatter
			if body_lines.is_empty() and trimmed == "":
				continue
			body_lines.append(line)
			
	var frontmatter = _parse_yaml(frontmatter_lines)
	var body = "\n".join(body_lines)
	
	return {
		"frontmatter": frontmatter,
		"body": body
	}

## Extremely simple YAML parser to parse frontmatter key-values
static func _parse_yaml(lines: Array[String]) -> Dictionary:
	var data = {}
	var current_list_key = ""
	
	for line in lines:
		# Skip comments
		var trimmed = line.strip_edges()
		if trimmed.begins_with("#") or trimmed.is_empty():
			continue
			
		# Check for list item
		if trimmed.begins_with("-") and not current_list_key.is_empty():
			var val = trimmed.substr(1).strip_edges()
			val = _strip_quotes(val)
			if data.has(current_list_key) and data[current_list_key] is Array:
				data[current_list_key].append(val)
			continue
			
		var colon_pos = line.find(":")
		if colon_pos != -1:
			var key = line.substr(0, colon_pos).strip_edges()
			var val = line.substr(colon_pos + 1).strip_edges()
			
			# If value is empty, it might be the start of a multiline list
			if val.is_empty():
				current_list_key = key
				data[key] = []
				continue
				
			current_list_key = "" # reset list context
			
			# Check for inline list brackets e.g. [a, b, c]
			if val.begins_with("[") and val.ends_with("]"):
				var items = val.substr(1, val.length() - 2).split(",")
				var arr = []
				for item in items:
					arr.append(_strip_quotes(item.strip_edges()))
				data[key] = arr
			else:
				# Normal value
				val = _strip_quotes(val)
				data[key] = _cast_value(val)
				
	return data

static func _strip_quotes(s: String) -> String:
	if (s.begins_with("\"") and s.ends_with("\"")) or (s.begins_with("'") and s.ends_with("'")):
		return s.substr(1, s.length() - 2)
	return s

static func _cast_value(s: String) -> Variant:
	if s.to_lower() == "true":
		return true
	if s.to_lower() == "false":
		return false
	if s.is_valid_int():
		return s.to_int()
	if s.is_valid_float():
		return s.to_float()
	return s
