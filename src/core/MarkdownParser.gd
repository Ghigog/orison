# res://src/core/MarkdownParser.gd
extends RefCounted
class_name MarkdownParser

## Parses a Markdown file, splitting YAML frontmatter from the body content
static func parse_file(path: String) -> Dictionary:
	var file = FileAccess.open(path, FileAccess.READ)
	if not file:
		printerr("Failed to open markdown file: ", path)
		return {
			"frontmatter": {},
			"body": "",
			"wiki_links": [],
			"tags": [],
			"callouts": [],
			"tables": []
		}
		
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
	
	var wiki_links = _extract_wiki_links(body)
	var hashtags = _extract_hashtags(body)
	var callouts = _extract_callouts(body_lines)
	var tables = _extract_tables(body_lines)
	
	return {
		"frontmatter": frontmatter,
		"body": body,
		"wiki_links": wiki_links,
		"tags": hashtags,
		"callouts": callouts,
		"tables": tables
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

## Extracts wiki-links of the form [[Target Entity]] or [[Target Entity|Display Label]]
static func _extract_wiki_links(body: String) -> Array[Dictionary]:
	var wiki_links: Array[Dictionary] = []
	var regex = RegEx.new()
	regex.compile("\\[\\[([^\\]]+)\\]\\]")
	var matches = regex.search_all(body)
	for m in matches:
		var content = m.get_string(1).strip_edges()
		if content.is_empty():
			continue
		var target = content
		var label = content
		if content.contains("|"):
			var parts = content.split("|")
			target = parts[0].strip_edges()
			if parts.size() > 1:
				label = parts[1].strip_edges()
		
		wiki_links.append({
			"target": target,
			"label": label
		})
	return wiki_links

## Extracts hashtags of the form #tag/subtag, ignoring hex colors and headings
static func _extract_hashtags(body: String) -> Array[String]:
	var hashtags: Array[String] = []
	var regex = RegEx.new()
	regex.compile("(?:^|\\s)#([a-zA-Z0-9_\\-/]+)")
	var matches = regex.search_all(body)
	for m in matches:
		var tag = m.get_string(1).strip_edges()
		var is_hex = false
		if tag.length() == 3 or tag.length() == 6:
			var hex_regex = RegEx.new()
			hex_regex.compile("^[a-fA-F0-9]+$")
			if hex_regex.search(tag):
				is_hex = true
		if not is_hex and not tag.is_empty():
			hashtags.append(tag)
	return hashtags

## Extracts callout blocks of the form > [!info] or > [!secret]
static func _extract_callouts(lines: Array[String]) -> Array[Dictionary]:
	var callouts: Array[Dictionary] = []
	var in_callout = false
	var current_callout = {}
	
	var callout_header_regex = RegEx.new()
	callout_header_regex.compile("^>\\s*\\[!([a-zA-Z0-9_\\-]+)\\](.*)")
	
	for line in lines:
		var trimmed = line.strip_edges()
		var match_header = callout_header_regex.search(trimmed)
		if match_header:
			if in_callout:
				callouts.append(current_callout)
			var type = match_header.get_string(1).to_lower()
			var title = match_header.get_string(2).strip_edges()
			in_callout = true
			current_callout = {
				"type": type,
				"title": title,
				"content": ""
			}
		elif in_callout and trimmed.begins_with(">"):
			var content_line = trimmed.substr(1).strip_edges()
			if current_callout["content"].is_empty():
				current_callout["content"] = content_line
			else:
				current_callout["content"] += "\n" + content_line
		elif in_callout:
			callouts.append(current_callout)
			in_callout = false
			current_callout = {}
			
	if in_callout:
		callouts.append(current_callout)
		
	return callouts

## Extracts Markdown tables from the document lines
static func _extract_tables(lines: Array[String]) -> Array[Dictionary]:
	var tables: Array[Dictionary] = []
	var current_table = null
	var table_state = 0 # 0: outside, 1: found header, 2: parsing data rows
	var potential_headers: Array[String] = []
	
	for line in lines:
		var trimmed = line.strip_edges()
		if trimmed.begins_with("|") and trimmed.ends_with("|"):
			var cells = _parse_table_row(trimmed)
			if table_state == 0:
				potential_headers = cells
				table_state = 1
			elif table_state == 1:
				var is_separator = true
				for cell in cells:
					var c_trimmed = cell.strip_edges()
					if c_trimmed.is_empty():
						continue
					var is_sep_cell = true
					for char in c_trimmed:
						if char != "-" and char != ":":
							is_sep_cell = false
							break
					if not is_sep_cell:
						is_separator = false
						break
				if is_separator:
					current_table = {
						"headers": potential_headers,
						"rows": []
					}
					table_state = 2
				else:
					potential_headers = cells
					table_state = 1
			elif table_state == 2:
				current_table["rows"].append(cells)
		else:
			if table_state == 2 and current_table != null:
				tables.append(current_table)
			table_state = 0
			current_table = null
			potential_headers = []
			
	if table_state == 2 and current_table != null:
		tables.append(current_table)
		
	return tables

static func _parse_table_row(row: String) -> Array[String]:
	var content = row.strip_edges()
	if content.begins_with("|"):
		content = content.substr(1)
	if content.ends_with("|"):
		content = content.substr(0, content.length() - 1)
	var parts = content.split("|")
	var result: Array[String] = []
	for p in parts:
		result.append(p.strip_edges())
	return result
