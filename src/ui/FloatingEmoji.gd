# res://src/ui/FloatingEmoji.gd
extends Label

func setup(text_content: String, start_position: Vector2) -> void:
	text = text_content
	position = start_position + Vector2(-16.0, -16.0)
	
	var anim_tween = create_tween().set_parallel(true)
	anim_tween.tween_property(self, "position", position + Vector2(0.0, -80.0), 1.2)\
		.set_trans(Tween.TRANS_QUAD)\
		.set_ease(Tween.EASE_OUT)
	anim_tween.tween_property(self, "modulate:a", 0.0, 1.2)\
		.set_trans(Tween.TRANS_LINEAR)
	anim_tween.chain().tween_callback(queue_free)
