# res://src/ui/ToastMessage.gd
extends PanelContainer

@onready var message_label: Label = %MessageLabel

func setup(message: String, is_error: bool = false) -> void:
	message_label.text = message
	modulate.a = 0.0
	
	var sb = StyleBoxFlat.new()
	sb.bg_color = Color(0.12, 0.08, 0.18, 0.85) if is_error else Color(0.08, 0.08, 0.12, 0.85)
	sb.border_width_left = ThemeManager.spacing_xs
	sb.border_color = Color(0.9, 0.2, 0.2, 1.0) if is_error else Color(0.2, 0.6, 0.9, 1.0)
	sb.corner_radius_top_right = ThemeManager.radius_sm
	sb.corner_radius_bottom_right = ThemeManager.radius_sm
	sb.corner_radius_top_left = ThemeManager.radius_sm / 2.0
	sb.corner_radius_bottom_left = ThemeManager.radius_sm / 2.0
	sb.content_margin_left = ThemeManager.spacing_sm
	sb.content_margin_right = ThemeManager.spacing_sm
	sb.content_margin_top = ThemeManager.spacing_sm
	sb.content_margin_bottom = ThemeManager.spacing_sm
	add_theme_stylebox_override("panel", sb)
	
	message_label.add_theme_color_override("font_color", Color(1.0, 0.95, 1.0) if is_error else Color(0.9, 0.9, 0.95))
