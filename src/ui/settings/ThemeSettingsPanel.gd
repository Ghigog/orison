# res://src/ui/settings/ThemeSettingsPanel.gd
extends VBoxContainer
class_name ThemeSettingsPanel

@onready var theme_dropdown: OptionButton = %ThemeDropdown
@onready var delete_theme_btn: Button = %DeleteThemeButton
@onready var save_name_input: LineEdit = %SaveNameInput
@onready var save_theme_btn: Button = %SaveThemeButton
@onready var font_size_dropdown: OptionButton = %FontSizeDropdown

# Color Picker Buttons
@onready var bg_picker: ColorPickerButton = %BgPicker
@onready var surface_picker: ColorPickerButton = %SurfacePicker
@onready var border_picker: ColorPickerButton = %BorderPicker
@onready var text_picker: ColorPickerButton = %TextPicker
@onready var accent_picker: ColorPickerButton = %AccentPicker

# Delete Confirmation Dialog
@onready var delete_confirm: ConfirmationDialog = %DeleteConfirmDialog

func _ready() -> void:
	# Connect buttons
	save_theme_btn.pressed.connect(_on_save_theme_pressed)
	delete_theme_btn.pressed.connect(_on_delete_theme_pressed)
	theme_dropdown.item_selected.connect(_on_theme_selected)
	
	# Connect color picker buttons
	bg_picker.color_changed.connect(_on_color_changed)
	surface_picker.color_changed.connect(_on_color_changed)
	border_picker.color_changed.connect(_on_color_changed)
	text_picker.color_changed.connect(_on_color_changed)
	accent_picker.color_changed.connect(_on_color_changed)
	
	# Connect confirmation dialog
	delete_confirm.confirmed.connect(_on_delete_confirmed)
	delete_confirm.theme = ThemeManager.active_theme
	var confirm_label = delete_confirm.get_label()
	if confirm_label:
		confirm_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		confirm_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	
	# Populate dropdown and pickers
	_refresh_themes_ui()
	_update_pickers_from_manager()
	
	# Apply active theme overrides when changed globally
	ThemeManager.theme_changed.connect(_on_global_theme_changed)
	
	# Populate font size options
	font_size_dropdown.clear()
	font_size_dropdown.add_item("Small (-2px)")
	font_size_dropdown.add_item("Normal")
	font_size_dropdown.add_item("Large (+2px)")
	font_size_dropdown.add_item("Extra Large (+4px)")
	font_size_dropdown.add_item("Huge (+8px)")
	font_size_dropdown.add_item("Gigantic (+12px)")
	_select_font_size_dropdown_from_modifier()
	font_size_dropdown.item_selected.connect(_on_font_size_selected)

func _on_color_changed(_color: Color) -> void:
	ThemeManager.color_bg = bg_picker.color
	ThemeManager.color_surface = surface_picker.color
	ThemeManager.color_border = border_picker.color
	ThemeManager.color_text = text_picker.color
	ThemeManager.color_accent = accent_picker.color
	ThemeManager.apply_active_theme()

func _on_theme_selected(index: int) -> void:
	var selected_name = theme_dropdown.get_item_text(index)
	ThemeManager.select_theme(selected_name)
	_update_pickers_from_manager()

func _on_save_theme_pressed() -> void:
	var new_name = save_name_input.text.strip_edges()
	if new_name.is_empty():
		return
		
	# Block saving over presets
	if ThemeManager.PRESETS.has(new_name):
		save_name_input.text = ""
		return
		
	ThemeManager.save_custom_theme(new_name)
	save_name_input.text = ""
	_refresh_themes_ui()
	
	# Select the new theme in dropdown
	for i in range(theme_dropdown.item_count):
		if theme_dropdown.get_item_text(i) == new_name:
			theme_dropdown.select(i)
			break

func _on_delete_theme_pressed() -> void:
	var current_name = ThemeManager.active_theme_name
	if ThemeManager.PRESETS.has(current_name):
		return
		
	delete_confirm.dialog_text = "Are you sure you want to delete the custom theme '%s'?" % current_name
	delete_confirm.popup_centered()

func _on_delete_confirmed() -> void:
	var theme_to_delete = ThemeManager.active_theme_name
	ThemeManager.delete_theme(theme_to_delete)
	_refresh_themes_ui()
	_update_pickers_from_manager()

func _update_pickers_from_manager() -> void:
	bg_picker.color = ThemeManager.color_bg
	surface_picker.color = ThemeManager.color_surface
	border_picker.color = ThemeManager.color_border
	text_picker.color = ThemeManager.color_text
	accent_picker.color = ThemeManager.color_accent

func _refresh_themes_ui() -> void:
	theme_dropdown.clear()
	var list = ThemeManager.get_theme_list()
	for t in list:
		theme_dropdown.add_item(t)
		
	var active_name = ThemeManager.active_theme_name
	for i in range(list.size()):
		if list[i] == active_name:
			theme_dropdown.select(i)
			break
			
	delete_theme_btn.disabled = ThemeManager.PRESETS.has(active_name)

func _on_global_theme_changed() -> void:
	_update_pickers_from_manager()
	_refresh_themes_ui()
	_select_font_size_dropdown_from_modifier()

func _select_font_size_dropdown_from_modifier() -> void:
	var mod = ThemeManager.font_size_modifier
	if mod == -2:
		font_size_dropdown.select(0)
	elif mod == 0:
		font_size_dropdown.select(1)
	elif mod == 2:
		font_size_dropdown.select(2)
	elif mod == 4:
		font_size_dropdown.select(3)
	elif mod == 8:
		font_size_dropdown.select(4)
	elif mod == 12:
		font_size_dropdown.select(5)
	else:
		font_size_dropdown.select(1)

func _on_font_size_selected(index: int) -> void:
	var mod = 0
	match index:
		0: mod = -2
		1: mod = 0
		2: mod = 2
		3: mod = 4
		4: mod = 8
		5: mod = 12
	ThemeManager.font_size_modifier = mod
	ThemeManager.apply_active_theme()
	ThemeManager.save_themes()
