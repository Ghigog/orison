# res://src/core/LLMStreamParser.gd
extends RefCounted
class_name LLMStreamParser

signal zone_started(zone_name: String) # "narration" or "dialogue"
signal char_received(char_val: String)
signal zone_ended()

var stream_zone: String = "none" # "none", "narration", "dialogue"
var dialogue_buffer: String = ""

func reset() -> void:
	stream_zone = "none"
	dialogue_buffer = ""

func ingest_chunk(chunk: String) -> void:
	if stream_zone == "none":
		dialogue_buffer += chunk
		
		# Look for narration first, then dialogue
		var narr_tag = "\"narration\":"
		var dial_tag = "\"dialogue\":"
		
		var narr_idx = dialogue_buffer.find(narr_tag)
		var dial_idx = dialogue_buffer.find(dial_tag)
		
		if narr_idx != -1 and (dial_idx == -1 or narr_idx < dial_idx):
			var val_after = dialogue_buffer.substr(narr_idx + narr_tag.length()).strip_edges()
			if val_after.begins_with("\""):
				stream_zone = "narration"
				zone_started.emit(stream_zone)
				var content = val_after.substr(1)
				dialogue_buffer = ""
				_process_stream_characters(content)
		elif dial_idx != -1:
			var val_after = dialogue_buffer.substr(dial_idx + dial_tag.length()).strip_edges()
			if val_after.begins_with("\""):
				stream_zone = "dialogue"
				zone_started.emit(stream_zone)
				var content = val_after.substr(1)
				dialogue_buffer = ""
				_process_stream_characters(content)
	else:
		_process_stream_characters(chunk)

func _process_stream_characters(text: String) -> void:
	for i in range(text.length()):
		var char_val = text[i]
		if char_val == "\"" and (i == 0 or text[i-1] != "\\"):
			stream_zone = "none"
			dialogue_buffer = ""
			zone_ended.emit()
			var remainder = text.substr(i + 1)
			if not remainder.is_empty():
				ingest_chunk(remainder)
			break
		else:
			char_received.emit(char_val)
