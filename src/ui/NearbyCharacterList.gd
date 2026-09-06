# res://src/ui/NearbyCharacterList.gd
extends VBoxContainer
class_name NearbyCharacterList

const CharacterListItemScene = preload("res://scenes/ui/CharacterListItem.tscn")

signal character_selected(char_id: String)

var graph_manager: KnowledgeGraphManager

func setup(p_graph_manager: KnowledgeGraphManager) -> void:
	graph_manager = p_graph_manager
	if not EventBus.location_changed.is_connected(_on_location_changed):
		EventBus.location_changed.connect(_on_location_changed)

func _on_location_changed(_location_id: String) -> void:
	refresh(CampaignState.get_campaign_meta("active_character", ""))

func refresh(active_character_id: String) -> void:
	if CampaignState.campaign_id.is_empty():
		for child in get_children():
			child.queue_free()
		return
		
	var nearby_ids = get_nearby_character_ids(active_character_id)
	# Map existing children by character_id
	var existing_items = {}
	for child in get_children():
		if child is CharacterListItem:
			existing_items[child.character_id] = child
			
	# Keep track of kids we want to keep
	var kept_items = []
	
	for char_key in nearby_ids:
		if not CampaignState.has_character(char_key):
			continue
		var char_data = CampaignState.get_character(char_key)
		
		var item: CharacterListItem = null
		if existing_items.has(char_key):
			item = existing_items[char_key]
			# Reuse and update
			item.setup(char_key, char_data.get("name", char_key), char_data.get("affinity", 0.0))
		else:
			# Instantiate new
			item = CharacterListItemScene.instantiate()
			add_child(item)
			item.setup(char_key, char_data.get("name", char_key), char_data.get("affinity", 0.0))
			item.selected.connect(func(char_id: String): character_selected.emit(char_id))
			
		kept_items.append(item)
		
	# Remove any children that are no longer nearby
	for child in get_children():
		if not child in kept_items:
			child.queue_free()
			
	# Reorder children to match kept_items order
	for i in range(kept_items.size()):
		move_child(kept_items[i], i)

func get_nearby_character_ids(active_character_id: String) -> Array[String]:
	var nearby_ids: Array[String] = []
	
	if not active_character_id.is_empty():
		nearby_ids.append(active_character_id)
		
	var active_location = CampaignState.get_campaign_meta("active_location", "")
	
	# Determine if any locations exist in the graph
	var has_locations = false
	var graph = CampaignState.state.get("knowledge_graph", {"nodes": {}, "edges": []})
	var nodes = graph.get("nodes", {})
	for node_id in nodes.keys():
		if nodes[node_id].get("type") == "location":
			has_locations = true
			break
			
	if active_location.is_empty():
		if has_locations:
			# If locations are defined in the campaign, but none is active,
			# do not return all characters. Just return the active character (if any).
			return nearby_ids
		else:
			# Fallback if no locations exist in the entire campaign (simple campaigns)
			var char_ids = CampaignState.get_character_ids()
			for char_id in char_ids:
				if char_id == "player":
					continue
				var char_data = CampaignState.get_character(char_id)
				if char_data.get("is_creature", false) or not char_data.get("can_speak", true):
					continue
				if not nearby_ids.has(char_id):
					nearby_ids.append(char_id)
			return nearby_ids
		
	if not graph_manager:
		return nearby_ids
		
	var loc_node = graph_manager.get_node(active_location)
	if not loc_node:
		return nearby_ids
		
	var loc_label = loc_node.get("label", active_location).to_lower()
	var char_ids = CampaignState.get_character_ids()
	
	for char_id in char_ids:
		if char_id == "player":
			continue
		if nearby_ids.has(char_id):
			continue
			
		var char_data = CampaignState.get_character(char_id)
		if char_data.get("is_creature", false) or not char_data.get("can_speak", true):
			continue
			
		var is_nearby = false
		
		# 1. Check direct or regional edges in the knowledge graph
		var edges = graph_manager.get_connected_edges(char_id)
		var active_neighbors: Array[String] = []
		for neighbor_id in graph_manager.get_neighbors(active_location):
			var neighbor_node = graph_manager.get_node(neighbor_id)
			if neighbor_node and neighbor_node.get("type", "") in ["location", "environment", "gate"]:
				active_neighbors.append(neighbor_id)
				
		for edge in edges:
			var f = edge.get("from", "")
			var t = edge.get("to", "")
			var rel = edge.get("relation", "")
			if rel in ["associated_with", "connected_to"]:
				if f == active_location or t == active_location:
					is_nearby = true
					break
				if f in active_neighbors or t in active_neighbors:
					is_nearby = true
					break
				
		# 2. Check biography mentions
		if not is_nearby:
			var bio = char_data.get("biography", "").to_lower()
			if bio.contains(loc_label) or bio.contains(active_location.to_lower().replace("_", " ")):
				is_nearby = true
				
		# 3. Check frontmatter properties connections
		if not is_nearby:
			var char_node = graph_manager.get_node(char_id)
			if char_node:
				var fm = char_node.get("properties", {})
				var connections = fm.get("connections", [])
				if connections is Array:
					for conn in connections:
						var conn_str = str(conn).to_lower().replace(" ", "_")
						if conn_str == active_location or conn_str == loc_label.replace(" ", "_"):
							is_nearby = true
							break
				elif connections is String:
					var conn_str = connections.to_lower().replace(" ", "_")
					if conn_str == active_location or conn_str == loc_label.replace(" ", "_"):
						is_nearby = true
					
		if is_nearby:
			nearby_ids.append(char_id)
			
	return nearby_ids
