# res://src/autoload/LLMClient.gd
extends Node

const LLMStreamRequest = preload("res://src/core/LLMStreamRequest.gd")

signal response_received(response_text: String)
signal response_chunk_received(chunk: String)
signal request_failed(error_msg: String)

const CONFIG_PATH = "user://client_config.json"

var api_url: String = "http://127.0.0.1:11434"
var world_builder_model: String = "llama3"
var dm_model: String:
	get:
		return world_builder_model
	set(val):
		world_builder_model = val
var character_model: String = "llama3"

## Context window sizes, in tokens, for each model role.
##
## These are the single source of truth: LLMClient sends them as `num_ctx` and
## PromptBuilder budgets against them via get_prompt_budget(). Nothing else may
## hardcode a context length. Previously PromptBuilder assumed 8192 for the
## character agent while this file sent num_ctx = 4096, so every NPC prompt was
## budgeted for twice the window it was actually served. See docs/migration_plan.md
## Appendix B-1.
##
## Both default to 8192. Raising either costs KV cache memory on the GPU, which
## matters because keep_alive is currently -1 and both models stay resident.
var world_builder_context: int = 8192
var character_context: int = 8192

## Tokens held back from the prompt budget so the model has room to answer.
## The character agent emits a JSON object with thinking, narration and dialogue
## fields and can be verbose. This reserve is deliberately generous because
## PromptBuilder still estimates tokens with a characters/4 heuristic rather than
## a real tokenizer (defect B-4, scheduled for migration Phase 2.5).
const PROMPT_RESPONSE_RESERVE: int = 1024

## Model roles. Pass one of these to the send_* functions so the correct context
## length is used regardless of how the models happen to be named.
const ROLE_AUTO: String = ""
const ROLE_CHARACTER: String = "character"
const ROLE_WORLD_BUILDER: String = "world_builder"

# Local Image Generation configuration
var image_gen_enabled: bool = false
var image_gen_provider: String = "automatic1111"
var image_gen_url: String = "http://127.0.0.1:7860"
var image_gen_denoising_strength: float = 0.65
var autosave_interval: int = 5
var chat_box_width: float = 900.0
var chat_box_height: float = 200.0
var chat_box_position_x: float = -1.0
var chat_box_position_y: float = -1.0

var _http_request: HTTPRequest

# Request Queue variables
enum RequestPriority {
	LOW = 0,
	HIGH = 1
}

var _request_queue: Array[Dictionary] = []
var _queue_processing: bool = false
var _active_req: Dictionary = {}
var _active_http_node: HTTPRequest = null
var _active_stream_req: LLMStreamRequest = null

# Testing support
var mock_response_handler: Callable = Callable()
var mock_embedding_handler: Callable = Callable()



# Stream state variables
var _client: HTTPClient = HTTPClient.new()
var _is_streaming: bool = false
var _stream_headers: PackedStringArray
var _stream_body: String = ""
var _chunk_buffer: String = ""
var _accumulated_response: String = ""
var _stream_started: bool = false
var _stream_has_entered_requesting: bool = false
var _stream_error_sent: bool = false
var _stream_timeout_timer: float = 0.0
const STREAM_TIMEOUT: float = 150.0


func _ready() -> void:
	_http_request = HTTPRequest.new()
	add_child(_http_request)
	_http_request.request_completed.connect(_on_request_completed)
	load_config()
	EventBus.location_changed.connect(func(_loc_id: String):
		if not CampaignState.campaign_id.is_empty():
			print("[LLMClient] Location changed, cancelling active stream.")
			cancel()
	)

func _process(delta: float) -> void:
	if not _is_streaming:
		return
		
	_stream_timeout_timer += delta
	if _stream_timeout_timer > STREAM_TIMEOUT:
		_emit_stream_error("Streaming request timed out.")
		return

		
	var status = _client.get_status()
	
	if status == HTTPClient.STATUS_DISCONNECTED:
		var parsed = _parse_url(api_url)
		var tls_options: TLSOptions = null
		if parsed.use_ssl:
			tls_options = TLSOptions.client()
		var err = _client.connect_to_host(parsed.host, parsed.port, tls_options)
		if err != OK:
			_emit_stream_error("Failed to connect to host: " + str(err))
			
	elif status == HTTPClient.STATUS_CONNECTING or status == HTTPClient.STATUS_RESOLVING:
		_client.poll()
		
	elif status == HTTPClient.STATUS_CONNECTED:
		if not _stream_started:
			_stream_started = true
			var err = _client.request(HTTPClient.METHOD_POST, "/api/generate", _stream_headers, _stream_body)
			if err != OK:
				_emit_stream_error("HTTP Request failed to start: " + str(err))
			else:
				_client.poll()
		else:
			if _stream_has_entered_requesting:
				_on_stream_completed()
			else:
				_client.poll()
			
	elif status == HTTPClient.STATUS_REQUESTING:
		_stream_has_entered_requesting = true
		_client.poll()
		
	elif status == HTTPClient.STATUS_BODY:
		_stream_has_entered_requesting = true
		_client.poll()
		if _client.has_response():
			var code = _client.get_response_code()
			if code != 200:
				_emit_stream_error("Server returned error code: " + str(code))
				return
				
			var chunk = _client.read_response_body_chunk()
			if chunk.size() > 0:
				_process_chunk_bytes(chunk)
				
	elif status == HTTPClient.STATUS_CONNECTION_ERROR or status == HTTPClient.STATUS_CANT_CONNECT or status == HTTPClient.STATUS_CANT_RESOLVE:
		_emit_stream_error("HTTP Client connection error (Status: " + str(status) + ")")

## Loads the global configuration
func load_config() -> void:
	var path_to_load = CONFIG_PATH
	var migrated = false
	if not FileAccess.file_exists(path_to_load):
		if FileAccess.file_exists("user://config.json"):
			path_to_load = "user://config.json"
			migrated = true
		else:
			print("[LLMClient] Config file not found. Using defaults: API URL: ", api_url, " | DM Model: ", world_builder_model, " | NPC Model: ", character_model)
			return
			
	print("[LLMClient] Loading configuration from ", path_to_load)
	var file = FileAccess.open(path_to_load, FileAccess.READ)
	if file:
		var content = file.get_as_text()
		file.close()
		var json = JSON.new()
		if json.parse(content) == OK and json.data is Dictionary:
			var data = json.data
			api_url = data.get("api_url", "http://127.0.0.1:11434")
			if "localhost" in api_url:
				api_url = api_url.replace("localhost", "127.0.0.1")
				print("[LLMClient] Upgraded API URL localhost to 127.0.0.1 to avoid macOS IPv6 handshake latency.")
				save_config()
			world_builder_model = data.get("world_builder_model", "llama3")
			character_model = data.get("character_model", "llama3")
			world_builder_context = int(data.get("world_builder_context", 8192))
			character_context = int(data.get("character_context", 8192))
			
			# Image generation configs
			image_gen_enabled = data.get("image_gen_enabled", false)
			image_gen_provider = data.get("image_gen_provider", "automatic1111")
			image_gen_url = data.get("image_gen_url", "http://127.0.0.1:7860")
			if "localhost" in image_gen_url:
				image_gen_url = image_gen_url.replace("localhost", "127.0.0.1")
			image_gen_denoising_strength = data.get("image_gen_denoising_strength", 0.65)
			autosave_interval = int(data.get("autosave_interval", 5))
			chat_box_width = float(data.get("chat_box_width", 900.0))
			chat_box_height = float(data.get("chat_box_height", 200.0))
			chat_box_position_x = float(data.get("chat_box_position_x", -1.0))
			chat_box_position_y = float(data.get("chat_box_position_y", -1.0))
			
			print("[LLMClient] Config loaded successfully. API URL: ", api_url, " | DM Model: ", world_builder_model, " | NPC Model: ", character_model)
			
			if migrated:
				print("[LLMClient] Migrated config from legacy config.json")
				save_config()
		else:
			push_warning("[LLMClient] Failed to parse config JSON. Using defaults.")

## Saves the global configuration
func save_config() -> void:
	var file = FileAccess.open(CONFIG_PATH, FileAccess.WRITE)
	if file:
		var data = {
			"api_url": api_url,
			"world_builder_model": world_builder_model,
			"character_model": character_model,
			"world_builder_context": world_builder_context,
			"character_context": character_context,
			"image_gen_enabled": image_gen_enabled,
			"image_gen_provider": image_gen_provider,
			"image_gen_url": image_gen_url,
			"image_gen_denoising_strength": image_gen_denoising_strength,
			"autosave_interval": autosave_interval,
			"chat_box_width": chat_box_width,
			"chat_box_height": chat_box_height,
			"chat_box_position_x": chat_box_position_x,
			"chat_box_position_y": chat_box_position_y
		}
		file.store_string(JSON.stringify(data, "\t"))
		file.close()

## Returns the context window size for a model role.
##
## Pass ROLE_CHARACTER or ROLE_WORLD_BUILDER explicitly wherever the role is
## known. ROLE_AUTO falls back to matching the model name, which is ambiguous
## when both roles are configured to the same model: the name alone cannot say
## which role the request is for. That ambiguity is harmless while both context
## values are equal, and is why they default to the same number, but callers
## should still pass the role.
func get_context_length(role: String = ROLE_AUTO, model_name: String = "") -> int:
	match role:
		ROLE_CHARACTER:
			return character_context
		ROLE_WORLD_BUILDER:
			return world_builder_context
		_:
			if not model_name.is_empty() and model_name == character_model:
				return character_context
			return world_builder_context


## Returns the tokens available for prompt content: the context window minus the
## reserve held back for the model's response. PromptBuilder budgets against this,
## never against the raw context length.
func get_prompt_budget(role: String = ROLE_AUTO, model_name: String = "") -> int:
	return maxi(get_context_length(role, model_name) - PROMPT_RESPONSE_RESERVE, 512)


## Returns true if the LLM client is busy processing, streaming, or running a stream request
func is_busy() -> bool:
	return _queue_processing or _is_streaming or (_active_stream_req != null)


## Sends a prompt to the local LLM generation endpoint (supports streaming)
func send_prompt(prompt: String, model_name: String = "", custom_url: String = "", stream: bool = true, role: String = ROLE_AUTO) -> void:
	var active_model = model_name if not model_name.is_empty() else character_model
	var active_url = custom_url if not custom_url.is_empty() else api_url.rstrip("/").path_join("api/generate")
	var active_ctx = get_context_length(role, active_model)
	
	print("[LLMClient] Sending request to model '%s' (Prompt length: %d chars, stream: %s)..." % [active_model, prompt.length(), str(stream)])
	
	var headers = ["Content-Type: application/json"]
	var body = {
		"model": active_model,
		"prompt": prompt,
		"stream": stream,
		"options": {
			"temperature": 0.7,
			"top_p": 0.9,
			"num_ctx": active_ctx
		},
		"keep_alive": -1
	}
	
	var json_body = JSON.stringify(body)
	
	if stream:
		# Cancel previous stream if active
		if _is_streaming:
			_client.close()
			
		_stream_headers = headers
		_stream_body = json_body
		_chunk_buffer = ""
		_accumulated_response = ""
		_stream_started = false
		_stream_has_entered_requesting = false
		_stream_error_sent = false
		_is_streaming = true
		_stream_timeout_timer = 0.0

	else:
		if not _http_request:
			printerr("HTTPRequest node not ready")
			request_failed.emit("Internal client node error")
			return
			
		var err = _http_request.request(active_url, headers, HTTPClient.METHOD_POST, json_body)
		if err != OK:
			printerr("HTTP Request start failed: ", err)
			request_failed.emit("Failed to issue HTTP request. Code: " + str(err))

func _process_chunk_bytes(bytes: PackedByteArray) -> void:
	_stream_timeout_timer = 0.0
	var text = bytes.get_string_from_utf8()

	_chunk_buffer += text
	
	var lines = _chunk_buffer.split("\n")
	_chunk_buffer = lines[-1] # keep the trailing partial line
	
	for i in range(lines.size() - 1):
		var line = lines[i].strip_edges()
		if line.is_empty():
			continue
			
		var json = JSON.new()
		if json.parse(line) == OK and json.data is Dictionary:
			var data = json.data
			if data.has("response"):
				var word = data["response"]
				_accumulated_response += word
				response_chunk_received.emit(word)
			if data.get("done", false):
				_on_stream_completed()

func _on_stream_completed() -> void:
	_is_streaming = false
	_client.close()
	
	if not _chunk_buffer.strip_edges().is_empty():
		var json = JSON.new()
		if json.parse(_chunk_buffer) == OK and json.data is Dictionary:
			var data = json.data
			if data.has("response"):
				_accumulated_response += data["response"]
				response_chunk_received.emit(data["response"])
				
	print("[LLMClient] Stream completed (Length: %d chars)." % _accumulated_response.length())
	response_received.emit(_accumulated_response)

func _emit_stream_error(msg: String) -> void:
	if _stream_error_sent:
		return
	_stream_error_sent = true
	_is_streaming = false
	_client.close()
	printerr("[LLMClient] Stream error: ", msg)
	request_failed.emit(msg)

func _parse_url(url: String) -> Dictionary:
	var host = "localhost"
	var port = 11434
	var use_ssl = false
	
	var clean = url.replace("http://", "").replace("https://", "")
	if url.begins_with("https://"):
		use_ssl = true
		
	var parts = clean.split(":")
	host = parts[0].split("/")[0]
	if parts.size() > 1:
		port = int(parts[1].split("/")[0])
	elif use_ssl:
		port = 443
	else:
		port = 80
		
	return {"host": host, "port": port, "use_ssl": use_ssl}

## Tests connection to the Ollama server and fetches available models
func test_connection(test_url: String, callback: Callable) -> void:
	var temp_http = HTTPRequest.new()
	temp_http.timeout = 25.0
	add_child(temp_http)
	var target_url = test_url.rstrip("/") + "/api/tags"
	print("[LLMClient] Starting connection test to: ", target_url)
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		var success = false
		var error_msg = ""
		var models: Array[String] = []
		print("[LLMClient] Connection test completed. Result: %d | HTTP Code: %d" % [result, response_code])
		if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
			var text = body.get_string_from_utf8()
			var json = JSON.new()
			if json.parse(text) == OK and json.data is Dictionary:
				success = true
				var model_list = json.data.get("models", [])
				for m in model_list:
					if m is Dictionary and m.has("name"):
						var m_name = str(m["name"])
						models.append(m_name)
						var base_name = m_name.split(":")[0]
						if base_name != m_name:
							models.append(base_name)
			else:
				error_msg = "Invalid JSON response from server."
		else:
			error_msg = "Could not connect to Ollama. Make sure the server is running."
			
		callback.call(success, error_msg, models)
		temp_http.queue_free()
	)
	
	var err = temp_http.request(target_url, [], HTTPClient.METHOD_GET)
	if err != OK:
		callback.call(false, "Failed to start HTTP request.", [])
		temp_http.queue_free()

func _on_request_completed(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray) -> void:
	if result != HTTPRequest.RESULT_SUCCESS:
		printerr("HTTP Request failed: result code ", result)
		request_failed.emit("Connection failed or timed out. (Code: " + str(result) + ")")
		return
		
	if response_code < 200 or response_code >= 300:
		printerr("Server returned error code: ", response_code)
		request_failed.emit("Server error: Code " + str(response_code))
		return
		
	var response_text = body.get_string_from_utf8()
	var json = JSON.new()
	var parse_err = json.parse(response_text)
	if parse_err != OK:
		printerr("Failed to parse Ollama JSON response: ", response_text)
		request_failed.emit("Malformed backend response JSON")
		return
		
	if json.data is Dictionary:
		var response_data = json.data
		if response_data.has("response"):
			var gen_text = response_data["response"]
			print("[LLMClient] Response received (Length: %d chars)." % gen_text.length())
			response_received.emit(gen_text)
			return
			
	# Fallback if raw text
	print("[LLMClient] Response received (raw fallback, Length: %d chars)." % response_text.length())
	response_received.emit(response_text)

# ==============================================================================
# Centralized Request Queue Execution
# ==============================================================================

func _enqueue_request(req: Dictionary) -> void:
	var priority = req.get("priority", RequestPriority.LOW)
	if priority == RequestPriority.HIGH:
		var insert_idx = 0
		while insert_idx < _request_queue.size() and _request_queue[insert_idx].get("priority", RequestPriority.LOW) == RequestPriority.HIGH:
			insert_idx += 1
		_request_queue.insert(insert_idx, req)
	else:
		_request_queue.append(req)
		
	var priority_name = "HIGH" if priority == RequestPriority.HIGH else "LOW"
	print("[LLMClient] Enqueued request type '%s' (Priority: %s). Queue size: %d" % [req.type, priority_name, _request_queue.size()])
	
	if priority == RequestPriority.HIGH and _queue_processing and not _active_req.is_empty():
		if _active_req.get("priority", RequestPriority.LOW) == RequestPriority.LOW:
			print("[LLMClient] HIGH priority request enqueued. Cancelling currently running LOW priority request type '%s'..." % _active_req.type)
			_cancel_active_low_priority_request()
			
	_process_queue()

func _cancel_active_low_priority_request() -> void:
	if _active_http_node and is_instance_valid(_active_http_node):
		var node = _active_http_node
		_active_http_node = null
		node.cancel_request()
		node.queue_free()
	
	# Snapshot the active request before clearing state so we can notify the caller.
	# The stored wrapped callbacks (for "custom" and "custom_vision") already call
	# _on_request_finished() internally, so we let them do it — the idempotency guard
	# in _on_request_finished() makes any redundant calls safe.
	var cancelled_req = _active_req.duplicate()
	
	# For stream types, the wrapped on_failed also calls _on_request_finished().
	# For non-stream types the wrapped callback does too.
	# So in all cases below, _on_request_finished() will be triggered by the callback.
	# We only call it directly here if there is no callback to fire.
	var callback_fired := false
	match cancelled_req.get("type", ""):
		"custom", "custom_vision":
			var cb = cancelled_req.get("callback")
			if cb and cb.is_valid():
				cb.call(false, "", "Request cancelled (preempted by higher priority request)")
				callback_fired = true
		"custom_stream", "custom_vision_stream":
			var on_failed = cancelled_req.get("on_failed")
			if on_failed and on_failed.is_valid():
				on_failed.call("Request cancelled (preempted by higher priority request)")
				callback_fired = true
	
	if not callback_fired:
		# No callback to fire (e.g. warmup or unknown type), advance the queue manually.
		_on_request_finished()

func _on_request_finished() -> void:
	if _active_req.is_empty() and not _queue_processing:
		return  # Guard against double-finish calls
	_active_req = {}
	_queue_processing = false
	print("[LLMClient] Request finished. Processing next queue item.")
	call_deferred("_process_queue")

func _process_queue() -> void:
	if _queue_processing:
		return
	if _request_queue.is_empty():
		return
		
	_queue_processing = true
	var req = _request_queue.pop_front()
	_active_req = req
	print("[LLMClient] Executing queued request type '%s' (Priority: %s). Remaining in queue: %d" % [req.type, "HIGH" if req.get("priority", RequestPriority.HIGH) == RequestPriority.HIGH else "LOW", _request_queue.size()])
	
	match req.type:
		"custom":
			_raw_send_custom_request(req.prompt, req.model_name, req.callback, req.timeout, req.get("json_mode", false), req.get("role", ROLE_AUTO))
		"custom_vision":
			_raw_send_custom_vision_request(req.prompt, req.image_path, req.model_name, req.callback, req.timeout, req.get("role", ROLE_AUTO))
		"custom_stream":
			_raw_send_custom_stream_request(req.prompt, req.model_name, req.on_chunk, req.on_completed, req.on_failed, req.timeout, req.json_mode, req.get("role", ROLE_AUTO))
		"custom_vision_stream":
			_raw_send_custom_vision_stream_request(req.prompt, req.image_path, req.model_name, req.on_chunk, req.on_completed, req.on_failed, req.timeout, req.get("role", ROLE_AUTO))
		"warmup":
			_raw_warmup_model(req.model_name, req.callback)

# ==============================================================================
# Public Budgeted & Queued APIs
# ==============================================================================

## Sends a custom prompt asynchronously using a temporary HTTPRequest node and returns the response via a callback (non-streaming, with keep_alive)
func send_custom_request(prompt: String, model_name: String, callback: Callable, timeout: float = 1500.0, priority: int = RequestPriority.LOW, json_mode: bool = false, role: String = ROLE_AUTO) -> void:
	if mock_response_handler.is_valid():
		mock_response_handler.call(prompt, model_name, callback, timeout)
		return
		
	var wrapped_callback = func(success: bool, response_text: String, error_msg: String):
		if callback.is_valid():
			callback.call(success, response_text, error_msg)
		_on_request_finished()
		
	_enqueue_request({
		"type": "custom",
		"prompt": prompt,
		"model_name": model_name,
		"callback": wrapped_callback,
		"timeout": timeout,
		"priority": priority,
		"json_mode": json_mode,
		"role": role
	})

## Sends a custom prompt with an image asynchronously using a temporary HTTPRequest node for vision tasks
func send_custom_vision_request(prompt: String, image_path: String, model_name: String, callback: Callable, timeout: float = 1500.0, priority: int = RequestPriority.LOW, role: String = ROLE_AUTO) -> void:
	var wrapped_callback = func(success: bool, response_text: String, error_msg: String):
		if callback.is_valid():
			callback.call(success, response_text, error_msg)
		_on_request_finished()
		
	_enqueue_request({
		"type": "custom_vision",
		"prompt": prompt,
		"image_path": image_path,
		"model_name": model_name,
		"callback": wrapped_callback,
		"timeout": timeout,
		"priority": priority,
		"role": role
	})

## Sends a custom prompt asynchronously and streams the response via callbacks.
func send_custom_stream_request(prompt: String, model_name: String, on_chunk: Callable, on_completed: Callable, on_failed: Callable, timeout: float = 300.0, json_mode: bool = false, priority: int = RequestPriority.HIGH, role: String = ROLE_AUTO) -> void:
	var wrapped_completed = func(full_text: String):
		if on_completed.is_valid():
			on_completed.call(full_text)
		_on_request_finished()
		
	var wrapped_failed = func(error_msg: String):
		if on_failed.is_valid():
			on_failed.call(error_msg)
		_on_request_finished()
		
	_enqueue_request({
		"type": "custom_stream",
		"prompt": prompt,
		"model_name": model_name,
		"on_chunk": on_chunk,
		"on_completed": wrapped_completed,
		"on_failed": wrapped_failed,
		"timeout": timeout,
		"json_mode": json_mode,
		"priority": priority,
		"role": role
	})

## Sends a custom prompt with an image asynchronously and streams the response via callbacks.
func send_custom_vision_stream_request(prompt: String, image_path: String, model_name: String, on_chunk: Callable, on_completed: Callable, on_failed: Callable, timeout: float = 300.0, priority: int = RequestPriority.HIGH, role: String = ROLE_AUTO) -> void:
	var wrapped_completed = func(full_text: String):
		if on_completed.is_valid():
			on_completed.call(full_text)
		_on_request_finished()
		
	var wrapped_failed = func(error_msg: String):
		if on_failed.is_valid():
			on_failed.call(error_msg)
		_on_request_finished()
		
	_enqueue_request({
		"type": "custom_vision_stream",
		"prompt": prompt,
		"image_path": image_path,
		"model_name": model_name,
		"on_chunk": on_chunk,
		"on_completed": wrapped_completed,
		"on_failed": wrapped_failed,
		"timeout": timeout,
		"priority": priority
	})

## Sends a request to Ollama to warm up/load a model into memory in the background
func warmup_model(model_name: String, callback: Callable = Callable(), priority: int = RequestPriority.LOW) -> void:
	var wrapped_callback = func(success: bool):
		if callback.is_valid():
			callback.call(success)
		_on_request_finished()
		
	_enqueue_request({
		"type": "warmup",
		"model_name": model_name,
		"callback": wrapped_callback,
		"priority": priority
	})

# ==============================================================================
# Raw Execution Methods
# ==============================================================================

func _raw_send_custom_request(prompt: String, model_name: String, callback: Callable, timeout: float = 1500.0, json_mode: bool = false, role: String = ROLE_AUTO) -> void:
	var temp_http = HTTPRequest.new()
	_active_http_node = temp_http
	temp_http.timeout = timeout
	add_child(temp_http)

	var active_model = model_name if not model_name.is_empty() else world_builder_model
	var active_url = api_url.rstrip("/").path_join("api/generate")
	print("[LLMClient] _raw_send_custom_request started. URL: %s | Model: %s" % [active_url, active_model])
	
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		if _active_http_node == temp_http:
			_active_http_node = null
		var success = false
		var response_text = ""
		var error_msg = ""
		print("[LLMClient] _raw_send_custom_request completed. Result: %d | HTTP Code: %d | Body bytes: %d" % [result, response_code, body.size()])
		if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
			var text = body.get_string_from_utf8()
			var json = JSON.new()
			if json.parse(text) == OK and json.data is Dictionary:
				var response_data = json.data
				if response_data.has("response"):
					response_text = response_data["response"]
					success = true
				else:
					response_text = text # raw fallback
					success = true
			else:
				response_text = text # raw fallback
				success = true
		else:
			error_msg = "HTTP Request failed. Code: " + str(response_code) + " | Result: " + str(result)
			
		callback.call(success, response_text, error_msg)
		temp_http.queue_free()
	)
	
	var active_ctx = get_context_length(role, active_model)
	
	var headers = ["Content-Type: application/json"]
	var req_body = {
		"model": active_model,
		"prompt": prompt,
		"stream": false,
		"options": {
			"temperature": 0.8,
			"top_p": 0.9,
			"num_ctx": active_ctx
		},
		"keep_alive": -1
	}
	if json_mode:
		req_body["format"] = "json"
	
	var err = temp_http.request(active_url, headers, HTTPClient.METHOD_POST, JSON.stringify(req_body))
	if err != OK:
		if _active_http_node == temp_http:
			_active_http_node = null
		callback.call(false, "", "Failed to start HTTP request.")
		temp_http.queue_free()

func _raw_send_custom_vision_request(prompt: String, image_path: String, model_name: String, callback: Callable, timeout: float = 1500.0, role: String = ROLE_AUTO) -> void:
	var temp_http = HTTPRequest.new()
	_active_http_node = temp_http
	temp_http.timeout = timeout
	add_child(temp_http)
	
	var active_model = model_name if not model_name.is_empty() else world_builder_model
	var active_url = api_url.rstrip("/").path_join("api/generate")
	print("[LLMClient] _raw_send_custom_vision_request started. URL: %s | Model: %s | Image: %s" % [active_url, active_model, image_path])
	
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		if _active_http_node == temp_http:
			_active_http_node = null
		var success = false
		var response_text = ""
		var error_msg = ""
		print("[LLMClient] _raw_send_custom_vision_request completed. Result: %d | HTTP Code: %d | Body bytes: %d" % [result, response_code, body.size()])
		if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
			var text = body.get_string_from_utf8()
			var json = JSON.new()
			if json.parse(text) == OK and json.data is Dictionary:
				var response_data = json.data
				if response_data.has("response"):
					response_text = response_data["response"]
					success = true
				else:
					response_text = text # raw fallback
					success = true
			else:
				response_text = text # raw fallback
				success = true
		else:
			error_msg = "HTTP Request failed. Code: " + str(response_code) + " | Result: " + str(result)
			
		callback.call(success, response_text, error_msg)
		temp_http.queue_free()
	)
	var active_ctx = get_context_length(role, active_model)
	
	var base64_images: Array[String] = []
	if not image_path.is_empty() and FileAccess.file_exists(image_path):
		var file = FileAccess.open(image_path, FileAccess.READ)
		if file:
			var bytes = file.get_buffer(file.get_length())
			var base64_str = Marshalls.raw_to_base64(bytes)
			base64_images.append(base64_str)
			file.close()
			
	var headers = ["Content-Type: application/json"]
	var req_body = {
		"model": active_model,
		"prompt": prompt,
		"stream": false,
		"options": {
			"temperature": 0.5,
			"top_p": 0.9,
			"num_ctx": active_ctx
		},
		"keep_alive": -1
	}
	
	if not base64_images.is_empty():
		req_body["images"] = base64_images
		
	var err = temp_http.request(active_url, headers, HTTPClient.METHOD_POST, JSON.stringify(req_body))
	if err != OK:
		if _active_http_node == temp_http:
			_active_http_node = null
		callback.call(false, "", "Failed to start HTTP request.")
		temp_http.queue_free()

func _raw_send_custom_stream_request(prompt: String, model_name: String, on_chunk: Callable, on_completed: Callable, on_failed: Callable, timeout: float = 300.0, json_mode: bool = false, role: String = ROLE_AUTO) -> void:
	var active_model = model_name if not model_name.is_empty() else world_builder_model
	
	# Estimate tokens (~4 characters per token) + leave 2048 tokens for generation
	var estimated_tokens = int(ceil(prompt.length() / 4.0)) + 2048
	# Determine context limit based on model and clamp
	var ctx_limit = get_context_length(role, active_model)
	var active_ctx = clampi(estimated_tokens, mini(4096, ctx_limit), ctx_limit)
	
	var stream_req = LLMStreamRequest.new()
	add_child(stream_req)
	_active_stream_req = stream_req
	stream_req.tree_exiting.connect(func():
		if _active_stream_req == stream_req:
			_active_stream_req = null
	)
	stream_req.start(api_url, prompt, active_model, active_ctx, on_chunk, on_completed, on_failed, timeout, [], json_mode)

func _raw_send_custom_vision_stream_request(prompt: String, image_path: String, model_name: String, on_chunk: Callable, on_completed: Callable, on_failed: Callable, timeout: float = 300.0, role: String = ROLE_AUTO) -> void:
	var active_model = model_name if not model_name.is_empty() else world_builder_model
	var active_ctx = get_context_length(role, active_model)
	
	var base64_images: Array[String] = []
	if not image_path.is_empty() and FileAccess.file_exists(image_path):
		var file = FileAccess.open(image_path, FileAccess.READ)
		if file:
			var bytes = file.get_buffer(file.get_length())
			var base64_str = Marshalls.raw_to_base64(bytes)
			base64_images.append(base64_str)
			file.close()
			
	var stream_req = LLMStreamRequest.new()
	add_child(stream_req)
	_active_stream_req = stream_req
	stream_req.tree_exiting.connect(func():
		if _active_stream_req == stream_req:
			_active_stream_req = null
	)
	stream_req.start(api_url, prompt, active_model, active_ctx, on_chunk, on_completed, on_failed, timeout, base64_images)

func _raw_warmup_model(model_name: String, callback: Callable = Callable()) -> void:
	if model_name.is_empty():
		if callback.is_valid():
			callback.call(false)
		return
	var temp_http = HTTPRequest.new()
	_active_http_node = temp_http
	temp_http.timeout = 300.0
	add_child(temp_http)
	var target_url = api_url.rstrip("/") + "/api/generate"
	var body = JSON.stringify({
		"model": model_name,
		"prompt": "",
		"keep_alive": -1
	})
	var headers = ["Content-Type: application/json"]
	print("[LLMClient] _raw_warmup_model in background: ", model_name)
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body_bytes: PackedByteArray):
		if _active_http_node == temp_http:
			_active_http_node = null
		var success = (result == HTTPRequest.RESULT_SUCCESS and response_code == 200)
		print("[LLMClient] _raw_warmup_model completed for ", model_name, ". Success: ", success, " | Result: ", result, " | Code: ", response_code)
		if callback.is_valid():
			callback.call(success)
		temp_http.queue_free()
	)
	var err = temp_http.request(target_url, headers, HTTPClient.METHOD_POST, body)
	if err != OK:
		if _active_http_node == temp_http:
			_active_http_node = null
		print("[LLMClient] Failed to start warmup request for ", model_name)
		if callback.is_valid():
			callback.call(false)
		temp_http.queue_free()

func cancel() -> void:
	print("[LLMClient] cancel() called. Clearing queue and aborting active requests.")
	# 1. Clear the queue
	_request_queue.clear()
	
	# 2. Cancel the active HTTP request node if running
	if _active_http_node and is_instance_valid(_active_http_node):
		var node = _active_http_node
		_active_http_node = null
		node.cancel_request()
		node.queue_free()
		
	# 3. Cancel the active streaming request
	if _active_stream_req and is_instance_valid(_active_stream_req):
		_active_stream_req.cancel()
		_active_stream_req = null
		
	# 4. Reset queue execution state
	_active_req = {}
	_queue_processing = false

## Gets the 768-dimensional embedding vector for a given text using nomic-embed-text
func get_embedding(text: String) -> Array:
	if mock_embedding_handler.is_valid():
		var mock_res = mock_embedding_handler.call(text)
		if mock_res is Array:
			return mock_res
			
	if text.strip_edges().is_empty():
		return []
		
	var temp_http = HTTPRequest.new()
	add_child(temp_http)
	
	var active_url = api_url.rstrip("/").path_join("api/embeddings")
	var headers = ["Content-Type: application/json"]
	var req_body = {
		"model": "nomic-embed-text",
		"prompt": text
	}
	
	var err = temp_http.request(active_url, headers, HTTPClient.METHOD_POST, JSON.stringify(req_body))
	if err != OK:
		temp_http.queue_free()
		return []
		
	var response = await temp_http.request_completed
	var result = response[0]
	var response_code = response[1]
	var body = response[3]
	
	if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
		var response_text = body.get_string_from_utf8()
		var json = JSON.new()
		if json.parse(response_text) == OK and json.data is Dictionary:
			var data = json.data
			if data.has("embedding"):
				var emb = data["embedding"]
				if emb is Array:
					var float_emb: Array[float] = []
					for val in emb:
						float_emb.append(float(val))
					temp_http.queue_free()
					return float_emb
					
	# Fallback to /api/embed if /api/embeddings failed (e.g. 404)
	var embed_url = api_url.rstrip("/").path_join("api/embed")
	var embed_body = {
		"model": "nomic-embed-text",
		"input": text
	}
	err = temp_http.request(embed_url, headers, HTTPClient.METHOD_POST, JSON.stringify(embed_body))
	if err != OK:
		temp_http.queue_free()
		return []
		
	response = await temp_http.request_completed
	temp_http.queue_free()
	
	result = response[0]
	response_code = response[1]
	body = response[3]
	
	if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
		var response_text = body.get_string_from_utf8()
		var json = JSON.new()
		if json.parse(response_text) == OK and json.data is Dictionary:
			var data = json.data
			if data.has("embeddings"):
				var embs = data["embeddings"]
				if embs is Array and not embs.is_empty() and embs[0] is Array:
					var float_emb: Array[float] = []
					for val in embs[0]:
						float_emb.append(float(val))
					return float_emb
	return []

## Returns true if the nomic-embed-text model is available locally in Ollama
func is_embedding_model_available() -> bool:
	var test_emb = await get_embedding("probe")
	return not test_emb.is_empty()
