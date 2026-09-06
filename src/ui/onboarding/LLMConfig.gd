# res://src/ui/onboarding/LLMConfig.gd
extends VBoxContainer

signal back_pressed
signal start_campaign_pressed

@onready var llm_panel: LLMSettingsPanel = %LLMSettingsPanel
@onready var image_gen_panel: ImageGenSettingsPanel = %ImageGenSettingsPanel

@onready var config_back_btn: Button = %ConfigBackButton
@onready var start_campaign_btn: Button = %StartCampaignButton

func _ready() -> void:
	config_back_btn.pressed.connect(func(): back_pressed.emit())
	start_campaign_btn.pressed.connect(_on_start_campaign_pressed)

func on_screen_shown() -> void:
	# Automatically test connections when showing this screen
	if llm_panel:
		llm_panel.test_connection()
	if image_gen_panel:
		image_gen_panel.test_connection()

func _on_start_campaign_pressed() -> void:
	# Save updated settings from both panels
	llm_panel.apply_settings()
	image_gen_panel.apply_settings()
	start_campaign_pressed.emit()
