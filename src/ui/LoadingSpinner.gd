# res://src/ui/LoadingSpinner.gd
extends Control
class_name LoadingSpinner

@export var rotation_speed: float = 3.0
@export var color: Color = Color("#A59EBF") # Matches Horizon Grey / subtle purple theme
@export var line_width: float = 3.0
@export var radius: float = 14.0

var current_angle: float = 0.0

func _ready() -> void:
	custom_minimum_size = Vector2(40, 40)

func _process(delta: float) -> void:
	if visible:
		current_angle += rotation_speed * delta
		if current_angle > TAU:
			current_angle -= TAU
		queue_redraw()

func _draw() -> void:
	var center = size / 2.0
	# Draw background subtle circle track
	draw_arc(center, radius, 0.0, TAU, 32, Color(color.r, color.g, color.b, 0.15), line_width, true)
	# Draw active spinning arc (approx. 270 degrees)
	draw_arc(center, radius, current_angle, current_angle + PI * 1.5, 32, color, line_width, true)
