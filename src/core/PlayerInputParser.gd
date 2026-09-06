# res://src/core/PlayerInputParser.gd
extends RefCounted
class_name PlayerInputParser

## Sanitizes player input string to strip and escape prompt injection keywords and delimiters.
static func sanitize_input(input_text: String) -> String:
	# 1. Escape early closing delimiters to prevent breaking out of the container
	var sanitized = input_text
	sanitized = sanitized.replace("<player_message>", "[player_message]")
	sanitized = sanitized.replace("</player_message>", "[/player_message]")
	sanitized = sanitized.replace("<system>", "[system]")
	sanitized = sanitized.replace("</system>", "[/system]")
	sanitized = sanitized.replace("<user>", "[user]")
	sanitized = sanitized.replace("</user>", "[/user]")
	sanitized = sanitized.replace("<assistant>", "[assistant]")
	sanitized = sanitized.replace("</assistant>", "[/assistant]")

	# 2. Process line-by-line to strip injection prefixes at start of lines (case-insensitive)
	var lines = sanitized.split("\n")
	var sanitized_lines: Array[String] = []
	
	# Prefixes to check/strip (case-insensitive)
	var prefixes_to_strip = [
		"system:",
		"user:",
		"assistant:",
		"narrator:",
		"player:",
		"[inst]",
		"[/inst]",
		"<<sys>>",
		"<</sys>>"
	]
	
	for line in lines:
		var trimmed_line = line.strip_edges()
		var lower_line = trimmed_line.to_lower()
		var matched = false
		
		for prefix in prefixes_to_strip:
			if lower_line.begins_with(prefix):
				var prefix_len = prefix.length()
				var idx = line.to_lower().find(prefix)
				if idx != -1:
					var remaining = line.substr(idx + prefix_len).strip_edges()
					sanitized_lines.append(remaining)
				else:
					sanitized_lines.append(line)
				matched = true
				break
				
		if not matched:
			sanitized_lines.append(line)
			
	var final_result = "\n".join(sanitized_lines)
	
	if final_result != input_text:
		print("[PromptSanitizer] Sanitized input. Original: '%s' | Sanitized: '%s'" % [input_text, final_result])
		
	return final_result


## Parses player input string into dialogue and action segments based on standard visual novel styles.
static func parse_input(input_text: String) -> Dictionary:
	var trimmed = input_text.strip_edges()
	if trimmed.is_empty():
		return {
			"dialogue": "",
			"action": "",
			"detected_format": "none"
		}
	
	# Style 1: Dialogue in Quotes ("take a look at that"! I shout)
	# Check if double quotes are present
	if trimmed.contains("\""):
		return _parse_quotes(trimmed)
		
	# Style 2: Action in Asterisks (take a look at that *shouting*)
	# Check if asterisks are present
	if trimmed.contains("*"):
		return _parse_asterisks(trimmed)
		
	# Style 3: Pure action or context
	# Check if it starts with pronoun/narrative prefix patterns
	if _is_pure_action_heuristic(trimmed):
		return {
			"dialogue": "",
			"action": trimmed,
			"detected_format": "pure_action"
		}
		
	# Default/Undifferentiated: No punctuation styling, could be either bare dialogue or mixed
	return {
		"dialogue": trimmed,
		"action": "",
		"detected_format": "none"
	}

static func _parse_quotes(text: String) -> Dictionary:
	var dialogue_parts: Array[String] = []
	var action_parts: Array[String] = []
	var in_quotes := false
	var current_part := ""
	var i := 0
	
	while i < text.length():
		var c = text[i]
		if c == '"':
			var stripped = current_part.strip_edges()
			if in_quotes:
				if not stripped.is_empty():
					dialogue_parts.append(stripped)
			else:
				# Strip punctuation leftovers like commas or exclamation points outside quotes if they lead action
				if not stripped.is_empty():
					action_parts.append(stripped)
			current_part = ""
			in_quotes = !in_quotes
		else:
			current_part += c
		i += 1
		
	var final_stripped = current_part.strip_edges()
	if not final_stripped.is_empty():
		if in_quotes:
			dialogue_parts.append(final_stripped)
		else:
			action_parts.append(final_stripped)
			
	var dialogue = " ".join(dialogue_parts)
	var action = " ".join(action_parts)
	
	# Clean up leftover characters at the start and end of actions
	action = _clean_action_remnants(action)
	
	return {
		"dialogue": dialogue,
		"action": action,
		"detected_format": "quotes"
	}

static func _parse_asterisks(text: String) -> Dictionary:
	var dialogue_parts: Array[String] = []
	var action_parts: Array[String] = []
	var in_asterisks := false
	var current_part := ""
	var i := 0
	
	while i < text.length():
		var c = text[i]
		if c == '*':
			var stripped = current_part.strip_edges()
			if in_asterisks:
				if not stripped.is_empty():
					action_parts.append(stripped)
			else:
				if not stripped.is_empty():
					dialogue_parts.append(stripped)
			current_part = ""
			in_asterisks = !in_asterisks
		else:
			current_part += c
		i += 1
		
	var final_stripped = current_part.strip_edges()
	if not final_stripped.is_empty():
		if in_asterisks:
			action_parts.append(final_stripped)
		else:
			dialogue_parts.append(final_stripped)
			
	var dialogue = " ".join(dialogue_parts)
	var action = " ".join(action_parts)
	
	# Clean up leftover characters
	dialogue = dialogue.strip_edges()
	action = action.strip_edges()
	
	return {
		"dialogue": dialogue,
		"action": action,
		"detected_format": "asterisks"
	}

static func _is_pure_action_heuristic(text: String) -> bool:
	# Check common pronoun-led or slash command start patterns
	var lower = text.to_lower()
	var pronouns = ["i ", "i'm ", "i've ", "i'll ", "i'd ", "we ", "we're ", "we've ", "we'll ", "we'd ", "he ", "she ", "they ", "me ", "my "]
	for p in pronouns:
		if lower.begins_with(p):
			return true
	if lower.begins_with("/"):
		return true
	return false

static func _clean_action_remnants(action_text: String) -> String:
	# Clean typical dialogue-to-action punctuation remnants (e.g. `! I shout`, `, I shout`)
	var cleaned = action_text.strip_edges()
	if cleaned.is_empty():
		return ""
	# Remove leading punctuation characters commonly left outside quotes
	while cleaned.length() > 0 and (cleaned[0] == '!' or cleaned[0] == ',' or cleaned[0] == '.' or cleaned[0] == '?' or cleaned[0] == ';' or cleaned[0] == ':' or cleaned[0] == ' '):
		cleaned = cleaned.substr(1).strip_edges()
	# Remove trailing punctuation characters commonly left outside quotes
	while cleaned.length() > 0 and (cleaned[-1] == '!' or cleaned[-1] == ',' or cleaned[-1] == '.' or cleaned[-1] == '?' or cleaned[-1] == ';' or cleaned[-1] == ':' or cleaned[-1] == ' '):
		cleaned = cleaned.substr(0, cleaned.length() - 1).strip_edges()
	return cleaned
