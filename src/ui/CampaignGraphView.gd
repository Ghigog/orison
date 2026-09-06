# res://src/ui/CampaignGraphView.gd
extends HSplitContainer
class_name CampaignGraphView

@onready var graph_edit: GraphEdit = %GraphEdit
@onready var search_input: LineEdit = %SearchInput
@onready var toggle_location: Button = %ToggleLocation
@onready var toggle_lore: Button = %ToggleLore
@onready var toggle_scene: Button = %ToggleScene
@onready var toggle_character: Button = %ToggleCharacter
@onready var zoom_out_btn: Button = %ZoomOutBtn
@onready var zoom_reset_btn: Button = %ZoomResetBtn
@onready var zoom_in_btn: Button = %ZoomInBtn
@onready var node_count_label: Label = %NodeCountLabel

@onready var inspector_panel: PanelContainer = $InspectorPanel
@onready var node_name_label: Label = %NodeNameLabel
@onready var type_dropdown: OptionButton = %TypeDropdown
@onready var content_preview: RichTextLabel = %ContentPreview
@onready var connections_list: VBoxContainer = %ConnectionsList
@onready var no_connections_label: Label = %NoConnectionsLabel
@onready var starting_loc_dropdown: OptionButton = %StartingLocDropdown
@onready var starting_char_dropdown: OptionButton = %StartingCharDropdown

var _last_zoom: float = 1.0

# Dropdowns for adding new connections
@onready var connect_target_dropdown: OptionButton = %ConnectTargetDropdown
@onready var connect_relation_dropdown: OptionButton = %ConnectRelationDropdown
@onready var add_connection_btn: Button = %AddConnectionButton
@onready var auto_arrange_btn: Button = %AutoArrangeButton

var _graph_data: Dictionary = {}
var _nodes: Dictionary = {}
var _edges: Array = []
var _selected_node_id: String = ""
var _starting_location_id: String = ""
var _starting_character_id: String = ""

const CATEGORIES = [
	{"id": "scene", "label": "Scene", "color": Color(1.0, 0.373, 0.220), "icon": "🎬"},
	{"id": "character", "label": "Character", "color": Color(0.957, 0.247, 0.369), "icon": "👤"},
	{"id": "location", "label": "Location", "color": Color(0.96, 0.62, 0.04), "icon": "📍"},
	{"id": "lore", "label": "Lore", "color": Color(0.624, 0.525, 0.753), "icon": "📚"}
]

const RELATION_TYPES = [
	"connected_to",
	"associated_with",
	"relationships",
	"leads_to",
	"friend",
	"enemy",
	"ally"
]

func _ready() -> void:
	# Connect signals
	graph_edit.connection_request.connect(_on_connection_request)
	graph_edit.disconnection_request.connect(_on_disconnection_request)
	graph_edit.node_selected.connect(_on_node_selected)
	graph_edit.node_deselected.connect(_on_node_deselected)
	
	type_dropdown.item_selected.connect(_on_type_selected)
	starting_loc_dropdown.item_selected.connect(_on_starting_loc_selected)
	starting_char_dropdown.item_selected.connect(_on_starting_char_selected)
	add_connection_btn.pressed.connect(_on_add_connection_pressed)
	auto_arrange_btn.pressed.connect(_on_auto_arrange_pressed)
	
	# Connect filter and search signals
	search_input.text_changed.connect(_on_filter_changed)
	toggle_location.toggled.connect(_on_filter_changed)
	toggle_lore.toggled.connect(_on_filter_changed)
	toggle_scene.toggled.connect(_on_filter_changed)
	toggle_character.toggled.connect(_on_filter_changed)
	
	# Connect zoom signals
	zoom_in_btn.pressed.connect(_on_zoom_in_pressed)
	zoom_out_btn.pressed.connect(_on_zoom_out_pressed)
	zoom_reset_btn.pressed.connect(_on_zoom_reset_pressed)
	
	# Populate type dropdown
	type_dropdown.clear()
	for i in range(CATEGORIES.size()):
		var cat = CATEGORIES[i]
		type_dropdown.add_item("%s %s" % [cat.icon, cat.label])
		type_dropdown.set_item_metadata(i, cat.id)
		
	# Populate relation types dropdown
	connect_relation_dropdown.clear()
	for i in range(RELATION_TYPES.size()):
		connect_relation_dropdown.add_item(RELATION_TYPES[i])
		
	# Initial UI state
	inspector_panel.visible = true
	_update_inspector_ui()
	_last_zoom = graph_edit.zoom
	_update_zoom_label()

## Initializes the view with compiled graph data
func initialize(graph_data: Dictionary, start_loc_id: String = "", start_char_id: String = "") -> void:
	_graph_data = graph_data
	
	# Extract nodes and edges
	_nodes = _graph_data.get("nodes", {}).duplicate(true)
	_edges = _graph_data.get("edges", []).duplicate(true)
	
	_starting_location_id = start_loc_id
	_starting_character_id = start_char_id
	
	_clear_graph()
	_render_nodes()
	_render_connections()
	_update_starting_dropdowns()
	_update_inspector_ui()
	_apply_filters()

## Returns the modified graph and starting selections
func get_graph_data() -> Dictionary:
	return {
		"nodes": _nodes,
		"edges": _edges,
		"starting_location_id": _starting_location_id,
		"starting_character_id": _starting_character_id
	}

# ==============================================================================
# Rendering
# ==============================================================================

func _clear_graph() -> void:
	for connection in graph_edit.get_connection_list():
		graph_edit.disconnect_node(
			connection.from_node,
			connection.from_port,
			connection.to_node,
			connection.to_port
		)
		
	for child in graph_edit.get_children():
		if child is GraphNode:
			child.queue_free()
			
	_selected_node_id = ""

func _render_nodes() -> void:
	_create_header_nodes()
	
	var target_positions = _compute_layout()
	for node_id in _nodes.keys():
		var node_data = _nodes[node_id]
		var pos = target_positions.get(node_id, Vector2(350, 350))
		_create_node_element(node_id, node_data, pos)
		
	# Render global anchor card if it exists in layout positions
	if target_positions.has("global_anchor"):
		var dummy_data = {
			"label": "Campaign-Wide",
			"type": "location",
			"desc": "Entities not associated with any specific location."
		}
		_create_node_element("global_anchor", dummy_data, target_positions["global_anchor"])

func _create_header_nodes() -> void:
	var headers = [
		{"id": "header_location", "title": "📍 LOCATIONS", "type": "location", "x": 320},
		{"id": "header_lore", "title": "📚 LORE", "type": "lore", "x": 520},
		{"id": "header_scene", "title": "🎬 SCENES", "type": "scene", "x": 720},
		{"id": "header_character", "title": "👤 CHARACTERS", "type": "character", "x": 920}
	]
	
	for h in headers:
		var gnode = GraphNode.new()
		gnode.name = h.id
		gnode.title = h.title
		gnode.position_offset = Vector2(h.x, 20)
		gnode.selectable = false
		gnode.draggable = false
		gnode.custom_minimum_size = Vector2(170, 0)
		
		var color = Color(0.5, 0.5, 0.5)
		for cat in CATEGORIES:
			if cat.id == h.type:
				color = cat.color
				break
				
		gnode.add_theme_color_override("title_color", Color(1.0, 1.0, 1.0))
		gnode.set_slot(0, false, 0, color, false, 0, color)
		
		var lbl = Label.new()
		lbl.text = "Column"
		lbl.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		lbl.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
		gnode.add_child(lbl)
		
		graph_edit.add_child(gnode)

## Calculates layout positions using a deterministic location-anchored 2D table layout
func _compute_layout() -> Dictionary:
	var positions = {}
	var node_ids = _nodes.keys()
	if node_ids.is_empty():
		return positions
		
	# Find all locations
	var locations = []
	for node_id in node_ids:
		if _nodes[node_id].get("type") == "location":
			locations.append(node_id)
			
	# Sort locations alphabetically for stable row order
	locations.sort_custom(func(a, b):
		var label_a = _nodes[a].get("label", a).to_lower()
		var label_b = _nodes[b].get("label", b).to_lower()
		return label_a < label_b
	)
	
	# Resolve node-to-location mappings based on edges
	var node_to_location = {}
	for loc_id in locations:
		node_to_location[loc_id] = loc_id
		
	# Direct edges connection pass
	for edge in _edges:
		var from_id = edge.get("from", "")
		var to_id = edge.get("to", "")
		if _nodes.has(from_id) and _nodes.has(to_id):
			var type_from = _nodes[from_id].get("type", "lore")
			var type_to = _nodes[to_id].get("type", "lore")
			
			if type_from == "location" and type_to != "location":
				if not node_to_location.has(to_id):
					node_to_location[to_id] = from_id
			elif type_to == "location" and type_from != "location":
				if not node_to_location.has(from_id):
					node_to_location[from_id] = to_id
					
	# Indirect edges propagation pass (1-hop check)
	for pass_idx in range(2):
		for edge in _edges:
			var from_id = edge.get("from", "")
			var to_id = edge.get("to", "")
			if _nodes.has(from_id) and _nodes.has(to_id):
				if node_to_location.has(from_id) and not node_to_location.has(to_id):
					if _nodes[to_id].get("type") != "location":
						node_to_location[to_id] = node_to_location[from_id]
				elif node_to_location.has(to_id) and not node_to_location.has(from_id):
					if _nodes[from_id].get("type") != "location":
						node_to_location[from_id] = node_to_location[to_id]
						
	# Initialize rows for each location
	var rows = {}
	for loc_id in locations:
		rows[loc_id] = {
			"location": [loc_id],
			"lore": [],
			"scene": [],
			"character": []
		}
		
	var global_row = {
		"location": [],
		"lore": [],
		"scene": [],
		"character": []
	}
	
	# Sort all non-location node IDs alphabetically for stable cell ordering
	var non_locations = []
	for node_id in node_ids:
		if _nodes[node_id].get("type") != "location":
			non_locations.append(node_id)
			
	non_locations.sort_custom(func(a, b):
		var label_a = _nodes[a].get("label", a).to_lower()
		var label_b = _nodes[b].get("label", b).to_lower()
		return label_a < label_b
	)
	
	# Populate rows with sorted nodes
	for node_id in non_locations:
		var type = _nodes[node_id].get("type", "lore")
		if type == "npc":
			type = "character"
		if not ["lore", "scene", "character"].has(type):
			type = "lore"
			
		var assoc_loc = node_to_location.get(node_id, "")
		if assoc_loc != "" and rows.has(assoc_loc):
			rows[assoc_loc][type].append(node_id)
		else:
			global_row[type].append(node_id)
			
	# Append global row if it has any elements
	var row_keys = locations.duplicate()
	var has_global = not global_row.lore.is_empty() or not global_row.scene.is_empty() or not global_row.character.is_empty()
	if has_global:
		row_keys.append("__global__")
		rows["__global__"] = global_row
		
	# X-coordinates for the 4 columns
	var col_x = {
		"location": 320.0,
		"lore": 520.0,
		"scene": 720.0,
		"character": 920.0
	}
	
	var current_y = 100.0
	for r_key in row_keys:
		var r = rows[r_key]
		var loc_nodes = r["location"]
		if r_key == "__global__":
			loc_nodes = ["global_anchor"]
			
		var lore_nodes = r["lore"]
		var scene_nodes = r["scene"]
		var char_nodes = r["character"]
		
		# Assign positions for each column in the row
		for i in range(loc_nodes.size()):
			positions[loc_nodes[i]] = Vector2(col_x["location"], current_y + i * 90.0)
		for i in range(lore_nodes.size()):
			positions[lore_nodes[i]] = Vector2(col_x["lore"], current_y + i * 90.0)
		for i in range(scene_nodes.size()):
			positions[scene_nodes[i]] = Vector2(col_x["scene"], current_y + i * 90.0)
		for i in range(char_nodes.size()):
			positions[char_nodes[i]] = Vector2(col_x["character"], current_y + i * 90.0)
			
		var max_nodes = max(1, max(lore_nodes.size(), max(scene_nodes.size(), char_nodes.size())))
		var row_height = max_nodes * 90.0
		current_y += row_height + 40.0 # spacer between rows
		
	return positions

func _on_auto_arrange_pressed() -> void:
	var target_positions = _compute_layout()
	
	var tween = create_tween()
	tween.set_parallel(true)
	tween.set_trans(Tween.TRANS_CUBIC)
	tween.set_ease(Tween.EASE_OUT)
	
	for child in graph_edit.get_children():
		if child is GraphNode:
			var node_id = child.name
			if target_positions.has(node_id):
				tween.tween_property(child, "position_offset", target_positions[node_id], 0.6)

func _create_node_element(node_id: String, node_data: Dictionary, pos: Vector2) -> void:
	var gnode = GraphNode.new()
	gnode.name = node_id
	gnode.title = node_data.get("label", node_id)
	gnode.position_offset = pos
	gnode.selectable = true
	gnode.draggable = true
	gnode.custom_minimum_size = Vector2(170, 70)
	
	# In Godot 4, connection points (slots) require child controls.
	var type_label = Label.new()
	type_label.text = "Type: " + node_data.get("type", "lore").capitalize()
	type_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	gnode.add_child(type_label)
	
	_style_node(gnode, node_data.get("type", "lore"))
	
	graph_edit.add_child(gnode)

func _style_node(gnode: GraphNode, type: String) -> void:
	var color = Color(0.5, 0.5, 0.5)
	for cat in CATEGORIES:
		if cat.id == type:
			color = cat.color
			break
			
	# Enable single input slot on the left (port 0) and output slot on the right (port 0)
	gnode.set_slot(0, true, 0, color, true, 0, color)
	
	# Apply styling via custom colors
	gnode.add_theme_color_override("title_color", Color(1.0, 1.0, 1.0))
	gnode.add_theme_color_override("resizer_color", color)

func _render_connections() -> void:
	for edge in _edges:
		var from_node = edge.get("from", "")
		var to_node = edge.get("to", "")
		
		# Make sure both nodes exist in the graph view before connecting
		if graph_edit.has_node(from_node) and graph_edit.has_node(to_node):
			graph_edit.connect_node(from_node, 0, to_node, 0)

# ==============================================================================
# UI Updates
# ==============================================================================

func _update_inspector_ui() -> void:
	if _selected_node_id.is_empty():
		node_name_label.text = "Select a node to inspect"
		type_dropdown.disabled = true
		content_preview.text = ""
		connect_target_dropdown.disabled = true
		connect_relation_dropdown.disabled = true
		add_connection_btn.disabled = true
		_clear_connections_list()
		return
		
	var node_data = _nodes.get(_selected_node_id, {})
	node_name_label.text = node_data.get("label", _selected_node_id)
	
	# Update category type dropdown
	type_dropdown.disabled = false
	var type = node_data.get("type", "lore")
	for i in range(CATEGORIES.size()):
		if CATEGORIES[i].id == type:
			type_dropdown.selected = i
			break
			
	# Content preview
	content_preview.text = node_data.get("desc", "No description available.")
	
	# Enable new connection form
	connect_target_dropdown.disabled = false
	connect_relation_dropdown.disabled = false
	add_connection_btn.disabled = false
	_populate_connect_target_dropdown()
	
	# Refresh connections list
	_populate_connections_list()

func _clear_connections_list() -> void:
	for child in connections_list.get_children():
		child.queue_free()

func _populate_connections_list() -> void:
	_clear_connections_list()
	
	var node_edges = []
	for edge in _edges:
		if edge.get("from") == _selected_node_id or edge.get("to") == _selected_node_id:
			node_edges.append(edge)
			
	if node_edges.is_empty():
		no_connections_label.visible = true
		return
		
	no_connections_label.visible = false
	for edge in node_edges:
		var from_id = edge.get("from", "")
		var to_id = edge.get("to", "")
		var rel = edge.get("relation", "connected_to")
		
		var target_id = to_id if from_id == _selected_node_id else from_id
		var is_outgoing = (from_id == _selected_node_id)
		
		var target_label = _nodes.get(target_id, {}).get("label", target_id)
		
		var item = preload("res://scenes/ui/ConnectionListItem.tscn").instantiate()
		connections_list.add_child(item)
		item.setup("%s (%s %s)" % [target_label, "➔" if is_outgoing else "⇠", rel])
		item.delete_pressed.connect(func():
			_remove_edge(edge)
		)

func _populate_connect_target_dropdown() -> void:
	connect_target_dropdown.clear()
	var index = 0
	for node_id in _nodes.keys():
		if node_id == _selected_node_id:
			continue
		var label = _nodes[node_id].get("label", node_id)
		connect_target_dropdown.add_item(label)
		connect_target_dropdown.set_item_metadata(index, node_id)
		index += 1

func _update_starting_dropdowns() -> void:
	starting_loc_dropdown.clear()
	starting_char_dropdown.clear()
	
	starting_loc_dropdown.add_item("Select a starting location...")
	starting_loc_dropdown.set_item_metadata(0, "")
	
	starting_char_dropdown.add_item("Select a starting character...")
	starting_char_dropdown.set_item_metadata(0, "")
	
	var loc_index = 1
	var char_index = 1
	
	var select_loc_idx = 0
	var select_char_idx = 0
	
	# Sort keys for consistent dropdown list
	var sorted_node_ids = _nodes.keys()
	sorted_node_ids.sort()
	
	for node_id in sorted_node_ids:
		var node = _nodes[node_id]
		var type = node.get("type", "lore")
		var label = node.get("label", node_id)
		
		if type == "location":
			starting_loc_dropdown.add_item(label)
			starting_loc_dropdown.set_item_metadata(loc_index, node_id)
			if node_id == _starting_location_id:
				select_loc_idx = loc_index
			loc_index += 1
		elif type in ["character", "npc"]:
			starting_char_dropdown.add_item(label)
			starting_char_dropdown.set_item_metadata(char_index, node_id)
			if node_id == _starting_character_id:
				select_char_idx = char_index
			char_index += 1
			
	starting_loc_dropdown.selected = select_loc_idx
	starting_char_dropdown.selected = select_char_idx

# ==============================================================================
# Graph Actions & Handlers
# ==============================================================================

func _on_node_selected(node: Node) -> void:
	_selected_node_id = node.name
	_update_inspector_ui()

func _on_node_deselected(node: Node) -> void:
	if _selected_node_id == node.name:
		_selected_node_id = ""
		_update_inspector_ui()

func _on_connection_request(from_node: StringName, from_port: int, to_node: StringName, to_port: int) -> void:
	# Add connection to internal model
	var edge = {
		"from": str(from_node),
		"to": str(to_node),
		"relation": "connected_to",
		"weight": 1.0
	}
	
	# Check if connection already exists
	var exists = false
	for e in _edges:
		if e.get("from") == edge.from and e.get("to") == edge.to:
			exists = true
			break
			
	if not exists:
		_edges.append(edge)
		graph_edit.connect_node(from_node, from_port, to_node, to_port)
		_update_inspector_ui()

func _on_disconnection_request(from_node: StringName, from_port: int, to_node: StringName, to_port: int) -> void:
	# Find and remove connection
	var edge_index = -1
	for i in range(_edges.size()):
		var e = _edges[i]
		if e.get("from") == str(from_node) and e.get("to") == str(to_node):
			edge_index = i
			break
			
	if edge_index != -1:
		_edges.remove_at(edge_index)
		graph_edit.disconnect_node(from_node, from_port, to_node, to_port)
		_update_inspector_ui()

func _remove_edge(edge: Dictionary) -> void:
	var from_node = edge.get("from", "")
	var to_node = edge.get("to", "")
	
	_edges.erase(edge)
	
	# Disconnect visually
	if graph_edit.has_node(from_node) and graph_edit.has_node(to_node):
		graph_edit.disconnect_node(from_node, 0, to_node, 0)
		
	_update_inspector_ui()

func _on_add_connection_pressed() -> void:
	if _selected_node_id.is_empty():
		return
		
	var target_idx = connect_target_dropdown.selected
	if target_idx == -1:
		return
		
	var target_id = connect_target_dropdown.get_item_metadata(target_idx)
	var rel_idx = connect_relation_dropdown.selected
	var relation = RELATION_TYPES[rel_idx] if rel_idx != -1 else "connected_to"
	
	# Add edge
	var edge = {
		"from": _selected_node_id,
		"to": target_id,
		"relation": relation,
		"weight": 1.0
	}
	
	# Check duplicates
	var exists = false
	for e in _edges:
		if e.get("from") == edge.from and e.get("to") == edge.to:
			exists = true
			break
			
	if not exists:
		_edges.append(edge)
		if graph_edit.has_node(_selected_node_id) and graph_edit.has_node(target_id):
			graph_edit.connect_node(_selected_node_id, 0, target_id, 0)
		_update_inspector_ui()

func _on_type_selected(idx: int) -> void:
	if _selected_node_id.is_empty():
		return
		
	var new_type = type_dropdown.get_item_metadata(idx)
	var node_data = _nodes[_selected_node_id]
	var old_type = node_data.get("type", "lore")
	
	if new_type != old_type:
		node_data["type"] = new_type
		
		# Update visual representation
		if graph_edit.has_node(_selected_node_id):
			var gnode = graph_edit.get_node(_selected_node_id) as GraphNode
			if gnode:
				var type_lbl = gnode.get_child(0) as Label
				if type_lbl:
					type_lbl.text = "Type: " + new_type.capitalize()
				_style_node(gnode, new_type)
				
		_update_starting_dropdowns()
		_update_inspector_ui()
		_on_auto_arrange_pressed()
		_apply_filters()

func _on_starting_loc_selected(idx: int) -> void:
	_starting_location_id = starting_loc_dropdown.get_item_metadata(idx)

func _on_starting_char_selected(idx: int) -> void:
	_starting_character_id = starting_char_dropdown.get_item_metadata(idx)

func _on_filter_changed(_new_text_or_toggled: Variant = null) -> void:
	_apply_filters()

func _apply_filters() -> void:
	var query = search_input.text.strip_edges().to_lower()
	var show_location = toggle_location.button_pressed
	var show_lore = toggle_lore.button_pressed
	var show_scene = toggle_scene.button_pressed
	var show_character = toggle_character.button_pressed
	
	var total_nodes_count = 0
	var matching_nodes_count = 0
	var first_match_node: GraphNode = null
	
	# 1. Update visibility & modulation for all nodes in the graph
	for child in graph_edit.get_children():
		if not child is GraphNode:
			continue
			
		var node_name = child.name
		
		# Handle column header nodes
		if node_name.begins_with("header_"):
			var header_type = node_name.substr(7) # e.g. "location", "lore", etc.
			var header_visible = true
			match header_type:
				"location": header_visible = show_location
				"lore": header_visible = show_lore
				"scene": header_visible = show_scene
				"character": header_visible = show_character
			child.visible = header_visible
			continue
			
		# Handle the special global anchor node
		if node_name == "global_anchor":
			child.visible = show_location or show_lore or show_scene or show_character
			continue
			
		# Skip nodes that aren't defined in _nodes
		if not _nodes.has(node_name):
			continue
			
		total_nodes_count += 1
		var node_data = _nodes[node_name]
		var node_type = node_data.get("type", "lore")
		if node_type == "npc":
			node_type = "character"
			
		# Category check
		var category_visible = true
		match node_type:
			"location": category_visible = show_location
			"lore": category_visible = show_lore
			"scene": category_visible = show_scene
			"character": category_visible = show_character
			_: category_visible = true
			
		if not category_visible:
			child.visible = false
			continue
			
		child.visible = true
		
		# Search check
		var label = node_data.get("label", node_name).to_lower()
		var desc = node_data.get("desc", "").to_lower()
		var matches_search = query.is_empty() or label.contains(query) or node_name.to_lower().contains(query) or desc.contains(query)
		
		if matches_search:
			child.modulate = Color(1.0, 1.0, 1.0, 1.0)
			matching_nodes_count += 1
			if first_match_node == null:
				first_match_node = child
		else:
			child.modulate = Color(1.0, 1.0, 1.0, 0.25)
			
	# 2. Re-render connections based on node visibility
	for conn in graph_edit.get_connection_list():
		graph_edit.disconnect_node(conn.from_node, conn.from_port, conn.to_node, conn.to_port)
		
	for edge in _edges:
		var from_node = edge.get("from", "")
		var to_node = edge.get("to", "")
		if graph_edit.has_node(from_node) and graph_edit.has_node(to_node):
			var from_child = graph_edit.get_node(from_node) as GraphNode
			var to_child = graph_edit.get_node(to_node) as GraphNode
			if from_child and to_child and from_child.visible and to_child.visible:
				graph_edit.connect_node(from_node, 0, to_node, 0)
				
	# 3. Update count label
	node_count_label.text = "Showing %d of %d nodes" % [matching_nodes_count, total_nodes_count]
	
	# 4. Center on the first match if search query is active and a match is found
	if not query.is_empty() and first_match_node != null:
		call_deferred("_center_on_node", first_match_node)

func _center_on_node(gnode: GraphNode) -> void:
	if not is_instance_valid(graph_edit) or not is_instance_valid(gnode):
		return
	var node_center = gnode.position_offset + gnode.size / 2.0
	var viewport_center = graph_edit.size / 2.0
	if graph_edit.size.x < 10 or graph_edit.size.y < 10:
		var default_center = Vector2(400, 300)
		graph_edit.scroll_offset = node_center * graph_edit.zoom - default_center
	else:
		graph_edit.scroll_offset = node_center * graph_edit.zoom - viewport_center

func _on_zoom_in_pressed() -> void:
	graph_edit.zoom = clamp(graph_edit.zoom + 0.1, graph_edit.zoom_min, graph_edit.zoom_max)
	_update_zoom_label()

func _on_zoom_out_pressed() -> void:
	graph_edit.zoom = clamp(graph_edit.zoom - 0.1, graph_edit.zoom_min, graph_edit.zoom_max)
	_update_zoom_label()

func _on_zoom_reset_pressed() -> void:
	graph_edit.zoom = 1.0
	_update_zoom_label()

func _update_zoom_label() -> void:
	zoom_reset_btn.text = "%d%%" % round(graph_edit.zoom * 100.0)

func _process(_delta: float) -> void:
	if is_instance_valid(graph_edit) and not is_equal_approx(graph_edit.zoom, _last_zoom):
		_last_zoom = graph_edit.zoom
		_update_zoom_label()
