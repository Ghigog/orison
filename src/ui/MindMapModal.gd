# res://src/ui/MindMapModal.gd
extends Control
class_name MindMapModal

signal closed

@onready var campaign_graph_view: CampaignGraphView = %CampaignGraphView
@onready var close_button: Button = %CloseButton
@onready var modal_panel: PanelContainer = %ModalPanel

func _ready() -> void:
	close_button.pressed.connect(_on_close_pressed)
	ThemeManager.theme_changed.connect(_on_global_theme_changed)
	_apply_modal_styles()
	
	# Load knowledge graph data from CampaignState
	var graph = CampaignState.state.get("knowledge_graph", {"nodes": {}, "edges": []})
	var current_location = CampaignState.get_campaign_meta("active_location", "")
	var current_character = CampaignState.get_campaign_meta("active_character", "")
	
	campaign_graph_view.initialize(graph, current_location, current_character)
	
	# Hide starting parameters in-game as they are setup-only
	var start_params = campaign_graph_view.get_node_or_null("InspectorPanel/MarginContainer/ScrollContainer/VBoxContainer/StartParamsGroup")
	if start_params:
		start_params.visible = false
	var separator = campaign_graph_view.get_node_or_null("InspectorPanel/MarginContainer/ScrollContainer/VBoxContainer/HSeparator2")
	if separator:
		separator.visible = false
		
	# Entrance transition
	var overlay = get_node_or_null("OverlayBG")
	if overlay and modal_panel:
		var target_bg_alpha = overlay.color.a
		overlay.color.a = 0.0
		modal_panel.modulate.a = 0.0
		modal_panel.scale = Vector2(0.95, 0.95)
		
		# Set pivot to center of screen
		modal_panel.pivot_offset = modal_panel.size / 2.0
		modal_panel.item_rect_changed.connect(func():
			modal_panel.pivot_offset = modal_panel.size / 2.0
		)
		
		var entrance_tween = create_tween().set_parallel(true)
		entrance_tween.tween_property(overlay, "color:a", target_bg_alpha, 0.3)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)
		entrance_tween.tween_property(modal_panel, "modulate:a", 1.0, 0.3)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)
		entrance_tween.tween_property(modal_panel, "scale", Vector2(1.0, 1.0), 0.3)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_OUT)

func _on_close_pressed() -> void:
	# Save updated graph back to CampaignState when closed
	var graph_result = campaign_graph_view.get_graph_data()
	
	var existing_nodes = CampaignState.state.get("knowledge_graph", {}).get("nodes", {})
	for node_id in graph_result.nodes.keys():
		var node = graph_result.nodes[node_id]
		if node.type in ["character", "npc"]:
			var existing_node = existing_nodes.get(node_id, {})
			var existing_properties = existing_node.get("properties", {})
			
			var properties = node.get("properties", {})
			if properties.is_empty():
				properties = existing_properties.duplicate(true)
			else:
				for key in existing_properties.keys():
					if not properties.has(key):
						properties[key] = existing_properties[key]
						
			properties["name"] = node.label
			properties["biography"] = node.desc
			node["properties"] = properties
			
	CampaignState.set_knowledge_graph_data(graph_result.nodes, graph_result.edges)
	CampaignState.save()
	
	closed.emit()
	
	# Exit animation
	var overlay = get_node_or_null("OverlayBG")
	if overlay and modal_panel:
		modal_panel.pivot_offset = modal_panel.size / 2.0
		var exit_tween = create_tween().set_parallel(true)
		exit_tween.tween_property(overlay, "color:a", 0.0, 0.25)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_IN)
		exit_tween.tween_property(modal_panel, "modulate:a", 0.0, 0.25)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_IN)
		exit_tween.tween_property(modal_panel, "scale", Vector2(0.95, 0.95), 0.25)\
			.set_trans(Tween.TRANS_CUBIC)\
			.set_ease(Tween.EASE_IN)
		exit_tween.chain().tween_callback(queue_free)
	else:
		queue_free()

func _on_global_theme_changed() -> void:
	_apply_modal_styles()

func _apply_modal_styles() -> void:
	self.theme = ThemeManager.active_theme

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
		get_viewport().set_input_as_handled()
		_on_close_pressed()

