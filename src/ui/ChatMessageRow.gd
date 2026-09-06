# res://src/ui/ChatMessageRow.gd
extends MarginContainer
class_name ChatMessageRow

signal meta_clicked(meta: Variant)

@onready var rich_text_label: RichTextLabel = %RichTextLabel

func _ready() -> void:
	if rich_text_label:
		rich_text_label.meta_clicked.connect(_on_meta_clicked)

func _on_meta_clicked(meta: Variant) -> void:
	meta_clicked.emit(meta)

func set_message(msg: Dictionary, is_light: bool) -> void:
	var sender = msg.get("sender", "")
	var text = msg.get("text", "")
	var type = msg.get("type", "chat")
	
	if not rich_text_label:
		return
		
	rich_text_label.text = ""
	
	match type:
		"system":
			var system_tag_color = "#2563EB" if is_light else "#60A5FA"
			rich_text_label.text = "[color=%s]ℹ️ [SYSTEM][/color]: %s" % [system_tag_color, text]
		"warning":
			var warning_tag_color = "#D97706" if is_light else "#FBBF24"
			rich_text_label.text = "[color=%s]⚠️ [WARNING][/color]: %s" % [warning_tag_color, text]
		"error":
			var error_color = "#DC2626" if is_light else "#F87171"
			rich_text_label.text = "[color=%s]❌ [ERROR][/color]: [color=%s][b]%s[/b][/color]" % [error_color, error_color, text]
		"chat":
			var friendly_name = sender.capitalize()
			if sender == "user" or sender == "player":
				friendly_name = "Player"
			elif sender == "narrator":
				friendly_name = "Narrator"
			else:
				var character = CampaignState.get_character(sender)
				if not character.is_empty():
					friendly_name = character.get("name", sender.capitalize())
			
			var sender_color = _get_adjusted_sender_color(sender, is_light)
			rich_text_label.text = "[color=%s]%s[/color]: %s" % [sender_color, friendly_name, text]

func _get_adjusted_sender_color(sender: String, is_light: bool) -> String:
	if sender == "user" or sender == "player":
		return "#EA580C" if is_light else "#FF5F38"
	elif sender == "system":
		return "#4B5563" if is_light else "#A59EBF"
	elif sender == "narrator":
		return "#4A3F35" if is_light else "#FFF8F2"
	else:
		var character = CampaignState.get_character(sender)
		if not character.is_empty():
			var emotions = character.get("emotions", [])
			var last_emotion = emotions[-1].get("emotion", "serenity") if not emotions.is_empty() else "serenity"
			return _get_emotion_hex_color(last_emotion, is_light)
		return "#BE123C" if is_light else "#F43F5E"

func _get_emotion_hex_color(emotion: String, is_light: bool) -> String:
	match emotion.to_lower():
		"joy": return "#D97706" if is_light else "#F59E0B"
		"anger": return "#B91C1C" if is_light else "#DC2626"
		"sadness": return "#1D4ED8" if is_light else "#3B82F6"
		"fear": return "#6D28D9" if is_light else "#7C3AED"
		"trust": return "#047857" if is_light else "#059669"
		"disgust": return "#4D7C0F" if is_light else "#65A30D"
		"surprise": return "#0891B2" if is_light else "#06B6D4"
		"serenity": return "#4B5563" if is_light else "#D1D5DB"
		_: return "#BE123C" if is_light else "#F43F5E"
