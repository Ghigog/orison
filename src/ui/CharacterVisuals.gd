# res://src/ui/CharacterVisuals.gd
extends Control
class_name CharacterVisuals

@onready var sprite_rect: TextureRect = %SpriteRect
@onready var emoji_spawn_point: Marker2D = %EmojiSpawnPoint
@onready var reaction_panel: PanelContainer = %ReactionPanel
@onready var reaction_label: RichTextLabel = %ReactionLabel
@onready var char_status_overlay: AssetStatusOverlay = %CharStatusOverlay
@onready var generate_portrait_btn: Button = %GeneratePortraitButton


var _original_position: Vector2
var _active_tweens: Array[Tween] = []
var _active_char_id: String = ""
var _reaction_tween: Tween
var _reaction_pending: bool = false

func _ready() -> void:
	if sprite_rect:
		_original_position = sprite_rect.position
	
	if reaction_panel:
		reaction_panel.modulate.a = 0.0
		reaction_panel.visible = false
		
	if generate_portrait_btn:
		generate_portrait_btn.pressed.connect(_on_generate_portrait_pressed)
		generate_portrait_btn.visible = false
		
	EventBus.location_changed.connect(func(_loc): _hide_reaction())
	
	if ImageGenManager:
		ImageGenManager.asset_generated.connect(_on_asset_generated)
		ImageGenManager.asset_generation_started.connect(_on_asset_generation_started)
		ImageGenManager.asset_generation_failed.connect(_on_asset_generation_failed)

func load_character(char_id: String) -> void:
	_active_char_id = char_id
	_hide_reaction()
	_kill_active_tweens()
	
	var target_path = ImageGenManager.get_avatar_path(char_id)
	if char_status_overlay:
		char_status_overlay.setup(target_path)
		
	var texture = await ImageGenManager.get_image_or_fallback(char_id, "avatar")
	if sprite_rect:
		sprite_rect.texture = texture
		sprite_rect.position = _original_position
		sprite_rect.self_modulate = Color.WHITE
		sprite_rect.scale = Vector2.ONE
		
		if texture:
			# Hide the generate button and fade in portrait
			_set_generate_button_visible(false)
			if sprite_rect.modulate.a < 1.0:
				sprite_rect.modulate.a = 0.0
				var fade_tween = create_tween()
				_active_tweens.append(fade_tween)
				fade_tween.tween_property(sprite_rect, "modulate:a", 1.0, 0.45)\
					.set_trans(Tween.TRANS_CUBIC)\
					.set_ease(Tween.EASE_OUT)
		else:
			# No portrait on disk — show the generate button if not already generating
			var state = ImageGenManager.get_asset_state(target_path)
			_set_generate_button_visible(state.status != "generating")


func fade_out(duration: float = 0.25) -> void:
	_kill_active_tweens()
	_hide_reaction()
	if sprite_rect and sprite_rect.modulate.a > 0.0:
		var fade_tween = create_tween()
		_active_tweens.append(fade_tween)
		fade_tween.tween_property(sprite_rect, "modulate:a", 0.0, duration)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)

func fade_in(duration: float = 0.25) -> void:
	_kill_active_tweens()
	if sprite_rect and sprite_rect.modulate.a < 1.0:
		var fade_tween = create_tween()
		_active_tweens.append(fade_tween)
		fade_tween.tween_property(sprite_rect, "modulate:a", 1.0, duration)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)

func _kill_active_tweens() -> void:
	for t in _active_tweens:
		if t and t.is_valid():
			t.kill()
	_active_tweens.clear()

func apply_emotion(emotion: String, _affinity: float) -> void:
	if not sprite_rect or _active_char_id.is_empty():
		return
		
	# Load default character avatar instead of emotion-specific one
	var target_path = ImageGenManager.get_avatar_path(_active_char_id)
	if char_status_overlay:
		char_status_overlay.setup(target_path)
		
	var texture = await ImageGenManager.get_image_or_fallback(_active_char_id, "avatar")
	if sprite_rect:
		sprite_rect.texture = texture
		if texture:
			_set_generate_button_visible(false)
		else:
			var state = ImageGenManager.get_asset_state(target_path)
			_set_generate_button_visible(state.status != "generating")

		
	# Check if character is hidden so we can fade them in
	var was_hidden = sprite_rect.modulate.a < 1.0
	_kill_active_tweens()
	
	# Reset properties before starting transition
	sprite_rect.position = _original_position
	sprite_rect.scale = Vector2.ONE
	
	if was_hidden:
		sprite_rect.modulate.a = 0.0
		var fade_tween = create_tween()
		_active_tweens.append(fade_tween)
		fade_tween.tween_property(sprite_rect, "modulate:a", 1.0, 0.45)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)
	else:
		sprite_rect.modulate.a = 1.0
	
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
	var emoji_scene = preload("res://scenes/ui/FloatingEmoji.tscn")
	var emoji_instance = emoji_scene.instantiate()
	add_child(emoji_instance)
	emoji_instance.setup(text_content, emoji_spawn_point.position)

func _hide_reaction() -> void:
	if not reaction_panel or not reaction_panel.visible:
		return
	if _reaction_tween and _reaction_tween.is_valid():
		_reaction_tween.kill()
	_reaction_tween = create_tween()
	_reaction_tween.tween_property(reaction_panel, "modulate:a", 0.0, 0.3)\
		.set_trans(Tween.TRANS_QUAD)\
		.set_ease(Tween.EASE_OUT)
	_reaction_tween.tween_callback(func():
		reaction_panel.visible = false
	)

func _show_reaction(text_content: String) -> void:
	if not reaction_panel or not reaction_label:
		return
	reaction_label.text = "[center][i]" + text_content + "[/i][/center]"
	
	if _reaction_tween and _reaction_tween.is_valid():
		_reaction_tween.kill()
		
	reaction_panel.visible = true
	_reaction_tween = create_tween()
	_reaction_tween.tween_property(reaction_panel, "modulate:a", 1.0, 0.45)\
		.set_trans(Tween.TRANS_CUBIC)\
		.set_ease(Tween.EASE_OUT)

func generate_physical_reaction(char_id: String, emotion: String) -> void:
	if _reaction_pending:
		return
	_reaction_pending = true
	
	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		_reaction_pending = false
		return
		
	var char_name = character.get("name", char_id.capitalize())
	var biography = character.get("biography", "")
	
	var emotions = character.get("emotions", [])
	var intensity_val = 0.5
	var reason_str = ""
	if not emotions.is_empty():
		var last_event = emotions[-1]
		if last_event.get("emotion") == emotion:
			intensity_val = last_event.get("intensity", 0.5)
			reason_str = last_event.get("context", "")
			
	if reason_str.is_empty():
		_hide_reaction()
		_reaction_pending = false
		return
		
	var prompt = "You are a creative writer and game master.\n"
	prompt += "Given a character's description and their current emotional state, write a single, short sentence (maximum 20 words) in the third person describing their immediate physical reaction, facial expression, body language, or movement. Do not use dialogue or sound effects. Focus only on visual cues. Do not repeat the prompt. Do not write anything other than the single sentence.\n\n"
	prompt += "Character Name: %s\n" % char_name
	var gender_str = character.get("gender", "")
	if not gender_str.strip_edges().is_empty():
		prompt += "Gender/Pronouns: %s\n" % gender_str.strip_edges()
	if not biography.strip_edges().is_empty():
		prompt += "Character Description: %s\n" % biography.strip_edges()
	prompt += "Current Emotion: %s (intensity: %.1f)\n" % [emotion, intensity_val]
	prompt += "Reason for feeling this way: %s\n\n" % reason_str
	prompt += "Output:"
	
	_hide_reaction()
	
	print("[CharacterVisuals] Querying reaction description for %s..." % char_name)
	LLMClient.send_custom_request(prompt, LLMClient.character_model, func(success: bool, response_text: String, error_msg: String):
		_reaction_pending = false
		if success:
			var reaction_text = response_text.strip_edges().replace('"', '')
			print("[CharacterVisuals] Reaction generated: %s" % reaction_text)
			_show_reaction(reaction_text)
		else:
			print("[CharacterVisuals] Failed to generate reaction: %s" % error_msg)
	)

func _on_asset_generated(output_path: String, _is_placeholder: bool) -> void:
	if _active_char_id.is_empty():
		return
		
	var filename = output_path.get_file()
	# Examples: character_elara_the_wise.png, character_player.png, etc.
	if filename.begins_with("character_" + _active_char_id):
		var expected_path = ImageGenManager.get_avatar_path(_active_char_id)
		if output_path == expected_path or output_path.get_file() == expected_path.get_file():
			var state = ImageGenManager.get_asset_state(output_path)
			if state.status == "success":
				var texture = await ImageGenManager.get_image_or_fallback(_active_char_id, "avatar")
				if sprite_rect:
					sprite_rect.texture = texture
					if texture:
						_set_generate_button_visible(false)
						sprite_rect.modulate.a = 0.0
						var fade_tween = create_tween()
						_active_tweens.append(fade_tween)
						fade_tween.tween_property(sprite_rect, "modulate:a", 1.0, 0.45)\
							.set_trans(Tween.TRANS_CUBIC)\
							.set_ease(Tween.EASE_OUT)
				else:
					if sprite_rect:
						sprite_rect.texture = null
					_set_generate_button_visible(true)

func _on_asset_generation_started(output_path: String) -> void:
	if _active_char_id.is_empty():
		return
	var expected_path = ImageGenManager.get_avatar_path(_active_char_id)
	if output_path == expected_path:
		# Generation kicked off — hide the generate button (spinner takes over)
		_set_generate_button_visible(false)

func _on_asset_generation_failed(output_path: String, _error_msg: String) -> void:
	if _active_char_id.is_empty():
		return
	var expected_path = ImageGenManager.get_avatar_path(_active_char_id)
	if output_path == expected_path:
		# Generation failed — restore the generate button so user can retry
		_set_generate_button_visible(true)

func _on_generate_portrait_pressed() -> void:
	if _active_char_id.is_empty():
		return
	_set_generate_button_visible(false)
	ImageGenManager.generate_character_portrait(_active_char_id)

func _set_generate_button_visible(show: bool) -> void:
	if generate_portrait_btn:
		generate_portrait_btn.visible = show
