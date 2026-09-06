# res://src/ui/CharacterListItem.gd
extends Button
class_name CharacterListItem

signal selected(character_id: String)

@onready var name_label: Label = %NameLabel
@onready var affinity_label: Label = %AffinityLabel
@onready var rapport_bar: ProgressBar = %RapportBar
@onready var emotion_label: Label = %EmotionLabel
@onready var reason_label: Label = %ReasonLabel
@onready var avatar_rect: TextureRect = %AvatarRect
@onready var avatar_status_overlay: AssetStatusOverlay = %AvatarStatusOverlay


var character_id: String = ""

func _ready() -> void:
	if ImageGenManager:
		ImageGenManager.asset_generated.connect(_on_asset_generated)

func setup(char_id: String, char_name: String, affinity: float) -> void:
	character_id = char_id
	
	# Wait for node binding if ready hasn't been called yet
	if not is_inside_tree():
		await ready
		
	var target_path = ImageGenManager.get_avatar_path(char_id)
	if avatar_status_overlay:
		avatar_status_overlay.setup(target_path)
		
	var texture = await ImageGenManager.get_image_or_fallback(char_id, "avatar")
	if avatar_rect:
		avatar_rect.texture = texture

		
	name_label.text = char_name
	affinity_label.text = "%+.1f" % affinity
	rapport_bar.value = affinity
	
	# Override RapportBar fill color based on emotional / affinity quality
	var fill_style = rapport_bar.get_theme_stylebox("fill").duplicate() as StyleBoxFlat
	if fill_style:
		if affinity > 0.1:
			fill_style.bg_color = ThemeManager.color_success
		elif affinity < -0.1:
			fill_style.bg_color = ThemeManager.color_danger
		else:
			var grey_c = ThemeManager.color_text
			grey_c.a = 0.65
			fill_style.bg_color = grey_c
		rapport_bar.add_theme_stylebox_override("fill", fill_style)
		
	# Retrieve emotion and reason from CampaignState
	var character = CampaignState.get_character(char_id)
	var emotions = character.get("emotions", [])
	
	if not emotions.is_empty():
		var last_emotion_event = emotions[-1]
		var emotion_name = str(last_emotion_event.get("emotion", "serenity"))
		var intensity = float(last_emotion_event.get("intensity", 0.5))
		var reason = str(last_emotion_event.get("context", ""))
		
		emotion_label.text = "%s (%.1f)" % [emotion_name.capitalize(), intensity]
		emotion_label.add_theme_color_override("font_color", ThemeManager.get_emotion_color(emotion_name))
		emotion_label.visible = true
		
		if not reason.strip_edges().is_empty():
			reason_label.text = reason
			reason_label.visible = true
		else:
			reason_label.visible = false
	else:
		emotion_label.visible = false
		reason_label.visible = false
		
	# Defer updating the custom minimum height to allow layout width calculation first
	call_deferred("_update_height")

func _update_height() -> void:
	var margin_container = $Margin
	if margin_container:
		custom_minimum_size.y = margin_container.get_combined_minimum_size().y

func get_emotion_color(emotion: String) -> Color:
	return ThemeManager.get_emotion_color(emotion)

func _pressed() -> void:
	selected.emit(character_id)

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
		else:
			if avatar_rect:
				avatar_rect.texture = null

