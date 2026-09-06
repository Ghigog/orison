# res://src/resources/EmotionEvent.gd
extends Resource
class_name EmotionEvent

@export var timestamp: String = ""
@export var emotion: String = ""
@export var intensity: float = 0.0
@export var target: String = ""
@export var context: String = ""
@export var rapport_delta: float = 0.0

func to_dict() -> Dictionary:
	return {
		"timestamp": timestamp,
		"emotion": emotion,
		"intensity": intensity,
		"target": target,
		"context": context,
		"rapport_delta": rapport_delta
	}

static func from_dict(d: Dictionary) -> EmotionEvent:
	var instance = EmotionEvent.new()
	instance.timestamp = d.get("timestamp", "")
	instance.emotion = d.get("emotion", "serenity")
	instance.intensity = d.get("intensity", 0.0)
	instance.target = d.get("target", "player")
	instance.context = d.get("context", "")
	instance.rapport_delta = d.get("rapport_delta", 0.0)
	return instance
