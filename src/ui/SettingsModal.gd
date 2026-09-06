# res://src/ui/SettingsModal.gd
extends Control
class_name SettingsModal

signal closed

@onready var close_btn: Button = %CloseButton
@onready var theme_panel: ThemeSettingsPanel = $ScrollContainer/CenterContainer/CardPanel/CardMargin/CardVBox/TabContainer/Theme
@onready var llm_panel: LLMSettingsPanel = $ScrollContainer/CenterContainer/CardPanel/CardMargin/CardVBox/TabContainer/LLM
@onready var image_gen_panel: ImageGenSettingsPanel = $"ScrollContainer/CenterContainer/CardPanel/CardMargin/CardVBox/TabContainer/Image Gen"

func _ready() -> void:
	close_btn.pressed.connect(_on_close_pressed)
	
	# Apply active theme overrides on modal itself
	ThemeManager.theme_changed.connect(_on_global_theme_changed)
	_apply_modal_styles()

	# Entrance transition
	var overlay = get_node_or_null("OverlayBG")
	var card = get_node_or_null("ScrollContainer/CenterContainer/CardPanel")
	if overlay and card:
		var target_bg_alpha = overlay.color.a
		overlay.color.a = 0.0
		card.modulate.a = 0.0
		card.scale = Vector2(0.9, 0.9)
		
		# Update pivot offset to center when size changes or initially
		card.pivot_offset = card.size / 2.0
		card.item_rect_changed.connect(func():
			card.pivot_offset = card.size / 2.0
		)
		
		var entrance_tween = create_tween().set_parallel(true)
		entrance_tween.tween_property(overlay, "color:a", target_bg_alpha, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(ThemeManager.ease_default)
		entrance_tween.tween_property(card, "modulate:a", 1.0, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(ThemeManager.ease_default)
		entrance_tween.tween_property(card, "scale", Vector2(1.0, 1.0), ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(ThemeManager.ease_default)

func _on_close_pressed() -> void:
	closed.emit()
	
	var overlay = get_node_or_null("OverlayBG")
	var card = get_node_or_null("ScrollContainer/CenterContainer/CardPanel")
	if overlay and card:
		card.pivot_offset = card.size / 2.0
		var exit_tween = create_tween().set_parallel(true)
		exit_tween.tween_property(overlay, "color:a", 0.0, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(Tween.EASE_IN)
		exit_tween.tween_property(card, "modulate:a", 0.0, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(Tween.EASE_IN)
		exit_tween.tween_property(card, "scale", Vector2(0.9, 0.9), ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(Tween.EASE_IN)
		exit_tween.chain().tween_callback(queue_free)
	else:
		queue_free()

func _on_global_theme_changed() -> void:
	_apply_modal_styles()

func _apply_modal_styles() -> void:
	self.theme = ThemeManager.active_theme
