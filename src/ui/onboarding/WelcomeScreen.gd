# res://src/ui/onboarding/WelcomeScreen.gd
extends VBoxContainer

signal start_new_pressed
signal load_pressed
signal settings_pressed

@onready var start_new_btn: Button = %StartNewButton
@onready var load_btn: Button = %LoadButton
@onready var settings_btn: Button = %SettingsButton

func _ready() -> void:
	start_new_btn.pressed.connect(func(): start_new_pressed.emit())
	load_btn.pressed.connect(func(): load_pressed.emit())
	settings_btn.pressed.connect(func(): settings_pressed.emit())
