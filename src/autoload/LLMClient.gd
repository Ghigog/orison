# res://src/autoload/LLMClient.gd
extends Node

signal response_received(response_text: String)
signal response_chunk_received(chunk: String)
signal request_failed(error_msg: String)

const CONFIG_PATH = "user://config.json"

var api_url: String = "http://localhost:11434"
var world_builder_model: String = "llama3"
var character_model: String = "llama3"

var _http_request: HTTPRequest

# Stream state variables
var _client: HTTPClient = HTTPClient.new()
var _is_streaming: bool = false
var _stream_headers: PackedStringArray
var _stream_body: String = ""
var _chunk_buffer: String = ""
var _accumulated_response: String = ""
var _stream_started: bool = false
var _stream_error_sent: bool = false

func _ready() -> void:
	_http_request = HTTPRequest.new()
	add_child(_http_request)
	_http_request.request_completed.connect(_on_request_completed)
	load_config()

func _process(_delta: float) -> void:
	if not _is_streaming:
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
			_on_stream_completed()
			
	elif status == HTTPClient.STATUS_REQUESTING:
		_client.poll()
		
	elif status == HTTPClient.STATUS_BODY:
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
	print("[LLMClient] Loading configuration from ", CONFIG_PATH)
	if not FileAccess.file_exists(CONFIG_PATH):
		print("[LLMClient] Config file not found. Using defaults: API URL: ", api_url, " | DM Model: ", world_builder_model, " | NPC Model: ", character_model)
		return
		
	var file = FileAccess.open(CONFIG_PATH, FileAccess.READ)
	if file:
		var content = file.get_as_text()
		file.close()
		var json = JSON.new()
		if json.parse(content) == OK and json.data is Dictionary:
			var data = json.data
			api_url = data.get("api_url", "http://localhost:11434")
			world_builder_model = data.get("world_builder_model", "llama3")
			character_model = data.get("character_model", "llama3")
			print("[LLMClient] Config loaded successfully. API URL: ", api_url, " | DM Model: ", world_builder_model, " | NPC Model: ", character_model)
		else:
			push_warning("[LLMClient] Failed to parse config JSON. Using defaults.")

## Saves the global configuration
func save_config() -> void:
	var file = FileAccess.open(CONFIG_PATH, FileAccess.WRITE)
	if file:
		var data = {
			"api_url": api_url,
			"world_builder_model": world_builder_model,
			"character_model": character_model
		}
		file.store_string(JSON.stringify(data, "\t"))
		file.close()

## Sends a prompt to the local LLM generation endpoint (supports streaming)
func send_prompt(prompt: String, model_name: String = "", custom_url: String = "", stream: bool = true) -> void:
	var active_model = model_name if not model_name.is_empty() else character_model
	var active_url = custom_url if not custom_url.is_empty() else api_url.rstrip("/").path_join("api/generate")
	var active_ctx = 4096 if active_model == character_model else 8192
	
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
		_stream_error_sent = false
		_is_streaming = true
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
	add_child(temp_http)
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		var success = false
		var error_msg = ""
		var models: Array[String] = []
		
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
	
	var target_url = test_url.rstrip("/") + "/api/tags"
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

## Sends a custom prompt asynchronously using a temporary HTTPRequest node and returns the response via a callback (non-streaming, with keep_alive)
func send_custom_request(prompt: String, model_name: String, callback: Callable) -> void:
	var temp_http = HTTPRequest.new()
	add_child(temp_http)
	
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		var success = false
		var response_text = ""
		var error_msg = ""
		
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
			error_msg = "HTTP Request failed. Code: " + str(response_code)
			
		callback.call(success, response_text, error_msg)
		temp_http.queue_free()
	)
	
	var active_model = model_name if not model_name.is_empty() else world_builder_model
	var active_url = api_url.rstrip("/").path_join("api/generate")
	var active_ctx = 4096 if active_model == character_model else 8192
	
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
	
	var err = temp_http.request(active_url, headers, HTTPClient.METHOD_POST, JSON.stringify(req_body))
	if err != OK:
		callback.call(false, "", "Failed to start HTTP request.")
		temp_http.queue_free()

## Sends a custom prompt with an image asynchronously using a temporary HTTPRequest node for vision tasks
func send_custom_vision_request(prompt: String, image_path: String, model_name: String, callback: Callable) -> void:
	var temp_http = HTTPRequest.new()
	add_child(temp_http)
	
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		var success = false
		var response_text = ""
		var error_msg = ""
		
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
			error_msg = "HTTP Request failed. Code: " + str(response_code)
			
		callback.call(success, response_text, error_msg)
		temp_http.queue_free()
	)
	
	var active_model = model_name if not model_name.is_empty() else world_builder_model
	var active_url = api_url.rstrip("/").path_join("api/generate")
	var active_ctx = 4096 if active_model == character_model else 8192
	
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
		callback.call(false, "", "Failed to start HTTP request.")
		temp_http.queue_free()

