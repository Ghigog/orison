# res://src/core/EmotionEngine.gd
extends RefCounted
class_name EmotionEngine

## Processes emotional update tags returned by the LLM
func process_response_tags(char_id: String, tags: Dictionary) -> void:
	if char_id.is_empty():
		return
		
	var emotion = str(tags.get("emotion", "serenity")).to_lower().strip_edges()
	var intensity = float(tags.get("intensity", 0.5))
	var target = str(tags.get("target", "player")).to_lower().strip_edges()
	var reason = str(tags.get("reason", ""))
	var rapport_delta = float(tags.get("rapport_delta", tags.get("affinity_delta", 0.0)))
	
	# Validate emotion type
	var valid_emotions = ["serenity", "joy", "sadness", "anger", "fear", "trust", "disgust", "surprise"]
	if not valid_emotions.has(emotion):
		# Default fallback
		emotion = "serenity"
		
	# Apply state changes to memory and affinity
	CampaignState.add_emotion_event(char_id, emotion, intensity, target, reason)
	CampaignState.adjust_affinity(char_id, rapport_delta)
