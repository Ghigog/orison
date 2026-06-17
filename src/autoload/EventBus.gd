# res://src/autoload/EventBus.gd
extends Node

signal campaign_loaded(campaign_id: String)
signal character_selected(character_id: String)
signal emotion_updated(character_id: String, emotion: String, affinity: float)
signal dialogue_streamed(role: String, content: String)
