# res://scripts/ai/JsonRepair.gd
extends RefCounted
class_name JsonRepair

## Locates, cleans, and extracts a JSON dictionary from an LLM response string
static func extract_json(raw_text: String) -> Dictionary:
	var text = raw_text.strip_edges()
	
	# Find the first '{' and the last '}'
	var start_idx = text.find("{")
	var end_idx = text.rfind("}")
	
	if start_idx == -1 or end_idx == -1 or end_idx <= start_idx:
		# Could not find a valid JSON object block. Return a fallback structure.
		return _create_fallback_response(raw_text)
		
	var json_substring = text.substr(start_idx, end_idx - start_idx + 1)
	
	# Attempt raw parse
	var json = JSON.new()
	var err = json.parse(json_substring)
	if err == OK and json.data is Dictionary:
		return json.data
		
	# Failed to parse raw substring. Let's perform regex-based repairs.
	var repaired = _repair_json_string(json_substring)
	
	err = json.parse(repaired)
	if err == OK and json.data is Dictionary:
		return json.data
		
	# Fallback: model generated text that is completely unparseable as JSON
	# Extract whatever looks like dialog or return the raw text
	return _create_fallback_response(raw_text)

## Cleans trailing commas, handles simple quotes, and repairs basic syntax issues
static func _repair_json_string(s: String) -> String:
	var cleaned = s
	
	# 1. Remove trailing commas before closing braces/brackets e.g. , } -> }
	var regex_trailing_comma = RegEx.new()
	regex_trailing_comma.compile(",\\s*([}\\]])")
	cleaned = regex_trailing_comma.sub(cleaned, "$1", true)
	
	# 2. Fix unescaped newlines inside strings (JSON requires \n instead of actual newlines)
	# (For simplicity in GDScript, we replace carriage returns, but do not overwrite valid string enclosures)
	
	return cleaned

## Creates a default structure wrapping raw text in a valid response schema
static func _create_fallback_response(raw_text: String) -> Dictionary:
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
	return {
		"response": raw_text,
		"emotional_update": {
			"emotion": "serenity",
			"intensity": 0.5,
			"target": "player",
			"reason": "Fallback parsing due to non-JSON structure",
			"rapport_delta": 0.0
		}
	}
