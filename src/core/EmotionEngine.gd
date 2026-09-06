# res://src/core/EmotionEngine.gd
extends RefCounted
class_name EmotionEngine

signal character_visual_update_requested(char_id: String, emotion: String, affinity: float)
signal sidebar_refresh_requested()

var decay_rate: float = 0.05

## Processes emotional update tags returned by the LLM
func process_response_tags(char_id: String, tags: Dictionary) -> void:
	if char_id.is_empty() or tags.is_empty():
		return
		
	var emotion = str(tags.get("emotion", "serenity")).to_lower().strip_edges()
	var intensity = clamp(float(tags.get("intensity", 0.5)), 0.0, 1.0)
	var target = str(tags.get("target", "player")).to_lower().strip_edges()
	var reason = str(tags.get("reason", ""))
	var rapport_delta = clamp(float(tags.get("rapport_delta", tags.get("affinity_delta", 0.0))), -0.2, 0.2)
	
	# Validate emotion type
	var valid_emotions = ["serenity", "joy", "sadness", "anger", "fear", "trust", "disgust", "surprise"]
	if not valid_emotions.has(emotion):
		# Default fallback
		emotion = "serenity"
		
	# Apply state changes to memory and affinity
	CampaignState.adjust_affinity(char_id, rapport_delta)
	CampaignState.add_emotion_event(char_id, emotion, intensity, target, reason, rapport_delta)
	
	# Check for no-op delta (RAG006)
	var character = CampaignState.get_character(char_id)
	var emotions = character.get("emotions", [])
	var current_emotion = "serenity"
	var current_intensity = 0.0
	
	# The last event we just appended is at index -1, so check the previous one if it exists
	if emotions.size() > 1:
		var prev_event = emotions[-2]
		current_emotion = prev_event.get("emotion", "serenity")
		current_intensity = float(prev_event.get("intensity", 0.0))
	
	if emotion == current_emotion and abs(intensity - current_intensity) < 0.05:
		print("[EmotionEngine] No emotional delta for %s — skipping visual update" % char_id)
	else:
		# Notify of updated visuals
		var updated_affinity = character.get("affinity", 0.0)
		character_visual_update_requested.emit(char_id, emotion, updated_affinity)

## Coordinates deducing a character's base emotion asynchronously from biography
func deduce_base_emotion_if_needed(char_id: String, char_data: Dictionary) -> void:
	var base_emo = char_data.get("base_emotion", "")
	var base_int = float(char_data.get("base_intensity", -1.0))
	
	if not base_emo.is_empty() and base_int >= 0.0:
		# Already explicitly specified in the campaign/frontmatter
		return
		
	var char_name = char_data.get("name", char_id.capitalize())
	var biography = char_data.get("biography", "")
	
	if biography.strip_edges().is_empty():
		CampaignState.add_emotion_event(char_id, "serenity", 0.5, "player", "Default baseline (no biography provided).", 0.0)
		return
		
	var prompt = SystemPrompts.get_deduce_base_emotion_prompt(char_name, biography)
	print("[SYSTEM] Deducing base emotion for character: %s..." % char_id)
	
	LLMClient.send_custom_request(prompt, LLMClient.character_model, func(success: bool, response_text: String, error_msg: String):
		var deduced_emo = "serenity"
		var deduced_int = 0.5
		var reason = "Default fallback (analysis failed)."
		
		if success:
			var parsed = JsonRepair.extract_json(response_text)
			if parsed.has("base_emotion") and not str(parsed["base_emotion"]).is_empty():
				var valid_emotions = ["serenity", "joy", "sadness", "anger", "fear", "trust", "disgust", "surprise"]
				var emotion = str(parsed["base_emotion"]).to_lower().strip_edges()
				if valid_emotions.has(emotion):
					deduced_emo = emotion
					deduced_int = clamp(float(parsed.get("base_intensity", 0.5)), 0.0, 1.0)
					reason = "Deduced from biography: %s" % deduced_emo.capitalize()
					
		# Save deduced stats on character
		var character = CampaignState.get_character(char_id)
		if not character.is_empty():
			character["base_emotion"] = deduced_emo
			character["base_intensity"] = deduced_int
			
		CampaignState.add_emotion_event(char_id, deduced_emo, deduced_int, "player", reason, 0.0)
		CampaignState.save()
		
		# Notify UI of updated visuals and sidebar refresh
		character_visual_update_requested.emit(char_id, deduced_emo, character.get("affinity", 0.0))
		sidebar_refresh_requested.emit()
	)

## Triggers and coordinates the asynchronous emotion reflection loop
func trigger_emotion_reflection(char_id: String, narration_text: String) -> void:
	if char_id.is_empty():
		return
		
	var prompt = PromptBuilder.get_emotion_reflection_prompt_for_id(char_id, narration_text)
	if prompt.is_empty():
		return
		
	print("[SYSTEM] Character %s is reflecting on the narrative beat..." % char_id)
	
	LLMClient.send_custom_request(prompt, LLMClient.character_model, func(success: bool, response_text: String, error_msg: String):
		if not success:
			print("[SYSTEM] Emotion reflection failed for %s: %s" % [char_id, error_msg])
			return
			
		var parsed = JsonRepair.extract_json(response_text)
		var emotional_update = parsed.get("emotional_update", {})
		if not emotional_update.is_empty():
			process_response_tags(char_id, emotional_update)
			
			# Log it
			var character = CampaignState.get_character(char_id)
			var friendly_char_name = character.get("name", char_id.capitalize()) if not character.is_empty() else char_id.capitalize()
			
			var emotion_name = str(emotional_update.get("emotion", "serenity")).capitalize()
			var intensity_val = clamp(float(emotional_update.get("intensity", 0.5)), 0.0, 1.0)
			var delta_val = clamp(float(emotional_update.get("rapport_delta", emotional_update.get("affinity_delta", 0.0))), -0.2, 0.2)
			var reason_str = str(emotional_update.get("reason", ""))
			
			var delta_str = "+%.2f" % delta_val if delta_val > 0 else ("%.2f" % delta_val if delta_val < 0 else "0.0")
			var msg = "%s reflects on environment: %s (intensity: %.1f) | Rapport: %s" % [
				friendly_char_name, emotion_name, intensity_val, delta_str
			]
			if not reason_str.is_empty():
				msg += "\nReason: %s" % reason_str
			print("[SYSTEM] " + msg)
			CampaignState.save()
			
			# Notify UI of updated visuals and sidebar refresh
			var updated_affinity = character.get("affinity", 0.0)
			character_visual_update_requested.emit(char_id, emotional_update.get("emotion", "serenity"), updated_affinity)
			sidebar_refresh_requested.emit()
	)

## Slowly decays character emotions towards a neutral baseline ("serenity" at intensity 0.0)
## If 'reinforced_char_id' is provided, we skip decay for that character.
func decay_emotions(reinforced_char_id: String = "", is_location_change: bool = false) -> void:
	var char_ids = CampaignState.get_character_ids()
	var updated_any = false
	
	for char_id in char_ids:
		if char_id == "player":
			continue
		if char_id == reinforced_char_id:
			continue
			
		var character = CampaignState.get_character(char_id)
		var emotions = character.get("emotions", [])
		if emotions.is_empty():
			continue
			
		var last_event = emotions[-1]
		var current_emotion = last_event.get("emotion", "serenity")
		var current_intensity = float(last_event.get("intensity", 0.5))
		
		# If intensity is already 0, and emotion is serenity, no decay needed
		if current_intensity <= 0.0 and current_emotion == "serenity":
			continue
			
		# Decay the intensity towards 0.0
		var new_intensity = max(current_intensity - decay_rate, 0.0)
		if new_intensity < 0.0001:
			new_intensity = 0.0
		var new_emotion = current_emotion
		
		# If intensity hits 0, the emotion fades to serenity
		if new_intensity <= 0.0:
			new_emotion = "serenity"
			
		var target = last_event.get("target", "player")
		var context = "Emotional decay over time."
		if is_location_change:
			context = "Emotional decay due to location change."
			
		CampaignState.add_emotion_event(char_id, new_emotion, new_intensity, target, context, 0.0)
		updated_any = true
		
		# Notify UI of updated visuals
		var updated_affinity = character.get("affinity", 0.0)
		character_visual_update_requested.emit(char_id, new_emotion, updated_affinity)
		
	if updated_any:
		CampaignState.save()
		sidebar_refresh_requested.emit()
