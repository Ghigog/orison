# res://src/ui/ConnectionListItem.gd
extends HBoxContainer

signal delete_pressed

@onready var text_label: Label = %TextLabel
@onready var delete_button: Button = %DeleteButton

func _ready() -> void:
	delete_button.pressed.connect(func(): delete_pressed.emit())

func setup(text_content: String) -> void:
	text_label.text = text_content
