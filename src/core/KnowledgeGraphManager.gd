# res://src/core/KnowledgeGraphManager.gd
extends RefCounted
class_name KnowledgeGraphManager

var _custom_graph: Dictionary
var _has_custom_graph: bool = false

func _init(custom_graph = null) -> void:
	if custom_graph is Dictionary:
		_custom_graph = custom_graph
		_has_custom_graph = true

## Fetches the knowledge_graph sub-dictionary
func _get_graph() -> Dictionary:
	if _has_custom_graph:
		return _custom_graph
	if not CampaignState.state.has("knowledge_graph"):
		CampaignState.state["knowledge_graph"] = {"nodes": {}, "edges": []}
	return CampaignState.state["knowledge_graph"]

# ==============================================================================
# Node Operations
# ==============================================================================

func add_node(node_id: String, label: String, type: String, description: String = "", properties: Dictionary = {}) -> void:
	if not _has_custom_graph and CampaignState and CampaignState.has_method("apply_state_change"):
		CampaignState.apply_state_change({
			"type": "add_graph_node",
			"node_id": node_id,
			"label": label,
			"node_type": type,
			"description": description,
			"properties": properties
		})
	else:
		_add_node_internal(node_id, label, type, description, properties)

func _add_node_internal(node_id: String, label: String, type: String, description: String = "", properties: Dictionary = {}) -> void:
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
	if not _has_custom_graph and CampaignState and CampaignState.has_method("apply_state_change"):
		CampaignState.apply_state_change({
			"type": "remove_graph_node",
			"node_id": node_id
		})
	else:
		_remove_node_internal(node_id)

func _remove_node_internal(node_id: String) -> void:
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
	if not _has_custom_graph and CampaignState and CampaignState.has_method("apply_state_change"):
		CampaignState.apply_state_change({
			"type": "add_graph_edge",
			"from_node": from_node,
			"to_node": to_node,
			"relation": relation,
			"weight": weight
		})
	else:
		_add_edge_internal(from_node, to_node, relation, weight)

func _add_edge_internal(from_node: String, to_node: String, relation: String, weight: float = 1.0) -> void:
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

## Searches node labels and types matching terms in user input using hybrid keyword + KNN semantic retrieval.
## Returns a formatted string representing the retrieved sub-graph to inject as prompt context.
func retrieve_context(user_prompt: String, max_tokens: int = -1, target_level: int = 0) -> String:
	var graph = _get_graph()
	var nodes: Dictionary = graph.get("nodes", {})
	if nodes.is_empty():
		return ""
		
	# Filter nodes by target level
	var filtered_nodes = {}
	for id in nodes.keys():
		var node = nodes[id]
		var type = node.get("type", "")
		var props = node.get("properties", {})
		var level = props.get("level", 0)
		
		if target_level == 2:
			if type == "summary" and level == 2:
				filtered_nodes[id] = node
		elif target_level == 1:
			if type == "summary" and level == 1:
				filtered_nodes[id] = node
		else:
			if type != "summary":
				filtered_nodes[id] = node
				
	if filtered_nodes.is_empty():
		return ""
		
	var normalized_prompt = user_prompt.to_lower()
	
	# 1. Keyword search (substring match)
	var keyword_matches: Array[String] = []
	for id in filtered_nodes.keys():
		var node = filtered_nodes[id]
		var label = node.get("label", "").to_lower()
		if normalized_prompt.contains(label) or normalized_prompt.contains(id.to_lower()):
			keyword_matches.append(id)
			
	# Rank keyword matches by label length descending (longer, more specific names rank higher)
	keyword_matches.sort_custom(func(a, b):
		return filtered_nodes[a].get("label", "").length() > filtered_nodes[b].get("label", "").length()
	)
	
	# 2. Semantic search (KNN)
	var knn_matches: Array[Dictionary] = []
	var query_vector = await LLMClient.get_embedding(user_prompt)
	if not query_vector.is_empty() and EmbeddingStore.has_embeddings():
		var raw_knn = EmbeddingStore.get_knn(query_vector, 10)
		for match_info in raw_knn:
			if filtered_nodes.has(match_info["node_id"]):
				knn_matches.append(match_info)
	else:
		if query_vector.is_empty() and not LLMClient.mock_response_handler.is_valid():
			print("[KnowledgeGraph] Warning: nomic-embed-text not available. Falling back to keyword search only.")
			
	# 3. Reciprocal Rank Fusion (RRF) with k=60
	var rrf_scores = {} # node_id (String) -> score (float)
	
	for i in range(keyword_matches.size()):
		var node_id = keyword_matches[i]
		var rank = i + 1
		rrf_scores[node_id] = rrf_scores.get(node_id, 0.0) + 1.0 / (60.0 + rank)
		
	for i in range(knn_matches.size()):
		var match_info = knn_matches[i]
		var node_id = match_info["node_id"]
		var rank = i + 1
		rrf_scores[node_id] = rrf_scores.get(node_id, 0.0) + 1.0 / (60.0 + rank)
		
	# Sort by RRF score descending
	var merged_ids = rrf_scores.keys()
	merged_ids.sort_custom(func(a, b):
		return rrf_scores[a] > rrf_scores[b]
	)
	
	# Fallback: For Level 2 summaries, if nothing matched, include all of them
	if merged_ids.is_empty():
		if target_level == 2:
			merged_ids = filtered_nodes.keys()
		else:
			return ""
		
	# Expand matches by 1 degree (fetch direct neighbors) to build narrative coherence
	var context_nodes: Dictionary = {}
	var ordered_keys: Array[String] = []
	for id in merged_ids:
		if not context_nodes.has(id):
			context_nodes[id] = filtered_nodes[id]
			ordered_keys.append(id)
			
		# Only expand neighbors for Level 0 (raw) nodes
		if target_level == 0:
			var neighbors = get_neighbors(id)
			for neighbor_id in neighbors:
				if filtered_nodes.has(neighbor_id) and not context_nodes.has(neighbor_id):
					context_nodes[neighbor_id] = filtered_nodes[neighbor_id]
					ordered_keys.append(neighbor_id)
				
	# Formulate output context text
	var text = "### Retrieved Memory & World Context:\n"
	for id in ordered_keys:
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
			
	var full_text = text
	if not connection_texts.is_empty():
		full_text += "\nRelationships:\n" + "\n".join(connection_texts) + "\n"
		
	var estimated_tokens = int(ceil(full_text.length() / 4.0))
	if max_tokens > 0 and estimated_tokens > max_tokens:
		print("[KnowledgeGraph] Lore context (%d tokens) exceeds budget (%d tokens). Truncating distant lore connections." % [estimated_tokens, max_tokens])
		text = "### Retrieved Memory & World Context:\n"
		for id in merged_ids:
			var node = filtered_nodes[id]
			var node_text = "- **%s** (%s): %s\n" % [node.get("label", id), node.get("type", "Entity"), node.get("desc", "No description.")]
			var prospective_text = text + node_text
			var prospective_tokens = int(ceil(prospective_text.length() / 4.0))
			
			if prospective_tokens <= max_tokens:
				text += node_text
			else:
				var current_tokens = int(ceil(text.length() / 4.0))
				var remaining_tokens = max_tokens - current_tokens
				var allowed_chars = remaining_tokens * 4
				var label_format = "- **%s** (%s): " % [node.get("label", id), node.get("type", "Entity")]
				var label_len = label_format.length()
				if allowed_chars > label_len + 5:
					var desc = node.get("desc", "No description.")
					var truncated_desc = desc.left(allowed_chars - label_len - 5) + "..."
					text += label_format + truncated_desc + "\n"
				break
		return text
		
	return full_text


## Returns all nodes matching a specific type
func get_nodes_by_type(type: String) -> Dictionary:
	var graph = _get_graph()
	var nodes: Dictionary = graph.get("nodes", {})
	var filtered = {}
	for node_id in nodes.keys():
		var node = nodes[node_id]
		if node.get("type") == type:
			filtered[node_id] = node
	return filtered

## Returns an array of entity node dictionaries connected to the starting entity_id
## via a specific edge_type (relation) within a given max_depth (multi-hop traversal).
func get_entities_connected_to(entity_id: String, edge_type: String, max_depth: int = 1) -> Array[Dictionary]:
	var results: Array[Dictionary] = []
	var visited: Dictionary = {}
	_traverse_graph(entity_id, edge_type, 0, max_depth, visited)
	
	# Exclude the starting entity_id itself from the results
	visited.erase(entity_id)
	
	for visited_id in visited.keys():
		var node = get_node(visited_id)
		if not node.is_empty():
			var node_copy = node.duplicate(true)
			node_copy["id"] = visited_id
			results.append(node_copy)
			
	return results

func _traverse_graph(current_id: String, edge_type: String, current_depth: int, max_depth: int, visited: Dictionary) -> void:
	if current_depth > max_depth:
		return
		
	# Mark as visited at the earliest depth we found it
	if visited.has(current_id) and visited[current_id] <= current_depth:
		return
	visited[current_id] = current_depth
	
	if current_depth == max_depth:
		return
		
	var graph = _get_graph()
	var edges: Array = graph.get("edges", [])
	for edge in edges:
		var rel = edge.get("relation", "")
		if rel == edge_type or edge_type == "*":
			var f = edge.get("from", "")
			var t = edge.get("to", "")
			if f == current_id:
				_traverse_graph(t, edge_type, current_depth + 1, max_depth, visited)
			elif t == current_id:
				_traverse_graph(f, edge_type, current_depth + 1, max_depth, visited)

