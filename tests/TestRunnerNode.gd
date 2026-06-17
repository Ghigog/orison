# res://tests/TestRunnerNode.gd
extends Node

const SystemPrompts = preload("res://src/core/SystemPrompts.gd")
const VaultScanner = preload("res://src/core/VaultScanner.gd")
const PlayerInputParser = preload("res://src/core/PlayerInputParser.gd")



func _ready() -> void:
	print("=================================================================")
	print("                 ORISON ENGINE TEST SUITE                        ")
	print("=================================================================")
	
	var pass_count = 0
	var total_tests = 10
	
	if test_json_save_and_state():
		pass_count += 1
		print("[PASS] Test 1: SaveManager & CampaignState Operations")
	else:
		print("[FAIL] Test 1: SaveManager & CampaignState Operations")
		
	if test_markdown_parser():
		pass_count += 1
		print("[PASS] Test 2: MarkdownParser Frontmatter Extraction")
	else:
		print("[FAIL] Test 2: MarkdownParser Frontmatter Extraction")
		
	if test_vault_compiler():
		pass_count += 1
		print("[PASS] Test 3: VaultCompiler Import Compilation")
	else:
		print("[FAIL] Test 3: VaultCompiler Import Compilation")
		
	if test_knowledge_graph():
		pass_count += 1
		print("[PASS] Test 4: KnowledgeGraph Node & Edge Queries")
	else:
		print("[FAIL] Test 4: KnowledgeGraph Node & Edge Queries")
		
	if test_json_repair():
		pass_count += 1
		print("[PASS] Test 5: JsonRepair Out-of-Format Parsing")
	else:
		print("[FAIL] Test 5: JsonRepair Out-of-Format Parsing")
		
	if test_system_prompts():
		pass_count += 1
		print("[PASS] Test 6: SystemPrompts Generator Output")
	else:
		print("[FAIL] Test 6: SystemPrompts Generator Output")
		
	if test_beginning_prompt():
		pass_count += 1
		print("[PASS] Test 7: Beginning Generation Prompt")
	else:
		print("[FAIL] Test 7: Beginning Generation Prompt")
		
	if test_vault_scanner_and_mappings():
		pass_count += 1
		print("[PASS] Test 8: VaultScanner & Compiler Mappings")
	else:
		print("[FAIL] Test 8: VaultScanner & Compiler Mappings")
		
	if test_theme_manager_and_font_scaling():
		pass_count += 1
		print("[PASS] Test 9: ThemeManager & Font Scaling Engine")
	else:
		print("[FAIL] Test 9: ThemeManager & Font Scaling Engine")
		
	if test_player_input_parser():
		pass_count += 1
		print("[PASS] Test 10: PlayerInputParser Visual Novel Syntax")
	else:
		print("[FAIL] Test 10: PlayerInputParser Visual Novel Syntax")
		
	print("=================================================================")
	print("Test Results: %d / %d Tests Passed" % [pass_count, total_tests])
	print("=================================================================")
	
	get_tree().quit(0 if pass_count == total_tests else 1)

func test_json_save_and_state() -> bool:
	var campaign_id = "test_run_state"
	
	# Clear old test run
	var path = SaveManager.SAVE_DIR + campaign_id + ".json"
	if FileAccess.file_exists(path):
		DirAccess.remove_absolute(path)
		
	# 1. Create campaign
	var raw_state = SaveManager.create_campaign(campaign_id, "Test Run State Campaign")
	if raw_state.is_empty():
		return false
		
	CampaignState.initialize(campaign_id, raw_state)
	
	# 2. Character operations
	CampaignState.init_character("char_test", "Test NPC", "A script testing npc.", "A tests writing style.", "", "anger", 0.7)
	var char_data = CampaignState.get_character("char_test")
	if char_data.get("name") != "Test NPC" or char_data.get("writing_style") != "A tests writing style.":
		return false
	if char_data.get("base_emotion") != "anger" or char_data.get("base_intensity") != 0.7:
		return false
	var initial_emotions = char_data.get("emotions", [])
	if initial_emotions.is_empty() or initial_emotions[0].get("emotion") != "anger" or initial_emotions[0].get("intensity") != 0.7:
		return false
		
	# 3. Affinity adjustment
	CampaignState.adjust_affinity("char_test", 0.25)
	if CampaignState.get_character("char_test").get("affinity") != 0.25:
		return false
		
	# 4. Emotion event
	CampaignState.add_emotion_event("char_test", "joy", 0.9, "player", "Translating ancient texts.")
	var emotions = CampaignState.get_character("char_test").get("emotions", [])
	if emotions.size() < 2 or emotions[-1].get("emotion") != "joy":
		return false
		
	# 5. Inventory additions
	CampaignState.add_to_inventory("char_test", "gold_pieces", 10)
	CampaignState.add_to_inventory("char_test", "gold_pieces", 5) # stacks
	CampaignState.add_to_inventory("char_test", "iron_key", 1)
	
	var inv = CampaignState.get_inventory("char_test")
	if inv.size() != 2:
		return false
		
	var gold_qty = 0
	for item in inv:
		if item.get("item") == "gold_pieces":
			gold_qty = item.get("quantity")
	if gold_qty != 15:
		return false
		
	# 6. Save and Reload
	var err = CampaignState.save()
	if err != OK:
		return false
		
	# Verify get_campaign_list() lists the saved campaign (for duplicate checks)
	var list = SaveManager.get_campaign_list()
	var found = false
	for entry in list:
		if entry.id == campaign_id:
			found = true
			break
	if not found:
		return false
		
	var loaded_data = SaveManager.load_campaign(campaign_id)
	if loaded_data.is_empty():
		return false
		
	CampaignState.initialize(campaign_id, loaded_data)
	if CampaignState.get_character("char_test").get("affinity") != 0.25 or CampaignState.get_character("char_test").get("writing_style") != "A tests writing style.":
		return false
		
	# Clean up using delete_campaign
	var del_err = SaveManager.delete_campaign(campaign_id)
	if del_err != OK or FileAccess.file_exists(path):
		return false
	return true

func test_markdown_parser() -> bool:
	# 1. Create a dummy markdown vault directory
	var test_vault = "user://test_vault/"
	if not DirAccess.dir_exists_absolute(test_vault):
		DirAccess.make_dir_absolute(test_vault)
		
	var test_md = test_vault + "test_character.md"
	var file = FileAccess.open(test_md, FileAccess.WRITE)
	if not file:
		return false
		
	file.store_string("""---
type: character
name: Elara the Wise
base_affinity: 0.45
base_emotion: anger
base_intensity: 0.85
connections: [loc_phandalin, loc_crypt]
writing_style: "Speaks with wisdom."
---
# Biography
Elara is a powerful wizard.
""")
	file.close()
	
	# 2. Parse file
	var result = MarkdownParser.parse_file(test_md)
	var fm = result.get("frontmatter", {})
	var body = result.get("body", "")
	
	if fm.get("type") != "character" or fm.get("name") != "Elara the Wise" or fm.get("writing_style") != "Speaks with wisdom.":
		return false
		
	if fm.get("base_affinity") != 0.45:
		return false
		
	if fm.get("base_emotion") != "anger" or fm.get("base_intensity") != 0.85:
		return false
		
	if not fm.get("connections") is Array or fm.get("connections").size() != 2:
		return false
		
	if not body.contains("Elara is a powerful wizard."):
		return false
		
	return true

func test_vault_compiler() -> bool:
	var test_vault = "user://test_vault/"
	
	# Ensure dummy files exist
	var loc_md = test_vault + "loc_phandalin.md"
	var file = FileAccess.open(loc_md, FileAccess.WRITE)
	if not file:
		return false
	file.store_string("""---
type: location
name: Phandalin Town
connections: test_character
---
A simple town square.
""")
	file.close()
	
	var scene_md = test_vault + "scene_intro.md"
	file = FileAccess.open(scene_md, FileAccess.WRITE)
	if not file:
		return false
	file.store_string("""---
type: scene
name: The Beginning
---
You arrive at the town of Phandalin.
""")
	file.close()
	
	# Create mock image file
	var mock_img_path = test_vault + "test_character.png"
	var img_file = FileAccess.open(mock_img_path, FileAccess.WRITE)
	if not img_file:
		return false
	img_file.store_string("PNG MOCK DATA")
	img_file.close()
	
	# Compile
	var compiled = VaultCompiler.compile_vault(test_vault)
	
	if compiled.characters.is_empty() or not compiled.characters.has("test_character"):
		return false
		
	var character_compiled = compiled.characters.get("test_character")
	if character_compiled.get("writing_style") != "Speaks with wisdom.":
		return false
		
	if character_compiled.get("base_emotion") != "anger" or character_compiled.get("base_intensity") != 0.85:
		return false
		
	if not compiled.writing_style.contains("You arrive at the town of Phandalin."):
		return false
		
	if compiled.knowledge_graph.nodes.is_empty():
		return false
		
	var nodes = compiled.knowledge_graph.nodes
	if not nodes.has("test_character") or not nodes.has("loc_phandalin"):
		return false
		
	# Confirm edges compiled
	var edges = compiled.knowledge_graph.edges
	if edges.is_empty():
		return false
		
	# Verify that the image was successfully copied to user://assets/characters/test_character.png
	var copied_img_path = "user://assets/characters/test_character.png"
	if not FileAccess.file_exists(copied_img_path):
		return false
		
	# Clean up mock files and copied assets
	DirAccess.remove_absolute(copied_img_path)
	DirAccess.remove_absolute(test_vault + "test_character.md")
	DirAccess.remove_absolute(mock_img_path)
	DirAccess.remove_absolute(loc_md)
	DirAccess.remove_absolute(scene_md)
	DirAccess.remove_absolute(test_vault)
	
	return true

func test_knowledge_graph() -> bool:
	CampaignState.initialize("graph_test", {})
	var kgm = KnowledgeGraphManager.new()
	
	# 1. Add nodes
	kgm.add_node("loc_phandalin", "Phandalin", "location", "A muddy frontier village.")
	kgm.add_node("char_elara", "Elara", "npc", "A fire wizard looking for scrolls.")
	kgm.add_node("loc_tavern", "Sleeping Giant Tavern", "location", "A rough tavern.")
	
	# 2. Add edges
	kgm.add_edge("char_elara", "loc_phandalin", "visiting", 1.0)
	kgm.add_edge("loc_tavern", "loc_phandalin", "inside", 1.0)
	
	# 3. Verify relations
	var neighbors = kgm.get_neighbors("loc_phandalin")
	if neighbors.size() != 2 or not neighbors.has("char_elara") or not neighbors.has("loc_tavern"):
		return false
		
	# 4. Search context synthesis
	var context = kgm.retrieve_context("I want to speak with Elara in Phandalin")
	if not context.contains("Phandalin") or not context.contains("Elara"):
		return false
		
	if not context.contains("visiting"):
		return false
		
	return true

func test_json_repair() -> bool:
	# Case A: Perfect JSON
	var raw_a = '{"response": "Hello traveler.", "emotional_update": {"emotion": "serenity", "intensity": 0.5}}'
	var parsed_a = JsonRepair.extract_json(raw_a)
	if parsed_a.get("response") != "Hello traveler.":
		return false
		
	# Case B: JSON wrapped in conversational text
	var raw_b = 'Sure, here is the state update: {"response": "Beware the woods.", "emotional_update": {"emotion": "fear", "intensity": 0.8}} Let me know if you need anything else!'
	var parsed_b = JsonRepair.extract_json(raw_b)
	if parsed_b.get("response") != "Beware the woods.":
		return false
		
	# Case C: JSON with trailing commas (very common with local LLMs)
	var raw_c = '{"response": "Stay safe,", "emotional_update": {"emotion": "trust", "intensity": 0.6,},}'
	var parsed_c = JsonRepair.extract_json(raw_c)
	if parsed_c.get("response") != "Stay safe,":
		return false
		
	# Case D: Complete raw text fallback
	var raw_d = "I don't know what you are talking about, prepare to fight!"
	var parsed_d = JsonRepair.extract_json(raw_d)
	if parsed_d.get("response") != raw_d:
		return false
		
	return true

func test_system_prompts() -> bool:
	# 1. Verify World Builder Prompt
	var wb_prompt = SystemPrompts.get_world_builder_prompt()
	if wb_prompt.is_empty():
		return false
	if not wb_prompt.contains("WORLD BUILDER") or not wb_prompt.contains("narration") or not wb_prompt.contains("choices"):
		return false
		
	# 2. Verify Character Agent Prompt
	var ca_prompt = SystemPrompts.get_character_agent_prompt(
		"Thorin",
		"A grumpy dwarf blacksmith.",
		-0.3,
		"anger",
		0.7,
		"player",
		"Player touched the forge.",
		"Speaks in dwarf slang."
	)
	if ca_prompt.is_empty():
		return false
	if not ca_prompt.contains("Thorin") or not ca_prompt.contains("blacksmith") or not ca_prompt.contains("Enemy") or not ca_prompt.contains("Anger") or not ca_prompt.contains("Speaks in dwarf slang."):
		return false
		
	# 3. Verify CampaignState integration
	CampaignState.initialize("prompt_test", {})
	CampaignState.init_character("elara", "Elara the Wise", "A friendly wizard.", "Speaks formally.")
	CampaignState.adjust_affinity("elara", 0.5) # Should result in 'Friend'
	CampaignState.add_emotion_event("elara", "joy", 0.8, "player", "Player shared a map.")
	
	var id_prompt = SystemPrompts.get_character_agent_prompt_for_id("elara")
	if id_prompt.is_empty():
		return false
	if not id_prompt.contains("Elara the Wise") or not id_prompt.contains("Friend") or not id_prompt.contains("Joy") or not id_prompt.contains("shared a map") or not id_prompt.contains("Speaks formally."):
		return false
		
	# 4. Verify Campaign Fallback
	CampaignState.initialize("prompt_test_campaign", {
		"adventure_meta": {
			"campaign_id": "prompt_test_campaign",
			"title": "Campaign Title",
			"writing_style": "Dark gothic tone."
		}
	})
	CampaignState.init_character("valen", "Valen the Ranger", "A ranger.")
	var id_prompt_fallback = SystemPrompts.get_character_agent_prompt_for_id("valen")
	if not id_prompt_fallback.contains("Dark gothic tone."):
		return false
		
	# 5. Verify World Builder memory prompts
	if not wb_prompt.contains("memory_updates"):
		return false
	var graph_man = KnowledgeGraphManager.new()
	var ep_builder = EmotionPromptBuilder.new()
	var p_builder = PromptBuilder.new(graph_man, ep_builder)
	var dm_test_prompt = p_builder.build_world_builder_prompt("Test action")
	if dm_test_prompt.is_empty():
		return false
	if not dm_test_prompt.contains("memory_updates") or not dm_test_prompt.contains("Adventure Memories") or not dm_test_prompt.contains("Player Action/Input"):
		return false
		
	# 6. Verify Emotion Reflection Prompt
	var refl_prompt = SystemPrompts.get_emotion_reflection_prompt_for_id("valen", "A sudden bolt of lightning strikes the tower nearby.")
	if refl_prompt.is_empty():
		return false
	if not refl_prompt.contains("Valen the Ranger") or not refl_prompt.contains("ENVIRONMENTAL EVENT") or not refl_prompt.contains("lightning strikes") or not refl_prompt.contains("emotional_update"):
		return false
		
	# 7. Verify Base Emotion Deduction Prompt
	var deduce_prompt = SystemPrompts.get_deduce_base_emotion_prompt("Thorin", "A grumpy dwarf blacksmith who is always angry at his apprentice.")
	if deduce_prompt.is_empty():
		return false
	if not deduce_prompt.contains("Thorin") or not deduce_prompt.contains("grumpy dwarf") or not deduce_prompt.contains("base_emotion") or not deduce_prompt.contains("base_intensity"):
		return false
		
	return true

func test_beginning_prompt() -> bool:
	var prompt = SystemPrompts.get_beginning_generation_prompt(
		"Lost Crypt Campaign",
		"Entrance to Crypt",
		"You stand before the dark iron door of the ancient crypt.",
		[{"name": "Valen", "biography": "A ranger."}],
		[{"name": "Phandalin", "description": "A quiet village."}],
		"Descriptive, epic fantasy style."
	)
	
	if prompt.is_empty():
		return false
	if not prompt.contains("Lost Crypt Campaign") or not prompt.contains("Entrance to Crypt") or not prompt.contains("Descriptive, epic fantasy style."):
		return false
	if not prompt.contains("Valen") or not prompt.contains("Phandalin"):
		return false
	if not prompt.contains("dark iron door"):
		return false
		
	return true


func test_vault_scanner_and_mappings() -> bool:
	var test_vault = "user://test_scanner_vault/"
	if not DirAccess.dir_exists_absolute(test_vault):
		DirAccess.make_dir_absolute(test_vault)
		
	# Create subfolders
	var char_dir = test_vault + "10_Entities/characters/"
	var event_dir = test_vault + "10_Entities/events/"
	var loc_dir = test_vault + "10_Entities/locations/"
	var gate_dir = test_vault + "30_Systems/gates/"
	
	DirAccess.make_dir_recursive_absolute(char_dir)
	DirAccess.make_dir_recursive_absolute(event_dir)
	DirAccess.make_dir_recursive_absolute(loc_dir)
	DirAccess.make_dir_recursive_absolute(gate_dir)
	
	# Create character file
	var char_file = char_dir + "sita_katchent.md"
	var file = FileAccess.open(char_file, FileAccess.WRITE)
	if not file:
		return false
	file.store_string("""---
type:
  - character
name: Sita Katchent
---
A character biography.
""")
	file.close()
	
	# Create event file
	var event_file = event_dir + "meeting_marcello.md"
	file = FileAccess.open(event_file, FileAccess.WRITE)
	if not file:
		return false
	file.store_string("""---
name: Meeting Marcello
---
An event scene.
""")
	file.close()

	# Create location file
	var loc_file = loc_dir + "yild.md"
	file = FileAccess.open(loc_file, FileAccess.WRITE)
	if not file:
		return false
	file.store_string("""---
type: location
name: Yild
---
A starting location description.
""")
	file.close()
	
	# Create gate file (empty)
	var gate_file = gate_dir + "mobility_barrier.md"
	file = FileAccess.open(gate_file, FileAccess.WRITE)
	if not file:
		return false
	file.store_string("")
	file.close()
	
	# 1. Run VaultScanner scan
	var scanned = VaultScanner.scan_vault(test_vault)
	if scanned.is_empty():
		return false
		
	var folders = scanned.folders
	if folders.get("10_Entities/characters") != "character":
		return false
	if folders.get("10_Entities/events") != "scene":
		return false
	if folders.get("10_Entities/locations") != "location":
		return false
	if folders.get("30_Systems/gates") != "lore": # Empty folder/files should default/skip
		return false
		
	if scanned.potential_characters.is_empty() or scanned.potential_characters[0].id != "sita_katchent":
		return false
		
	# 2. Run VaultCompiler with custom mappings
	var custom_mappings = {
		"folders": {
			"10_Entities/characters": "character",
			"10_Entities/events": "scene",
			"10_Entities/locations": "location",
			"30_Systems/gates": "lore"
		},
		"starting_location_id": "yild",
		"starting_character_id": "sita_katchent"
	}
	
	var compiled = VaultCompiler.compile_vault(test_vault, custom_mappings)
	if compiled.scenes.is_empty() or not compiled.scenes.has("meeting_marcello"):
		return false
	if compiled.characters.is_empty() or not compiled.characters.has("sita_katchent"):
		return false
	if not compiled.knowledge_graph.nodes.has("yild") or compiled.knowledge_graph.nodes["yild"].get("type") != "location":
		return false
		
	# Check that the starting character is correct and is the first element
	if compiled.characters.keys()[0] != "sita_katchent":
		return false
		
	# Clean up mock files
	DirAccess.remove_absolute(char_file)
	DirAccess.remove_absolute(event_file)
	DirAccess.remove_absolute(loc_file)
	DirAccess.remove_absolute(gate_file)
	DirAccess.remove_absolute(char_dir)
	DirAccess.remove_absolute(event_dir)
	DirAccess.remove_absolute(loc_dir)
	DirAccess.remove_absolute(gate_dir)
	DirAccess.remove_absolute(test_vault + "10_Entities/")
	DirAccess.remove_absolute(test_vault + "30_Systems/")
	DirAccess.remove_absolute(test_vault)
	
	return true

func test_theme_manager_and_font_scaling() -> bool:
	# 1. Verify default preset exists
	if not ThemeManager.PRESETS.has("Dawn"):
		return false
		
	# 2. Select Dawn theme
	ThemeManager.select_theme("Dawn")
	if ThemeManager.active_theme_name != "Dawn":
		return false
		
	# 3. Verify font size modifier updates default theme font size
	ThemeManager.font_size_modifier = 2
	ThemeManager.apply_active_theme()
	if ThemeManager.active_theme.default_font_size != 16:
		return false
		
	# 4. Verify apply_theme_to_hierarchy updates Control override values and preserves original metadata
	var test_lbl = Label.new()
	test_lbl.add_theme_font_size_override("font_size", 12)
	ThemeManager.apply_theme_to_hierarchy(test_lbl)
	
	if test_lbl.get_theme_font_size("font_size") != 14: # 12 + 2 = 14
		return false
	if test_lbl.get_meta("original_font_size") != 12:
		return false
		
	# 5. Verify consecutive modifier changes update relative to the original metadata correctly
	ThemeManager.font_size_modifier = -2
	ThemeManager.apply_theme_to_hierarchy(test_lbl)
	if test_lbl.get_theme_font_size("font_size") != 10: # 12 - 2 = 10
		return false
		
	# 6. Verify RichTextLabel normal_font_size override
	var test_rtl = RichTextLabel.new()
	test_rtl.add_theme_font_size_override("normal_font_size", 10)
	ThemeManager.font_size_modifier = 4
	ThemeManager.apply_theme_to_hierarchy(test_rtl)
	if test_rtl.get_theme_font_size("normal_font_size") != 14: # 10 + 4 = 14
		return false
	if test_rtl.get_meta("original_normal_font_size") != 10:
		return false
		
	# Clean up and reset modifier to 0
	ThemeManager.font_size_modifier = 0
	ThemeManager.apply_active_theme()
	test_lbl.queue_free()
	test_rtl.queue_free()
	
	return true

func test_player_input_parser() -> bool:
	# 1. Test quotes style
	var res1 = PlayerInputParser.parse_input("\"Take a look at that\"! I shout")
	if res1.dialogue != "Take a look at that" or res1.action != "I shout" or res1.detected_format != "quotes":
		print("[FAIL] Quotes parsing failed: ", res1)
		return false
		
	# Test multiple quotes
	var res1b = PlayerInputParser.parse_input("\"Wait,\" I said, \"look at this.\"")
	if res1b.dialogue != "Wait, look at this." or res1b.action != "I said" or res1b.detected_format != "quotes":
		print("[FAIL] Multiple quotes parsing failed: ", res1b)
		return false

	# 2. Test asterisks style
	var res2 = PlayerInputParser.parse_input("Take a look at that *shouting*")
	if res2.dialogue != "Take a look at that" or res2.action != "shouting" or res2.detected_format != "asterisks":
		print("[FAIL] Asterisks parsing failed: ", res2)
		return false
		
	# 3. Test pure action style (heuristic)
	var res3 = PlayerInputParser.parse_input("I shout at andino to look out for that rock.")
	if res3.dialogue != "" or res3.action != "I shout at andino to look out for that rock." or res3.detected_format != "pure_action":
		print("[FAIL] Pure action heuristic failed: ", res3)
		return false
		
	# Test another pronoun
	var res3b = PlayerInputParser.parse_input("We search the ruins carefully.")
	if res3b.dialogue != "" or res3b.action != "We search the ruins carefully." or res3b.detected_format != "pure_action":
		print("[FAIL] Pure action (We) failed: ", res3b)
		return false

	# 4. Test normal style
	var res4 = PlayerInputParser.parse_input("Hello there, nice to meet you.")
	if res4.dialogue != "Hello there, nice to meet you." or res4.action != "" or res4.detected_format != "none":
		print("[FAIL] Default none parsing failed: ", res4)
		return false
		
	return true


