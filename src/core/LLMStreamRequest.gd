# res://src/core/LLMStreamRequest.gd
class_name LLMStreamRequest
extends Node

signal chunk_received(chunk: String)
signal completed(full_text: String)
signal failed(error_msg: String)

var api_url: String
var prompt: String
var model_name: String
var num_ctx: int = 8192
var timeout_seconds: float = 300.0

var _client: HTTPClient = HTTPClient.new()
var _is_running: bool = false
var _stream_headers: PackedStringArray
var _stream_body: String = ""
var _chunk_buffer: String = ""
var _accumulated_response: String = ""
var _stream_started: bool = false
var _has_entered_requesting: bool = false
var _elapsed_time: float = 0.0
var _chunk_count: int = 0
var _response_code: int = -1
var _chunk_timeout_timer: float = 0.0
const CHUNK_TIMEOUT: float = 900.0 # Time out if no data arrives for 15 minutes

var _on_chunk_callback: Callable
var _on_completed_callback: Callable
var _on_failed_callback: Callable

func start(
	p_api_url: String,
	p_prompt: String,
	p_model_name: String,
	p_num_ctx: int,
	p_on_chunk: Callable,
	p_on_completed: Callable,
	p_on_failed: Callable,
	p_timeout: float = 300.0,
	p_images: Array = [],
	p_json_mode: bool = false
) -> void:
	api_url = p_api_url
	prompt = p_prompt
	model_name = p_model_name
	num_ctx = p_num_ctx
	_on_chunk_callback = p_on_chunk
	_on_completed_callback = p_on_completed
	_on_failed_callback = p_on_failed
	timeout_seconds = p_timeout
	
	_chunk_buffer = ""
	_accumulated_response = ""
	_stream_started = false
	_has_entered_requesting = false
	_elapsed_time = 0.0
	_chunk_timeout_timer = 0.0
	_chunk_count = 0
	_response_code = -1
	
	var mode_str = " [JSON mode]" if p_json_mode else ""
	print("[LLMStream] Starting request to model '%s' (ctx: %d, timeout: %ds, prompt length: %d chars)%s" % [model_name, num_ctx, int(timeout_seconds), prompt.length(), mode_str])
	
	# Set up headers and request payload
	_stream_headers = ["Content-Type: application/json"]
	var body = {
		"model": model_name,
		"prompt": prompt,
		"stream": true,
		"options": {
			"temperature": 0.8,
			"top_p": 0.9,
			"num_ctx": num_ctx
		},
		"keep_alive": -1
	}
	if p_json_mode:
		body["format"] = "json"
	if not p_images.is_empty():
		body["images"] = p_images
	_stream_body = JSON.stringify(body)
	
	_is_running = true

func _process(delta: float) -> void:
	if not _is_running:
		return
		
	_elapsed_time += delta
	_chunk_timeout_timer += delta
	
	if _elapsed_time > timeout_seconds:
		# Prevent timeout if we are actively receiving text chunks
		if _chunk_count == 0:
			_fail("Request timed out after %d seconds." % int(timeout_seconds))
			return
		
	if _chunk_timeout_timer > CHUNK_TIMEOUT:
		_fail("Connection stalled (no data received for %d seconds)." % int(CHUNK_TIMEOUT))
		return
		
	var status = _client.get_status()
	
	if status == HTTPClient.STATUS_DISCONNECTED:
		var parsed = _parse_url(api_url)
		var tls_options: TLSOptions = null
		if parsed.use_ssl:
			tls_options = TLSOptions.client()
		print("[LLMStream] Connecting to %s:%d..." % [parsed.host, parsed.port])
		var err = _client.connect_to_host(parsed.host, parsed.port, tls_options)
		if err != OK:
			_fail("Failed to connect to host: " + str(err))
			
	elif status == HTTPClient.STATUS_CONNECTING or status == HTTPClient.STATUS_RESOLVING:
		_client.poll()
		
	elif status == HTTPClient.STATUS_CONNECTED:
		if not _stream_started:
			_stream_started = true
			print("[LLMStream] Connected. Sending POST /api/generate for model '%s'..." % model_name)
			var err = _client.request(HTTPClient.METHOD_POST, "/api/generate", _stream_headers, _stream_body)
			if err != OK:
				_fail("HTTP Request failed to start: " + str(err))
			else:
				_client.poll() # Transition state immediately
		else:
			if _has_entered_requesting:
				print("[LLMStream] Status returned to CONNECTED after requesting. Completing. (chunks: %d, response length: %d)" % [_chunk_count, _accumulated_response.length()])
				_complete()
			else:
				_client.poll()
			
	elif status == HTTPClient.STATUS_REQUESTING:
		_has_entered_requesting = true
		_client.poll()
		
	elif status == HTTPClient.STATUS_BODY:
		_has_entered_requesting = true
		_client.poll()
		if _client.has_response():
			var code = _client.get_response_code()
			if _response_code == -1:
				_response_code = code
				print("[LLMStream] Received HTTP %d from server for model '%s'." % [code, model_name])
			if code != 200:
				_fail("Server returned error code: " + str(code))
				return
				
			var chunk = _client.read_response_body_chunk()
			if chunk.size() > 0:
				_chunk_timeout_timer = 0.0 # Reset chunk timer
				_chunk_count += 1
				_process_chunk_bytes(chunk)
				
	elif status == HTTPClient.STATUS_CONNECTION_ERROR or status == HTTPClient.STATUS_CANT_CONNECT or status == HTTPClient.STATUS_CANT_RESOLVE:
		_fail("HTTP Client connection error (Status: " + str(status) + ")")

func _process_chunk_bytes(bytes: PackedByteArray) -> void:
	var text = bytes.get_string_from_utf8()
	_chunk_buffer += text
	
	var lines = _chunk_buffer.split("\n")
	_chunk_buffer = lines[-1] # keep trailing partial line
	
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
				if _on_chunk_callback.is_valid():
					_on_chunk_callback.call(word)
			if data.get("done", false):
				_complete()

func _complete() -> void:
	_is_running = false
	_client.close()
	
	# Parse trailing partial buffer content if present
	if not _chunk_buffer.strip_edges().is_empty():
		var json = JSON.new()
		if json.parse(_chunk_buffer) == OK and json.data is Dictionary:
			var data = json.data
			if data.has("response"):
				_accumulated_response += data["response"]
				if _on_chunk_callback.is_valid():
					_on_chunk_callback.call(data["response"])

	if _accumulated_response.strip_edges().is_empty():
		print("[LLMStream] WARNING: Model '%s' completed with EMPTY response after %.1fs. (HTTP %d, chunks received: %d)" % [model_name, _elapsed_time, _response_code, _chunk_count])
	else:
		print("[LLMStream] Completed for model '%s'. Response: %d chars, %d chunks, %.1fs elapsed." % [model_name, _accumulated_response.length(), _chunk_count, _elapsed_time])

	if _on_completed_callback.is_valid():
		_on_completed_callback.call(_accumulated_response)
	queue_free()

func _fail(msg: String) -> void:
	_is_running = false
	_client.close()
	print("[LLMStream] FAILED for model '%s' after %.1fs: %s" % [model_name, _elapsed_time, msg])
	if _on_failed_callback.is_valid():
		_on_failed_callback.call(msg)
	queue_free()

func cancel() -> void:
	if not _is_running:
		return
	_is_running = false
	_client.close()
	print("[LLMStream] Cancelled request for model '%s'." % model_name)
	if _on_failed_callback.is_valid():
		_on_failed_callback.call("Stream cancelled by user")
	queue_free()


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
