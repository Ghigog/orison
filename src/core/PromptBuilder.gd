# res://src/core/PromptBuilder.gd
extends RefCounted
class_name PromptBuilder

const PlayerInputParser = preload("res://src/core/PlayerInputParser.gd")

var graph_manager: KnowledgeGraphManager
var emotion_prompt_builder: EmotionPromptBuilder

func _init(graph: KnowledgeGraphManager, ep_builder: EmotionPromptBuilder) -> void:
	graph_manager = graph
	emotion_prompt_builder = ep_builder

## Builds the fully synthesized prompt context to send to the local LLM
func build_prompt(char_id: String, user_prompt: String, is_director_busy: bool = false) -> String:
	var prompt = ""
	
	# 1. Modular System Instructions from SystemPrompts
	prompt += SystemPrompts.get_character_agent_prompt_for_id(char_id)
	prompt += "\n"
	
	if is_director_busy:
		prompt += "CRITICAL NARRATIVE STALLING INSTRUCTION:\n"
		prompt += "A major plot transition is currently loading in the background. Your job in this response is to KEEP THE DIALOGUE FLOWING naturally. Do NOT advance the core plot, do not make any permanent decisions, and do not introduce new scenes. Keep your dialogue focused on the current environment, your feelings, or minor details. Deflect and build anticipation or sustain the conversation nicely.\n\n"
	
	# 2. Inject Active Character's Emotional Tone Details (adds specific behavioral hints)
	if not char_id.is_empty():
		prompt += "Tone and Behavioral Guidance:\n"
		prompt += emotion_prompt_builder.build_emotion_block(char_id)
		prompt += "\n"
		
	# 2.5. Inject Player Character Profile (protagonist details)
	var pc = CampaignState.state.get("player_character", {})
	if not pc.is_empty():
		prompt += "Player Character Profile (The protagonist you are interacting with):\n"
		prompt += "- Name: %s\n" % pc.get("name", "Player")
		if not pc.get("physical_description", "").is_empty():
			prompt += "- Physical Description: %s\n" % pc.get("physical_description", "")
		if not pc.get("personality", "").is_empty():
			prompt += "- Personality: %s\n" % pc.get("personality", "")
		if not pc.get("backstory", "").is_empty():
			prompt += "- Backstory: %s\n" % pc.get("backstory", "")
		prompt += "\n"
		
	# 3. Inject Dynamic Game State Context (Inventory & Plot Flags)
	prompt += "Active World & Inventory State:\n"
	prompt += "- Plot Flags:\n"
	var plot_states = CampaignState.state.get("plot_states", {})
	if plot_states.is_empty():
		prompt += "  - No active world flags set.\n"
	else:
		for key in plot_states.keys():
			prompt += "  - %s: %s\n" % [key, str(plot_states[key])]
			
	prompt += "- Character Inventories:\n"
	if not char_id.is_empty():
		var inv = CampaignState.get_inventory(char_id)
		if inv.is_empty():
			prompt += "  - %s's inventory is empty.\n" % char_id.capitalize()
		else:
			prompt += "  - %s's items:\n" % char_id.capitalize()
			for item in inv:
				prompt += "    - %s (x%d)\n" % [str(item.get("item", "")), int(item.get("quantity", 1))]
	prompt += "\n"
	
	# 3.5. Inject Adventure Memories
	prompt += "Adventure Memories:\n"
	var memory = CampaignState.state.get("memory", {})
	prompt += "- Short-Term Memory: %s\n" % memory.get("short_term", "None")
	prompt += "- Medium-Term Memory: %s\n" % memory.get("medium_term", "None")
	prompt += "- Long-Term Memory: %s\n" % memory.get("long_term", "None")
	prompt += "\n"
	
	# 4. Inject Dynamic Semantic Knowledge Graph Memory Context
	var graph_context = graph_manager.retrieve_context(user_prompt)
	if not graph_context.is_empty():
		prompt += graph_context + "\n"
		
	# 5. Inject Dialogue History (last N turns)
	prompt += "Dialogue History:\n"
	var recent_history = CampaignState.get_recent_history(5)
	if recent_history.is_empty():
		prompt += "No previous dialogue in this session.\n"
	else:
		for log_entry in recent_history:
			var role = log_entry.get("role", "user")
			var content = log_entry.get("content", "")
			var sender = log_entry.get("sender", "")
			var friendly_name = sender.capitalize() if not sender.is_empty() else role.capitalize()
			if sender == "user" or sender == "player":
				friendly_name = "Player"
			elif sender == "narrator":
				friendly_name = "Narrator"
			prompt += "%s: %s\n" % [friendly_name, content]
	prompt += "\n"
	
	# 6. Append the user prompt trigger
	var parsed = PlayerInputParser.parse_input(user_prompt)
	prompt += "Player Input:\n"
	prompt += "- Raw Input: " + user_prompt + "\n"
	prompt += "- Parsed Dialogue: " + (parsed.dialogue if not parsed.dialogue.is_empty() else "None") + "\n"
	prompt += "- Parsed Action/Context: " + (parsed.action if not parsed.action.is_empty() else "None") + "\n"
	prompt += "- Syntax Style: " + parsed.detected_format + "\n\n"
	prompt += "Assistant: "
	
	return prompt

## Builds the fully synthesized prompt context to send to the World Builder / Dungeon Master
func build_world_builder_prompt(user_prompt: String, active_char_id: String = "") -> String:
	var prompt = ""
	
	# 1. Modular System Instructions from SystemPrompts
	prompt += SystemPrompts.get_world_builder_prompt()
	prompt += "\n"
	
	# 2. Inject Campaign and Setting Context
	prompt += "Campaign Setting:\n"
	var campaign_title = CampaignState.get_campaign_meta("title", "Unknown Campaign")
	prompt += "- Title: %s\n" % campaign_title
	
	var active_location_id = CampaignState.get_campaign_meta("active_location", "")
	if not active_location_id.is_empty():
		var loc_node = graph_manager.get_node(active_location_id)
		if not loc_node.is_empty():
			prompt += "- Current Location: %s\n" % loc_node.get("label", active_location_id.capitalize())
			prompt += "  Description: %s\n" % loc_node.get("desc", "No description.")
			
	if not active_char_id.is_empty():
		var character = CampaignState.get_character(active_char_id)
		var char_name = character.get("name", active_char_id.capitalize()) if not character.is_empty() else active_char_id.capitalize()
		prompt += "- Active Conversation Partner: %s (The player is currently addressing and interacting directly with this character. Do not write dialogue for them or have other characters reply to their questions.)\n" % char_name
	prompt += "\n"
	
	# 2.5. Inject Player Character Profile (protagonist details)
	var pc = CampaignState.state.get("player_character", {})
	if not pc.is_empty():
		prompt += "Player Character Profile (The protagonist of this adventure):\n"
		prompt += "- Name: %s\n" % pc.get("name", "Player")
		if not pc.get("physical_description", "").is_empty():
			prompt += "- Physical Description: %s\n" % pc.get("physical_description", "")
		if not pc.get("personality", "").is_empty():
			prompt += "- Personality: %s\n" % pc.get("personality", "")
		if not pc.get("backstory", "").is_empty():
			prompt += "- Backstory: %s\n" % pc.get("backstory", "")
		prompt += "\n"
		
	# 3. Inject Short, Medium, and Long term memory
	prompt += "Adventure Memories:\n"
	var memory = CampaignState.state.get("memory", {})
	prompt += "- Short-Term Memory: %s\n" % memory.get("short_term", "None")
	prompt += "- Medium-Term Memory: %s\n" % memory.get("medium_term", "None")
	prompt += "- Long-Term Memory: %s\n" % memory.get("long_term", "None")
	prompt += "\n"
	
	# 4. Inject Dynamic Game State Context (Inventory & Plot Flags)
	prompt += "Active World & Inventory State:\n"
	prompt += "- Plot Flags:\n"
	var plot_states = CampaignState.state.get("plot_states", {})
	if plot_states.is_empty():
		prompt += "  - No active world flags set.\n"
	else:
		for key in plot_states.keys():
			prompt += "  - %s: %s\n" % [key, str(plot_states[key])]
	prompt += "\n"
	
	# 5. Inject Dynamic Semantic Knowledge Graph Memory Context
	var graph_context = graph_manager.retrieve_context(user_prompt)
	if not graph_context.is_empty():
		prompt += graph_context + "\n"
		
	# 6. Inject Dialogue/Event History (last N turns)
	prompt += "Dialogue & Event History:\n"
	var recent_history = CampaignState.get_recent_history(4)
	if recent_history.is_empty():
		prompt += "No previous dialogue in this session.\n"
	else:
		for log_entry in recent_history:
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
				
			prompt += "%s: %s\n" % [friendly_name, content]
	prompt += "\n"
	
	# 7. Append the user prompt trigger
	var parsed = PlayerInputParser.parse_input(user_prompt)
	prompt += "Player Action/Input:\n"
	prompt += "- Raw Input: " + user_prompt + "\n"
	prompt += "- Parsed Dialogue: " + (parsed.dialogue if not parsed.dialogue.is_empty() else "None") + "\n"
	prompt += "- Parsed Action/Context: " + (parsed.action if not parsed.action.is_empty() else "None") + "\n"
	prompt += "- Syntax Style: " + parsed.detected_format + "\n\n"
	prompt += "Narrator: "
	
	return prompt
