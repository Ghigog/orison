# res://src/autoload/ImageGenManager.gd
extends Node

signal asset_generated(output_path: String, is_placeholder: bool)
signal asset_generation_started(output_path: String)
signal asset_generation_completed(output_path: String)
signal asset_generation_failed(output_path: String, error_msg: String)

const ProceduralArtEngineClass = preload("res://src/core/ProceduralArtEngine.gd")
var procedural_art_engine: Node
var _image_cache: Dictionary = {}
var generation_errors: Dictionary = {}


const EMOTION_PROMPTS = {
	"joy": "joyful expression, wide smiling, happy eyes, laughing, cheerful look",
	"anger": "angry expression, scowling, frowning, narrowed eyes, furious look, tensed face",
	"sadness": "sad expression, crying, tearing up, downturned eyes, melancholic look, grieving",
	"fear": "fearful expression, wide-eyed terrified look, gasping, frightened, scared",
	"trust": "trusting expression, gentle smiling, warm kind eyes, friendly look, welcoming",
	"disgust": "disgusted expression, sneering, wrinkled nose, grimacing, repulsed look",
	"surprise": "surprised expression, open mouth, wide startled eyes, shocked look, amazed",
	"serenity": "serene expression, calm peaceful face, slight gentle smile, relaxed eyes, tranquil look"
}

var _generations_in_progress: Dictionary = {}
var _image_request_queue: Array[Dictionary] = []
var _is_generating: bool = false

func get_style_prompt_modifier(style: String) -> String:
	match style:
		"Digital Anime Art":
			return "digital anime art style, vibrant colors, clean lines, detailed illustration"
		"Watercolor Fantasy":
			return "watercolor fantasy painting style, soft edges, textured paper, pastel colors, artistic"
		"Realistic Concept Art":
			return "realistic concept art style, highly detailed, dramatic lighting, photographic quality, digital painting"
		"Pixel Art Portrait":
			return "retro pixel art style, 8-bit, detailed pixels, vibrant pixelated colors, pixelated texture"
		_:
			return "detailed illustration"

func make_background_transparent(image: Image) -> Image:
	image.convert(Image.FORMAT_RGBA8)
	var width = image.get_width()
	var height = image.get_height()
	
	# If the image already has transparency, skip keying
	var has_transparency = false
	for y in range(height):
		for x in range(width):
			if image.get_pixel(x, y).a < 0.95:
				has_transparency = true
				break
		if has_transparency:
			break
	if has_transparency:
		return image
		
	# Let's inspect the corner pixels to see if the background is light or dark
	var corners = [
		image.get_pixel(0, 0),
		image.get_pixel(width - 1, 0),
		image.get_pixel(0, height - 1),
		image.get_pixel(width - 1, height - 1)
	]
	var avg_gray = 0.0
	for c in corners:
		avg_gray += c.get_luminance()
	avg_gray /= 4.0
	
	var is_dark_bg = avg_gray < 0.25
	
	for y in range(height):
		for x in range(width):
			var pixel = image.get_pixel(x, y)
			if is_dark_bg:
				# Key out dark colors (black background)
				if pixel.r < 0.12 and pixel.g < 0.12 and pixel.b < 0.12:
					# Soft transition
					var max_val = max(pixel.r, max(pixel.g, pixel.b))
					var alpha = smoothstep(0.02, 0.12, max_val)
					pixel.a = alpha
					image.set_pixel(x, y, pixel)
			else:
				# Key out light colors (white background)
				if pixel.r > 0.88 and pixel.g > 0.88 and pixel.b > 0.88:
					var min_val = min(pixel.r, min(pixel.g, pixel.b))
					var alpha = 1.0 - smoothstep(0.88, 0.98, min_val)
					pixel.a = alpha
					image.set_pixel(x, y, pixel)
	return image

func _ready() -> void:
	procedural_art_engine = ProceduralArtEngineClass.new()
	add_child(procedural_art_engine)
	print("[ImageGenManager] Initialized with ProceduralArtEngine.")

func _process(_delta: float) -> void:
	if not _is_generating and not _image_request_queue.is_empty():
		_process_image_queue()

func _process_image_queue() -> void:
	if _is_generating:
		return
	if _image_request_queue.is_empty():
		return
	if LLMClient.is_busy():
		return
		
	var req = _image_request_queue.pop_front()
	_raw_generate_asset(req.seed_name, req.prompt, req.category, req.output_path)

## Generates a game asset (scene, avatar, or item) asynchronously.
## If local AI generation is enabled, sends the prompt to Stable Diffusion for generation.
func generate_asset(seed_name: String, prompt: String, category: String, output_path: String) -> void:
	# Block all campaign image generations if CampaignState.campaign_id is empty (during onboarding).
	# Allow manual player character avatar generation via the Magic Wand.
	if CampaignState.campaign_id.is_empty():
		var is_player_avatar = (category.to_lower() == "avatar" and (
			seed_name == "player" or 
			seed_name == "temp_pc_avatar" or 
			output_path.get_file().contains("temp_pc_avatar") or 
			output_path == "user://temp_pc_avatar.png"
		))
		if not is_player_avatar:
			print("[ImageGenManager] Blocking campaign image generation during onboarding: ", output_path)
			return

	print("[ImageGenManager] Enqueuing generation for category '%s' with seed '%s'" % [category, seed_name])
	
	# Avoid adding duplicate requests to the queue
	for req in _image_request_queue:
		if req.output_path == output_path:
			print("[ImageGenManager] Request for %s already in queue." % output_path)
			return
			
	if _generations_in_progress.has(output_path):
		print("[ImageGenManager] Generation already in progress for: ", output_path)
		return
		
	_image_request_queue.append({
		"seed_name": seed_name,
		"prompt": prompt,
		"category": category,
		"output_path": output_path
	})
	
	_process_image_queue()

func _raw_generate_asset(seed_name: String, prompt: String, category: String, output_path: String) -> void:
	print("[ImageGenManager] Raw starting generation for category '%s' with seed '%s'" % [category, seed_name])
	_is_generating = true
	invalidate_cache(seed_name, category)
	invalidate_cache(output_path, category)
	
	if _generations_in_progress.has(output_path):
		print("[ImageGenManager] Generation already in progress for: ", output_path)
		_is_generating = false
		_process_image_queue()
		return
		
	_generations_in_progress[output_path] = true
	if generation_errors.has(output_path):
		generation_errors.erase(output_path)
		
	asset_generation_started.emit(output_path)
	
	# Ensure directory exists
	var dir_path = output_path.get_base_dir()
	if not DirAccess.dir_exists_absolute(dir_path):
		var err = DirAccess.make_dir_recursive_absolute(dir_path)
		if err != OK:
			printerr("[ImageGenManager] Failed to create directories for: ", dir_path)
			_generations_in_progress.erase(output_path)
			var err_msg = "Failed to create directory for asset."
			generation_errors[output_path] = err_msg
			asset_generation_failed.emit(output_path, err_msg)
			asset_generated.emit(output_path, false)
			_is_generating = false
			_process_image_queue()
			return

	var width: int = 512
	var height: int = 512
	
	match category.to_lower():
		"scene", "location":
			width = 768
			height = 512
		"avatar", "character":
			width = 512
			height = 512
		"item":
			width = 512
			height = 512
		_:
			width = 512
			height = 512

	# Generate with Stable Diffusion if enabled (txt2img)
	if LLMClient.image_gen_enabled:
		print("[ImageGenManager] dispatching txt2img generation to local Stable Diffusion...")
		
		# Resolve art style
		var art_style = "Digital Anime Art"
		if CampaignState and CampaignState.state and CampaignState.state.has("adventure_meta"):
			art_style = CampaignState.state.adventure_meta.get("art_style", "Digital Anime Art")
		else:
			# Fallback to OnboardingFlow dropdown lookup
			var main_root = get_tree().root
			var dropdown = main_root.find_child("CampaignArtStyleDropdown", true, false) as OptionButton
			if dropdown:
				art_style = dropdown.get_item_text(dropdown.selected)
				
		var dispatch_sd = func(final_visual_prompt: String):
			var sd_prompt = final_visual_prompt
			if category.to_lower() == "avatar" or category.to_lower() == "character":
				var style_mod = get_style_prompt_modifier(art_style)
				sd_prompt = "masterpiece, dnd character portrait, detailed facial features, isolated on a solid black background, standalone character sprite, transparent background, png, alpha channel, " + style_mod + ", " + final_visual_prompt
			elif category.to_lower() == "scene" or category.to_lower() == "location":
				sd_prompt = "masterpiece, environment scenery concept art, atmospheric background, " + final_visual_prompt
			elif category.to_lower() == "item":
				sd_prompt = "masterpiece, dnd item icon, fantasy object, game icon, glowing details, " + final_visual_prompt
				
			ImageGenClient.send_txt2img_request(sd_prompt, width, height, func(success: bool, refined_image: Image, error_msg: String):
				_generations_in_progress.erase(output_path)
				if success and refined_image:
					invalidate_cache(seed_name, category)
					invalidate_cache(output_path, category)
					if category.to_lower() == "avatar" or category.to_lower() == "character":
						refined_image = make_background_transparent(refined_image)
					var ref_err = refined_image.save_png(output_path)
					if ref_err == OK:
						print("[ImageGenManager] Overwrote placeholder with Stable Diffusion txt2img image: ", output_path)
						asset_generation_completed.emit(output_path)
						asset_generated.emit(output_path, false)
					else:
						var err_str = "Failed to save generated image: " + error_string(ref_err)
						printerr("[ImageGenManager] ", err_str)
						generation_errors[output_path] = err_str
						asset_generation_failed.emit(output_path, err_str)
						asset_generated.emit(output_path, false)
				else:
					var err_str = "AI generation failed: " + error_msg
					printerr("[ImageGenManager] ", err_str)
					generation_errors[output_path] = err_str
					asset_generation_failed.emit(output_path, err_str)
					asset_generated.emit(output_path, false)
				
				_is_generating = false
				_process_image_queue()
			)
			
		if category.to_lower() == "scene" or category.to_lower() == "location":
			var clean_desc = prompt.strip_edges()
			var summarize_prompt = (
				"You are an expert Stable Diffusion prompt engineer.\n" +
				"Analyze the following location description:\n" +
				"---\n" + clean_desc + "\n---\n" +
				"Create a concise, comma-separated text2img prompt describing the visual appearance, environment, lighting, landscape, and style.\n" +
				"Do NOT include any narrative explanation, characters, quotes, action, introduction, or formatting. Output ONLY the raw comma-separated prompt tags."
			)
			LLMClient.send_custom_request(summarize_prompt, LLMClient.world_builder_model, func(llm_success: bool, response_text: String, llm_error: String):
				var final_prompt = prompt
				if llm_success and not response_text.strip_edges().is_empty():
					final_prompt = response_text.strip_edges().replace("\n", " ").replace("\"", "")
					print("[ImageGenManager] LLM successfully summarized location description to visual prompt: ", final_prompt)
				else:
					print("[ImageGenManager] LLM summarization failed or returned empty. Falling back to raw description: ", llm_error)
				dispatch_sd.call(final_prompt)
			)
		else:
			dispatch_sd.call(prompt)
	else:
		_generations_in_progress.erase(output_path)
		var err_msg = "AI image generation is disabled."
		generation_errors[output_path] = err_msg
		asset_generation_failed.emit(output_path, err_msg)
		asset_generated.emit(output_path, false)
		_is_generating = false
		_process_image_queue()

## Centralized helper to get a real image. Removes procedural fallback placeholders for characters and scenes.
## If size is Vector2i(0, 0), default sizes for each entity type are used.
func get_image_or_fallback(entity_id: String, entity_type: String, size: Vector2i = Vector2i(0, 0)) -> Texture2D:
	if size == Vector2i(0, 0):
		match entity_type.to_lower():
			"scene", "location":
				size = Vector2i(768, 512)
			"avatar", "character":
				size = Vector2i(512, 512)
			"item":
				size = Vector2i(512, 512)
			_:
				size = Vector2i(512, 512)

	# Check cache first
	var cache_key = "%s_%s_%dx%d" % [entity_id, entity_type, size.x, size.y]
	if _image_cache.has(cache_key):
		return _image_cache[cache_key]

	var texture: Texture2D = null
	var real_path = ""

	# Resolve real image path
	if FileAccess.file_exists(entity_id):
		real_path = entity_id
	else:
		match entity_type.to_lower():
			"avatar", "character":
				# A. Check if the entity_id has an emotion suffix (e.g. char_id_emotion)
				var emotion = ""
				var base_char_id = entity_id
				var emotions_list = ["joy", "anger", "sadness", "fear", "trust", "disgust", "surprise", "serenity"]
				for emo in emotions_list:
					if entity_id.ends_with("_" + emo):
						emotion = emo
						base_char_id = entity_id.substr(0, entity_id.length() - emo.length() - 1)
						break
				
				if not emotion.is_empty():
					real_path = get_avatar_path(base_char_id, emotion)
					if not FileAccess.file_exists(real_path):
						# Fall back to base character avatar texture
						return await get_image_or_fallback(base_char_id, "avatar", size)
				else:
					# Try CampaignState character properties
					if CampaignState and CampaignState.has_method("get_character"):
						var character = CampaignState.get_character(entity_id)
						var avatar_path = character.get("avatar", "")
						if not avatar_path.is_empty() and FileAccess.file_exists(avatar_path):
							real_path = avatar_path
					if real_path.is_empty():
						for ext in ["png", "jpg", "jpeg"]:
							var test_path = "user://assets/characters/%s.%s" % [entity_id, ext]
							if FileAccess.file_exists(test_path):
								real_path = test_path
								break
					if real_path.is_empty() and entity_id == "temp_pc_avatar":
						real_path = "user://temp_pc_avatar.png"
						
			"scene", "location":
				real_path = get_scene_path(entity_id)
				if not FileAccess.file_exists(real_path):
					for ext in ["png", "jpg", "jpeg"]:
						var test_path = "user://assets/scenes/%s.%s" % [entity_id, ext]
						if FileAccess.file_exists(test_path):
							real_path = test_path
							break
							
			"item":
				for ext in ["png", "jpg", "jpeg"]:
					var test_path = "user://assets/items/%s.%s" % [entity_id, ext]
					if FileAccess.file_exists(test_path):
						real_path = test_path
						break

	# Load real image if path found
	if not real_path.is_empty() and FileAccess.file_exists(real_path):
		var img = Image.load_from_file(real_path)
		if img:
			texture = ImageTexture.create_from_image(img)
			_image_cache[cache_key] = texture
			return texture

	# Items still use procedural fallback
	if entity_type.to_lower() == "item":
		var fallback_image = await procedural_art_engine.generate_item(entity_id, entity_id, size.x, size.y)
		if fallback_image:
			texture = ImageTexture.create_from_image(fallback_image)
			_image_cache[cache_key] = texture
			return texture

	# No file exists and no fallback — return null.
	# Generation must be triggered explicitly via generate_character_portrait() or generate_scene_background().
	return null

## Invalidates the cache for a specific entity
func invalidate_cache(entity_id: String, entity_type: String) -> void:
	var keys_to_remove = []
	var prefix = "%s_%s_" % [entity_id, entity_type]
	for key in _image_cache.keys():
		if key.begins_with(prefix):
			keys_to_remove.append(key)
	for key in keys_to_remove:
		_image_cache.erase(key)
		print("[ImageGenManager] Invalidated cache for: ", key)

## Explicit manual trigger: uses the heavy world-builder LLM to read the full character
## profile and produce a concise Stable Diffusion visual description, then dispatches
## to Stable Diffusion. Idempotent — safe to call multiple times.
func generate_character_portrait(char_id: String) -> void:
	if not LLMClient.image_gen_enabled:
		return
	if CampaignState.campaign_id.is_empty():
		return
	var target_path = get_avatar_path(char_id)
	if _generations_in_progress.has(target_path):
		print("[ImageGenManager] Portrait generation already in progress for: ", char_id)
		return

	var character = CampaignState.get_character(char_id)
	if character.is_empty():
		printerr("[ImageGenManager] generate_character_portrait: character not found: ", char_id)
		return

	# Mark as in-progress immediately so the UI can show a spinner
	_generations_in_progress[target_path] = true
	asset_generation_started.emit(target_path)

	var char_name = character.get("name", char_id.capitalize())
	var biography = character.get("biography", "").strip_edges()
	var phys_desc = character.get("physical_description", "").strip_edges()

	var profile_text = "Character Name: %s\n" % char_name
	if not phys_desc.is_empty():
		profile_text += "Physical Description: %s\n" % phys_desc
	if not biography.is_empty():
		profile_text += "Biography: %s\n" % biography

	var art_style = "Digital Anime Art"
	if CampaignState.state and CampaignState.state.has("adventure_meta"):
		art_style = CampaignState.state.adventure_meta.get("art_style", "Digital Anime Art")
	var style_mod = get_style_prompt_modifier(art_style)

	var llm_prompt = (
		"You are a professional Stable Diffusion prompt engineer.\n"
		+ "Read the following character profile and write a concise, comma-separated "
		+ "visual description for generating a character portrait image.\n"
		+ "Focus ONLY on physical appearance: face shape, hair color and style, eye color, "
		+ "skin tone, body build, clothing, accessories, and any notable visual features such as scars or tattoos.\n"
		+ "Do NOT include personality traits, backstory, relationships, emotions, or narrative. "
		+ "Output ONLY raw comma-separated visual tags. Maximum 80 words.\n\n"
		+ profile_text
		+ "\nOutput:"
	)

	print("[ImageGenManager] Querying LLM for portrait prompt for: ", char_name)
	LLMClient.send_custom_request(
		llm_prompt,
		LLMClient.world_builder_model,
		func(success: bool, response_text: String, error_msg: String):
			var visual_prompt = char_name
			if success and not response_text.strip_edges().is_empty():
				visual_prompt = response_text.strip_edges().replace("\n", ", ")
				print("[ImageGenManager] LLM portrait prompt for '%s': %s" % [char_name, visual_prompt])
			else:
				print("[ImageGenManager] LLM portrait prompt failed (%s), using name fallback." % error_msg)
			# Release the early in-progress guard so generate_asset can re-acquire it
			_generations_in_progress.erase(target_path)
			generate_asset(char_id, visual_prompt, "avatar", target_path),
		60.0,
		LLMClient.RequestPriority.LOW
	)

## Explicit manual trigger: queues background scene generation for a location.
## Uses the same LLM summarization pass as the Snapshot button.
func generate_scene_background(location_id: String) -> void:
	if not LLMClient.image_gen_enabled:
		return
	if CampaignState.campaign_id.is_empty():
		return
	var target_path = get_scene_path(location_id)
	if _generations_in_progress.has(target_path):
		print("[ImageGenManager] Scene generation already in progress for: ", location_id)
		return
	var desc = location_id
	if CampaignState.graph_manager:
		var loc_node = CampaignState.graph_manager.get_node(location_id)
		if loc_node and not loc_node.is_empty():
			desc = loc_node.get("desc", location_id)
	generate_asset(location_id, desc, "scene", target_path)

## Generates an emotional variant for a character in the background.
func generate_emotion_variant(char_id: String, emotion: String) -> void:
	if not LLMClient.image_gen_enabled:
		return
		
	var target_path = get_avatar_path(char_id, emotion)
	if FileAccess.file_exists(target_path):
		return
		
	var key = "%s_%s" % [char_id, emotion]
	if _generations_in_progress.has(key):
		return
		
	_generations_in_progress[key] = true
	print("[ImageGenManager] generate_emotion_variant starting for: ", key)
	
	var physical_desc = ""
	if CampaignState and CampaignState.has_method("get_character"):
		var character = CampaignState.get_character(char_id)
		if not character.is_empty():
			if char_id == "player":
				physical_desc = character.get("physical_description", "")
			else:
				physical_desc = character.get("biography", "")
				
	if physical_desc.is_empty():
		physical_desc = char_id.capitalize()
		
	var emotion_prompt = EMOTION_PROMPTS.get(emotion, "")
	var final_prompt = physical_desc
	if not emotion_prompt.is_empty():
		final_prompt += ", " + emotion_prompt
		
	var cleanup: Callable
	cleanup = func(path: String, is_placeholder: bool):
		if path == target_path:
			_generations_in_progress.erase(key)
			if asset_generated.is_connected(cleanup):
				asset_generated.disconnect(cleanup)
	asset_generated.connect(cleanup)
	
	generate_asset(char_id + "_" + emotion, final_prompt, "avatar", target_path)

func get_avatar_path(char_id: String, emotion: String = "") -> String:
	if char_id == "temp_pc_avatar":
		return "user://temp_pc_avatar.png"
	var campaign_id = "default"
	if CampaignState and CampaignState.state:
		campaign_id = CampaignState.state.get("adventure_meta", {}).get("campaign_id", "default")
	if emotion.is_empty():
		return "user://adventures/%s/generated_assets/character_%s.png" % [campaign_id, char_id]
	else:
		return "user://adventures/%s/generated_assets/character_%s_%s.png" % [campaign_id, char_id, emotion]

func get_scene_path(location_id: String) -> String:
	var campaign_id = "default"
	if CampaignState and CampaignState.state:
		campaign_id = CampaignState.state.get("adventure_meta", {}).get("campaign_id", "default")
	var filename = location_id.to_lower().replace(" ", "_")
	return "user://adventures/%s/generated_assets/scene_%s.png" % [campaign_id, filename]

func get_asset_state(path: String) -> Dictionary:
	if _generations_in_progress.has(path):
		return { "status": "generating", "error": "" }
	elif generation_errors.has(path):
		return { "status": "error", "error": generation_errors[path] }
	elif FileAccess.file_exists(path):
		return { "status": "success", "error": "" }
	return { "status": "none", "error": "" }
