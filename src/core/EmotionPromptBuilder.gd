# res://src/core/EmotionPromptBuilder.gd
extends RefCounted
class_name EmotionPromptBuilder

## Builds the prompt block explaining the character's active emotion and rapport
func build_emotion_block(char_id: String) -> String:
	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		return ""
		
	var char_name = character.get("name", char_id)
	var affinity = character.get("affinity", 0.0)
	var relationship_level = CharacterProfile.get_relationship_label(affinity)
	
	# Fetch last emotional event
	var emotions = character.get("emotions", [])
	var active_emotion = "serenity"
	var intensity = 0.5
	var target = "player"
	var context = "Calm atmosphere."
	
	if not emotions.is_empty():
		var last_event = emotions[-1]
		if last_event is Dictionary:
			active_emotion = last_event.get("emotion", "serenity")
			intensity = last_event.get("intensity", 0.5)
			target = last_event.get("target", "player")
			context = last_event.get("context", "Recent dialogue.")
			
	var tone_guidance = _get_tone_guidance(active_emotion)
	var relationship_guidance = _get_relationship_guidance(relationship_level)
	
	var prompt = "You are %s. You have an internal emotional state that influences your personality.\n" % char_name
	prompt += "Current feeling: %s (intensity: %.1f) towards %s. Reason: %s\n" % [active_emotion.capitalize(), intensity, target, context]
	prompt += "Relationship with player: %s (Affinity: %.2f). %s\n" % [relationship_level, affinity, relationship_guidance]
	prompt += "Tone instruction: %s\n" % tone_guidance
	prompt += "Rules:\n"
	prompt += "- Let these feelings naturally shape your words, choices, and attitude.\n"
	prompt += "- Do not mention your raw affinity score or emotion parameters directly unless specifically asked.\n"
	
	return prompt


func _get_tone_guidance(emotion: String) -> String:
	match emotion:
		"serenity":
			return "Calm, warm, clear, and steady. Your responses feel balanced."
		"joy":
			return "Upbeat, eager, helpful, cooperative, and enthusiastic."
		"sadness":
			return "Gentle, quiet, slightly flat, or melancholic. You speak softly."
		"anger":
			return "Clipped, blunt, impatient, or tense. Your dialogue is sharp."
		"fear":
			return "Careful, tentative, defensive, or guarded. You hesitate to trust."
		"trust":
			return "Open, warm, supportive, and willing to share details."
		"disgust":
			return "Cold, dismissive, revolted, or highly disapproving."
		"surprise":
			return "Expressive, unsettled, highly reactive, or stunned."
		_:
			return "Balanced and calm."

func _get_relationship_guidance(level: String) -> String:
	match level:
		"Nemesis":
			return "You are deeply hostile, suspicious, and actively work against the player's interests."
		"Enemy":
			return "You are guarded, cold, dismissive, and prefer brief, uncooperative interactions."
		"Acquaintance":
			return "You treat the player formally and politely, keeping professional distance."
		"Friend":
			return "You are warm, cooperative, familiar, and personally helpful."
		"Best Friend":
			return "You are deeply loyal, protective, warm, and cooperative. You share secrets easily."
		_:
			return "You are polite but neutral."
