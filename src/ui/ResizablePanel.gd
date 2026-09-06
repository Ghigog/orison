# res://src/ui/ResizablePanel.gd
extends PanelContainer
class_name ResizablePanel

const RESIZE_BORDER = 12
const MIN_SIZE = Vector2(400, 150)
const MAX_SIZE = Vector2(1600, 800)

var dragging = false
var drag_mode = 0
var drag_offset = Vector2.ZERO

enum DragMode {
	NONE = 0,
	LEFT = 1,
	RIGHT = 2,
	TOP = 4,
	MOVE = 8
}

func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_STOP
	# Load saved size from client configuration
	custom_minimum_size = Vector2(LLMClient.chat_box_width, LLMClient.chat_box_height)
	
	# Connect drag handle if present
	var drag_handle = get_node_or_null("%DragHandle")
	if drag_handle:
		drag_handle.gui_input.connect(_on_drag_handle_gui_input)
		
	# Restore position if valid
	if LLMClient.chat_box_position_x >= 0 and LLMClient.chat_box_position_y >= 0:
		anchor_left = 0.0
		anchor_right = 0.0
		anchor_top = 0.0
		anchor_bottom = 0.0
		
		# Wait for layout to settle, then set and clamp position
		await get_tree().process_frame
		var saved_pos = Vector2(LLMClient.chat_box_position_x, LLMClient.chat_box_position_y)
		var parent_size = get_parent_control().size if get_parent_control() else get_viewport_rect().size
		saved_pos.x = clamp(saved_pos.x, 0, parent_size.x - size.x)
		saved_pos.y = clamp(saved_pos.y, 0, parent_size.y - size.y)
		global_position = saved_pos

func _process(_delta: float) -> void:
	if not dragging:
		var local_mouse = get_local_mouse_position()
		var mode = _get_drag_mode_at(local_mouse)
		_update_cursor_shape(mode)

func _get_drag_mode_at(pos: Vector2) -> int:
	var mode = 0
	if pos.x >= 0 and pos.x <= size.x and pos.y >= 0 and pos.y <= size.y:
		if pos.x < RESIZE_BORDER:
			mode |= DragMode.LEFT
		elif pos.x > size.x - RESIZE_BORDER:
			mode |= DragMode.RIGHT
		
		if pos.y < RESIZE_BORDER:
			mode |= DragMode.TOP
	return mode

func _update_cursor_shape(mode: int) -> void:
	if mode == DragMode.NONE:
		mouse_default_cursor_shape = Control.CURSOR_ARROW
	elif mode == DragMode.TOP:
		mouse_default_cursor_shape = Control.CURSOR_VSIZE
	elif mode == (DragMode.LEFT | DragMode.TOP):
		mouse_default_cursor_shape = Control.CURSOR_FDIAGSIZE
	elif mode == (DragMode.RIGHT | DragMode.TOP):
		mouse_default_cursor_shape = Control.CURSOR_BDIAGSIZE
	elif mode == DragMode.LEFT or mode == DragMode.RIGHT:
		mouse_default_cursor_shape = Control.CURSOR_HSIZE
	else:
		mouse_default_cursor_shape = Control.CURSOR_ARROW

func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				var mode = _get_drag_mode_at(event.position)
				if mode != DragMode.NONE:
					_convert_to_absolute_position()
					dragging = true
					drag_mode = mode
					accept_event()
				else:
					# Click on empty background / margins triggers drag to move
					_convert_to_absolute_position()
					dragging = true
					drag_mode = DragMode.MOVE
					drag_offset = event.global_position - global_position
					accept_event()

func _on_drag_handle_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_convert_to_absolute_position()
				dragging = true
				drag_mode = DragMode.MOVE
				drag_offset = event.global_position - global_position
				accept_event()

func _convert_to_absolute_position() -> void:
	var current_global_pos = global_position
	anchor_left = 0.0
	anchor_right = 0.0
	anchor_top = 0.0
	anchor_bottom = 0.0
	grow_horizontal = GROW_DIRECTION_BOTH
	grow_vertical = GROW_DIRECTION_BOTH
	global_position = current_global_pos

func _input(event: InputEvent) -> void:
	if not is_visible_in_tree():
		return
		
	if not dragging:
		if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
			var local_mouse = get_local_mouse_position()
			var mode = _get_drag_mode_at(local_mouse)
			if mode != DragMode.NONE:
				_convert_to_absolute_position()
				dragging = true
				drag_mode = mode
				get_viewport().set_input_as_handled()
		return
		
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_LEFT and not event.pressed:
			dragging = false
			drag_mode = DragMode.NONE
			# Save to config and persist
			LLMClient.chat_box_width = custom_minimum_size.x
			LLMClient.chat_box_height = custom_minimum_size.y
			if anchor_left == 0.0 and anchor_top == 0.0:
				LLMClient.chat_box_position_x = global_position.x
				LLMClient.chat_box_position_y = global_position.y
			LLMClient.save_config()
			get_viewport().set_input_as_handled()
			
	elif event is InputEventMouseMotion:
		if drag_mode == DragMode.MOVE:
			if anchor_left != 0.0 or anchor_top != 0.0:
				_convert_to_absolute_position()
			
			var parent_size = get_parent_control().size if get_parent_control() else get_viewport_rect().size
			var target_pos = event.global_position - drag_offset
			target_pos.x = clamp(target_pos.x, 0, parent_size.x - size.x)
			target_pos.y = clamp(target_pos.y, 0, parent_size.y - size.y)
			global_position = target_pos
			get_viewport().set_input_as_handled()
		else:
			if anchor_left != 0.0 or anchor_top != 0.0:
				_convert_to_absolute_position()
				
			var delta = event.relative
			var prev_size = custom_minimum_size
			var new_size = custom_minimum_size
			
			if drag_mode & DragMode.TOP:
				new_size.y -= delta.y
				
			if drag_mode & DragMode.LEFT:
				new_size.x -= delta.x
			elif drag_mode & DragMode.RIGHT:
				new_size.x += delta.x
				
			new_size.x = clamp(new_size.x, MIN_SIZE.x, MAX_SIZE.x)
			new_size.y = clamp(new_size.y, MIN_SIZE.y, MAX_SIZE.y)
			
			if anchor_left == 0.0 and anchor_top == 0.0:
				if drag_mode & DragMode.LEFT:
					var size_diff = new_size.x - prev_size.x
					global_position.x -= size_diff
				if drag_mode & DragMode.TOP:
					var size_diff = new_size.y - prev_size.y
					global_position.y -= size_diff
					
			custom_minimum_size = new_size
			get_viewport().set_input_as_handled()
