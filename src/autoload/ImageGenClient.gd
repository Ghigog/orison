# res://src/autoload/ImageGenClient.gd
extends Node

signal request_completed(success: bool, image: Image, error_msg: String)

var mock_handler: Callable = Callable()

## Tests the connection to the Stable Diffusion server
func test_connection(callback: Callable) -> void:
	if mock_handler.is_valid():
		mock_handler.call({
			"method": "test_connection",
			"callback": callback
		})
		return

	var temp_http = HTTPRequest.new()
	add_child(temp_http)
	
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		var success = false
		var error_msg = ""
		
		if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
			success = true
		else:
			error_msg = "Could not connect to local Stable Diffusion API. Make sure it is running on " + LLMClient.image_gen_url
			
		callback.call(success, error_msg)
		temp_http.queue_free()
	)
	
	# Try calling a simple GET endpoint, like /sdapi/v1/options or / (root) depending on provider
	var target_url = LLMClient.image_gen_url.rstrip("/") + "/sdapi/v1/options"
	var err = temp_http.request(target_url, [], HTTPClient.METHOD_GET)
	if err != OK:
		callback.call(false, "Failed to start connection test request.")
		temp_http.queue_free()

## Sends a text-to-image request
func send_txt2img_request(prompt: String, width: int = 512, height: int = 512, callback: Callable = Callable()) -> void:
	if mock_handler.is_valid():
		mock_handler.call({
			"method": "send_txt2img_request",
			"prompt": prompt,
			"width": width,
			"height": height,
			"callback": callback
		})
		return

	var temp_http = HTTPRequest.new()
	add_child(temp_http)
	
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		var success = false
		var gen_image: Image = null
		var error_msg = ""
		
		if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
			var text = body.get_string_from_utf8()
			var json = JSON.new()
			if json.parse(text) == OK and json.data is Dictionary:
				var data = json.data
				var images_list = data.get("images", [])
				if not images_list.is_empty():
					var base64_str = str(images_list[0])
					var bytes = Marshalls.base64_to_raw(base64_str)
					gen_image = Image.new()
					var err = gen_image.load_png_from_buffer(bytes)
					if err == OK:
						success = true
					else:
						error_msg = "Failed to load generated PNG from buffer. Error: " + str(err)
				else:
					error_msg = "Stable Diffusion returned empty image list."
			else:
				error_msg = "Failed to parse JSON response from Stable Diffusion."
		else:
			error_msg = "HTTP request failed. Response code: " + str(response_code)
			
		if callback.is_valid():
			callback.call(success, gen_image, error_msg)
		request_completed.emit(success, gen_image, error_msg)
		temp_http.queue_free()
	)
	
	var target_url = LLMClient.image_gen_url.rstrip("/") + "/sdapi/v1/txt2img"
	var headers = ["Content-Type: application/json"]
	var req_body = {
		"prompt": prompt,
		"negative_prompt": "easynegative, blurry, lowres, monochrome, signature, watermark",
		"steps": 20,
		"cfg_scale": 7.0,
		"width": width,
		"height": height
	}
	
	var err = temp_http.request(target_url, headers, HTTPClient.METHOD_POST, JSON.stringify(req_body))
	if err != OK:
		if callback.is_valid():
			callback.call(false, null, "Failed to initiate txt2img request.")
		request_completed.emit(false, null, "Failed to initiate txt2img request.")
		temp_http.queue_free()

## Sends an image-to-image request, using the input Image as composition reference
func send_img2img_request(prompt: String, init_image: Image, callback: Callable = Callable()) -> void:
	if mock_handler.is_valid():
		mock_handler.call({
			"method": "send_img2img_request",
			"prompt": prompt,
			"init_image": init_image,
			"callback": callback
		})
		return

	var temp_http = HTTPRequest.new()
	add_child(temp_http)
	
	temp_http.request_completed.connect(func(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
		var success = false
		var gen_image: Image = null
		var error_msg = ""
		
		if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
			var text = body.get_string_from_utf8()
			var json = JSON.new()
			if json.parse(text) == OK and json.data is Dictionary:
				var data = json.data
				var images_list = data.get("images", [])
				if not images_list.is_empty():
					var base64_str = str(images_list[0])
					var bytes = Marshalls.base64_to_raw(base64_str)
					gen_image = Image.new()
					var err = gen_image.load_png_from_buffer(bytes)
					if err == OK:
						success = true
					else:
						error_msg = "Failed to load generated PNG from buffer. Error: " + str(err)
				else:
					error_msg = "Stable Diffusion returned empty image list."
			else:
				error_msg = "Failed to parse JSON response from Stable Diffusion."
		else:
			error_msg = "HTTP request failed. Response code: " + str(response_code)
			
		if callback.is_valid():
			callback.call(success, gen_image, error_msg)
		request_completed.emit(success, gen_image, error_msg)
		temp_http.queue_free()
	)
	
	# Save image to PNG buffer, then base64 encode
	var png_bytes = init_image.save_png_to_buffer()
	var base64_str = Marshalls.raw_to_base64(png_bytes)
	
	var target_url = LLMClient.image_gen_url.rstrip("/") + "/sdapi/v1/img2img"
	var headers = ["Content-Type: application/json"]
	var req_body = {
		"init_images": [base64_str],
		"prompt": prompt,
		"negative_prompt": "easynegative, blurry, lowres, monochrome, signature, watermark",
		"steps": 20,
		"cfg_scale": 7.0,
		"denoising_strength": LLMClient.image_gen_denoising_strength,
		"width": init_image.get_width(),
		"height": init_image.get_height()
	}
	
	var err = temp_http.request(target_url, headers, HTTPClient.METHOD_POST, JSON.stringify(req_body))
	if err != OK:
		if callback.is_valid():
			callback.call(false, null, "Failed to initiate img2img request.")
		request_completed.emit(false, null, "Failed to initiate img2img request.")
		temp_http.queue_free()
