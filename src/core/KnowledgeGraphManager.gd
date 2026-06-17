# res://src/core/KnowledgeGraphManager.gd
extends RefCounted
class_name KnowledgeGraphManager

## Fetches the knowledge_graph sub-dictionary from the global CampaignState autoload
func _get_graph() -> Dictionary:
	if not CampaignState.state.has("knowledge_graph"):
		CampaignState.state["knowledge_graph"] = {"nodes": {}, "edges": []}
	return CampaignState.state["knowledge_graph"]

# ==============================================================================
# Node Operations
# ==============================================================================

func add_node(node_id: String, label: String, type: String, description: String = "", properties: Dictionary = {}) -> void:
	var graph = _get_graph()
	var nodes: Dictionary = graph.get("nodes", {})
	
	nodes[node_id] = {
		"label": label,
		"type": type,
		"desc": description,
		"properties": properties
	}
	graph["nodes"] = nodes

func get_node(node_id: String) -> Dictionary:
	var graph = _get_graph()
	var nodes: Dictionary = graph.get("nodes", {})
	return nodes.get(node_id, {})

func has_node(node_id: String) -> bool:
	var graph = _get_graph()
	var nodes: Dictionary = graph.get("nodes", {})
	return nodes.has(node_id)

func remove_node(node_id: String) -> void:
	var graph = _get_graph()
	var nodes: Dictionary = graph.get("nodes", {})
	if nodes.has(node_id):
		nodes.erase(node_id)
		
	# Clean up any orphaned edges
	var edges: Array = graph.get("edges", [])
	var filtered_edges = []
	for edge in edges:
		if edge.get("from") != node_id and edge.get("to") != node_id:
			filtered_edges.append(edge)
	graph["edges"] = filtered_edges

# ==============================================================================
# Edge Operations
# ==============================================================================

func add_edge(from_node: String, to_node: String, relation: String, weight: float = 1.0) -> void:
	var graph = _get_graph()
	var edges: Array = graph.get("edges", [])
	
	# Check if edge already exists to update it
	var found = false
	for edge in edges:
		if edge.get("from") == from_node and edge.get("to") == to_node and edge.get("relation") == relation:
			edge["weight"] = clamp(weight, 0.0, 1.0)
			found = true
			break
			
	if not found:
		edges.append({
			"from": from_node,
			"to": to_node,
			"relation": relation,
			"weight": clamp(weight, 0.0, 1.0)
		})
		
	graph["edges"] = edges

func get_connected_edges(node_id: String) -> Array:
	var graph = _get_graph()
	var edges: Array = graph.get("edges", [])
	var list = []
	for edge in edges:
		if edge.get("from") == node_id or edge.get("to") == node_id:
			list.append(edge)
	return list

func get_neighbors(node_id: String) -> Array[String]:
	var neighbors: Array[String] = []
	var edges = get_connected_edges(node_id)
	for edge in edges:
		var from_n = edge.get("from", "")
		var to_n = edge.get("to", "")
		if from_n == node_id and to_n != "" and not neighbors.has(to_n):
			neighbors.append(to_n)
		elif to_n == node_id and from_n != "" and not neighbors.has(from_n):
			neighbors.append(from_n)
	return neighbors

# ==============================================================================
# Search and Synthesis Context Retrieval
# ==============================================================================

## Searches node labels and types matching terms in user input.
## Returns a formatted string representing the retrieved sub-graph to inject as prompt context.
func retrieve_context(user_prompt: String) -> String:
	var graph = _get_graph()
	var nodes: Dictionary = graph.get("nodes", {})
	if nodes.is_empty():
		return ""
		
	var matched_ids: Array[String] = []
	var normalized_prompt = user_prompt.to_lower()
	
	# Match keywords
	for id in nodes.keys():
		var node = nodes[id]
		var label = node.get("label", "").to_lower()
		var type = node.get("type", "").to_lower()
		
		# Simple token search
		if normalized_prompt.contains(label) or normalized_prompt.contains(id.to_lower()):
			matched_ids.append(id)
			
	if matched_ids.is_empty():
		return ""
		
	# Expand matches by 1 degree (fetch direct neighbors) to build narrative coherence
	var context_nodes: Dictionary = {}
	for id in matched_ids:
		context_nodes[id] = nodes[id]
		var neighbors = get_neighbors(id)
		for neighbor_id in neighbors:
			if nodes.has(neighbor_id) and not context_nodes.has(neighbor_id):
				context_nodes[neighbor_id] = nodes[neighbor_id]
				
	# Formulate output context text
	var text = "### Retrieved Memory & World Context:\n"
	for id in context_nodes.keys():
		var node = context_nodes[id]
		text += "- **%s** (%s): %s\n" % [node.get("label", id), node.get("type", "Entity"), node.get("desc", "No description.")]
		
	# Append connections
	var edges: Array = graph.get("edges", [])
	var connection_texts = []
	for edge in edges:
		var f = edge.get("from", "")
		var t = edge.get("to", "")
		if context_nodes.has(f) and context_nodes.has(t):
			var from_lbl = context_nodes[f].get("label", f)
			var to_lbl = context_nodes[t].get("label", t)
			connection_texts.append("  - Relation: %s is [%s] -> %s" % [from_lbl, edge.get("relation", "connected"), to_lbl])
			
	if not connection_texts.is_empty():
		text += "\nRelationships:\n" + "\n".join(connection_texts) + "\n"
		
	return text
