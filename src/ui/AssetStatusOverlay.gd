# res://src/ui/AssetStatusOverlay.gd
extends Control
class_name AssetStatusOverlay

@onready var spinner_container: Control = %SpinnerContainer
@onready var loading_spinner: LoadingSpinner = %LoadingSpinner
@onready var error_container: Control = %ErrorContainer
@onready var error_label: Label = %ErrorLabel
@onready var error_panel: PanelContainer = %ErrorPanel

var target_path: String = ""

func _ready() -> void:
	# Hide overlay elements initially
	spinner_container.visible = false
	error_container.visible = false
	
	_apply_theme_colors()
	if ThemeManager:
		ThemeManager.theme_changed.connect(_apply_theme_colors)
		
	if ImageGenManager:
		ImageGenManager.asset_generation_started.connect(_on_generation_started)
		ImageGenManager.asset_generation_completed.connect(_on_generation_completed)
		ImageGenManager.asset_generation_failed.connect(_on_generation_failed)
		
	# Trigger initial check on size change
	resized.connect(_update_ui)

func setup(path: String) -> void:
	target_path = path
	_update_ui()

func _apply_theme_colors() -> void:
	if not is_inside_tree():
		return
	if loading_spinner and ThemeManager:
		loading_spinner.color = ThemeManager.color_accent
	if error_label and ThemeManager:
		error_label.add_theme_color_override("font_color", ThemeManager.color_danger)

func _update_ui() -> void:
	if not is_inside_tree() or target_path.is_empty():
		spinner_container.visible = false
		error_container.visible = false
		return
		
	var state = ImageGenManager.get_asset_state(target_path)
	var is_small = size.x > 0 and size.y > 0 and (size.x < 120 or size.y < 120)
	
	match state.status:
		"generating":
			spinner_container.visible = true
			if is_small:
				loading_spinner.radius = 8.0
				loading_spinner.line_width = 2.0
				loading_spinner.custom_minimum_size = Vector2(20, 20)
			else:
				loading_spinner.radius = 14.0
				loading_spinner.line_width = 3.0
				loading_spinner.custom_minimum_size = Vector2(40, 40)
			loading_spinner.visible = true
			error_container.visible = false
		"error":
			spinner_container.visible = false
			loading_spinner.visible = false
			error_container.visible = true
			if is_small:
				# Show a tiny error exclamation mark/warning directly
				error_panel.visible = false
				if not error_container.has_node("TinyErrorLabel"):
					var tiny_lbl = Label.new()
					tiny_lbl.name = "TinyErrorLabel"
					tiny_lbl.text = "⚠"
					if ThemeManager:
						tiny_lbl.add_theme_color_override("font_color", ThemeManager.color_danger)
					tiny_lbl.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
					tiny_lbl.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
					tiny_lbl.theme_type_variation = &"LabelAccent"
					error_container.add_child(tiny_lbl)
				else:
					var tiny_lbl = error_container.get_node("TinyErrorLabel")
					tiny_lbl.visible = true
					if ThemeManager:
						tiny_lbl.add_theme_color_override("font_color", ThemeManager.color_danger)
			else:
				error_panel.visible = true
				if error_container.has_node("TinyErrorLabel"):
					error_container.get_node("TinyErrorLabel").visible = false
				error_label.text = state.error
		_:
			spinner_container.visible = false
			loading_spinner.visible = false
			error_container.visible = false
			if error_container.has_node("TinyErrorLabel"):
				error_container.get_node("TinyErrorLabel").visible = false

func _on_generation_started(path: String) -> void:
	if path == target_path or path.get_file() == target_path.get_file():
		_update_ui()

func _on_generation_completed(path: String) -> void:
	if path == target_path or path.get_file() == target_path.get_file():
		_update_ui()

func _on_generation_failed(path: String, _error_msg: String) -> void:
	if path == target_path or path.get_file() == target_path.get_file():
		_update_ui()
