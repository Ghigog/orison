# res://src/ui/onboarding/ReviewScreen.gd
extends VBoxContainer

signal back_pressed
signal next_pressed

@onready var scan_summary_label: Label = %ScanSummaryLabel
@onready var review_back_btn: Button = %ReviewBackButton
@onready var review_next_btn: Button = %ReviewNextButton

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

var _generated_starters: Array = []
var _selected_starter_idx: int = -1

func _ready() -> void:
	review_back_btn.pressed.connect(func(): back_pressed.emit())
	review_next_btn.pressed.connect(func(): next_pressed.emit())
	
	starter_card_1.pressed.connect(func(): _select_starter(0))
	starter_card_2.pressed.connect(func(): _select_starter(1))
	starter_card_3.pressed.connect(func(): _select_starter(2))
	
	review_next_btn.disabled = true

func on_screen_shown() -> void:
	# Clear any previous selection when shown anew
	_selected_starter_idx = -1
	_update_card_selection_visuals()
	review_next_btn.disabled = true

func set_summary_text(text: String) -> void:
	scan_summary_label.text = text

func get_selected_starter_index() -> int:
	return _selected_starter_idx

func display_starters(starters: Array, compiled_data: Dictionary) -> void:
	_generated_starters = starters
	
	var titles = [card_title_1, card_title_2, card_title_3]
	var descs = [card_desc_1, card_desc_2, card_desc_3]
	var metas = [card_meta_1, card_meta_2, card_meta_3]
	
	var nodes = compiled_data.get("knowledge_graph", {}).get("nodes", {})
	
	for i in range(3):
		if i >= starters.size():
			continue
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
