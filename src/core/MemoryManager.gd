# res://src/core/MemoryManager.gd
extends RefCounted
class_name MemoryManager

const COMPACTION_THRESHOLD = 30
const COMPACTION_KEEP_COUNT = 10
const LTM_THRESHOLD = 10

var game_loop_controller: Node

# Lock dictionaries to prevent concurrent overlapping LLM requests for the same character
var _active_compactions: Dictionary = {}
var _active_distillations: Dictionary = {}

func _init(controller: Node) -> void:
	game_loop_controller = controller

## Returns the conversation logs belonging to a specific character (including user inputs while they are active)
func get_character_history(char_id: String) -> Array:
	var logs = CampaignState.get_recent_history(9999)
	var result = []
	for entry in logs:
		if entry.get("active_character") == char_id:
			result.append(entry)
		elif entry.get("sender") == char_id: # fallback for legacy logs
			result.append(entry)
	return result

## Checks if active character's history needs compaction and triggers it
func check_and_compact_history(char_id: String) -> void:
	if char_id.is_empty():
		return
	if _active_compactions.get(char_id, false):
		print("[MemoryManager] Compaction already in progress for character: ", char_id)
		return
		
	var char_logs = get_character_history(char_id)
	if char_logs.size() <= COMPACTION_THRESHOLD:
		return
		
	# Determine how many older logs to summarize, keeping the most recent COMPACTION_KEEP_COUNT logs
	var summarize_count = char_logs.size() - COMPACTION_KEEP_COUNT
	var logs_to_summarize = char_logs.slice(0, summarize_count)
	
	_active_compactions[char_id] = true
	_run_summarization(char_id, logs_to_summarize, false)

## Summarizes the recent session dialogue entries for a character (even if below compaction threshold)
func summarize_session_for_character(char_id: String) -> void:
	if char_id.is_empty():
		return
	if _active_compactions.get(char_id, false):
		print("[MemoryManager] Summarization already in progress for character: ", char_id)
		return
		
	var char_logs = get_character_history(char_id)
	# Only summarize if there are actually dialogue logs to summarize
	if char_logs.is_empty():
		return
		
	_active_compactions[char_id] = true
	_run_summarization(char_id, char_logs, true)

func _run_summarization(char_id: String, logs_to_summarize: Array, is_session_summary: bool) -> void:
	var character = CampaignState.get_character(char_id)
	var char_name = character.get("name", char_id.capitalize())
	
	# Format transcript
	var transcript = ""
	for entry in logs_to_summarize:
		var sender = entry.get("sender", "")
		var role = entry.get("role", "")
		var content = entry.get("content", "")
		var friendly_name = sender.capitalize() if not sender.is_empty() else role.capitalize()
		if sender == "user" or sender == "player":
			friendly_name = "Player"
		elif sender == "narrator":
			friendly_name = "Narrator"
		transcript += "%s: %s\n" % [friendly_name, content]
		
	var prompt = "=== MEMORY COMPACTION & SUMMARIZATION ===\n"
	prompt += "You are the memory summarization assistant. Summarize the following dialogue segment between the Player and %s in a single, compact, objective paragraph (maximum 3-4 sentences).\n" % char_name
	prompt += "Focus strictly on key events, decisions, actions, character reactions, and relationship changes.\n"
	prompt += "Do not write in the first person. Describe the interaction from an objective third-person narrator's perspective.\n"
	prompt += "PRIORITY PRESERVATION: Core character traits, relationship status, and critical plot points must not be lost or compacted; focus only on narrative event details.\n\n"
	prompt += "Dialogue History Segment:\n"
	prompt += transcript + "\n"
	prompt += "Summary paragraph:"
	
	print("[MemoryManager] Requesting summary for character: %s (%d entries, is_session: %s)..." % [char_id, logs_to_summarize.size(), str(is_session_summary)])
	
	LLMClient.send_custom_request(
		prompt,
		LLMClient.character_model,
		func(success: bool, response_text: String, error_msg: String):
			_active_compactions.erase(char_id)
			if not success:
				printerr("[MemoryManager] Summarization failed for %s: %s" % [char_id, error_msg])
				return
				
			var summary = response_text.strip_edges()
			if summary.is_empty() or summary == "null":
				printerr("[MemoryManager] Empty summary returned for ", char_id)
				return
				
			print("[MemoryManager] Summary generated for %s: %s" % [char_id, summary])
			_apply_medium_term_summary(char_id, summary, logs_to_summarize, is_session_summary)
	)

func _apply_medium_term_summary(char_id: String, summary: String, logs_to_summarize: Array, is_session_summary: bool) -> void:
	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		return
		
	# 1. Store medium term summary
	var mt_memories = character.get("medium_term_memories", [])
	mt_memories.append(summary)
	character["medium_term_memories"] = mt_memories
	
	# 2. Reset turn counter
	if is_session_summary:
		character["turns_since_last_summary"] = 0
		
	# 3. Remove summarized raw logs from history_logs
	var current_logs = CampaignState.get_recent_history(9999)
	var new_logs = []
	for log in current_logs:
		var should_remove = false
		for s_log in logs_to_summarize:
			if log.get("timestamp") == s_log.get("timestamp") and log.get("content") == s_log.get("content") and log.get("sender") == s_log.get("sender"):
				should_remove = true
				break
		if not should_remove:
			new_logs.append(log)
	CampaignState.set_history_logs(new_logs)
	
	# 4. Save campaign state
	CampaignState.save()
	
	# 5. Emit signals
	EventBus.character_state_updated.emit(char_id)
	if game_loop_controller and game_loop_controller.has_signal("memory_updated"):
		game_loop_controller.memory_updated.emit()
		
	# 6. Check if long-term memory distillation is triggered
	if mt_memories.size() >= LTM_THRESHOLD:
		distill_long_term_memory(char_id)

## Distills all medium-term summaries for a character into a single long-term memory block
func distill_long_term_memory(char_id: String) -> void:
	if char_id.is_empty():
		return
	if _active_distillations.get(char_id, false):
		print("[MemoryManager] Distillation already in progress for character: ", char_id)
		return
		
	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		return
		
	var mt_memories = character.get("medium_term_memories", [])
	if mt_memories.is_empty():
		return
		
	var current_ltm = character.get("long_term_memory", "")
	var char_name = character.get("name", char_id.capitalize())
	
	var summaries_text = ""
	for i in range(mt_memories.size()):
		summaries_text += "- Summary %d: %s\n" % [i + 1, mt_memories[i]]
		
	var prompt = "=== LONG-TERM MEMORY DISTILLATION ===\n"
	prompt += "You are the memory coordinator. Distill the following chronological list of event summaries involving the character %s and the Player into a single, cohesive, long-term memory narrative block (1-2 paragraphs).\n" % char_name
	prompt += "Your response must integrate the new summaries with the existing long-term memory to form an updated, unified narrative.\n"
	prompt += "Focus on long-term character development, major milestones, relationship shifts, and critical plot outcomes.\n"
	prompt += "Do not write in the first person. Describe the relationship from an objective third-person narrator's perspective.\n\n"
	if not current_ltm.is_empty():
		prompt += "Existing Long-Term Memory:\n%s\n\n" % current_ltm
	prompt += "New Summaries to Distill:\n%s\n" % summaries_text
	prompt += "Updated Long-Term Memory:"
	
	print("[MemoryManager] Requesting long-term memory distillation for character: %s (%d summaries)..." % [char_id, mt_memories.size()])
	
	_active_distillations[char_id] = true
	
	# We capture the exact list of summaries we are distilling so we can remove them safely on success
	var distilled_summaries = mt_memories.duplicate()
	
	LLMClient.send_custom_request(
		prompt,
		LLMClient.character_model,
		func(success: bool, response_text: String, error_msg: String):
			_active_distillations.erase(char_id)
			if not success:
				printerr("[MemoryManager] Distillation failed for %s: %s" % [char_id, error_msg])
				return
				
			var distilled_ltm = response_text.strip_edges()
			if distilled_ltm.is_empty() or distilled_ltm == "null":
				printerr("[MemoryManager] Empty distilled memory returned for ", char_id)
				return
				
			print("[MemoryManager] Long-term memory updated for %s" % char_id)
			_apply_long_term_distillation(char_id, distilled_ltm, distilled_summaries)
	)

func _apply_long_term_distillation(char_id: String, distilled_ltm: String, distilled_summaries: Array) -> void:
	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		return
		
	# 1. Update long term memory
	character["long_term_memory"] = distilled_ltm
	
	# 2. Remove the distilled medium term memories (retaining any new ones added in the meantime)
	var mt_memories = character.get("medium_term_memories", [])
	var remaining_mt = []
	for mt in mt_memories:
		if not mt in distilled_summaries:
			remaining_mt.append(mt)
	character["medium_term_memories"] = remaining_mt
	
	# 3. Save campaign state
	CampaignState.save()
	
	# 4. Emit signals
	EventBus.character_state_updated.emit(char_id)
	if game_loop_controller and game_loop_controller.has_signal("memory_updated"):
		game_loop_controller.memory_updated.emit()
