# res://src/ui/CharacterVisuals.gd
extends Control
class_name CharacterVisuals

@onready var sprite_rect: TextureRect = %SpriteRect
@onready var emoji_spawn_point: Marker2D = %EmojiSpawnPoint

var _original_position: Vector2
var _active_tweens: Array[Tween] = []

func _ready() -> void:
	if sprite_rect:
		_original_position = sprite_rect.position

func load_character(char_id: String) -> void:
	_kill_active_tweens()
	if sprite_rect:
		sprite_rect.position = _original_position
		sprite_rect.self_modulate = Color.WHITE
		sprite_rect.modulate.a = 1.0
		sprite_rect.scale = Vector2.ONE
		
	var texture = CampaignState.get_character_avatar(char_id)
	if sprite_rect:
		sprite_rect.texture = texture

func _kill_active_tweens() -> void:
	for t in _active_tweens:
		if t and t.is_valid():
			t.kill()
	_active_tweens.clear()

func apply_emotion(emotion: String, _affinity: float) -> void:
	if not sprite_rect:
		return
		
	_kill_active_tweens()
	
	# Reset properties before starting transition
	sprite_rect.position = _original_position
	sprite_rect.modulate.a = 1.0
	sprite_rect.scale = Vector2.ONE
	
	var target_color = Color.WHITE
	var motion_tween = create_tween()
	_active_tweens.append(motion_tween)
	
	match emotion:
		"anger":
			target_color = Color("#DC2626") # Crimson
			# Anger Jitter Shake: Offset position.x randomly for 0.25s
			motion_tween.set_parallel(true)
			for i in range(5):
				var offset = randf_range(-5.0, 5.0)
				motion_tween.tween_property(sprite_rect, "position:x", _original_position.x + offset, 0.05).set_delay(i * 0.05)
			motion_tween.chain().tween_property(sprite_rect, "position:x", _original_position.x, 0.05)
			
		"sadness":
			target_color = Color("#3B82F6") # Melancholic blue
			# Sadness Dull Fade: Modulate and lower position.y by 15px
			motion_tween.tween_property(sprite_rect, "position:y", _original_position.y + 15.0, 0.4).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
			
		"joy":
			target_color = Color("#F59E0B") # Warm Golden-Amber
			# Joy Bounce Jump: Bounce up 30px and back down
			motion_tween.tween_property(sprite_rect, "position:y", _original_position.y - 30.0, 0.2).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
			motion_tween.chain().tween_property(sprite_rect, "position:y", _original_position.y, 0.25).set_trans(Tween.TRANS_BOUNCE).set_ease(Tween.EASE_OUT)
			
		"fear":
			target_color = Color("#7C3AED") # Dark Purple
			# Fear Flickering: Flickers opacity/modulate:a
			motion_tween.set_parallel(true)
			motion_tween.tween_property(sprite_rect, "modulate:a", 0.4, 0.1)
			motion_tween.chain().tween_property(sprite_rect, "modulate:a", 0.9, 0.08)
			motion_tween.chain().tween_property(sprite_rect, "modulate:a", 0.5, 0.12)
			motion_tween.chain().tween_property(sprite_rect, "modulate:a", 1.0, 0.1)
			
		"trust":
			target_color = Color("#059669") # Emerald Green
			# Trust gentle breathing: slow small scale pulse
			sprite_rect.pivot_offset = sprite_rect.size / 2.0
			motion_tween.tween_property(sprite_rect, "scale", Vector2(1.03, 1.03), 0.6).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
			motion_tween.chain().tween_property(sprite_rect, "scale", Vector2(1.0, 1.0), 0.6).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
			
		"disgust":
			target_color = Color("#65A30D") # Toxic Olive/Green
			# static/shiver shake
			motion_tween.set_parallel(true)
			for i in range(4):
				var offset = randf_range(-1.5, 1.5)
				motion_tween.tween_property(sprite_rect, "position:x", _original_position.x + offset, 0.08).set_delay(i * 0.08)
			motion_tween.chain().tween_property(sprite_rect, "position:x", _original_position.x, 0.08)
			
		"surprise":
			target_color = Color("#06B6D4") # Electric Cyan
			# Surprise Flash Scale: Scale jump rapidly
			sprite_rect.pivot_offset = sprite_rect.size / 2.0
			motion_tween.tween_property(sprite_rect, "scale", Vector2(1.08, 1.08), 0.15).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
			motion_tween.chain().tween_property(sprite_rect, "scale", Vector2(1.0, 1.0), 0.25).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)
			
		"serenity":
			target_color = Color("#D1D5DB") # Soft Silver
			motion_tween.tween_property(sprite_rect, "scale", Vector2(1.0, 1.0), 0.3)
			
		_:
			target_color = Color.WHITE

	# Tween sprite color/tint shift smoothly
	var color_tween = create_tween()
	_active_tweens.append(color_tween)
	color_tween.tween_property(sprite_rect, "self_modulate", target_color, 0.4).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	
	# Spawn floating emoji indicator
	var emoji_char = _get_emotion_emoji(emotion)
	if not emoji_char.is_empty():
		spawn_floating_text(emoji_char)

func _get_emotion_emoji(emotion: String) -> String:
	match emotion:
		"joy": return "😄"
		"anger": return "💢"
		"sadness": return "😢"
		"fear": return "😨"
		"trust": return "💖"
		"disgust": return "🤢"
		"surprise": return "😲"
		"serenity": return "✨"
		_: return ""

func spawn_floating_text(text_content: String) -> void:
	var label = Label.new()
	label.text = text_content
	label.add_theme_font_size_override("font_size", 32)
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.position = emoji_spawn_point.position + Vector2(-16.0, -16.0)
	add_child(label)
	
	var anim_tween = create_tween().set_parallel(true)
	anim_tween.tween_property(label, "position", label.position + Vector2(0.0, -80.0), 1.2).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	anim_tween.tween_property(label, "modulate:a", 0.0, 1.2).set_trans(Tween.TRANS_LINEAR)
	anim_tween.chain().tween_callback(label.queue_free)
