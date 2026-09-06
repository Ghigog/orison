# res://src/ui/VirtualScrollContainer.gd
extends ScrollContainer
class_name VirtualScrollContainer

signal meta_clicked(meta: Variant)

const ChatMessageRowScene = preload("res://scenes/ui/ChatMessageRow.tscn")

# Scroll state
var scroll_following: bool = true
var _is_programmatic_scroll: bool = false

# Message data
var _messages: Array = []
var _is_light: bool = false

# Height / position caching
var _msg_heights: Array[float] = []
var _msg_is_measured: Array[bool] = []
var _msg_y_positions: Array[float] = []
var _total_height: float = 0.0

# Node pool
var _active_nodes: Dictionary = {} # index -> ChatMessageRow
var _recycled_nodes: Array = []

# Viewport content container
var _content_node: Control = null

# Configuration
var buffer_messages: int = 10
var default_msg_height: float = 50.0

func _ready() -> void:
	# Configure scroll modes
	vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	
	if get_child_count() > 0:
		_content_node = get_child(0) as Control
		
	if not _content_node:
		_content_node = Control.new()
		add_child(_content_node)
		
	_content_node.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	
	# Connect signals
	var v_scroll = get_v_scroll_bar()
	if v_scroll:
		v_scroll.value_changed.connect(_on_scroll_changed)
		
	resized.connect(_on_resized)
	_on_resized()

func _exit_tree() -> void:
	# Clean up pooled nodes to prevent memory leaks
	for node in _active_nodes.values():
		if is_instance_valid(node):
			node.queue_free()
	_active_nodes.clear()
	
	for node in _recycled_nodes:
		if is_instance_valid(node):
			node.queue_free()
	_recycled_nodes.clear()

func set_messages(new_messages: Array, is_light: bool) -> void:
	_is_light = is_light
	
	var old_size = _messages.size()
	_messages = new_messages
	
	var new_size = _messages.size()
	_msg_heights.resize(new_size)
	_msg_is_measured.resize(new_size)
	
	for i in range(new_size):
		# Re-estimate if new, not measured, or if it is the very last element (which might be streaming chunks)
		if i >= old_size or not _msg_is_measured[i] or i == new_size - 1:
			_msg_heights[i] = _estimate_message_height(_messages[i])
			_msg_is_measured[i] = false
			
	_rebuild_y_positions()
	_update_visible_range()
	
	if scroll_following:
		call_deferred("_scroll_to_bottom")

func clear_messages() -> void:
	_messages.clear()
	_msg_heights.clear()
	_msg_is_measured.clear()
	_msg_y_positions.clear()
	_total_height = 0.0
	
	if _content_node:
		_content_node.custom_minimum_size.y = 0.0
		
	for idx in _active_nodes.keys():
		_recycle_node(idx)
	_active_nodes.clear()
	
	scroll_following = true

func _estimate_message_height(msg: Dictionary) -> float:
	var text = msg.get("text", "")
	var sender = msg.get("sender", "")
	var type = msg.get("type", "chat")
	
	var header_text = sender
	if type == "system" or type == "warning" or type == "error":
		header_text = "ℹ️ [SYSTEM]"
		
	# Split paragraphs to estimate height line-by-line
	var paragraphs = text.split("\n")
	var est_lines = 0
	
	var W = size.x - 60.0 # accounting for left/right margins + padding
	if W <= 100.0:
		W = 800.0 # default fallback
		
	var chars_per_line = max(15.0, W / 9.0) # ~9px per character on average for 18px font
	
	for p in paragraphs:
		var p_len = p.length()
		if p_len == 0:
			est_lines += 1
		else:
			# add header characters to first paragraph estimate if it is chat
			var segment_len = p_len
			if p == paragraphs[0] and type == "chat":
				segment_len += header_text.length() + 3
			est_lines += int(ceil(float(segment_len) / chars_per_line))
			
	var line_h = 28.0 # font size 18px with 1.6 line height
	var padding = 16.0 # margins
	
	return max(default_msg_height, est_lines * line_h + padding)

func _rebuild_y_positions() -> void:
	_msg_y_positions.clear()
	var current_y = 0.0
	for i in range(_messages.size()):
		_msg_y_positions.append(current_y)
		current_y += _msg_heights[i]
	_total_height = current_y
	
	if _content_node:
		_content_node.custom_minimum_size.y = _total_height

func _update_visible_range() -> void:
	if _messages.is_empty():
		return
		
	var scroll_y = scroll_vertical
	var view_h = size.y
	if view_h <= 0:
		view_h = 200.0 # default fallback
		
	var start_y = scroll_y - view_h # buffer: 1 viewport height above
	var end_y = scroll_y + view_h * 2.0 # buffer: 1 viewport height below
	
	var start_idx = -1
	var end_idx = -1
	
	# Linear scan is perfectly fast for hundreds of entries
	for i in range(_messages.size()):
		var msg_y = _msg_y_positions[i]
		var msg_h = _msg_heights[i]
		
		if msg_y + msg_h >= start_y and msg_y <= end_y:
			if start_idx == -1:
				start_idx = i
			end_idx = i
			
	if start_idx == -1:
		start_idx = 0
		end_idx = min(10, _messages.size() - 1)
		
	# Apply item-count buffer
	start_idx = max(0, start_idx - buffer_messages)
	end_idx = min(_messages.size() - 1, end_idx + buffer_messages)
	
	# Recycle nodes no longer in view
	var to_recycle = []
	for idx in _active_nodes.keys():
		if idx < start_idx or idx > end_idx:
			to_recycle.append(idx)
			
	for idx in to_recycle:
		_recycle_node(idx)
		
	# Bind or update nodes in view
	for i in range(start_idx, end_idx + 1):
		_bind_node(i)

func _recycle_node(idx: int) -> void:
	var node = _active_nodes[idx]
	_active_nodes.erase(idx)
	node.visible = false
	_recycled_nodes.append(node)

func _bind_node(i: int) -> void:
	var node: Control = null
	if _active_nodes.has(i):
		node = _active_nodes[i]
	else:
		if not _recycled_nodes.is_empty():
			node = _recycled_nodes.pop_back()
		else:
			node = ChatMessageRowScene.instantiate()
			node.anchor_left = 0.0
			node.anchor_right = 0.0
			node.anchor_top = 0.0
			node.anchor_bottom = 0.0
			_content_node.add_child(node)
			node.meta_clicked.connect(_on_node_meta_clicked)
		_active_nodes[i] = node
		
	node.visible = true
	node.set_message(_messages[i], _is_light)
	node.position = Vector2(0, _msg_y_positions[i])
	node.size.x = _content_node.size.x
	
	# Force immediate layout pass to measure exact size
	node.custom_minimum_size = Vector2(node.size.x, 0.0)
	node.size = node.get_combined_minimum_size()
	var actual_h = node.size.y
	
	# Check if measurement changed from cached height
	if not _msg_is_measured[i] or abs(actual_h - _msg_heights[i]) > 0.5:
		var diff = actual_h - _msg_heights[i]
		_msg_heights[i] = actual_h
		_msg_is_measured[i] = true
		
		# Shift all subsequent messages
		for j in range(i + 1, _messages.size()):
			_msg_y_positions[j] += diff
		_total_height += diff
		_content_node.custom_minimum_size.y = _total_height
		
		# Prevent scroll jump if this node is above the current scroll offset
		if _msg_y_positions[i] < scroll_vertical:
			_is_programmatic_scroll = true
			scroll_vertical += int(diff)
			_is_programmatic_scroll = false
			
		# Shift position of other active nodes below this one
		for active_idx in _active_nodes.keys():
			if active_idx > i:
				_active_nodes[active_idx].position.y += diff

func _on_node_meta_clicked(meta: Variant) -> void:
	meta_clicked.emit(meta)

func _on_scroll_changed(value: float) -> void:
	if not _is_programmatic_scroll:
		var v_scroll = get_v_scroll_bar()
		if v_scroll:
			# Re-evaluate scroll follow: true if user is scrolled to the bottom
			var is_at_bottom = value >= (v_scroll.max_value - v_scroll.page - 10.0)
			if is_at_bottom:
				scroll_following = true
			else:
				scroll_following = false
	_update_visible_range()

func _on_resized() -> void:
	if not _content_node:
		return
		
	var scrollbar_w = 0.0
	var v_scroll = get_v_scroll_bar()
	if v_scroll and v_scroll.visible:
		scrollbar_w = v_scroll.size.x
		
	_content_node.size.x = size.x - scrollbar_w
	
	# Invalidate height cache on resize since word wrapping width changed
	for i in range(_msg_is_measured.size()):
		_msg_is_measured[i] = false
		_msg_heights[i] = _estimate_message_height(_messages[i])
		
	_rebuild_y_positions()
	_update_visible_range()
	
	if scroll_following:
		call_deferred("_scroll_to_bottom")

func _scroll_to_bottom() -> void:
	var v_scroll = get_v_scroll_bar()
	if v_scroll:
		_is_programmatic_scroll = true
		v_scroll.value = v_scroll.max_value - v_scroll.page
		_is_programmatic_scroll = false
