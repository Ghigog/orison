# res://src/ui/CharacterDetailModal.gd
extends Control
class_name CharacterDetailModal

signal talk_requested(char_id: String)
signal closed

@onready var name_label: Label = %NameLabel
@onready var avatar_rect: TextureRect = %AvatarRect
@onready var bio_label: RichTextLabel = %BioLabel
@onready var affinity_label: Label = %AffinityLabel
@onready var rapport_bar: ProgressBar = %RapportBar
@onready var emotion_label: Label = %EmotionLabel
@onready var reason_label: Label = %ReasonLabel
@onready var talk_button: Button = %TalkButton
@onready var close_button: Button = %CloseButton
@onready var avatar_status_overlay: AssetStatusOverlay = %AvatarStatusOverlay
@onready var generate_portrait_btn: Button = %GeneratePortraitButton

var character_id: String = ""

func _ready() -> void:
	talk_button.pressed.connect(_on_talk_pressed)
	close_button.pressed.connect(_on_close_pressed)
	if generate_portrait_btn:
		generate_portrait_btn.pressed.connect(_on_generate_portrait_pressed)
		generate_portrait_btn.visible = false
	
	ThemeManager.theme_changed.connect(_on_global_theme_changed)
	_apply_modal_styles()
	
	if ImageGenManager:
		ImageGenManager.asset_generated.connect(_on_asset_generated)
		ImageGenManager.asset_generation_started.connect(_on_asset_generation_started)
		ImageGenManager.asset_generation_failed.connect(_on_asset_generation_failed)
	
	# Entrance transition
	var overlay = get_node_or_null("OverlayBG")
	var card = get_node_or_null("ScrollContainer/CenterContainer/CardPanel")
	if overlay and card:
		var target_bg_alpha = overlay.color.a
		overlay.color.a = 0.0
		card.modulate.a = 0.0
		card.scale = Vector2(0.9, 0.9)
		
		# Set pivot offset for zoom transition
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

func initialize(char_id: String) -> void:
	character_id = char_id
	
	if not is_inside_tree():
		await ready
		
	var character = CampaignState.get_character(char_id)
	name_label.text = character.get("name", char_id)
	
	var affinity = character.get("affinity", 0.0)
	affinity_label.text = "%+.1f" % affinity
	rapport_bar.value = affinity
	
	var target_path = ImageGenManager.get_avatar_path(char_id)
	if avatar_status_overlay:
		avatar_status_overlay.setup(target_path)
		
	var texture = await ImageGenManager.get_image_or_fallback(char_id, "avatar")
	if avatar_rect:
		avatar_rect.texture = texture
		if texture:
			_set_generate_button_visible(false)
		else:
			var state = ImageGenManager.get_asset_state(target_path)
			_set_generate_button_visible(state.status != "generating")
		
	var bio = character.get("biography", "")
	if bio.is_empty():
		bio = character.get("physical_description", character.get("personality", "No details available."))
	bio_label.text = bio
	
	var emotions = character.get("emotions", [])
	if not emotions.is_empty():
		var last_emotion_event = emotions[-1]
		var emotion_name = str(last_emotion_event.get("emotion", "serenity"))
		var intensity = float(last_emotion_event.get("intensity", 0.5))
		var reason = str(last_emotion_event.get("context", ""))
		
		emotion_label.text = "Feeling: %s (Intensity: %.1f)" % [emotion_name.capitalize(), intensity]
		emotion_label.add_theme_color_override("font_color", ThemeManager.get_emotion_color(emotion_name))
		
		if not reason.strip_edges().is_empty():
			reason_label.text = "Reason: " + reason
			reason_label.visible = true
		else:
			reason_label.visible = false
	else:
		emotion_label.text = "Feeling: Serenity (0.5)"
		reason_label.visible = false

func _on_talk_pressed() -> void:
	talk_requested.emit(character_id)
	_on_close_pressed()

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

func _on_generate_portrait_pressed() -> void:
	if character_id.is_empty():
		return
	_set_generate_button_visible(false)
	ImageGenManager.generate_character_portrait(character_id)

func _set_generate_button_visible(show: bool) -> void:
	if generate_portrait_btn:
		generate_portrait_btn.visible = show

func _on_asset_generation_started(path: String) -> void:
	if character_id.is_empty():
		return
	var expected_path = ImageGenManager.get_avatar_path(character_id)
	if path == expected_path:
		_set_generate_button_visible(false)

func _on_asset_generation_failed(path: String, _error_msg: String) -> void:
	if character_id.is_empty():
		return
	var expected_path = ImageGenManager.get_avatar_path(character_id)
	if path == expected_path:
		_set_generate_button_visible(true)

func _on_asset_generated(output_path: String, _is_placeholder: bool) -> void:
	if character_id.is_empty():
		return
	var expected_path = ImageGenManager.get_avatar_path(character_id)
	if output_path == expected_path or output_path.get_file() == expected_path.get_file():
		var state = ImageGenManager.get_asset_state(output_path)
		if state.status == "success":
			var texture = await ImageGenManager.get_image_or_fallback(character_id, "avatar")
			if avatar_rect:
				avatar_rect.texture = texture
				if texture:
					_set_generate_button_visible(false)
				else:
					_set_generate_button_visible(true)
		else:
			if avatar_rect:
				avatar_rect.texture = null
			_set_generate_button_visible(true)
