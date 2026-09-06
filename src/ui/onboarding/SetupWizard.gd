# res://src/ui/onboarding/SetupWizard.gd
extends VBoxContainer

signal back_pressed
signal campaign_crafted(title: String, campaign_id: String, vault_path: String, use_sample: bool)

@onready var title_input: LineEdit = %TitleInput
@onready var campaign_id_input: LineEdit = %CampaignIdInput
@onready var vault_path_input: LineEdit = %VaultPathInput
@onready var browse_btn: Button = %BrowseButton
@onready var use_sample_check: CheckBox = %UseSampleCheck
@onready var craft_btn: Button = %CraftButton
@onready var setup_back_btn: Button = %SetupBackButton

@onready var file_dialog: FileDialog = %SetupFileDialog

func _ready() -> void:
	setup_back_btn.pressed.connect(func(): back_pressed.emit())
	craft_btn.pressed.connect(_on_craft_pressed)
	title_input.text_changed.connect(_on_title_changed)
	use_sample_check.toggled.connect(_on_use_sample_toggled)
	browse_btn.pressed.connect(_on_browse_pressed)
	vault_path_input.text_changed.connect(func(_txt): _update_craft_button_state())
	
	# Setup FileDialog
	if "use_native_dialog" in file_dialog:
		file_dialog.set("use_native_dialog", true)
	file_dialog.theme = ThemeManager.active_theme
	file_dialog.dir_selected.connect(_on_dir_selected)
	
	_on_use_sample_toggled(use_sample_check.button_pressed)
	_update_craft_button_state()

func on_screen_shown() -> void:
	# Refresh theme if theme has updated
	file_dialog.theme = ThemeManager.active_theme
	_update_craft_button_state()

func _on_title_changed(new_text: String) -> void:
	var slug = _slugify(new_text)
	campaign_id_input.text = slug
	_update_craft_button_state()

func _on_use_sample_toggled(is_checked: bool) -> void:
	var default_path = OS.get_user_data_dir().path_join("adventures/sample_vault")
	if is_checked:
		vault_path_input.text = default_path
		vault_path_input.editable = false
		browse_btn.disabled = true
	else:
		if vault_path_input.text == default_path:
			vault_path_input.text = ""
		vault_path_input.editable = true
		browse_btn.disabled = false
	_update_craft_button_state()

func _on_browse_pressed() -> void:
	var current_dir = vault_path_input.text.strip_edges()
	if not current_dir.is_empty() and DirAccess.dir_exists_absolute(current_dir):
		file_dialog.current_dir = current_dir
	file_dialog.popup()

func _on_dir_selected(dir_path: String) -> void:
	vault_path_input.text = dir_path
	_update_craft_button_state()

func _on_craft_pressed() -> void:
	var title = title_input.text.strip_edges()
	var campaign_id = campaign_id_input.text.strip_edges()
	var vault_path = vault_path_input.text.strip_edges()
	
	if title.is_empty() or campaign_id.is_empty() or vault_path.is_empty():
		return
		
	campaign_crafted.emit(title, campaign_id, vault_path, use_sample_check.button_pressed)

func _update_craft_button_state() -> void:
	var title = title_input.text.strip_edges()
	var campaign_id = campaign_id_input.text.strip_edges()
	var vault_path = vault_path_input.text.strip_edges()
	
	craft_btn.disabled = title.is_empty() or campaign_id.is_empty() or vault_path.is_empty()

func _slugify(text: String) -> String:
	var result = text.to_lower().strip_edges()
	var regex = RegEx.new()
	regex.compile("[^a-z0-9\\s_-]")
	result = regex.sub(result, "", true)
	regex.compile("[\\s_-]+")
	result = regex.sub(result, "_", true)
	return result
