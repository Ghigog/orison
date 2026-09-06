# res://src/core/JsonRepair.gd
extends RefCounted
class_name JsonRepair

## Locates, cleans, and extracts a JSON dictionary from an LLM response string
static func extract_json(raw_text: String) -> Dictionary:
	var text = raw_text.strip_edges()
	
	# Find the first '{' and the last '}'
	var start_idx = text.find("{")
	var end_idx = text.rfind("}")
	
	var json_substring = ""
	if start_idx != -1:
		if end_idx == -1 or end_idx <= start_idx:
			# Truncated JSON without a closing brace. Try taking everything from start_idx to the end.
			json_substring = text.substr(start_idx)
		else:
			json_substring = text.substr(start_idx, end_idx - start_idx + 1)
	else:
		return _create_fallback_response(raw_text, "Missing opening bracket '{'")
		
	# Attempt raw parse
	var json = JSON.new()
	var err = json.parse(json_substring)
	if err == OK and json.data is Dictionary:
		return json.data
		
	# Failed to parse raw substring. Let's perform regex-based and structural repairs.
	var repaired = _repair_json_string(json_substring)
	
	err = json.parse(repaired)
	if err == OK and json.data is Dictionary:
		return json.data
		
	# Fallback: model generated text that is completely unparseable as JSON
	var error_msg = "Parse error after repair: %s at line %d" % [json.get_error_message(), json.get_error_line()]
	return _create_fallback_response(raw_text, error_msg)

## Cleans trailing commas, handles simple quotes, and repairs basic syntax issues
static func _repair_json_string(s: String) -> String:
	var cleaned = s
	
	# 1. Convert single quotes to double quotes when appropriate (not inside double quotes)
	var inside_double_quote = false
	var escaped = false
	var repaired_quotes = ""
	for i in range(cleaned.length()):
		var c = cleaned[i]
		if c == '\\':
			escaped = not escaped
			repaired_quotes += c
		elif c == '"' and not escaped:
			inside_double_quote = not inside_double_quote
			repaired_quotes += c
			escaped = false
		elif c == "'" and not escaped and not inside_double_quote:
			repaired_quotes += '"'
			escaped = false
		else:
			repaired_quotes += c
			escaped = false
	cleaned = repaired_quotes

	# 2. Remove trailing commas before closing braces/brackets e.g. , } -> }
	var regex_trailing_comma = RegEx.new()
	regex_trailing_comma.compile(",\\s*([}\\]])")
	cleaned = regex_trailing_comma.sub(cleaned, "$1", true)
	
	# 3. Auto-close any unclosed quotes, braces, and brackets
	var stack = []
	var inside_str = false
	var str_char = ""
	escaped = false
	
	for i in range(cleaned.length()):
		var c = cleaned[i]
		if c == '\\':
			escaped = not escaped
		elif (c == '"' or c == "'") and not escaped:
			if inside_str:
				if c == str_char:
					inside_str = false
					str_char = ""
			else:
				inside_str = true
				str_char = c
			escaped = false
		elif not inside_str:
			if c == '{' or c == '[':
				stack.append(c)
			elif c == '}':
				if not stack.is_empty() and stack[-1] == '{':
					stack.pop_back()
			elif c == ']':
				if not stack.is_empty() and stack[-1] == '[':
					stack.pop_back()
			escaped = false
		else:
			escaped = false
			
	if inside_str:
		cleaned += str_char
		
	while not stack.is_empty():
		var open_char = stack.pop_back()
		if open_char == '{':
			cleaned += "}"
		elif open_char == '[':
			cleaned += "]"
	
	return cleaned

## Creates a default structure wrapping raw text in a valid response schema
static func _create_fallback_response(raw_text: String, error_msg: String = "") -> Dictionary:
	# Check if the raw text is actually a JSON format that just had text before/after it
	# Strip markdown code enclosures e.g. ```json ... ```
	var text = raw_text.strip_edges()
	if text.begins_with("```json"):
		text = text.substr(7)
	elif text.begins_with("```"):
		text = text.substr(3)
		
	if text.ends_with("```"):
		text = text.substr(0, text.length() - 3)
		
	text = text.strip_edges()
	
	var start_idx = text.find("{")
	var end_idx = text.rfind("}")
	if start_idx != -1 and end_idx != -1 and end_idx > start_idx:
		var inner = text.substr(start_idx, end_idx - start_idx + 1)
		var json = JSON.new()
		if json.parse(inner) == OK and json.data is Dictionary:
			return json.data
			
	# If all else fails, return raw text under response and flat emotion
	var response = {
		"response": raw_text,
		"parsing_failed": true,
		"emotional_update": {
			"emotion": "serenity",
			"intensity": 0.5,
			"target": "player",
			"reason": "Fallback parsing due to non-JSON structure",
			"rapport_delta": 0.0
		}
	}
	if not error_msg.is_empty():
		response["error"] = error_msg
	return response

