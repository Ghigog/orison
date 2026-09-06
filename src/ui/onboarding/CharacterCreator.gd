# res://src/ui/onboarding/CharacterCreator.gd
extends VBoxContainer

signal back_pressed
signal character_created(character_data: Dictionary)

@onready var pc_avatar_overlay: AssetStatusOverlay = %PcAvatarOverlay


@onready var pc_name_input: LineEdit = %PcNameInput
@onready var pc_avatar_preview: TextureRect = %PcAvatarPreview
@onready var pc_avatar_browse_btn: Button = %PcAvatarBrowseButton
@onready var pc_magic_wand_btn: Button = %PcMagicWandButton
@onready var pc_avatar_magic_wand_btn: Button = %PcAvatarMagicWandButton
@onready var pc_description_input: TextEdit = %PcDescriptionInput
@onready var pc_wand_status_label: Label = %PcWandStatusLabel
@onready var pc_bg_progress_label: Label = %PcBgProgressLabel

@onready var pc_personality_input: TextEdit = %PcPersonalityInput
@onready var pc_backstory_input: TextEdit = %PcBackstoryInput
@onready var pc_back_btn: Button = %PcBackButton
@onready var pc_next_btn: Button = %PcNextButton
@onready var campaign_art_style_dropdown: OptionButton = %CampaignArtStyleDropdown

# New validation and counter nodes
@onready var pc_name_error_label: Label = %PcNameErrorLabel
@onready var pc_description_counter: Label = %PcDescriptionCounter
@onready var pc_personality_counter: Label = %PcPersonalityCounter
@onready var pc_backstory_counter: Label = %PcBackstoryCounter

@onready var image_dialog: FileDialog = %ImageFileDialog
var _player_character: Dictionary = {
	"name": "",
	"physical_description": "",
	"personality": "",
	"backstory": "",
	"avatar": ""
}
var _avatar_gen_callable: Callable

var _name_regex: RegEx
var _name_touched: bool = false
var _desc_history: TextHistory
var _personality_history: TextHistory
var _backstory_history: TextHistory

func _ready() -> void:
	pc_back_btn.pressed.connect(func(): back_pressed.emit())
	pc_next_btn.pressed.connect(_on_next_pressed)
	pc_avatar_browse_btn.pressed.connect(_on_pc_avatar_browse_pressed)
	pc_magic_wand_btn.pressed.connect(_on_pc_magic_wand_pressed)
	pc_avatar_magic_wand_btn.pressed.connect(_on_pc_avatar_magic_wand_pressed)
	pc_name_input.text_changed.connect(_on_pc_name_changed)
	


	
	# Set tooltips for accessibility
	pc_magic_wand_btn.tooltip_text = "Generate a random description using the local LLM"
	pc_avatar_magic_wand_btn.tooltip_text = "Generate a character avatar portrait using the local AI image generator"
	pc_avatar_browse_btn.tooltip_text = "Select a local avatar image file from your system"
	
	
	# Setup ImageFileDialog
	if "use_native_dialog" in image_dialog:
		image_dialog.set("use_native_dialog", true)
	image_dialog.theme = ThemeManager.active_theme
	image_dialog.file_selected.connect(_on_image_selected)

func _exit_tree() -> void:
	_disconnect_avatar_gen()

func setup_character(char_data: Dictionary) -> void:
	_player_character = char_data.duplicate(true)
	_name_touched = false
	
	pc_name_input.text = _player_character.get("name", "")
	
	if not _desc_history:
		_desc_history = TextHistory.new(pc_description_input, self)
		_personality_history = TextHistory.new(pc_personality_input, self)
		_backstory_history = TextHistory.new(pc_backstory_input, self)
		
	_desc_history.reset(_player_character.get("physical_description", ""))
	_personality_history.reset(_player_character.get("personality", ""))
	_backstory_history.reset(_player_character.get("backstory", ""))
	
	var avatar_path = _player_character.get("avatar", "")
	var target_path = ""
	if not avatar_path.is_empty() and FileAccess.file_exists(avatar_path):
		target_path = avatar_path
	elif not _player_character.get("name", "").is_empty():
		var name_path = ImageGenManager.get_avatar_path(_player_character.get("name"))
		if FileAccess.file_exists(name_path):
			target_path = name_path
		
	if pc_avatar_overlay:
		pc_avatar_overlay.setup(target_path)
		
	if not target_path.is_empty():
		var tex = await ImageGenManager.get_image_or_fallback(target_path, "avatar")
		pc_avatar_preview.texture = tex
	else:
		pc_avatar_preview.texture = null

		
	pc_wand_status_label.visible = false
	pc_bg_progress_label.visible = false
	image_dialog.theme = ThemeManager.active_theme
	
	_update_fields_state()

func _on_pc_name_changed(_txt: String) -> void:
	_name_touched = true
	_update_fields_state()

func _get_error_color() -> Color:
	if ThemeManager.color_bg.get_luminance() > 0.5:
		return Color(0.9, 0.2, 0.2)
	return Color(1.0, 0.4, 0.4)

func _validate_name() -> bool:
	var raw_name = pc_name_input.text
	var trimmed_name = raw_name.strip_edges()
	
	if trimmed_name.is_empty():
		if raw_name.length() > 0 or _name_touched:
			pc_name_error_label.text = "Character name is required."
			pc_name_error_label.add_theme_color_override("font_color", _get_error_color())
			pc_name_error_label.visible = true
		else:
			pc_name_error_label.visible = false
		return false
		
	if trimmed_name.length() > 50:
		pc_name_error_label.text = "Character name cannot exceed 50 characters."
		pc_name_error_label.add_theme_color_override("font_color", _get_error_color())
		pc_name_error_label.visible = true
		return false
		
	if not _name_regex:
		_name_regex = RegEx.new()
		_name_regex.compile("^[a-zA-Z0-9' -]+$")
		
	if not _name_regex.search(trimmed_name):
		pc_name_error_label.text = "Character name can only contain letters, numbers, spaces, hyphens, and apostrophes."
		pc_name_error_label.add_theme_color_override("font_color", _get_error_color())
		pc_name_error_label.visible = true
		return false
		
	pc_name_error_label.visible = false
	return true

func _update_counter(edit: TextEdit, label: Label) -> void:
	var length = edit.text.length()
	label.text = "%d/2000" % length
	if length >= 2000:
		label.add_theme_color_override("font_color", _get_error_color())
	else:
		label.remove_theme_color_override("font_color")

func _update_fields_state() -> void:
	_update_counter(pc_description_input, pc_description_counter)
	_update_counter(pc_personality_input, pc_personality_counter)
	_update_counter(pc_backstory_input, pc_backstory_counter)
	
	var name_is_valid = _validate_name()
	pc_next_btn.disabled = not name_is_valid
	
	_update_avatar_magic_wand_state()
	_update_magic_wand_button_state()

func _update_avatar_magic_wand_state() -> void:
	var has_desc = not pc_description_input.text.strip_edges().is_empty()
	pc_avatar_magic_wand_btn.disabled = not has_desc

func _update_magic_wand_button_state() -> void:
	var avatar_path = _player_character.get("avatar", "")
	var has_avatar = not avatar_path.is_empty() and FileAccess.file_exists(avatar_path)
	pc_magic_wand_btn.disabled = not has_avatar

func _on_pc_avatar_browse_pressed() -> void:
	image_dialog.popup()

func _on_image_selected(file_path: String) -> void:
	_player_character["avatar"] = file_path
	if pc_avatar_overlay:
		pc_avatar_overlay.setup(file_path)
	var tex = await ImageGenManager.get_image_or_fallback(file_path, "avatar")
	pc_avatar_preview.texture = tex
	_update_magic_wand_button_state()


func _on_pc_magic_wand_pressed() -> void:
	var avatar_path = _player_character.get("avatar", "")
	if avatar_path.is_empty() or not FileAccess.file_exists(avatar_path):
		return
		
	pc_magic_wand_btn.disabled = true
	pc_magic_wand_btn.text = " 🪄 ..."
	_set_wand_status("✨ Analysing portrait...", false)
	
	if _desc_history:
		_desc_history.reset("")
		_desc_history.is_updating = true
	else:
		pc_description_input.text = ""
	_update_avatar_magic_wand_state()
	
	var prompt = "Analyze this character portrait. Provide a very brief, one-paragraph summary of their key physical features (e.g. hair, eyes, facial features, clothing, and overall aesthetic). Be extremely concise, direct, and limit your description to 2-3 short sentences max."
	
	var on_chunk = func(chunk: String):
		pc_description_input.text += chunk
		_update_fields_state()
		
	var on_completed = func(full_text: String):
		_on_vision_description_completed(true, full_text, "")
		
	var on_failed = func(error_msg: String):
		_on_vision_description_completed(false, "", error_msg)
		
	LLMClient.send_custom_vision_stream_request(
		prompt,
		avatar_path,
		LLMClient.world_builder_model,
		on_chunk,
		on_completed,
		on_failed
	)

func _on_vision_description_completed(success: bool, response_text: String, error_msg: String) -> void:
	pc_magic_wand_btn.disabled = false
	pc_magic_wand_btn.text = " 🪄 "
	
	if _desc_history:
		_desc_history.is_updating = false
	
	if success:
		var final_desc = response_text.strip_edges()
		if _desc_history:
			_desc_history.reset(final_desc)
		else:
			pc_description_input.text = final_desc
		_set_wand_status("✅ Description generated from portrait.", false)
	else:
		var fallback_desc = _player_character.get("physical_description", "")
		if _desc_history:
			_desc_history.reset(fallback_desc)
		else:
			pc_description_input.text = fallback_desc
		print("[CharacterCreator] Vision model analysis failed: ", error_msg)
		var hint := "❌ Vision analysis failed."
		if "400" in error_msg or "Code: 400" in error_msg:
			hint += " The selected model may not support image input — try a vision-capable model (e.g. llava or minicpm-v)."
		else:
			hint += " " + error_msg
		_set_wand_status(hint, true)
		
	_update_fields_state()

func _on_pc_avatar_magic_wand_pressed() -> void:
	var physical_desc = pc_description_input.text.strip_edges()
	if physical_desc.is_empty():
		return
		
	var img_gen = get_node_or_null("/root/ImageGenManager")
	if not img_gen:
		printerr("[CharacterCreator] ImageGenManager autoload not found.")
		return
		
	pc_avatar_magic_wand_btn.disabled = true
	pc_avatar_magic_wand_btn.text = " 🪄 ..."
	
	var target_avatar = "user://temp_pc_avatar.png"
	if pc_avatar_overlay:
		pc_avatar_overlay.setup(target_avatar)
		
	pc_avatar_preview.texture = null
	
	# Disconnect any stale generator callback
	_disconnect_avatar_gen()
	
	_avatar_gen_callable = func(output_path: String, is_placeholder: bool):
		if output_path == target_avatar:
			var state = ImageGenManager.get_asset_state(output_path)
			if state.status == "success":
				ImageGenManager.invalidate_cache("temp_pc_avatar", "avatar")
				ImageGenManager.invalidate_cache(output_path, "avatar")
				var tex = await ImageGenManager.get_image_or_fallback(output_path, "avatar")
				pc_avatar_preview.texture = tex
				_player_character["avatar"] = output_path
				_update_magic_wand_button_state()
			else:
				pc_avatar_preview.texture = null
				
			pc_avatar_magic_wand_btn.disabled = false
			pc_avatar_magic_wand_btn.text = " 🪄 "
			_disconnect_avatar_gen()

				
	img_gen.asset_generated.connect(_avatar_gen_callable)
	
	img_gen.generate_asset(
		pc_name_input.text.strip_edges(),
		physical_desc,
		"avatar",
		target_avatar
	)

func _disconnect_avatar_gen() -> void:
	var img_gen = get_node_or_null("/root/ImageGenManager")
	if img_gen and _avatar_gen_callable.is_valid():
		if img_gen.asset_generated.is_connected(_avatar_gen_callable):
			img_gen.asset_generated.disconnect(_avatar_gen_callable)
		_avatar_gen_callable = Callable()

func _set_wand_status(message: String, is_error: bool) -> void:
	pc_wand_status_label.text = message
	pc_wand_status_label.visible = not message.is_empty()
	if is_error:
		pc_wand_status_label.add_theme_color_override("font_color", Color(1.0, 0.4, 0.4))
	else:
		var _muted_c = ThemeManager.color_text; _muted_c.a = 0.65
		pc_wand_status_label.add_theme_color_override("font_color", _muted_c)

func set_compilation_progress(text: String, is_completed: bool, warmup_success: bool) -> void:
	pc_bg_progress_label.visible = true
	pc_bg_progress_label.text = text
	
	var is_light = ThemeManager.color_bg.get_luminance() > 0.5
	if is_completed:
		if warmup_success:
			var green_color = Color.html("#52cc7a") if is_light else Color.html("#6be296")
			pc_bg_progress_label.add_theme_color_override("font_color", green_color)
		else:
			var orange_color = Color.html("#d97706") if is_light else Color.html("#f59e0b")
			pc_bg_progress_label.add_theme_color_override("font_color", orange_color)
	else:
		pc_bg_progress_label.remove_theme_color_override("font_color")

func _sanitize_text(text: String) -> String:
	var result := ""
	for i in range(text.length()):
		var code = text.unicode_at(i)
		# Strip control characters (ASCII < 32 except tab (9), newline (10), CR (13))
		if code < 32 and code != 9 and code != 10 and code != 13:
			continue
		result += text[i]
	
	result = result.replace("\"", "'")
	result = result.replace("\\", "")
	return result

func _on_next_pressed() -> void:
	_name_touched = true
	if not _validate_name():
		return
		
	var pc_name = pc_name_input.text.strip_edges()
	var pc_desc = _sanitize_text(pc_description_input.text.strip_edges())
	var pc_personality = _sanitize_text(pc_personality_input.text.strip_edges())
	var pc_backstory = _sanitize_text(pc_backstory_input.text.strip_edges())
	
	_player_character["name"] = pc_name
	_player_character["physical_description"] = pc_desc
	_player_character["personality"] = pc_personality
	_player_character["backstory"] = pc_backstory
	if campaign_art_style_dropdown:
		_player_character["art_style"] = campaign_art_style_dropdown.get_item_text(campaign_art_style_dropdown.selected)
	else:
		_player_character["art_style"] = "Digital Anime Art"
	
	character_created.emit(_player_character)

# ==============================================================================
# Inner Helper Classes
# ==============================================================================

class TextHistory:
	var history: Array[String] = []
	var index: int = -1
	var max_history: int = 50
	var text_edit: TextEdit
	var is_updating: bool = false
	var parent_creator: VBoxContainer
	
	func _init(edit: TextEdit, creator: VBoxContainer) -> void:
		text_edit = edit
		parent_creator = creator
		history.append(edit.text)
		index = 0
		text_edit.text_changed.connect(_on_text_changed)
		text_edit.gui_input.connect(_on_gui_input)
		
	func reset(new_text: String) -> void:
		is_updating = true
		text_edit.text = new_text
		history.clear()
		history.append(new_text)
		index = 0
		is_updating = false
		
	func _on_text_changed() -> void:
		if is_updating:
			return
			
		if text_edit.text.length() > 2000:
			is_updating = true
			var truncated_text = text_edit.text.left(2000)
			
			var column = text_edit.get_caret_column()
			var line = text_edit.get_caret_line()
			
			text_edit.text = truncated_text
			
			text_edit.set_caret_line(min(line, text_edit.get_line_count() - 1))
			text_edit.set_caret_column(min(column, text_edit.text.length()))
			
			if history.is_empty() or history[index] != truncated_text:
				if index < history.size() - 1:
					history = history.slice(0, index + 1)
				history.append(truncated_text)
				if history.size() > max_history:
					history.remove_at(0)
				else:
					index += 1
					
			is_updating = false
			parent_creator._update_fields_state()
			return
			
		if index < history.size() - 1:
			history = history.slice(0, index + 1)
			
		var current_text = text_edit.text
		if history.is_empty() or history[index] != current_text:
			history.append(current_text)
			if history.size() > max_history:
				history.remove_at(0)
			else:
				index += 1
				
		parent_creator._update_fields_state()

	func _on_gui_input(event: InputEvent) -> void:
		if event is InputEventKey and event.pressed:
			var is_cmd_or_ctrl = event.command_or_control_autoremap
			if is_cmd_or_ctrl:
				if event.keycode == KEY_Z:
					if event.shift_pressed:
						redo()
					else:
						undo()
					text_edit.accept_event()
				elif event.keycode == KEY_Y:
					redo()
					text_edit.accept_event()
					
	func undo() -> void:
		if index > 0:
			index -= 1
			is_updating = true
			var column = text_edit.get_caret_column()
			var line = text_edit.get_caret_line()
			
			text_edit.text = history[index]
			
			text_edit.set_caret_line(min(line, text_edit.get_line_count() - 1))
			text_edit.set_caret_column(min(column, text_edit.text.length()))
			is_updating = false
			parent_creator._update_fields_state()
			
	func redo() -> void:
		if index < history.size() - 1:
			index += 1
			is_updating = true
			var column = text_edit.get_caret_column()
			var line = text_edit.get_caret_line()
			
			text_edit.text = history[index]
			
			text_edit.set_caret_line(min(line, text_edit.get_line_count() - 1))
			text_edit.set_caret_column(min(column, text_edit.text.length()))
			is_updating = false
			parent_creator._update_fields_state()
