# res://src/core/PromptBuilder.gd
extends RefCounted
class_name PromptBuilder

## PromptBuilder
##
## This class is responsible for dynamic context aggregation, layout composition,
## token estimation, and prompt assembly for Ollama/LLM client requests.
##
## IMPORTANT:
## 1. This file owns all runtime state gathering logic (reading from CampaignState,
##    KnowledgeGraphManager, and history summaries).
## 2. All static, base instructions, system guidelines, and output schemas must be
##    stored in SystemPrompts (SystemPrompts.gd).

const PlayerInputParser = preload("res://src/core/PlayerInputParser.gd")

var graph_manager: KnowledgeGraphManager
var emotion_prompt_builder: EmotionPromptBuilder

func _init(graph: KnowledgeGraphManager, ep_builder: EmotionPromptBuilder) -> void:
	graph_manager = graph
	emotion_prompt_builder = ep_builder

## Estimate token count using heuristic 1 token ≈ 4 characters
static func estimate_tokens(text: String) -> int:
	return int(ceil(text.length() / 4.0))

## Compress history content by stripping parenthetical actions and truncating
static func compress_history_content(content: String) -> String:
	var regex = RegEx.new()
	regex.compile("\\s*\\([^)]*\\)\\s*")
	var result = regex.sub(content, " ", true).strip_edges()
	if result.is_empty():
		result = content
	if result.length() > 100:
		result = result.left(97) + "..."
	return result

## Helper to build the character agent prompt using a character's ID by querying CampaignState
static func get_character_agent_prompt_for_id(char_id: String, max_bio_len: int = -1, max_style_len: int = -1) -> String:
	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		var meta = CampaignState.state.get("adventure_meta", {})
		var fallback_style = meta.get("writing_style", "") if not meta.is_empty() else ""
		return SystemPrompts.get_character_agent_prompt(char_id.capitalize(), "", 0.0, "serenity", 1.0, "player", "Initial encounter.", fallback_style)
		
	var char_name = character.get("name", char_id.capitalize())
	var biography = character.get("biography", "")
	if max_bio_len > 0 and biography.length() > max_bio_len:
		biography = biography.left(max_bio_len) + "..."
		
	var affinity = character.get("affinity", 0.0)
	var writing_style = character.get("writing_style", "")
	var is_creature = character.get("is_creature", false)
	var can_speak = character.get("can_speak", true)
	var humanoid = character.get("humanoid", true)
	
	var personality = character.get("personality", "")
	var appearance = character.get("appearance", "")
	var gender = character.get("gender", "")
	var goals = character.get("goals", "")
	
	if writing_style.is_empty():
		var meta = CampaignState.state.get("adventure_meta", {})
		writing_style = meta.get("writing_style", "") if not meta.is_empty() else ""
		
	if max_style_len > 0 and writing_style.length() > max_style_len:
		writing_style = writing_style.left(max_style_len) + "..."
	
	# Fetch last emotional event
	var emotions = character.get("emotions", [])
	var active_emotion = "serenity"
	var intensity = 0.5
	var target = "player"
	var context = "Initial state."
	
	if not emotions.is_empty():
		var last_event = emotions[-1]
		if last_event is Dictionary:
			active_emotion = last_event.get("emotion", "serenity")
			intensity = last_event.get("intensity", 0.5)
			target = last_event.get("target", "player")
			context = last_event.get("context", "Recent dialogue.")
			
	return SystemPrompts.get_character_agent_prompt(
		char_name, 
		biography, 
		affinity, 
		active_emotion, 
		intensity, 
		target, 
		context, 
		writing_style,
		is_creature,
		can_speak,
		humanoid,
		personality,
		appearance,
		gender,
		goals
	)

## Helper to build the emotion reflection prompt using a character's ID by querying CampaignState
static func get_emotion_reflection_prompt_for_id(char_id: String, narration_text: String) -> String:
	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		return ""
		
	var char_name = character.get("name", char_id.capitalize())
	var biography = character.get("biography", "")
	var affinity = character.get("affinity", 0.0)
	
	# Fetch last emotional event
	var emotions = character.get("emotions", [])
	var active_emotion = "serenity"
	var intensity = 0.5
	var target = "player"
	var context = "Initial state."
	
	if not emotions.is_empty():
		var last_event = emotions[-1]
		if last_event is Dictionary:
			active_emotion = last_event.get("emotion", "serenity")
			intensity = last_event.get("intensity", 0.5)
			target = last_event.get("target", "player")
			context = last_event.get("context", "Recent dialogue.")
			
	return SystemPrompts.get_emotion_reflection_prompt(
		char_name,
		biography,
		affinity,
		active_emotion,
		intensity,
		target,
		context,
		narration_text
	)

## Builds the fully synthesized prompt context to send to the local LLM
func build_prompt(char_id: String, user_prompt: String, is_director_busy: bool = false) -> String:
	var context_limit = 8192 # NPC character model context limit
	
	# Budgets (in tokens)
	var sys_budget = int(context_limit * 0.30)  # 2457 tokens
	var id_budget = int(context_limit * 0.30)   # 2457 tokens
	var hist_budget = int(context_limit * 0.30) # 2457 tokens
	var lore_budget = int(context_limit * 0.15) # 1228 tokens
	
	# 1. Truncate biography and writing style if needed to fit NPC Identity budget.
	# Base NPC identity is ~300 tokens. Remaining budget for biography + writing style is ~928 tokens (~3712 characters).
	var max_bio_len = 2000
	var max_style_len = 1500
	
	var system_prompt_with_rules = get_character_agent_prompt_for_id(char_id, max_bio_len, max_style_len)
	
	var stalling_text = ""
	if is_director_busy:
		stalling_text = SystemPrompts.get_director_busy_stalling_prompt()
	
	# 2. NPC Behavioral Guidance
	var guidance_text = ""
	if not char_id.is_empty():
		guidance_text += "Tone and Behavioral Guidance:\n"
		guidance_text += emotion_prompt_builder.build_emotion_block(char_id)
		guidance_text += "\n"
		
	# 3. Player Character Profile
	var pc_text = ""
	var pc = CampaignState.state.get("player_character", {})
	if not pc.is_empty():
		pc_text += "Player Character Profile (The protagonist you are interacting with):\n"
		pc_text += "- Name: %s\n" % pc.get("name", "Player")
		if not pc.get("physical_description", "").is_empty():
			pc_text += "- Physical Description: %s\n" % pc.get("physical_description", "")
		if not pc.get("personality", "").is_empty():
			pc_text += "- Personality: %s\n" % pc.get("personality", "")
		if not pc.get("backstory", "").is_empty():
			pc_text += "- Backstory: %s\n" % pc.get("backstory", "")
		pc_text += "\n"
		
	# 4. Active World & Inventory State
	var state_text = "Active World & Inventory State:\n"
	state_text += "- Plot Flags:\n"
	var plot_states = CampaignState.state.get("plot_states", {})
	if plot_states.is_empty():
		state_text += "  - No active world flags set.\n"
	else:
		for key in plot_states.keys():
			state_text += "  - %s: %s\n" % [key, str(plot_states[key])]
			
	state_text += "- Character Inventories:\n"
	if not char_id.is_empty():
		var inv = CampaignState.get_inventory(char_id)
		if inv.is_empty():
			state_text += "  - %s's inventory is empty.\n" % char_id.capitalize()
		else:
			state_text += "  - %s's items:\n" % char_id.capitalize()
			for item in inv:
				state_text += "    - %s (x%d)\n" % [str(item.get("item", "")), int(item.get("quantity", 1))]
	state_text += "\n"
	
	# 5. Memories
	var mem_text = "Adventure Memories:\n"
	var memory = CampaignState.state.get("memory", {})
	mem_text += "- Short-Term Memory: %s\n" % memory.get("short_term", "None")
	mem_text += "- Medium-Term Memory: %s\n" % memory.get("medium_term", "None")
	mem_text += "- Long-Term Memory: %s\n" % memory.get("long_term", "None")
	if not char_id.is_empty():
		var character = CampaignState.get_character(char_id)
		if not character.is_empty():
			var char_name = character.get("name", char_id.capitalize())
			var char_lt = character.get("long_term_memory", "")
			var char_mt_list = character.get("medium_term_memories", [])
			if not char_lt.is_empty() or not char_mt_list.is_empty():
				mem_text += "- Memories for %s:\n" % char_name
				if not char_lt.is_empty():
					mem_text += "  - Long-Term Character Memory: %s\n" % char_lt
				if not char_mt_list.is_empty():
					mem_text += "  - Past Session & Event Summaries:\n"
					for summary in char_mt_list:
						mem_text += "    * %s\n" % summary
	mem_text += "\n"
	
	# 6. Lore/Knowledge Graph Context (10% budget = lore_budget)
	var graph_context = await graph_manager.retrieve_context(user_prompt, lore_budget, 0)
	if not graph_context.is_empty():
		graph_context += "\n"
		
	# 7. Recent Chat History (30% budget = hist_budget)
	var last_beat = CampaignState.get_last_director_beat()
	var director_beat_text = ""
	if not last_beat.is_empty():
		var capped_beat = last_beat
		if capped_beat.length() > 500:
			capped_beat = capped_beat.left(497) + "..."
		director_beat_text = "[Narrative Context]: %s\n\n" % capped_beat
		CampaignState.set_last_director_beat("")
		
	var hist_text = director_beat_text + "Dialogue History:\n"
	var all_recent_history = CampaignState.get_recent_history(50)
	if all_recent_history.is_empty():
		hist_text += "No previous dialogue in this session.\n"
	else:
		var history_lines: Array[String] = []
		var accumulated_history_tokens = 0
		var remaining_hist_budget = hist_budget - estimate_tokens(hist_text)
		
		# Iterate from most recent to oldest
		for idx in range(all_recent_history.size() - 1, -1, -1):
			var log_entry = all_recent_history[idx]
			var role = log_entry.get("role", "user")
			var content = log_entry.get("content", "")
			var sender = log_entry.get("sender", "")
			var friendly_name = sender.capitalize() if not sender.is_empty() else role.capitalize()
			if sender == "user" or sender == "player":
				friendly_name = "Player"
			elif sender == "narrator":
				friendly_name = "Narrator"
				
			# Compress older entries (older than the last 3 turns)
			var distance_from_recent = all_recent_history.size() - 1 - idx
			if distance_from_recent >= 3:
				content = compress_history_content(content)
				
			var line = "%s: %s\n" % [friendly_name, content]
			var line_tokens = estimate_tokens(line)
			
			if accumulated_history_tokens + line_tokens <= remaining_hist_budget:
				history_lines.push_front(line)
				accumulated_history_tokens += line_tokens
			else:
				print("[PromptBuilder] History budget exceeded. Trimmed older logs (removed %d entries)." % [idx + 1])
				break
				
		hist_text += "".join(history_lines)
	hist_text += "\n"
	
	# 8. Player Input trigger
	var sanitized_prompt = PlayerInputParser.sanitize_input(user_prompt)
	var parsed = PlayerInputParser.parse_input(sanitized_prompt)
	var trigger_text = "Player Input:\n"
	trigger_text += "<player_message>\n"
	trigger_text += "- Raw Input: " + sanitized_prompt + "\n"
	trigger_text += "- Parsed Dialogue: " + (parsed.dialogue if not parsed.dialogue.is_empty() else "None") + "\n"
	trigger_text += "- Parsed Action/Context: " + (parsed.action if not parsed.action.is_empty() else "None") + "\n"
	trigger_text += "- Syntax Style: " + parsed.detected_format + "\n"
	trigger_text += "</player_message>\n\n"
	trigger_text += "Response (JSON): "

	
	# Assemble the final prompt
	var prompt = ""
	prompt += system_prompt_with_rules + "\n"
	if not stalling_text.is_empty():
		prompt += stalling_text
	if not guidance_text.is_empty():
		prompt += guidance_text
	if not pc_text.is_empty():
		prompt += pc_text
	if not state_text.is_empty():
		prompt += state_text
	if not mem_text.is_empty():
		prompt += mem_text
	if not graph_context.is_empty():
		prompt += graph_context
	if not hist_text.is_empty():
		prompt += hist_text
	prompt += trigger_text
	
	# Warn if overall budget is exceeded
	var final_tokens = estimate_tokens(prompt)
	if final_tokens > context_limit:
		push_warning("[PromptBuilder] WARNING: Compiled character prompt (%d tokens) exceeds target context size (%d tokens)!" % [final_tokens, context_limit])
		
	return prompt

## Builds the fully synthesized prompt context to send to the World Builder / Dungeon Master
func build_world_builder_prompt(user_prompt: String, active_char_id: String = "", research_findings: String = "") -> String:
	var context_limit = 8192 # World builder model context limit
	
	# Budgets (in tokens)
	var sys_budget = int(context_limit * 0.30)  # 2457 tokens
	var id_budget = int(context_limit * 0.30)   # 2457 tokens
	var hist_budget = int(context_limit * 0.30) # 2457 tokens
	var lore_budget = int(context_limit * 0.10) # 819 tokens
	
	# 1. System instructions
	var system_prompt = SystemPrompts.get_world_builder_prompt() + "\n"
	
	# 2. Campaign and Setting Context (NPC/Location Identity - 30% budget)
	var campaign_text = "Campaign Setting:\n"
	var campaign_title = CampaignState.get_campaign_meta("title", "Unknown Campaign")
	campaign_text += "- Title: %s\n" % campaign_title
	
	var active_location_id = CampaignState.get_campaign_meta("active_location", "")
	if not active_location_id.is_empty():
		var loc_node = graph_manager.get_node(active_location_id)
		if not loc_node.is_empty():
			campaign_text += "- Current Location: %s\n" % loc_node.get("label", active_location_id.capitalize())
			var desc = loc_node.get("desc", "No description.")
			# Truncate location description if too long
			if desc.length() > 2000:
				desc = desc.left(2000) + "..."
			campaign_text += "  Description: %s\n" % desc
			
	if not active_char_id.is_empty():
		var character = CampaignState.get_character(active_char_id)
		var char_name = character.get("name", active_char_id.capitalize()) if not character.is_empty() else active_char_id.capitalize()
		campaign_text += "- Active Conversation Partner: %s (The player is currently addressing and interacting directly with this character. Do not write dialogue for them or have other characters reply to their questions.)\n" % char_name
	campaign_text += "\n"
	
	# 3. Player Character Profile
	var pc_text = ""
	var pc = CampaignState.state.get("player_character", {})
	if not pc.is_empty():
		pc_text += "Player Character Profile (The protagonist of this adventure):\n"
		pc_text += "- Name: %s\n" % pc.get("name", "Player")
		if not pc.get("physical_description", "").is_empty():
			pc_text += "- Physical Description: %s\n" % pc.get("physical_description", "")
		if not pc.get("personality", "").is_empty():
			pc_text += "- Personality: %s\n" % pc.get("personality", "")
		if not pc.get("backstory", "").is_empty():
			pc_text += "- Backstory: %s\n" % pc.get("backstory", "")
		pc_text += "\n"
		
	# 4. Memories
	var mem_text = "Adventure Memories:\n"
	var memory = CampaignState.state.get("memory", {})
	mem_text += "- Short-Term Memory: %s\n" % memory.get("short_term", "None")
	mem_text += "- Medium-Term Memory: %s\n" % memory.get("medium_term", "None")
	mem_text += "- Long-Term Memory: %s\n" % memory.get("long_term", "None")
	if not active_char_id.is_empty():
		var character = CampaignState.get_character(active_char_id)
		if not character.is_empty():
			var char_name = character.get("name", active_char_id.capitalize())
			var char_lt = character.get("long_term_memory", "")
			var char_mt_list = character.get("medium_term_memories", [])
			if not char_lt.is_empty() or not char_mt_list.is_empty():
				mem_text += "- Memories for %s:\n" % char_name
				if not char_lt.is_empty():
					mem_text += "  - Long-Term Character Memory: %s\n" % char_lt
				if not char_mt_list.is_empty():
					mem_text += "  - Past Session & Event Summaries:\n"
					for summary in char_mt_list:
						mem_text += "    * %s\n" % summary
	mem_text += "\n"
	
	# 5. Active World State
	var state_text = "Active World & Inventory State:\n"
	state_text += "- Plot Flags:\n"
	var plot_states = CampaignState.state.get("plot_states", {})
	if plot_states.is_empty():
		state_text += "  - No active world flags set.\n"
	else:
		for key in plot_states.keys():
			state_text += "  - %s: %s\n" % [key, str(plot_states[key])]
	state_text += "\n"
	
	# 6. Lore/Knowledge Graph Context (10% budget = lore_budget)
	var graph_context = await graph_manager.retrieve_context(user_prompt, lore_budget, 2)
	if not graph_context.is_empty():
		graph_context += "\n"
		
	# 7. Recent Chat/Event History (30% budget = hist_budget)
	var hist_text = "Dialogue & Event History:\n"
	var all_recent_history = CampaignState.get_recent_history(50)
	if all_recent_history.is_empty():
		hist_text += "No previous dialogue in this session.\n"
	else:
		var history_lines: Array[String] = []
		var accumulated_history_tokens = 0
		var remaining_hist_budget = hist_budget - estimate_tokens(hist_text)
		
		# Iterate backwards
		for idx in range(all_recent_history.size() - 1, -1, -1):
			var log_entry = all_recent_history[idx]
			var role = log_entry.get("role", "user")
			var content = log_entry.get("content", "")
			var sender = log_entry.get("sender", "")
			
			var friendly_name = sender.capitalize()
			if sender == "user" or sender == "player":
				friendly_name = "Player"
			elif sender == "narrator":
				friendly_name = "Narrator"
			elif friendly_name.is_empty():
				friendly_name = role.capitalize()
				
			# Compress older entries (older than the last 3 turns)
			var distance_from_recent = all_recent_history.size() - 1 - idx
			if distance_from_recent >= 3:
				content = compress_history_content(content)
				
			var line = "%s: %s\n" % [friendly_name, content]
			var line_tokens = estimate_tokens(line)
			
			if accumulated_history_tokens + line_tokens <= remaining_hist_budget:
				history_lines.push_front(line)
				accumulated_history_tokens += line_tokens
			else:
				print("[PromptBuilder] History budget exceeded in DM prompt. Trimmed older logs (removed %d entries)." % [idx + 1])
				break
				
		hist_text += "".join(history_lines)
	hist_text += "\n"
	
	# 8. Player Input trigger
	var sanitized_prompt = PlayerInputParser.sanitize_input(user_prompt)
	var parsed = PlayerInputParser.parse_input(sanitized_prompt)
	var trigger_text = "Player Action/Input:\n"
	trigger_text += "<player_message>\n"
	trigger_text += "- Raw Input: " + sanitized_prompt + "\n"
	trigger_text += "- Parsed Dialogue: " + (parsed.dialogue if not parsed.dialogue.is_empty() else "None") + "\n"
	trigger_text += "- Parsed Action/Context: " + (parsed.action if not parsed.action.is_empty() else "None") + "\n"
	trigger_text += "- Syntax Style: " + parsed.detected_format + "\n"
	trigger_text += "</player_message>\n\n"
	trigger_text += "Narrator: "

	
	# Assemble
	var prompt = ""
	prompt += system_prompt
	prompt += campaign_text
	if not pc_text.is_empty():
		prompt += pc_text
	if not mem_text.is_empty():
		prompt += mem_text
	if not state_text.is_empty():
		prompt += state_text
	if not graph_context.is_empty():
		prompt += graph_context
	if not hist_text.is_empty():
		prompt += hist_text
	if not research_findings.is_empty():
		prompt += research_findings + "\n"
	prompt += trigger_text
	
	# Warn if overall budget is exceeded
	var final_tokens = estimate_tokens(prompt)
	if final_tokens > context_limit:
		push_warning("[PromptBuilder] WARNING: Compiled world builder prompt (%d tokens) exceeds target context size (%d tokens)!" % [final_tokens, context_limit])
		
	return prompt

## Builds the prompt for each step of the Director ReAct loop
func build_react_prompt(user_prompt: String, active_char_id: String, research_history: Array) -> String:
	var prompt = ""
	prompt += SystemPrompts.get_director_react_system_prompt() + "\n"
	
	prompt += "CURRENT CONTEXT:\n"
	var campaign_title = CampaignState.get_campaign_meta("title", "Unknown Campaign")
	prompt += "- Campaign Title: %s\n" % campaign_title
	
	var active_location_id = CampaignState.get_campaign_meta("active_location", "")
	if not active_location_id.is_empty():
		var loc_node = graph_manager.get_node(active_location_id)
		if not loc_node.is_empty():
			prompt += "- Current Location: %s\n" % loc_node.get("label", active_location_id.capitalize())
			
	if not active_char_id.is_empty():
		var character = CampaignState.get_character(active_char_id)
		var char_name = character.get("name", active_char_id.capitalize()) if not character.is_empty() else active_char_id.capitalize()
		prompt += "- Active Conversation Partner: %s\n" % char_name
	prompt += "\n"
	
	var sanitized_prompt = PlayerInputParser.sanitize_input(user_prompt)
	prompt += "PLAYER INPUT:\n%s\n\n" % sanitized_prompt
	
	if not research_history.is_empty():
		prompt += "RESEARCH LOGS SO FAR:\n"
		for i in range(research_history.size()):
			var step = research_history[i]
			prompt += "Step %d:\n" % (i + 1)
			prompt += "Thought: %s\n" % step.get("thought", "")
			var action = step.get("action", "")
			var args = step.get("args", {})
			prompt += "Action: %s(%s)\n" % [action, JSON.stringify(args)]
			prompt += "Observation:\n%s\n\n" % step.get("observation", "")
			
	prompt += "Next Step (JSON): "
	return prompt
