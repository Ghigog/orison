# res://src/ui/OnboardingFlow.gd
extends Control
class_name OnboardingFlow

signal adventure_started(campaign_id: String, title: String, vault_path: String, custom_mappings: Dictionary)
signal adventure_loaded(campaign_id: String)

const VaultScanner = preload("res://src/core/VaultScanner.gd")

# Bound UI Nodes via @onready
@onready var welcome_panel: VBoxContainer = %WelcomePanel
@onready var setup_panel: VBoxContainer = %SetupPanel
@onready var load_panel: VBoxContainer = %LoadPanel
@onready var llm_config_panel: VBoxContainer = %LLMConfigPanel
@onready var character_panel: VBoxContainer = %CharacterPanel

# Character Creation Panel Fields
@onready var pc_name_input: LineEdit = %PcNameInput
@onready var pc_avatar_preview: TextureRect = %PcAvatarPreview
@onready var pc_avatar_browse_btn: Button = %PcAvatarBrowseButton
@onready var pc_magic_wand_btn: Button = %PcMagicWandButton
@onready var pc_description_input: TextEdit = %PcDescriptionInput
@onready var pc_personality_input: TextEdit = %PcPersonalityInput
@onready var pc_backstory_input: TextEdit = %PcBackstoryInput
@onready var pc_back_btn: Button = %PcBackButton
@onready var pc_next_btn: Button = %PcNextButton

# Welcome Panel Buttons
@onready var start_new_btn: Button = %StartNewButton
@onready var load_btn: Button = %LoadButton
@onready var settings_btn: Button = %SettingsButton
@onready var overlay_bg: ColorRect = $OverlayBG
@onready var card_panel: PanelContainer = $CenterContainer/CardPanel
@onready var card_vbox: VBoxContainer = $CenterContainer/CardPanel/CardMargin/CardVBox

# Setup Panel Fields
@onready var title_input: LineEdit = %TitleInput
@onready var campaign_id_input: LineEdit = %CampaignIdInput
@onready var vault_path_input: LineEdit = %VaultPathInput
@onready var browse_btn: Button = %BrowseButton
@onready var use_sample_check: CheckBox = %UseSampleCheck
@onready var craft_btn: Button = %CraftButton
@onready var setup_back_btn: Button = %SetupBackButton

var file_dialog: FileDialog
var overwrite_dialog: ConfirmationDialog
var delete_confirm_dialog: ConfirmationDialog
var _campaign_id_to_delete: String = ""

# LLM Config Panel Fields
@onready var ollama_url_input: LineEdit = %OllamaUrlInput
@onready var world_builder_model_input: LineEdit = %WorldBuilderModelInput
@onready var character_model_input: LineEdit = %CharacterModelInput
@onready var test_connection_btn: Button = %TestConnectionButton
@onready var connection_status_label: Label = %ConnectionStatusLabel
@onready var config_back_btn: Button = %ConfigBackButton
@onready var start_campaign_btn: Button = %StartCampaignButton

# Load Panel Nodes
@onready var campaign_list: VBoxContainer = %CampaignList
@onready var load_back_btn: Button = %LoadBackButton
@onready var no_saves_label: Label = %NoSavesLabel

# Review Panel UI Bindings
@onready var review_panel: VBoxContainer = %ReviewPanel
@onready var scan_summary_label: Label = %ScanSummaryLabel
@onready var review_back_btn: Button = %ReviewBackButton
@onready var review_next_btn: Button = %ReviewNextButton

# Starter Card Bindings
@onready var starter_card_1: Button = %StarterCard1
@onready var card_title_1: Label = %CardTitle1
@onready var card_desc_1: Label = %CardDesc1
@onready var card_meta_1: Label = %CardMeta1

@onready var starter_card_2: Button = %StarterCard2
@onready var card_title_2: Label = %CardTitle2
@onready var card_desc_2: Label = %CardDesc2
@onready var card_meta_2: Label = %CardMeta2

@onready var starter_card_3: Button = %StarterCard3
@onready var card_title_3: Label = %CardTitle3
@onready var card_desc_3: Label = %CardDesc3
@onready var card_meta_3: Label = %CardMeta3

@onready var loading_overlay: PanelContainer = %LoadingOverlay
@onready var loading_text: Label = %LoadingText

# Stores scanned data
var _scanned_results: Dictionary = {}
# Stores final mappings
var _custom_mappings: Dictionary = {}
# Stores pre-compiled vault data
var _compiled_data: Dictionary = {}
var _is_transitioning: bool = false
var _selected_starter_idx: int = -1
var _generated_starters: Array = []
var _selected_clusters: Array = []
var _player_character: Dictionary = {
	"name": "",
	"physical_description": "",
	"personality": "",
	"backstory": "",
	"avatar": ""
}
var image_dialog: FileDialog


const CATEGORIES = [
	{
		"id": "scene",
		"label": "Scenes & Story",
		"icon": "🎬",
		"color": Color(1.0, 0.373, 0.220)
	},
	{
		"id": "character",
		"label": "Characters & NPCs",
		"icon": "👤",
		"color": Color(0.957, 0.247, 0.369)
	},
	{
		"id": "location",
		"label": "Locations & World",
		"icon": "📍",
		"color": Color(0.96, 0.62, 0.04)
	},
	{
		"id": "lore",
		"label": "Lore & Systems",
		"icon": "📚",
		"color": Color(0.624, 0.525, 0.753)
	}
]


func _ready() -> void:
	# 1. Connect signals
	start_new_btn.pressed.connect(_on_start_new_pressed)
	load_btn.pressed.connect(_on_load_pressed)
	settings_btn.pressed.connect(_on_settings_pressed)
	ThemeManager.theme_changed.connect(_on_theme_changed)
	_on_theme_changed()
	setup_back_btn.pressed.connect(_on_setup_back_pressed)
	load_back_btn.pressed.connect(_on_load_back_pressed)
	craft_btn.pressed.connect(_on_craft_pressed)
	
	config_back_btn.pressed.connect(_on_config_back_pressed)
	start_campaign_btn.pressed.connect(_on_start_campaign_pressed)
	test_connection_btn.pressed.connect(_on_test_connection_pressed)
	review_back_btn.pressed.connect(_on_review_back_pressed)
	review_next_btn.pressed.connect(_on_review_next_pressed)
	
	# Connect Character Creation Panel signals
	pc_back_btn.pressed.connect(_on_pc_back_pressed)
	pc_next_btn.pressed.connect(_on_pc_next_pressed)
	pc_avatar_browse_btn.pressed.connect(_on_pc_avatar_browse_pressed)
	pc_magic_wand_btn.pressed.connect(_on_pc_magic_wand_pressed)
	pc_name_input.text_changed.connect(_on_pc_name_changed)
	
	title_input.text_changed.connect(_on_title_changed)
	use_sample_check.toggled.connect(_on_use_sample_toggled)
	
	# Setup FileDialog
	file_dialog = FileDialog.new()
	file_dialog.file_mode = FileDialog.FILE_MODE_OPEN_DIR
	file_dialog.access = FileDialog.ACCESS_FILESYSTEM
	file_dialog.title = "Select Obsidian Vault Directory"
	if "use_native_dialog" in file_dialog:
		file_dialog.set("use_native_dialog", true)
	file_dialog.theme = ThemeManager.active_theme
	file_dialog.dir_selected.connect(_on_dir_selected)
	add_child(file_dialog)
	
	# Setup ImageFileDialog
	image_dialog = FileDialog.new()
	image_dialog.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	image_dialog.access = FileDialog.ACCESS_FILESYSTEM
	image_dialog.title = "Select Profile Picture"
	image_dialog.add_filter("*.png,*.jpg,*.jpeg", "Images")
	if "use_native_dialog" in image_dialog:
		image_dialog.set("use_native_dialog", true)
	image_dialog.theme = ThemeManager.active_theme
	image_dialog.file_selected.connect(_on_image_selected)
	add_child(image_dialog)
	
	# Setup Overwrite Confirmation Dialog
	overwrite_dialog = ConfirmationDialog.new()
	overwrite_dialog.title = "Confirm Overwrite"
	overwrite_dialog.theme = ThemeManager.active_theme
	overwrite_dialog.confirmed.connect(_on_overwrite_confirmed)
	add_child(overwrite_dialog)
	var overwrite_label = overwrite_dialog.get_label()
	if overwrite_label:
		overwrite_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		overwrite_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	
	# Setup Delete Confirmation Dialog
	delete_confirm_dialog = ConfirmationDialog.new()
	delete_confirm_dialog.title = "Delete Adventure"
	delete_confirm_dialog.dialog_text = "Are you sure you want to delete this adventure? This will permanently erase the save file."
	delete_confirm_dialog.theme = ThemeManager.active_theme
	delete_confirm_dialog.confirmed.connect(_on_delete_confirmed)
	add_child(delete_confirm_dialog)
	var delete_label = delete_confirm_dialog.get_label()
	if delete_label:
		delete_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		delete_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	
	browse_btn.pressed.connect(_on_browse_pressed)
	vault_path_input.text_changed.connect(func(_txt): _update_craft_button_state())
	
	# Load active LLM config into inputs
	ollama_url_input.text = LLMClient.api_url
	world_builder_model_input.text = LLMClient.world_builder_model
	character_model_input.text = LLMClient.character_model
	
	# 2. Set initial state
	welcome_panel.visible = true
	setup_panel.visible = false
	llm_config_panel.visible = false
	character_panel.visible = false
	load_panel.visible = false
	review_panel.visible = false
	
	welcome_panel.modulate.a = 1.0
	setup_panel.modulate.a = 0.0
	llm_config_panel.modulate.a = 0.0
	character_panel.modulate.a = 0.0
	load_panel.modulate.a = 0.0
	review_panel.modulate.a = 0.0
	
	pc_avatar_preview.texture = load("res://resources/assets/orisonlogo2.png")

	
	# Hide loading overlay initially
	loading_overlay.visible = false
	
	# Connect card buttons
	starter_card_1.pressed.connect(func(): _select_starter(0))
	starter_card_2.pressed.connect(func(): _select_starter(1))
	starter_card_3.pressed.connect(func(): _select_starter(2))
	
	# Disable next button initially until a card is selected
	review_next_btn.disabled = true
	
	# Setup initial sample state
	_on_use_sample_toggled(use_sample_check.button_pressed)
	_update_craft_button_state()

# ==============================================================================
# Screen Transitions
# ==============================================================================

func _transition_to(to_panel: Control, from_panel: Control) -> void:
	if not to_panel or not from_panel:
		return
		
	if _is_transitioning:
		return
	_is_transitioning = true
	
	if to_panel == llm_config_panel:
		ollama_url_input.text = LLMClient.api_url
		world_builder_model_input.text = LLMClient.world_builder_model
		character_model_input.text = LLMClient.character_model
		_on_test_connection_pressed()
		
	var is_to_card = to_panel.get_parent() == card_vbox
	var is_from_card = from_panel.get_parent() == card_vbox
	
	if is_to_card and is_from_card:
		# Lock card_panel's size and perform a smooth height/size tween transition
		var start_size = card_panel.size
		card_panel.custom_minimum_size = start_size
		
		# Temporarily toggle visibility to measure target size
		# DO NOT call reset_size() here, as it directly resizes the window on this frame!
		from_panel.visible = false
		to_panel.visible = true
		card_panel.custom_minimum_size = Vector2(480, 0)
		
		var target_size = card_panel.get_combined_minimum_size()
		target_size.x = max(target_size.x, 480.0)
		
		# Restore original visibility for the first phase of transition
		from_panel.visible = true
		to_panel.visible = false
		card_panel.custom_minimum_size = start_size
		
		var main_tween = create_tween()
		
		# Phase 1: Fade out from_panel
		main_tween.tween_property(from_panel, "modulate:a", 0.0, 0.15)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)
			
		main_tween.tween_callback(func():
			from_panel.visible = false
			from_panel.modulate.a = 1.0 # Reset for future use
		)
		
		# Phase 2: Resize card panel (sequential)
		main_tween.tween_property(card_panel, "custom_minimum_size", target_size, 0.20)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)
			
		# Phase 3: Fade in to_panel (sequential)
		main_tween.tween_callback(func():
			to_panel.visible = true
			to_panel.modulate.a = 0.0
			card_panel.custom_minimum_size = Vector2(480, 0) # Restore natural sizing
		)
		
		main_tween.tween_property(to_panel, "modulate:a", 1.0, 0.15)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)
			
		main_tween.tween_callback(func():
			_is_transitioning = false
		)
		

	else:
		# Fallback transition for any other nodes
		from_panel.visible = true
		to_panel.visible = true
		
		var tween = create_tween().set_parallel(true)
		tween.tween_property(from_panel, "modulate:a", 0.0, 0.25).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
		tween.tween_property(to_panel, "modulate:a", 1.0, 0.25).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
		
		tween.chain().tween_callback(func():
			from_panel.visible = false
			from_panel.modulate.a = 1.0
			_is_transitioning = false
		)

# ==============================================================================
# Button Callbacks
# ==============================================================================

func _on_start_new_pressed() -> void:
	_transition_to(setup_panel, welcome_panel)

func _on_load_pressed() -> void:
	_refresh_campaign_list()
	_transition_to(load_panel, welcome_panel)

func _on_setup_back_pressed() -> void:
	_transition_to(welcome_panel, setup_panel)

func _on_load_back_pressed() -> void:
	_transition_to(welcome_panel, load_panel)

func _on_config_back_pressed() -> void:
	_transition_to(setup_panel, llm_config_panel)


func _on_title_changed(new_text: String) -> void:
	if not use_sample_check.button_pressed:
		campaign_id_input.text = _slugify(new_text)
	_update_craft_button_state()

func _on_use_sample_toggled(is_checked: bool) -> void:
	if is_checked:
		title_input.text = "The Lost Crypt"
		campaign_id_input.text = "lost_crypt"
		vault_path_input.text = "user://sample_vault/"
		title_input.editable = false
		campaign_id_input.editable = false
		vault_path_input.editable = false
		browse_btn.disabled = true
	else:
		title_input.text = ""
		campaign_id_input.text = ""
		vault_path_input.text = ""
		title_input.editable = true
		campaign_id_input.editable = false # Keep auto-generated
		vault_path_input.editable = true
		browse_btn.disabled = false
	_update_craft_button_state()

func _on_browse_pressed() -> void:
	file_dialog.popup_centered_ratio(0.7)

func _on_dir_selected(dir_path: String) -> void:
	vault_path_input.text = dir_path
	_update_craft_button_state()

func _on_craft_pressed() -> void:
	var title = title_input.text.strip_edges()
	var campaign_id = campaign_id_input.text.strip_edges()
	var vault_path = vault_path_input.text.strip_edges()
	
	if title.is_empty() or campaign_id.is_empty() or vault_path.is_empty():
		return
		
	# Check if campaign already exists on disk
	var campaign_exists = false
	var saves = SaveManager.get_campaign_list()
	for save in saves:
		if save.id == campaign_id:
			campaign_exists = true
			break
			
	if campaign_exists:
		overwrite_dialog.dialog_text = "An adventure with the ID '%s' already exists. Overwriting will delete all previous saves and progress for this adventure. Do you want to proceed?" % campaign_id
		overwrite_dialog.popup_centered()
	else:
		_proceed_to_next_step()

func _on_overwrite_confirmed() -> void:
	_proceed_to_next_step()

func _proceed_to_next_step() -> void:
	_transition_to(llm_config_panel, setup_panel)

func _on_start_campaign_pressed() -> void:
	var title = title_input.text.strip_edges()
	var campaign_id = campaign_id_input.text.strip_edges()
	var vault_path = vault_path_input.text.strip_edges()
	
	if title.is_empty() or campaign_id.is_empty() or vault_path.is_empty():
		return
		
	# Save updated LLM Settings
	LLMClient.api_url = ollama_url_input.text.strip_edges()
	LLMClient.world_builder_model = world_builder_model_input.text.strip_edges()
	LLMClient.character_model = character_model_input.text.strip_edges()
	LLMClient.save_config()
	
	# Show loading overlay
	loading_overlay.visible = true
	loading_text.text = "Initializing campaign vault..."
	
	# Force Godot to repaint a frame so the overlay displays immediately
	await get_tree().process_frame
	
	if use_sample_check.button_pressed:
		_generate_sample_vault(vault_path)
		
	# Run vault scanner
	_scanned_results = VaultScanner.scan_vault(vault_path)
	
	# Setup initial custom mappings
	_custom_mappings = {
		"folders": {},
		"starting_location_id": "",
		"starting_character_id": ""
	}
	var folders = _scanned_results.get("folders", {})
	for folder_path in folders.keys():
		_custom_mappings["folders"][folder_path] = folders[folder_path]
		
	# Compile vault
	_compiled_data = VaultCompiler.compile_vault(vault_path, _custom_mappings)
	_custom_mappings["compiled_data"] = _compiled_data
	
	# Populate metrics summary on review panel
	_populate_review_panel()
	
	# Update loading message for LLM generation
	loading_text.text = "Drafting adventure hooks via local LLM..."
	await get_tree().process_frame
	
	# Transition to Hook Selection screen while keeping the overlay visible
	_transition_to(review_panel, llm_config_panel)
	
	# Trigger LLM Hook Generation
	_generate_adventure_hooks()

func _on_test_connection_pressed() -> void:
	var test_url = ollama_url_input.text.strip_edges()
	if test_url.is_empty():
		connection_status_label.add_theme_color_override("font_color", Color.html("#e05353"))
		connection_status_label.text = "URL is empty"
		return
		
	connection_status_label.add_theme_color_override("font_color", Color.html("#611765"))
	connection_status_label.text = "Testing connection..."
	test_connection_btn.disabled = true
	
	LLMClient.test_connection(test_url, _on_connection_test_completed)

func _on_connection_test_completed(success: bool, error_msg: String, models: Array[String]) -> void:
	test_connection_btn.disabled = false
	
	if success:
		var wb_model = world_builder_model_input.text.strip_edges()
		var char_model = character_model_input.text.strip_edges()
		
		var wb_found = wb_model in models
		var char_found = char_model in models
		
		var status_text = "Success! Connected to Ollama server."
		var color = Color.html("#52cc7a") # Green
		
		if models.is_empty():
			status_text += "\nWarning: No models are pulled yet."
			color = Color.html("#e6c229") # Yellow
		else:
			var unique_models: Array[String] = []
			for m in models:
				if not unique_models.has(m):
					unique_models.append(m)
			status_text += "\nModels found: " + ", ".join(unique_models.slice(0, 4))
			if unique_models.size() > 4:
				status_text += " (and %d more)" % (unique_models.size() - 4)
				
			if not wb_found or not char_found:
				color = Color.html("#e6c229") # Yellow
				if not wb_found:
					status_text += "\nWarning: DM Model '%s' not found locally." % wb_model
				if not char_found:
					status_text += "\nWarning: NPC Model '%s' not found locally." % char_model
					
		connection_status_label.add_theme_color_override("font_color", color)
		connection_status_label.text = status_text
	else:
		connection_status_label.add_theme_color_override("font_color", Color.html("#e05353")) # Red
		connection_status_label.text = "Error: %s" % error_msg

# ==============================================================================
# Save Game List Handler
# ==============================================================================

func _refresh_campaign_list() -> void:
	# Clear previous list items
	for child in campaign_list.get_children():
		child.queue_free()
		
	var saves = SaveManager.get_campaign_list()
	if saves.is_empty():
		no_saves_label.visible = true
	else:
		no_saves_label.visible = false
		for save in saves:
			var hbox = HBoxContainer.new()
			hbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			
			var btn = Button.new()
			btn.text = "%s (%s)" % [save.get("title", save.id), save.get("last_played", "Unknown Date")]
			btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
			btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			btn.pressed.connect(func():
				_load_campaign(save.id)
			)
			hbox.add_child(btn)
			
			var del_btn = Button.new()
			del_btn.text = "🗑️"
			del_btn.tooltip_text = "Delete Adventure"
			del_btn.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
			del_btn.add_theme_color_override("font_hover_color", Color(1.0, 0.5, 0.5))
			del_btn.pressed.connect(func():
				_prompt_delete_campaign(save.id)
			)
			hbox.add_child(del_btn)
			
			campaign_list.add_child(hbox)

func _prompt_delete_campaign(campaign_id: String) -> void:
	_campaign_id_to_delete = campaign_id
	delete_confirm_dialog.popup_centered()

func _on_delete_confirmed() -> void:
	if not _campaign_id_to_delete.is_empty():
		SaveManager.delete_campaign(_campaign_id_to_delete)
		_campaign_id_to_delete = ""
		_refresh_campaign_list()

func _load_campaign(campaign_id: String) -> void:
	var tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, 0.4).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.tween_callback(func():
		adventure_loaded.emit(campaign_id)
		queue_free()
	)

# ==============================================================================
# Helper Methods
# ==============================================================================

func _slugify(text: String) -> String:
	var slug = text.to_lower().strip_edges()
	var regex = RegEx.new()
	regex.compile("[^a-z0-9_]")
	slug = slug.replace(" ", "_")
	slug = regex.sub(slug, "", true)
	return slug

func _update_craft_button_state() -> void:
	var title = title_input.text.strip_edges()
	var path = vault_path_input.text.strip_edges()
	craft_btn.disabled = title.is_empty() or path.is_empty()

# ==============================================================================
# Sample Vault Creator
# ==============================================================================

func _generate_sample_vault(path: String) -> void:
	# Ensure the directory exists
	if not DirAccess.dir_exists_absolute(path):
		DirAccess.make_dir_absolute(path)
		
	# 1. Elara Character File
	_write_file(path.path_join("elara_the_wise.md"), """---
type: character
name: Elara the Wise
base_affinity: 0.2
connections: [phandalin_town_square, crypt_of_shadows]
relationships:
  valen_the_ranger: Friend
writing_style: "Speaks with formal, archaic elven grace. Often uses terms like 'indeed', 'observe', or references ancient lore."
---
Elara is an elven wizard who spends her days studying the ancient ley lines of the valley. She is searching for a lost spellbook hidden within the Crypt of Shadows. She appears calm but has a fierce intellectual curiosity.
""")

	# 2. Valen Character File
	_write_file(path.path_join("valen_the_ranger.md"), """---
type: character
name: Valen the Ranger
base_affinity: 0.1
connections: [phandalin_town_square]
relationships:
  elara_the_wise: Companion
---
Valen is a gruff but loyal ranger who guards the borderlands of Phandalin. He knows the dangers of the forest and helps travelers navigate the mountain passes. He is highly suspicious of magic.

## Dialogue Style
- Speaks gruffly, using short sentences and contractions.
- E.g., "Watch your step." or "Magic's trouble."
""")

	# 3. Phandalin Location File
	_write_file(path.path_join("phandalin_town_square.md"), """---
type: location
name: Phandalin Town Square
connections: [crypt_of_shadows]
---
The bustling center of the frontier town of Phandalin. Merchants set up wooden stalls around a stone well, and town guards watch the road. A dusty track leads north toward the mountains.
""")

	# 4. Crypt Location File
	_write_file(path.path_join("crypt_of_shadows.md"), """---
type: location
name: Crypt of Shadows
connections: [phandalin_town_square]
---
A dark, cold stone structure half-buried in the mountainside. Sigils of protection are carved into the heavy iron-reinforced wooden door, though some have been defaced. A chill draft escapes from the gaps.
""")

	# 5. Intro Scene File
	_write_file(path.path_join("intro_the_journey_begins.md"), """---
type: scene
name: The Journey Begins
---
The wind howls through the mountain passes of Phandalin as you arrive at the town square. Standing near the stone well is Elara the Wise, her robes fluttering in the cold air. She looks at you with anticipation, hoping you can help her recover the lost spellbook.

Valen says, "Hmph. More magic nonsense. We should stay clear of that crypt."
""")

	# 6. Global Writing Style File
	_write_file(path.path_join("writing_style.md"), """---
type: lore
name: Campaign Writing Style
---
A dark fantasy prose style, with atmospheric, descriptive sentences. Pacing is slow and measured.
""")

func _write_file(file_path: String, content: String) -> void:
	var file = FileAccess.open(file_path, FileAccess.WRITE)
	if file:
		file.store_string(content)
		file.close()
	else:
		printerr("Failed to create sample vault file: ", file_path)

func _on_review_back_pressed() -> void:
	_transition_to(character_panel, review_panel)

func _on_review_next_pressed() -> void:
	if _selected_starter_idx == -1 or _selected_starter_idx >= _generated_starters.size():
		return
		
	var selected_hook = _generated_starters[_selected_starter_idx]
	
	# Save updated mappings
	_custom_mappings["starting_location_id"] = selected_hook.location_id
	_custom_mappings["starting_character_id"] = selected_hook.character_id
	_custom_mappings["intro_narration"] = selected_hook.narration
	
	# Recompile the vault with selected starting mappings to sort it correctly
	var vault_path = vault_path_input.text.strip_edges()
	_compiled_data = VaultCompiler.compile_vault(vault_path, _custom_mappings)
	_custom_mappings["compiled_data"] = _compiled_data
	
	var title = title_input.text.strip_edges()
	var campaign_id = campaign_id_input.text.strip_edges()
	
	# Animate closing overlay
	var tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, 0.4).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	tween.tween_callback(func():
		adventure_started.emit(campaign_id, title, vault_path, _custom_mappings)
		queue_free()
	)

func _populate_review_panel() -> void:
	var folders = _scanned_results.get("folders", {})
	var all_files = _scanned_results.get("all_files", [])
	
	var locations_count = 0
	var characters_count = 0
	var scenes_count = 0
	var lore_count = 0
	
	var vault_path_clean = vault_path_input.text.strip_edges()
	for file_entry in all_files:
		var file_path = file_entry.file_path
		var rel_path = file_path.substr(vault_path_clean.length()).lstrip("/")
		var parent_dir = rel_path.get_base_dir()
		var type = folders.get(parent_dir, "lore")
		match type:
			"location": locations_count += 1
			"character": characters_count += 1
			"scene": scenes_count += 1
			"lore": lore_count += 1
			
	var summary = ""
	summary += "•  Locations / Regions: %d\n" % locations_count
	summary += "•  Characters / NPCs: %d\n" % characters_count
	summary += "•  Scenes / Quests: %d\n" % scenes_count
	summary += "•  Lore / Systems: %d" % lore_count
	
	scan_summary_label.text = summary

func _build_connected_starting_clusters() -> Array:
	var clusters = []
	
	var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
	var edges = _compiled_data.get("knowledge_graph", {}).get("edges", [])
	
	# Group nodes by type
	var locations = []
	var characters = []
	var lore_and_scenes = []
	
	for node_id in nodes.keys():
		var node = nodes[node_id]
		var type = node.get("type", "")
		var info = {
			"id": node_id,
			"name": node.get("label", node_id),
			"desc": node.get("desc", "")
		}
		if type == "location":
			locations.append(info)
		elif type == "character":
			characters.append(info)
		elif type in ["scene", "lore"]:
			lore_and_scenes.append(info)
			
	# Map location connections (adjacencies)
	var loc_connections = {}
	for loc in locations:
		loc_connections[loc.id] = {
			"characters": [],
			"lore": []
		}
		
	for edge in edges:
		var f = edge.get("from", "")
		var t = edge.get("to", "")
		
		var f_node = nodes.get(f)
		var t_node = nodes.get(t)
		if not f_node or not t_node:
			continue
			
		var f_type = f_node.get("type")
		var t_type = t_node.get("type")
		
		var add_conn = func(loc_id: String, other_id: String, other_type: String):
			if not loc_connections.has(loc_id):
				return
			var other_info = {
				"id": other_id,
				"name": nodes[other_id].get("label", other_id),
				"desc": nodes[other_id].get("desc", "")
			}
			if other_type == "character":
				var exists = false
				for c in loc_connections[loc_id].characters:
					if c.id == other_id:
						exists = true
						break
				if not exists:
					loc_connections[loc_id].characters.append(other_info)
			elif other_type in ["scene", "lore"]:
				var exists = false
				for l in loc_connections[loc_id].lore:
					if l.id == other_id:
						exists = true
						break
				if not exists:
					loc_connections[loc_id].lore.append(other_info)
					
		if f_type == "location" and t_type == "character":
			add_conn.call(f, t, "character")
		elif t_type == "location" and f_type == "character":
			add_conn.call(t, f, "character")
		elif f_type == "location" and t_type in ["scene", "lore"]:
			add_conn.call(f, t, "lore")
		elif t_type == "location" and f_type in ["scene", "lore"]:
			add_conn.call(t, f, "lore")
			
	# Rank locations by connection count to find the most "substantial" candidates
	var loc_candidates = locations.duplicate()
	loc_candidates.sort_custom(func(a, b):
		var a_conns = loc_connections[a.id].characters.size() + loc_connections[a.id].lore.size()
		var b_conns = loc_connections[b.id].characters.size() + loc_connections[b.id].lore.size()
		return a_conns > b_conns # Descending order
	)
	
	# Select up to 3 clusters
	var selected_locs = []
	for loc in loc_candidates:
		# We prefer locations that have at least one character connection
		if loc_connections[loc.id].characters.size() > 0:
			selected_locs.append(loc)
		if selected_locs.size() == 3:
			break
			
	# If we couldn't find 3 locations with character connections, pad with other locations
	if selected_locs.size() < 3:
		for loc in loc_candidates:
			if not selected_locs.has(loc):
				selected_locs.append(loc)
			if selected_locs.size() == 3:
				break
				
	# Pad selected_locs with duplicates or empty location models if fewer than 3
	while selected_locs.size() < 3:
		if selected_locs.size() > 0:
			selected_locs.append(selected_locs[0].duplicate())
		else:
			selected_locs.append({"id": "", "name": "Unknown Location", "desc": "A mysterious starting point."})
			
	# Build the clusters
	for loc in selected_locs:
		var loc_id = loc.id
		var conns = loc_connections.get(loc_id, {"characters": [], "lore": []})
		
		# A. Select character
		var selected_char = null
		if conns.characters.size() > 0:
			var char_pool = conns.characters.duplicate()
			char_pool.shuffle()
			selected_char = char_pool[0]
		else:
			if characters.size() > 0:
				var char_pool = characters.duplicate()
				char_pool.shuffle()
				selected_char = char_pool[0]
			else:
				selected_char = {"id": "", "name": "Companion", "desc": "A quiet traveler assisting you."}
				
		# B. Select lore/scene
		var selected_lore = null
		if conns.lore.size() > 0:
			var lore_pool = conns.lore.duplicate()
			lore_pool.shuffle()
			selected_lore = lore_pool[0]
		else:
			if lore_and_scenes.size() > 0:
				var lore_pool = lore_and_scenes.duplicate()
				lore_pool.shuffle()
				selected_lore = lore_pool[0]
			else:
				selected_lore = {"id": "", "name": "", "desc": ""}
				
		clusters.append({
			"location": loc,
			"character": selected_char,
			"lore": selected_lore
		})
		
	return clusters

func _generate_adventure_hooks() -> void:
	var campaign_title = title_input.text.strip_edges()
	
	# Programmatically find pre-connected clusters using the Knowledge Graph
	_selected_clusters = _build_connected_starting_clusters()
	
	_selected_starter_idx = -1
	_update_card_selection_visuals()
	review_next_btn.disabled = true
	
	var writing_style = _compiled_data.get("writing_style", "")
	
	# Verify that we have any starting locations
	var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
	var has_locations = false
	for node_id in nodes.keys():
		if nodes[node_id].get("type") == "location":
			has_locations = true
			break
			
	if not has_locations:
		var fallback_starters = _generate_fallback_starters([])
		_display_starters(fallback_starters)
		loading_overlay.visible = false
		return
		
	# Call local LLM to generate the narrative text for these pre-connected clusters
	var prompt = SystemPrompts.get_starters_generation_prompt(campaign_title, _selected_clusters, writing_style, _player_character)
	LLMClient.send_custom_request(prompt, LLMClient.world_builder_model, _on_hooks_generated)

func _on_hooks_generated(success: bool, response_text: String, error_msg: String) -> void:
	loading_overlay.visible = false
	
	if not success:
		print("[OnboardingFlow] LLM hook generation failed. Error: ", error_msg, ". Using fallbacks.")
		var fallback_starters = _generate_fallback_starters(_selected_clusters)
		_display_starters(fallback_starters)
		return
		
	var parsed = _parse_json_response(response_text)
	var starters = parsed.get("starters", [])
	
	if not (starters is Array) or starters.is_empty():
		print("[OnboardingFlow] LLM returned empty or invalid starters JSON. Using fallbacks.")
		print("[OnboardingFlow] Raw response from LLM was:\n", response_text)
		var fallback_starters = _generate_fallback_starters(_selected_clusters)
		_display_starters(fallback_starters)
		return
		
	var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
	var clean_starters = []
	for i in range(starters.size()):
		var s = starters[i]
		if s is Dictionary and s.has("title") and s.has("description") and s.has("narration"):
			# Match to the corresponding pre-selected cluster
			var cluster_index = min(i, _selected_clusters.size() - 1)
			var cluster = _selected_clusters[cluster_index]
			
			var loc_id = str(s.get("location_id", "")).to_lower().replace(" ", "_")
			var char_id = str(s.get("character_id", "")).to_lower().replace(" ", "_")
			
			# Fallback to cluster's predefined IDs if the LLM returned invalid IDs
			if loc_id.is_empty() or not nodes.has(loc_id):
				loc_id = cluster.location.id
			if char_id.is_empty() or not nodes.has(char_id):
				char_id = cluster.character.id
				
			clean_starters.append({
				"title": str(s["title"]),
				"description": str(s["description"]),
				"narration": str(s["narration"]),
				"location_id": loc_id,
				"character_id": char_id
			})
			
	if clean_starters.is_empty():
		clean_starters = _generate_fallback_starters(_selected_clusters)
		
	# Pad to exactly 3 starters
	while clean_starters.size() < 3:
		var fallbacks = _generate_fallback_starters(_selected_clusters)
		var index = clean_starters.size()
		clean_starters.append(fallbacks[index])
		
	_display_starters(clean_starters)

func _display_starters(starters: Array) -> void:
	_generated_starters = starters
	
	var titles = [card_title_1, card_title_2, card_title_3]
	var descs = [card_desc_1, card_desc_2, card_desc_3]
	var metas = [card_meta_1, card_meta_2, card_meta_3]
	
	var nodes = _compiled_data.get("knowledge_graph", {}).get("nodes", {})
	
	for i in range(3):
		var starter = starters[i]
		var title_lbl = titles[i]
		var desc_lbl = descs[i]
		var meta_lbl = metas[i]
		
		title_lbl.text = starter.get("title", "Adventure Option %d" % (i + 1))
		desc_lbl.text = starter.get("description", "A mysterious starting point.")
		
		var loc_id = starter.get("location_id", "")
		var char_id = starter.get("character_id", "")
		
		var loc_name = nodes[loc_id].get("label", loc_id) if nodes.has(loc_id) else "Unknown Area"
		var char_name = nodes[char_id].get("label", char_id) if nodes.has(char_id) else "Unknown Companion"
		
		meta_lbl.text = "📍 %s  •  👤 %s" % [loc_name, char_name]

func _generate_fallback_starters(clusters: Array) -> Array:
	var starters = []
	
	for i in range(3):
		var cluster = null
		if i < clusters.size():
			cluster = clusters[i]
		else:
			# Absolute baseline fallback if no clusters supplied
			cluster = {
				"location": {"id": "", "name": "Unknown Location"},
				"character": {"id": "", "name": "Unknown Character"}
			}
			
		var loc = cluster.get("location", {"id": "", "name": "Unknown Location"})
		var ch = cluster.get("character", {"id": "", "name": "Unknown Character"})
		
		var title = ""
		var desc = ""
		var narration = ""
		
		match i:
			0:
				title = "A Whisper in the Shadows"
				desc = "A sudden, freezing breeze sweeps through %s. A quiet whisper calls out your name." % loc.name
				narration = "The air grows thin and cold as you arrive at %s. A quiet, raspy whisper echoes from the shadows, calling your name. Standing nearby, %s notices the sudden drop in temperature, their hand instinctively reaching for their weapon as they look around in quiet alarm." % [loc.name, ch.name]
			1:
				title = "The Burning Sigil"
				desc = "A flash of crimson light erupts in %s, carving a glowing, hot sigil into the stone." % loc.name
				narration = "A sharp hiss echoes through %s as a brilliant crimson sigil begins to glow on a nearby stone surface. The rock cracks under the sudden intense heat. %s stares at the burning mark, face pale in the red light, whispering that this is a sign they had hoped never to see again." % [loc.name, ch.name]
			2:
				title = "The Shattered Mirror"
				desc = "A loud crack splits the air in %s, leaving behind a shimmering rift that distorts reality." % loc.name
				narration = "A deafening sound like shattering glass reverberates through %s. In its wake, a vertical shimmer hangs in the air, refracting light like a warped lens. %s takes a step back, warning you that the fabric of this place has just fractured, and whatever is on the other side is starting to look back." % [loc.name, ch.name]
				
		starters.append({
			"title": title,
			"description": desc,
			"location_id": loc.id,
			"character_id": ch.id,
			"narration": narration
		})
		
	return starters

func _select_starter(index: int) -> void:
	if index < 0 or index >= _generated_starters.size():
		return
	_selected_starter_idx = index
	_update_card_selection_visuals()
	review_next_btn.disabled = false

func _update_card_selection_visuals() -> void:
	var cards = [starter_card_1, starter_card_2, starter_card_3]
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	
	var selected_sb = StyleBoxFlat.new()
	selected_sb.bg_color = ThemeManager.color_accent
	selected_sb.bg_color.a = 0.25 if is_light else 0.20
	selected_sb.border_color = ThemeManager.color_accent
	selected_sb.border_width_left = 3
	selected_sb.border_width_top = 3
	selected_sb.border_width_right = 3
	selected_sb.border_width_bottom = 3
	selected_sb.content_margin_left = 8
	selected_sb.content_margin_right = 8
	selected_sb.content_margin_top = 8
	selected_sb.content_margin_bottom = 8
	
	for i in range(cards.size()):
		var card = cards[i]
		if i == _selected_starter_idx:
			card.add_theme_stylebox_override("normal", selected_sb)
			card.add_theme_stylebox_override("hover", selected_sb)
			card.add_theme_stylebox_override("focus", selected_sb)
		else:
			card.remove_theme_stylebox_override("normal")
			card.remove_theme_stylebox_override("hover")
			card.remove_theme_stylebox_override("focus")

func _parse_json_response(raw_text: String) -> Dictionary:
	var result = {}
	var text = raw_text.strip_edges()
	
	if text.contains("```json"):
		var start_idx = text.find("```json") + 7
		var end_idx = text.find("```", start_idx)
		if end_idx != -1:
			text = text.substr(start_idx, end_idx - start_idx).strip_edges()
	elif text.contains("```"):
		var start_idx = text.find("```") + 3
		var end_idx = text.find("```", start_idx)
		if end_idx != -1:
			text = text.substr(start_idx, end_idx - start_idx).strip_edges()
			
	var first_brace = text.find("{")
	var last_brace = text.rfind("}")
	if first_brace != -1 and last_brace != -1 and last_brace > first_brace:
		text = text.substr(first_brace, last_brace - first_brace + 1)
		
	var json = JSON.new()
	if json.parse(text) == OK:
		if json.data is Dictionary:
			result = json.data
	return result

func show_screen(screen_name: String) -> void:
	match screen_name:
		"welcome":
			welcome_panel.visible = true
			setup_panel.visible = false
			llm_config_panel.visible = false
			character_panel.visible = false
			load_panel.visible = false
			review_panel.visible = false
			welcome_panel.modulate.a = 1.0
		"setup":
			welcome_panel.visible = false
			setup_panel.visible = true
			llm_config_panel.visible = false
			character_panel.visible = false
			load_panel.visible = false
			review_panel.visible = false
			setup_panel.modulate.a = 1.0
			_on_use_sample_toggled(use_sample_check.button_pressed)
		"character":
			welcome_panel.visible = false
			setup_panel.visible = false
			llm_config_panel.visible = false
			character_panel.visible = true
			review_panel.visible = false
			load_panel.visible = false
			character_panel.modulate.a = 1.0
			
			# Populate inputs
			pc_name_input.text = _player_character.get("name", "")
			pc_description_input.text = _player_character.get("physical_description", "")
			pc_personality_input.text = _player_character.get("personality", "")
			pc_backstory_input.text = _player_character.get("backstory", "")
			
			var avatar_path = _player_character.get("avatar", "")
			if not avatar_path.is_empty() and FileAccess.file_exists(avatar_path):
				var img = Image.load_from_file(avatar_path)
				if img:
					pc_avatar_preview.texture = ImageTexture.create_from_image(img)
			else:
				pc_avatar_preview.texture = load("res://resources/assets/orisonlogo2.png")
				
			_update_pc_next_button_state()
		"review":
			welcome_panel.visible = false
			setup_panel.visible = false
			llm_config_panel.visible = false
			character_panel.visible = false
			load_panel.visible = false
			review_panel.visible = true
			review_panel.modulate.a = 1.0
		"config":
			welcome_panel.visible = false
			setup_panel.visible = false
			llm_config_panel.visible = true
			character_panel.visible = false
			load_panel.visible = false
			review_panel.visible = false
			llm_config_panel.modulate.a = 1.0
			_on_test_connection_pressed()
		"load":
			welcome_panel.visible = false
			setup_panel.visible = false
			llm_config_panel.visible = false
			character_panel.visible = false
			load_panel.visible = true
			review_panel.visible = false
			load_panel.modulate.a = 1.0
			_refresh_campaign_list()


func _on_settings_pressed() -> void:
	var modal_scene = load("res://scenes/ui/SettingsModal.tscn")
	if modal_scene:
		var modal = modal_scene.instantiate()
		modal.theme = ThemeManager.active_theme
		add_child(modal)

func _on_theme_changed() -> void:
	self.theme = ThemeManager.active_theme

# ==============================================================================
# Character Creation Callbacks
# ==============================================================================

func _on_pc_back_pressed() -> void:
	_transition_to(llm_config_panel, character_panel)

func _on_pc_next_pressed() -> void:
	var pc_name = pc_name_input.text.strip_edges()
	if pc_name.is_empty():
		return
		
	_player_character["name"] = pc_name
	_player_character["physical_description"] = pc_description_input.text.strip_edges()
	_player_character["personality"] = pc_personality_input.text.strip_edges()
	_player_character["backstory"] = pc_backstory_input.text.strip_edges()
	
	# Show loading overlay
	loading_overlay.visible = true
	loading_text.text = "Initializing campaign vault..."
	
	# Force Godot to repaint a frame so the overlay displays immediately
	await get_tree().process_frame
	
	# Copy player avatar image to local user directory if selected
	var pc_avatar_source = _player_character.get("avatar", "")
	if not pc_avatar_source.is_empty() and FileAccess.file_exists(pc_avatar_source):
		var ext = pc_avatar_source.get_extension().to_lower()
		var target_dir = "user://assets/characters"
		var dir = DirAccess.open("user://")
		if dir:
			if not dir.dir_exists("assets/characters"):
				dir.make_dir_recursive("assets/characters")
		var target_path = target_dir.path_join("player.%s" % ext)
		if pc_avatar_source != target_path:
			var copy_err = DirAccess.copy_absolute(pc_avatar_source, target_path)
			if copy_err == OK:
				_player_character["avatar"] = target_path
				print("[OnboardingFlow] Copied player avatar to: ", target_path)
				
	_custom_mappings["player_character"] = _player_character
	
	var vault_path = vault_path_input.text.strip_edges()
	if use_sample_check.button_pressed:
		_generate_sample_vault(vault_path)
		
	# Run vault scanner
	_scanned_results = VaultScanner.scan_vault(vault_path)
	
	# Setup initial custom mappings
	_custom_mappings["folders"] = {}
	_custom_mappings["starting_location_id"] = ""
	_custom_mappings["starting_character_id"] = ""
	
	var folders = _scanned_results.get("folders", {})
	for folder_path in folders.keys():
		_custom_mappings["folders"][folder_path] = folders[folder_path]
		
	# Compile vault
	_compiled_data = VaultCompiler.compile_vault(vault_path, _custom_mappings)
	_custom_mappings["compiled_data"] = _compiled_data
	
	# Populate metrics summary on review panel
	_populate_review_panel()
	
	# Update loading message for LLM generation
	loading_text.text = "Drafting adventure hooks via local LLM..."
	await get_tree().process_frame
	
	# Transition to Hook Selection screen while keeping the overlay visible
	_transition_to(review_panel, character_panel)
	
	# Trigger LLM Hook Generation
	_generate_adventure_hooks()

func _on_pc_avatar_browse_pressed() -> void:
	image_dialog.popup_centered_ratio(0.7)

func _on_image_selected(file_path: String) -> void:
	_player_character["avatar"] = file_path
	var img = Image.load_from_file(file_path)
	if img:
		pc_avatar_preview.texture = ImageTexture.create_from_image(img)
	else:
		pc_avatar_preview.texture = load("res://resources/assets/orisonlogo2.png")

func _on_pc_name_changed(_txt: String) -> void:
	_update_pc_next_button_state()

func _update_pc_next_button_state() -> void:
	var pc_name = pc_name_input.text.strip_edges()
	pc_next_btn.disabled = pc_name.is_empty()

func _on_pc_magic_wand_pressed() -> void:
	var avatar_path = _player_character.get("avatar", "")
	if avatar_path.is_empty() or not FileAccess.file_exists(avatar_path):
		return
		
	pc_magic_wand_btn.disabled = true
	pc_magic_wand_btn.text = " 🪄 ..."
	
	var prompt = "Analyze this character portrait. Provide a super quick summary of their physical features, such as hair color and length, eye color, facial structure, interesting details, dress, gender representation, and overall aesthetic. Be concise, direct, and descriptive, in 1-2 short paragraphs."
	LLMClient.send_custom_vision_request(prompt, avatar_path, LLMClient.world_builder_model, _on_vision_description_completed)

func _on_vision_description_completed(success: bool, response_text: String, error_msg: String) -> void:
	pc_magic_wand_btn.disabled = false
	pc_magic_wand_btn.text = " 🪄 "
	
	if success:
		pc_description_input.text = response_text.strip_edges()
	else:
		print("[OnboardingFlow] Vision model analysis failed: ", error_msg)
