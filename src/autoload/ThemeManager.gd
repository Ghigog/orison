# res://src/autoload/ThemeManager.gd
extends Node

signal theme_changed

const CONFIG_PATH = "user://theme_config.json"
const BASE_THEME_PATH = "res://resources/themes/orison_ui.tres"

const PRESETS = {
	"Dawn": {
		"color_bg": "#FAF6F2",
		"color_surface": "#FCFAF7",
		"color_border": "#FCDAC7",
		"color_text": "#2C2521",
		"color_accent": "#FF5E3A"
	},
	"Ethereal Codex": {
		"color_bg": "#0B0D11",
		"color_surface": "#161A22",
		"color_border": "#FFFFFF",
		"color_text": "#F3F4F6",
		"color_accent": "#8B5CF6"
	},
	"Daybreak Meadow": {
		"color_bg": "#F4FBF7",
		"color_surface": "#F9FDFB",
		"color_border": "#A7F3D0",
		"color_text": "#14291E",
		"color_accent": "#10B981"
	},
	"Solstice Obsidian": {
		"color_bg": "#08080C",
		"color_surface": "#14141A",
		"color_border": "#808090",
		"color_text": "#E4E4E7",
		"color_accent": "#E11D48"
	}
}

# Theme Colors
var color_bg: Color
var color_surface: Color
var color_border: Color
var color_text: Color
var color_accent: Color

# Design System Tokens (design_philosophy.md)
# Spacing scale
var spacing_xs: float = 4.0
var spacing_sm: float = 8.0
var spacing_md: float = 16.0
var spacing_lg: float = 24.0
var spacing_xl: float = 40.0

# Border radii
var radius_sm: float = 0.0
var radius_md: float = 0.0
var radius_lg: float = 0.0
var radius_pill: float = 0.0

# Success and Danger colors
var color_success: Color = Color("#10B981")
var color_danger: Color = Color("#E11D48")

# Animation durations and easing
var duration_fast: float = 0.12
var duration_normal: float = 0.25
var duration_slow: float = 0.4
var ease_default: int = Tween.EASE_OUT
var trans_default: int = Tween.TRANS_CUBIC

# Shadow StyleBoxes (presets)
var shadow_subtle: StyleBoxFlat
var shadow_medium: StyleBoxFlat
var shadow_elevated: StyleBoxFlat

var active_theme_name: String = "Dawn"
var custom_themes: Dictionary = {}
var active_theme: Theme
var font_size_modifier: int = 0

## Computes relative luminance of a color according to WCAG 2.x formula
static func get_relative_luminance(color: Color) -> float:
	var r = color.r
	var g = color.g
	var b = color.b
	
	var r_c = r / 12.92 if r <= 0.04045 else pow((r + 0.055) / 1.055, 2.4)
	var g_c = g / 12.92 if g <= 0.04045 else pow((g + 0.055) / 1.055, 2.4)
	var b_c = b / 12.92 if b <= 0.04045 else pow((b + 0.055) / 1.055, 2.4)
	
	return 0.2126 * r_c + 0.7152 * g_c + 0.0722 * b_c

## Computes the contrast ratio between two colors
static func get_contrast_ratio(c1: Color, c2: Color) -> float:
	var l1 = get_relative_luminance(c1)
	var l2 = get_relative_luminance(c2)
	
	var lighter = maxf(l1, l2)
	var darker = minf(l1, l2)
	
	return (lighter + 0.05) / (darker + 0.05)

## Validates that a theme dictionary has proper contrast ratio (> 4.5:1)
static func validate_theme_contrast(theme_data: Dictionary) -> bool:
	if not theme_data.has("color_bg") or not theme_data.has("color_text"):
		return false
	var bg = Color(theme_data["color_bg"])
	var text = Color(theme_data["color_text"])
	var ratio = get_contrast_ratio(bg, text)
	return ratio >= 4.5

func validate_presets_contrast() -> void:
	for name in PRESETS.keys():
		var p = PRESETS[name]
		var ratio = get_contrast_ratio(Color(p["color_bg"]), Color(p["color_text"]))
		if ratio < 4.5:
			printerr("[ThemeManager] Preset theme '", name, "' fails WCAG AA contrast ratio check: ", ratio)

func _ready() -> void:
	_init_shadows()
	
	# Load base theme
	var base_res = load(BASE_THEME_PATH)
	if base_res is Theme:
		active_theme = base_res.duplicate(true)
	else:
		active_theme = Theme.new()
		
	load_themes()
	apply_active_theme()
	validate_presets_contrast()


func _init_shadows() -> void:
	shadow_subtle = StyleBoxFlat.new()
	shadow_subtle.draw_center = false
	shadow_subtle.shadow_offset = Vector2(0, 1)
	
	shadow_medium = StyleBoxFlat.new()
	shadow_medium.draw_center = false
	shadow_medium.shadow_offset = Vector2(0, 2)
	
	shadow_elevated = StyleBoxFlat.new()
	shadow_elevated.draw_center = false
	shadow_elevated.shadow_offset = Vector2(0, 4)

func get_emotion_color(emotion: String) -> Color:
	var is_light = color_bg.get_luminance() > 0.5
	match emotion.to_lower():
		"joy": return Color("#D97706") if is_light else Color("#F59E0B")
		"anger": return Color("#B91C1C") if is_light else Color("#DC2626")
		"sadness": return Color("#1D4ED8") if is_light else Color("#3B82F6")
		"fear": return Color("#6D28D9") if is_light else Color("#7C3AED")
		"trust": return Color("#047857") if is_light else Color("#059669")
		"disgust": return Color("#4D7C0F") if is_light else Color("#65A30D")
		"surprise": return Color("#0891B2") if is_light else Color("#06B6D4")
		"serenity": return Color("#4B5563") if is_light else Color("#D1D5DB")
		_: return Color("#4B5563") if is_light else Color("#D1D5DB")

func load_themes() -> void:
	var path_to_load = CONFIG_PATH
	var migrated = false
	if not FileAccess.file_exists(path_to_load):
		if FileAccess.file_exists("user://config.json"):
			path_to_load = "user://config.json"
			migrated = true
		else:
			_load_preset("Dawn")
			return
			
	var file = FileAccess.open(path_to_load, FileAccess.READ)
	if not file:
		_load_preset("Dawn")
		return
		
	var content = file.get_as_text()
	file.close()
	
	var json = JSON.new()
	if json.parse(content) == OK and json.data is Dictionary:
		var data = json.data
		custom_themes = data.get("custom_themes", {})
		active_theme_name = data.get("active_theme", "Dawn")
		font_size_modifier = int(data.get("font_size_modifier", 0))
		
		if PRESETS.has(active_theme_name):
			_load_preset(active_theme_name)
		elif custom_themes.has(active_theme_name):
			_load_custom(active_theme_name)
		else:
			_load_preset("Dawn")
			
		if migrated:
			print("[ThemeManager] Migrated config from legacy config.json")
			save_themes()
	else:
		_load_preset("Dawn")

func save_themes() -> void:
	var current_data = {}
	current_data["active_theme"] = active_theme_name
	current_data["custom_themes"] = custom_themes
	current_data["font_size_modifier"] = font_size_modifier
	
	var file_write = FileAccess.open(CONFIG_PATH, FileAccess.WRITE)
	if file_write:
		file_write.store_string(JSON.stringify(current_data, "\t"))
		file_write.close()

func select_theme(theme_name: String) -> void:
	if PRESETS.has(theme_name):
		active_theme_name = theme_name
		_load_preset(theme_name)
		apply_active_theme()
		save_themes()
	elif custom_themes.has(theme_name):
		active_theme_name = theme_name
		_load_custom(theme_name)
		apply_active_theme()
		save_themes()

func save_custom_theme(theme_name: String) -> void:
	var theme_data = {
		"color_bg": color_bg.to_html(false),
		"color_surface": color_surface.to_html(false),
		"color_border": color_border.to_html(false),
		"color_text": color_text.to_html(false),
		"color_accent": color_accent.to_html(false)
	}
	var ratio = get_contrast_ratio(color_bg, color_text)
	if ratio < 4.5:
		print("[ThemeManager] WARNING: Custom theme '", theme_name, "' fails WCAG AA contrast ratio check: ", ratio)
		
	custom_themes[theme_name] = theme_data
	active_theme_name = theme_name
	save_themes()

func delete_theme(theme_name: String) -> void:
	if custom_themes.has(theme_name):
		custom_themes.erase(theme_name)
		if active_theme_name == theme_name:
			active_theme_name = "Dawn"
			_load_preset("Dawn")
			apply_active_theme()
		save_themes()

func apply_active_theme() -> void:
	if not active_theme:
		return
		
	# Determine if it's a light theme to apply solid/high-contrast styling
	var is_light = color_bg.get_luminance() > 0.5
	
	# Update shadow presets dynamically depending on theme brightness
	if is_light:
		shadow_subtle.shadow_size = 4
		shadow_subtle.shadow_color = Color(0, 0, 0, 0.05)
		
		shadow_medium.shadow_size = 12
		shadow_medium.shadow_color = Color(0, 0, 0, 0.05)
		
		shadow_elevated.shadow_size = 24
		shadow_elevated.shadow_color = Color(0, 0, 0, 0.05)
	else:
		shadow_subtle.shadow_size = 6
		shadow_subtle.shadow_color = Color(0, 0, 0, 0.2)
		
		shadow_medium.shadow_size = 16
		shadow_medium.shadow_color = Color(0, 0, 0, 0.3)
		
		shadow_elevated.shadow_size = 24
		shadow_elevated.shadow_color = Color(0, 0, 0, 0.4)
		
	# Set default font sizes in active_theme
	var def_size = 14 + font_size_modifier
	active_theme.default_font_size = def_size
	active_theme.set_font_size("normal_font_size", "RichTextLabel", def_size)
	active_theme.set_font_size("bold_font_size", "RichTextLabel", def_size)
	active_theme.set_font_size("italics_font_size", "RichTextLabel", def_size)
	active_theme.set_font_size("bold_italics_font_size", "RichTextLabel", def_size)
	active_theme.set_font_size("mono_font_size", "RichTextLabel", def_size)
		
	# 1. Update theme colors
	active_theme.set_color("font_color", "Label", color_text)
	active_theme.set_color("default_color", "RichTextLabel", color_text)
	
	for btn_type in ["Button", "CheckBox", "CheckButton", "OptionButton"]:
		active_theme.set_color("font_color", btn_type, color_text)
		active_theme.set_color("font_hover_color", btn_type, color_text)
		active_theme.set_color("font_focus_color", btn_type, color_text)
		active_theme.set_color("font_pressed_color", btn_type, color_text)
		active_theme.set_color("font_hover_pressed_color", btn_type, color_text)
		
		var disabled_txt = color_text
		disabled_txt.a = 0.35
		active_theme.set_color("font_disabled_color", btn_type, disabled_txt)
		
	active_theme.set_color("font_color", "LineEdit", color_text)
	var placeholder_c = color_text
	placeholder_c.a = 0.45
	active_theme.set_color("font_placeholder_color", "LineEdit", placeholder_c)
	
	var uneditable_c = color_text
	uneditable_c.a = 0.5
	active_theme.set_color("font_uneditable_color", "LineEdit", uneditable_c)
	
	# Constants for clean spacing
	active_theme.set_constant("h_separation", "CheckBox", 12)
	active_theme.set_constant("h_separation", "CheckButton", 12)
	active_theme.set_constant("h_separation", "Button", 8)
	
	var btn_normal_opacity = 0.95 if is_light else 0.4
	var btn_border_opacity = 0.8 if is_light else 0.4
	var lineedit_opacity = 0.95 if is_light else 0.5
	var lineedit_border = 0.8 if is_light else 0.4
	var panel_opacity = 0.98 if is_light else 0.75
	var panel_border = 0.8 if is_light else 0.4
	
	# 2. Update StyleBox colors, spacing, and border radii
	_update_sb(active_theme, "disabled", "Button", color_surface, color_border, 0.2, 0.1, radius_md, spacing_md, spacing_sm)
	_update_sb(active_theme, "hover", "Button", color_accent, color_accent, 0.15, 1.0, radius_md, spacing_md, spacing_sm)
	_update_sb(active_theme, "focus", "Button", color_accent, color_accent, 0.15, 1.0, radius_md, spacing_md, spacing_sm)
	_update_sb(active_theme, "normal", "Button", color_surface, color_border, btn_normal_opacity, btn_border_opacity, radius_md, spacing_md, spacing_sm)
	_update_sb(active_theme, "pressed", "Button", color_accent, color_accent, 0.4, 1.0, radius_md, spacing_md, spacing_sm)
	
	_update_sb(active_theme, "focus", "LineEdit", color_surface, color_accent, 0.6, 0.8, radius_md, spacing_md, spacing_sm)
	_update_sb(active_theme, "normal", "LineEdit", color_surface, color_border, lineedit_opacity, lineedit_border, radius_md, spacing_md, spacing_sm)
	_update_sb(active_theme, "read_only", "LineEdit", color_surface, color_border, lineedit_opacity * 0.8, lineedit_border * 0.5, radius_md, spacing_md, spacing_sm)
	
	_update_sb(active_theme, "panel", "PanelContainer", color_surface, color_border, panel_opacity, panel_border, radius_lg, spacing_md, spacing_md, shadow_medium.shadow_size, shadow_medium.shadow_color)
	
	# 3. Style TabContainer dynamically for a minimalist look
	var tab_panel = StyleBoxEmpty.new()
	active_theme.set_stylebox("panel", "TabContainer", tab_panel)
	
	var tab_selected = StyleBoxFlat.new()
	tab_selected.draw_center = false
	tab_selected.border_width_bottom = 2
	tab_selected.border_color = color_accent
	tab_selected.content_margin_left = spacing_md
	tab_selected.content_margin_right = spacing_md
	tab_selected.content_margin_top = spacing_sm
	tab_selected.content_margin_bottom = spacing_sm
	active_theme.set_stylebox("tab_selected", "TabContainer", tab_selected)
	
	var tab_unselected = StyleBoxFlat.new()
	tab_unselected.draw_center = false
	tab_unselected.content_margin_left = spacing_md
	tab_unselected.content_margin_right = spacing_md
	tab_unselected.content_margin_top = spacing_sm
	tab_unselected.content_margin_bottom = spacing_sm
	active_theme.set_stylebox("tab_unselected", "TabContainer", tab_unselected)
	
	var tab_hovered = StyleBoxFlat.new()
	tab_hovered.draw_center = true
	var hover_bg = color_accent
	hover_bg.a = 0.05
	tab_hovered.bg_color = hover_bg
	tab_hovered.border_width_bottom = 2
	var hover_border = color_accent
	hover_border.a = 0.5
	tab_hovered.border_color = hover_border
	tab_hovered.content_margin_left = spacing_md
	tab_hovered.content_margin_right = spacing_md
	tab_hovered.content_margin_top = spacing_sm
	tab_hovered.content_margin_bottom = spacing_sm
	active_theme.set_stylebox("tab_hover", "TabContainer", tab_hovered)
	active_theme.set_stylebox("tab_hovered", "TabContainer", tab_hovered)
	
	active_theme.set_color("font_selected_color", "TabContainer", color_text)
	var unselected_text = color_text
	unselected_text.a = 0.5
	active_theme.set_color("font_unselected_color", "TabContainer", unselected_text)
	var hovered_text = color_text
	hovered_text.a = 0.8
	active_theme.set_color("font_hovered_color", "TabContainer", hovered_text)
	
	# 4. Style PopupMenu dynamically (OptionButton items)
	var popup_panel = StyleBoxFlat.new()
	popup_panel.bg_color = color_surface
	popup_panel.border_width_left = 1
	popup_panel.border_width_top = 1
	popup_panel.border_width_right = 1
	popup_panel.border_width_bottom = 1
	popup_panel.border_color = color_border
	popup_panel.set_corner_radius_all(radius_md)
	popup_panel.content_margin_left = spacing_sm
	popup_panel.content_margin_right = spacing_sm
	popup_panel.content_margin_top = spacing_xs
	popup_panel.content_margin_bottom = spacing_xs
	popup_panel.shadow_color = shadow_subtle.shadow_color
	popup_panel.shadow_size = shadow_subtle.shadow_size
	active_theme.set_stylebox("panel", "PopupMenu", popup_panel)
	
	var popup_hover = StyleBoxFlat.new()
	var pop_h_c = color_accent
	pop_h_c.a = 0.15
	popup_hover.bg_color = pop_h_c
	popup_hover.set_corner_radius_all(radius_sm)
	active_theme.set_stylebox("hover", "PopupMenu", popup_hover)
	
	active_theme.set_color("font_color", "PopupMenu", color_text)
	active_theme.set_color("font_hover_color", "PopupMenu", color_text)
	
	# 5. ProgressBar updates
	var prog_bg = active_theme.get_stylebox("background", "ProgressBar") as StyleBoxFlat
	if prog_bg:
		var bg_c = color_border
		bg_c.a = 0.25
		prog_bg.bg_color = bg_c
		prog_bg.set_corner_radius_all(radius_sm)
		active_theme.set_stylebox("background", "ProgressBar", prog_bg)
		
	var prog_fill = active_theme.get_stylebox("fill", "ProgressBar") as StyleBoxFlat
	if prog_fill:
		prog_fill.bg_color = color_accent
		prog_fill.set_corner_radius_all(radius_sm)
		active_theme.set_stylebox("fill", "ProgressBar", prog_fill)
		
	# 6. Style Window and Dialogs dynamically (AcceptDialog, ConfirmationDialog, FileDialog)
	var dialog_panel = StyleBoxFlat.new()
	dialog_panel.bg_color = color_bg
	dialog_panel.border_width_left = 1
	dialog_panel.border_width_top = 1
	dialog_panel.border_width_right = 1
	dialog_panel.border_width_bottom = 1
	dialog_panel.border_color = color_border
	dialog_panel.set_corner_radius_all(radius_lg)
	dialog_panel.content_margin_left = spacing_md
	dialog_panel.content_margin_right = spacing_md
	dialog_panel.content_margin_top = spacing_md
	dialog_panel.content_margin_bottom = spacing_md
	dialog_panel.shadow_color = shadow_elevated.shadow_color
	dialog_panel.shadow_size = shadow_elevated.shadow_size
		
	active_theme.set_stylebox("panel", "AcceptDialog", dialog_panel)
	active_theme.set_stylebox("panel", "ConfirmationDialog", dialog_panel)
	active_theme.set_stylebox("panel", "FileDialog", dialog_panel)
	active_theme.set_stylebox("panel", "PopupPanel", dialog_panel)
	
	# Clean up underlying Window and Popup backgrounds
	var win_panel = StyleBoxFlat.new()
	win_panel.bg_color = color_bg
	win_panel.set_corner_radius_all(radius_lg)
	active_theme.set_stylebox("panel", "Window", win_panel)
	active_theme.set_stylebox("panel", "Popup", win_panel)
	
	# 7. Style Tree and ItemList dynamically for the file selector lists
	var tree_panel = StyleBoxFlat.new()
	tree_panel.bg_color = color_surface
	tree_panel.border_width_left = 1
	tree_panel.border_width_top = 1
	tree_panel.border_width_right = 1
	tree_panel.border_width_bottom = 1
	tree_panel.border_color = color_border
	tree_panel.set_corner_radius_all(radius_md)
	active_theme.set_stylebox("panel", "Tree", tree_panel)
	active_theme.set_stylebox("panel", "ItemList", tree_panel)
	
	var tree_selected = StyleBoxFlat.new()
	var sel_c = color_accent
	sel_c.a = 0.2
	tree_selected.bg_color = sel_c
	tree_selected.border_width_left = 2
	tree_selected.border_color = color_accent
	tree_selected.set_corner_radius_all(radius_sm)
	active_theme.set_stylebox("selected", "Tree", tree_selected)
	active_theme.set_stylebox("selected_focus", "Tree", tree_selected)
	
	var item_hovered = StyleBoxFlat.new()
	var h_c = color_accent
	h_c.a = 0.1
	item_hovered.bg_color = h_c
	item_hovered.set_corner_radius_all(radius_sm)
	active_theme.set_stylebox("hovered", "ItemList", item_hovered)
	
	active_theme.set_color("font_color", "Tree", color_text)
	active_theme.set_color("font_selected_color", "Tree", color_text)
	active_theme.set_color("font_hover_color", "Tree", color_text)
	active_theme.set_color("font_hovered_color", "Tree", color_text)
	active_theme.set_color("title_button_color", "Tree", color_text)
	
	active_theme.set_color("font_color", "ItemList", color_text)
	active_theme.set_color("font_selected_color", "ItemList", color_text)
	active_theme.set_color("font_hover_color", "ItemList", color_text)
	active_theme.set_color("font_hovered_color", "ItemList", color_text)
	
	# 8. Style Window border & title bar dynamically (Embedded file dialog frame)
	var win_border = StyleBoxFlat.new()
	win_border.bg_color = color_surface
	win_border.border_width_left = 1
	win_border.border_width_top = 28 # Space for title bar
	win_border.border_width_right = 1
	win_border.border_width_bottom = 1
	win_border.border_color = color_border
	win_border.expand_margin_top = 28 # Push border outwards at the top
	win_border.set_corner_radius_all(radius_lg)
	win_border.shadow_color = shadow_elevated.shadow_color
	win_border.shadow_size = shadow_elevated.shadow_size
		
	active_theme.set_stylebox("embedded_border", "Window", win_border)
	active_theme.set_stylebox("embedded_unfocused_border", "Window", win_border)
	active_theme.set_color("title_color", "Window", color_text)
	active_theme.set_color("title_outline_modulate", "Window", Color(0, 0, 0, 0))
		
	# 9. Update Theme Type Variations
	# Write colors and font sizes directly to each named variation.
	# Any node using theme_type_variation = "..." picks these up automatically — no tree traversal needed.
	var sz = font_size_modifier
 
	# LabelTitle — primary section headings, full text color
	active_theme.set_color("font_color", "LabelTitle", color_text)
	active_theme.set_font_size("font_size", "LabelTitle", 14 + sz)
 
	# LabelMuted — secondary/sub labels, 65% opacity
	var muted_c = color_text
	muted_c.a = 0.65
	active_theme.set_color("font_color", "LabelMuted", muted_c)
	active_theme.set_font_size("font_size", "LabelMuted", 13 + sz)
 
	# LabelSubtle — tertiary info / reason labels, 45% opacity
	var subtle_c = color_text
	subtle_c.a = 0.45
	active_theme.set_color("font_color", "LabelSubtle", subtle_c)
	active_theme.set_font_size("font_size", "LabelSubtle", 10 + sz)
 
	# LabelSmall — form field labels, full text color at small size
	active_theme.set_color("font_color", "LabelSmall", color_text)
	active_theme.set_font_size("font_size", "LabelSmall", 11 + sz)
 
	# LabelAccent — accent-colored headings (logos, modal titles)
	active_theme.set_color("font_color", "LabelAccent", color_accent)
	active_theme.set_font_size("font_size", "LabelAccent", 14 + sz)
 
	# RichTextSmall — sidebar memory labels at 12 px
	active_theme.set_color("default_color", "RichTextSmall", color_text)
	var sans_font = active_theme.default_font
	if not sans_font:
		sans_font = active_theme.get_font("font", "Label")
	for rt_size_key in ["normal_font_size", "bold_font_size", "italics_font_size", "bold_italics_font_size", "mono_font_size"]:
		active_theme.set_font_size(rt_size_key, "RichTextSmall", 12 + sz)
	if sans_font:
		active_theme.set_font("normal_font", "RichTextSmall", sans_font)
		active_theme.set_font("bold_font", "RichTextSmall", sans_font)
		active_theme.set_font("italics_font", "RichTextSmall", sans_font)
		active_theme.set_font("bold_italics_font", "RichTextSmall", sans_font)
		active_theme.set_font("mono_font", "RichTextSmall", sans_font)
 
	# ButtonAccent — primary CTA: accent normal style (hover/pressed inherit from base Button)
	_update_sb(active_theme, "normal", "ButtonAccent", color_accent, color_accent, 0.2, 1.0, radius_md, spacing_md, spacing_sm)
 
	active_theme.emit_changed()
	theme_changed.emit()

func get_theme_list() -> Array[String]:
	var list: Array[String] = []
	for p in PRESETS.keys():
		list.append(p)
	for c in custom_themes.keys():
		list.append(c)
	return list

func _load_preset(theme_name: String) -> void:
	var p = PRESETS[theme_name]
	color_bg = Color(p["color_bg"])
	color_surface = Color(p["color_surface"])
	color_border = Color(p["color_border"])
	color_text = Color(p["color_text"])
	color_accent = Color(p["color_accent"])

func _load_custom(theme_name: String) -> void:
	var c = custom_themes[theme_name]
	color_bg = Color(c["color_bg"])
	color_surface = Color(c["color_surface"])
	color_border = Color(c["color_border"])
	color_text = Color(c["color_text"])
	color_accent = Color(c["color_accent"])

func _update_sb(theme: Theme, style_name: String, type_name: String, bg: Color, border: Color, bg_a: float, border_a: float, radius: float = -1.0, margin_h: float = -1.0, margin_v: float = -1.0, shadow_size: int = -1, shadow_color: Color = Color(0,0,0,0)) -> void:
	var sb = theme.get_stylebox(style_name, type_name) as StyleBoxFlat
	if sb:
		var b_color = bg
		b_color.a = bg_a
		sb.bg_color = b_color
		
		var border_color = border
		border_color.a = border_a
		sb.border_color = border_color
		
		if radius >= 0:
			sb.set_corner_radius_all(radius)
			
		if margin_h >= 0:
			sb.content_margin_left = margin_h
			sb.content_margin_right = margin_h
		if margin_v >= 0:
			sb.content_margin_top = margin_v
			sb.content_margin_bottom = margin_v
			
		if shadow_size >= 0:
			sb.shadow_size = shadow_size
			sb.shadow_color = shadow_color
			
		theme.set_stylebox(style_name, type_name, sb)

# DEPRECATED — No longer called. All styling is now handled by Theme Type Variations
# defined in orison_ui.tres and updated by apply_active_theme(). Kept for reference
# in case a future dynamic node (e.g. a runtime-instantiated panel) needs a one-off update.
func apply_theme_to_hierarchy(node: Node) -> void:
	if not node:
		return
		
	# 1. Apply root theme property if it has it
	if node is Control or node is Window:
		node.theme = active_theme
		
	# Apply font size scaling to custom overrides
	if node is Control:
		if node.has_theme_font_size_override("font_size"):
			var orig = 14
			if not node.has_meta("original_font_size"):
				orig = node.get_theme_font_size("font_size")
				node.set_meta("original_font_size", orig)
			else:
				orig = node.get_meta("original_font_size")
			node.add_theme_font_size_override("font_size", orig + font_size_modifier)
			
		if node is RichTextLabel:
			for rt_font_type in ["normal_font_size", "bold_font_size", "italics_font_size", "bold_italics_font_size", "mono_font_size"]:
				if node.has_theme_font_size_override(rt_font_type):
					var orig = 14
					var meta_key = "original_" + rt_font_type
					if not node.has_meta(meta_key):
						orig = node.get_theme_font_size(rt_font_type)
						node.set_meta(meta_key, orig)
					else:
						orig = node.get_meta(meta_key)
					node.add_theme_font_size_override(rt_font_type, orig + font_size_modifier)
		
	# 2. Handle specific control types
	if node is Label:
		var n = node.name.to_lower()
		if n.contains("emotion"):
			# Preserve emotion color if set, or set to text color
			if not node.has_theme_color_override("font_color"):
				var sec_color = color_text
				sec_color.a = 0.65
				node.add_theme_color_override("font_color", sec_color)
		elif n.contains("desc") or n.contains("tagline") or n.contains("sub") or n.contains("sec") or n.contains("reason") or n.contains("affinity") or n.contains("info") or n.contains("status") or n.contains("no"):
			var sec_color = color_text
			sec_color.a = 0.65
			node.add_theme_color_override("font_color", sec_color)
		else:
			node.add_theme_color_override("font_color", color_text)
			
	elif node is RichTextLabel:
		node.add_theme_color_override("default_color", color_text)
		
	elif node is LineEdit:
		node.add_theme_color_override("font_color", color_text)
		var placeholder_c = color_text
		placeholder_c.a = 0.45
		node.add_theme_color_override("font_placeholder_color", placeholder_c)
		
	elif node is Tree:
		node.add_theme_color_override("font_color", color_text)
		node.add_theme_color_override("title_button_color", color_text)
		
	elif node is ColorRect:
		# Check if it's a background
		var n = node.name.to_lower()
		if n.contains("bg") or n.contains("background"):
			if n.contains("overlay"):
				var bg_c = color_bg
				bg_c.a = 0.85
				node.color = bg_c
			else:
				node.color = color_bg
				
	elif node is PanelContainer:
		# If it has a local stylebox override, we update its colors to match the theme
		# while keeping its other properties (like corner radius and shadow).
		for style_name in ["panel"]:
			if node.has_theme_stylebox_override(style_name):
				var sb = node.get_theme_stylebox(style_name)
				if sb is StyleBoxFlat:
					var new_sb = sb.duplicate()
					var bg_c = color_surface
					bg_c.a = sb.bg_color.a
					new_sb.bg_color = bg_c
					
					var border_c = color_border
					border_c.a = sb.border_color.a
					new_sb.border_color = border_c
					
					# Soften shadow if light theme
					var is_light = color_bg.get_luminance() > 0.5
					if is_light:
						new_sb.shadow_color = Color(0, 0, 0, 0.05)
						new_sb.shadow_size = 12
					else:
						new_sb.shadow_color = Color(0, 0, 0, 0.4)
						new_sb.shadow_size = 24
						
					node.add_theme_stylebox_override(style_name, new_sb)
					
	elif node is Button:
		# Check for local stylebox overrides
		for style_name in ["normal", "hover", "pressed", "focus", "disabled"]:
			if node.has_theme_stylebox_override(style_name):
				var sb = node.get_theme_stylebox(style_name)
				if sb is StyleBoxFlat:
					var new_sb = sb.duplicate()
					# Check if it's an accent/primary button or a normal button
					var n = node.name.to_lower()
					var is_accent = n.contains("accent") or n.contains("start") or n.contains("craft") or n.contains("save") or n.contains("confirm") or n.contains("next")
					var is_light = color_bg.get_luminance() > 0.5
					
					if is_accent:
						var bg_c = color_accent
						bg_c.a = 0.25 if is_light else (sb.bg_color.a if sb.bg_color.a > 0 else 0.2)
						new_sb.bg_color = bg_c
						
						var border_c = color_accent
						border_c.a = 1.0
						new_sb.border_color = border_c
					else:
						var bg_c = color_surface
						bg_c.a = 0.95 if is_light else (sb.bg_color.a if sb.bg_color.a > 0 else 0.4)
						new_sb.bg_color = bg_c
						
						var border_c = color_border
						border_c.a = 0.8 if is_light else (sb.border_color.a if sb.border_color.a > 0 else 0.4)
						new_sb.border_color = border_c
						
					node.add_theme_stylebox_override(style_name, new_sb)
					
	# Recurse children
	for child in node.get_children():
		apply_theme_to_hierarchy(child)
