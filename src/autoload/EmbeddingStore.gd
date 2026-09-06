# res://src/autoload/EmbeddingStore.gd
extends Node

var _embeddings: Dictionary = {} # node_id (String) -> vector (Array of floats)

func _ready() -> void:
	if EventBus:
		EventBus.campaign_loaded.connect(_on_campaign_loaded)

func _on_campaign_loaded(campaign_id: String) -> void:
	load_store(campaign_id)

func add_embedding(node_id: String, vector: Array) -> void:
	var float_vec: Array[float] = []
	for val in vector:
		float_vec.append(float(val))
	_embeddings[node_id] = float_vec

func has_embeddings() -> bool:
	return not _embeddings.is_empty()

func clear() -> void:
	_embeddings.clear()

func get_knn(query_vector: Array, k: int = 10) -> Array[Dictionary]:
	if query_vector.is_empty() or _embeddings.is_empty():
		return []
		
	var results: Array[Dictionary] = []
	for node_id in _embeddings.keys():
		var vec = _embeddings[node_id]
		var sim = _cosine_similarity(query_vector, vec)
		if sim > 0.0:
			results.append({
				"node_id": node_id,
				"score": sim
			})
		
	results.sort_custom(func(a, b):
		return a["score"] > b["score"]
	)
	
	if results.size() > k:
		return results.slice(0, k)
	return results

func save_store(campaign_id: String) -> void:
	if campaign_id.is_empty():
		return
		
	var path = "user://adventures/" + campaign_id + "_embeddings.json"
	var file = FileAccess.open(path, FileAccess.WRITE)
	if not file:
		printerr("[EmbeddingStore] Failed to save embeddings store to: ", path)
		return
		
	var json_string = JSON.stringify(_embeddings)
	file.store_string(json_string)
	file.close()
	print("[EmbeddingStore] Saved embeddings store for campaign: ", campaign_id)

func load_store(campaign_id: String) -> void:
	if campaign_id.is_empty():
		clear()
		return
		
	var path = "user://adventures/" + campaign_id + "_embeddings.json"
	if not FileAccess.file_exists(path):
		print("[EmbeddingStore] Embeddings store file not found for campaign: ", campaign_id)
		clear()
		return
		
	var file = FileAccess.open(path, FileAccess.READ)
	if not file:
		printerr("[EmbeddingStore] Failed to open embeddings store from: ", path)
		clear()
		return
		
	var content = file.get_as_text()
	file.close()
	
	var json = JSON.new()
	if json.parse(content) == OK and json.data is Dictionary:
		_embeddings.clear()
		var data = json.data
		for k in data.keys():
			var vec = data[k]
			if vec is Array:
				var float_vec: Array[float] = []
				for val in vec:
					float_vec.append(float(val))
				_embeddings[k] = float_vec
		print("[EmbeddingStore] Loaded %d embeddings for campaign: %s" % [_embeddings.size(), campaign_id])
	else:
		printerr("[EmbeddingStore] Failed to parse embeddings JSON.")
		clear()

func _cosine_similarity(vec_a: Array, vec_b: Array) -> float:
	if vec_a.size() != vec_b.size() or vec_a.is_empty():
		return 0.0
	var dot = 0.0
	var norm_a = 0.0
	var norm_b = 0.0
	for i in range(vec_a.size()):
		var val_a = vec_a[i]
		var val_b = vec_b[i]
		dot += val_a * val_b
		norm_a += val_a * val_a
		norm_b += val_b * val_b
	if norm_a == 0.0 or norm_b == 0.0:
		return 0.0
	return dot / (sqrt(norm_a) * sqrt(norm_b))
