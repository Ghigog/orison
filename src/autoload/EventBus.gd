# res://src/autoload/EventBus.gd
extends Node

signal campaign_loaded(campaign_id: String)
signal character_selected(character_id: String)
signal emotion_updated(character_id: String, emotion: String, affinity: float)
signal dialogue_streamed(role: String, content: String)

signal location_changed(location_id: String)
signal campaign_saved()
signal turn_started()
signal turn_completed()
signal director_prompt_generated(prompt: String)
signal character_state_updated(character_id: String)
signal scan_progress(current: int, total: int)
signal character_speaking(character_id: String)

## Emitted when an assembled prompt exceeds its token budget. The model will
## silently drop the oldest context (the system prompt) when this happens, so
## the UI should surface it rather than let it pass unnoticed.
signal prompt_budget_exceeded(role: String, tokens: int, limit: int)

