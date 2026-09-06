# res://src/core/ProceduralArtEngine.gd
extends Node
class_name ProceduralArtEngine

## Helper to generate a deterministic color palette from a string seed
func get_colors_from_seed(seed_str: String, count: int = 3) -> Array[Color]:
	var colors: Array[Color] = []
	var hash_val = seed_str.hash()
	var rng = RandomNumberGenerator.new()
	rng.seed = hash_val
	
	# Generate a base hue
	var base_hue = rng.randf()
	for i in range(count):
		# Create harmonious colors using HSL/HSV shift
		var hue = fmod(base_hue + (i * 0.2), 1.0)
		var sat = rng.randf_range(0.4, 0.75)
		var val = rng.randf_range(0.25, 0.85)
		colors.append(Color.from_hsv(hue, sat, val))
		
	return colors

## Generates a procedural landscape scene image
func generate_scene(description: String, width: int = 512, height: int = 512) -> Image:
	if DisplayServer.get_name() == "headless":
		var mock = Image.create(width, height, false, Image.FORMAT_RGBA8)
		mock.fill(Color(0.2, 0.3, 0.4))
		return mock
		
	var viewport = SubViewport.new()
	viewport.size = Vector2i(width, height)
	viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(viewport)
	
	var colors = get_colors_from_seed(description, 5)
	
	# 1. Sky Gradient Background
	var bg = TextureRect.new()
	bg.size = Vector2(width, height)
	var grad = Gradient.new()
	grad.colors = [colors[0], colors[1]]
	grad.offsets = [0.0, 1.0]
	
	var grad_tex = GradientTexture2D.new()
	grad_tex.gradient = grad
	grad_tex.fill_from = Vector2(0.5, 0.0)
	grad_tex.fill_to = Vector2(0.5, 1.0)
	bg.texture = grad_tex
	viewport.add_child(bg)
	
	# Seed generator for mountains
	var rng = RandomNumberGenerator.new()
	rng.seed = description.hash()
	
	# 2. Add mountain layers (back, mid, front)
	for layer in range(3):
		var poly = Polygon2D.new()
		var points = PackedVector2Array()
		
		# Back range: higher, lighter, lower contrast
		# Front range: lower, darker, higher contrast
		var baseline = height * (0.4 + layer * 0.15)
		var amplitude = height * (0.18 - layer * 0.04)
		var freq = rng.randf_range(0.005, 0.02)
		var phase = rng.randf_range(0, 100)
		
		points.append(Vector2(0, height))
		for x in range(0, width + 10, 10):
			var y = baseline + sin(x * freq + phase) * amplitude + cos(x * freq * 0.5 + phase * 2.0) * (amplitude * 0.3)
			points.append(Vector2(x, y))
		points.append(Vector2(width, height))
		
		poly.polygon = points
		poly.color = colors[2 + (layer % 3)]
		poly.color.a = 0.5 + (layer * 0.2) # Higher opacity foreground
		viewport.add_child(poly)
		
	# 3. Add simple atmospheric moon/sun
	var sun = Polygon2D.new()
	var sun_points = PackedVector2Array()
	var sun_center = Vector2(width * rng.randf_range(0.3, 0.7), height * rng.randf_range(0.2, 0.4))
	var radius = rng.randf_range(30, 60)
	for i in range(32):
		var angle = i * (PI * 2.0 / 32.0)
		sun_points.append(sun_center + Vector2(cos(angle), sin(angle)) * radius)
	sun.polygon = sun_points
	sun.color = colors[4]
	sun.color.a = 0.3
	# Insert sun behind front layers (at index 1)
	viewport.add_child(sun)
	viewport.move_child(sun, 1)

	# Await rendering frame
	await get_tree().process_frame
	await RenderingServer.frame_post_draw
	
	var img = viewport.get_texture().get_image()
	remove_child(viewport)
	viewport.queue_free()
	return img

## Generates a procedural character avatar image
func generate_avatar(char_name: String, description: String, width: int = 256, height: int = 256) -> Image:
	if DisplayServer.get_name() == "headless":
		var mock = Image.create(width, height, false, Image.FORMAT_RGBA8)
		mock.fill(Color(0.5, 0.4, 0.3))
		return mock
		
	var viewport = SubViewport.new()
	viewport.size = Vector2i(width, height)
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(viewport)
	
	var combined_seed = char_name + "_" + description
	var colors = get_colors_from_seed(combined_seed, 5)
	
	var center = Vector2(width / 2.0, height / 2.0)
	var rng = RandomNumberGenerator.new()
	rng.seed = combined_seed.hash()
	
	# 2. Torso/Shoulders
	var shoulders = Polygon2D.new()
	var shoulder_points = PackedVector2Array([
		Vector2(width * 0.1, height),
		Vector2(width * 0.25, height * 0.65),
		Vector2(width * 0.75, height * 0.65),
		Vector2(width * 0.9, height),
	])
	shoulders.polygon = shoulder_points
	shoulders.color = colors[2]
	viewport.add_child(shoulders)
	
	# 3. Hair (Back Layer)
	var hair_back = Polygon2D.new()
	var hair_points = PackedVector2Array()
	var hair_center = center - Vector2(0, height * 0.08)
	var hair_radius = width * 0.28
	for i in range(16):
		var angle = PI + i * (PI / 15.0) # semi-circle for top hair
		var r = hair_radius * rng.randf_range(0.9, 1.15)
		hair_points.append(hair_center + Vector2(cos(angle), sin(angle)) * r)
	# Closed base
	hair_points.append(Vector2(width * 0.75, height * 0.65))
	hair_points.append(Vector2(width * 0.25, height * 0.65))
	hair_back.polygon = hair_points
	hair_back.color = colors[3]
	viewport.add_child(hair_back)
	
	# 4. Head/Face
	var head = Polygon2D.new()
	var head_points = PackedVector2Array()
	var face_center = center - Vector2(0, height * 0.05)
	var head_rx = width * 0.2
	var head_ry = height * 0.25
	for i in range(32):
		var angle = i * (PI * 2.0 / 32.0)
		head_points.append(face_center + Vector2(cos(angle) * head_rx, sin(angle) * head_ry))
	head.polygon = head_points
	head.color = Color(0.95, 0.8, 0.7) # skin tone base
	head.color = head.color.lerp(colors[4], 0.15)
	viewport.add_child(head)
	
	# 5. Eyes
	var eye_color = colors[0]
	var eye_y = face_center.y - (head_ry * 0.15)
	var eye_offset = head_rx * 0.4
	
	for eye in [-1, 1]:
		var eye_dot = Polygon2D.new()
		var eye_points = PackedVector2Array()
		var e_center = Vector2(face_center.x + eye_offset * eye, eye_y)
		var e_radius = width * 0.03
		for i in range(16):
			var angle = i * (PI * 2.0 / 16.0)
			eye_points.append(e_center + Vector2(cos(angle), sin(angle)) * e_radius)
		eye_dot.polygon = eye_points
		eye_dot.color = eye_color
		viewport.add_child(eye_dot)
		
	# 6. Smile/Mouth
	var mouth = Line2D.new()
	mouth.width = width * 0.015
	mouth.default_color = Color(0.6, 0.3, 0.3)
	var my = face_center.y + (head_ry * 0.4)
	mouth.points = PackedVector2Array([
		Vector2(face_center.x - head_rx * 0.25, my - height * 0.01),
		Vector2(face_center.x, my + height * 0.015),
		Vector2(face_center.x + head_rx * 0.25, my - height * 0.01),
	])
	viewport.add_child(mouth)

	await get_tree().process_frame
	await RenderingServer.frame_post_draw
	
	var img = viewport.get_texture().get_image()
	remove_child(viewport)
	viewport.queue_free()
	return img

## Generates a procedural item icon image
func generate_item(item_name: String, description: String, width: int = 128, height: int = 128) -> Image:
	if DisplayServer.get_name() == "headless":
		var mock = Image.create(width, height, false, Image.FORMAT_RGBA8)
		mock.fill(Color(0.1, 0.6, 0.5))
		return mock
		
	var viewport = SubViewport.new()
	viewport.size = Vector2i(width, height)
	viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(viewport)
	
	var combined_seed = item_name + "_" + description
	var colors = get_colors_from_seed(combined_seed, 4)
	
	# 1. Background vignette
	var bg = TextureRect.new()
	bg.size = Vector2(width, height)
	var grad = Gradient.new()
	grad.colors = [colors[0].darkened(0.5), Color(0.08, 0.08, 0.12)]
	var grad_tex = GradientTexture2D.new()
	grad_tex.gradient = grad
	grad_tex.fill = GradientTexture2D.FILL_RADIAL
	grad_tex.fill_from = Vector2(0.5, 0.5)
	bg.texture = grad_tex
	viewport.add_child(bg)
	
	# 2. Glowing icon center
	var glow = Polygon2D.new()
	var glow_points = PackedVector2Array()
	var center = Vector2(width / 2.0, height / 2.0)
	var glow_rad = width * 0.25
	for i in range(16):
		var angle = i * (PI * 2.0 / 16.0)
		glow_points.append(center + Vector2(cos(angle), sin(angle)) * glow_rad)
	glow.polygon = glow_points
	glow.color = colors[1]
	glow.color.a = 0.25
	viewport.add_child(glow)
	
	# Determine item type from text keywords
	var desc_lower = (item_name + " " + description).to_lower()
	var icon = Polygon2D.new()
	var icon_points = PackedVector2Array()
	
	if "sword" in desc_lower or "blade" in desc_lower or "dagger" in desc_lower or "weapon" in desc_lower:
		# Draw a simple sword
		icon_points = PackedVector2Array([
			Vector2(width * 0.5, height * 0.2), # Blade tip
			Vector2(width * 0.53, height * 0.6),
			Vector2(width * 0.6, height * 0.6), # Guard right
			Vector2(width * 0.6, height * 0.65),
			Vector2(width * 0.53, height * 0.65), # Hilt start
			Vector2(width * 0.52, height * 0.8), # Handle
			Vector2(width * 0.48, height * 0.8),
			Vector2(width * 0.47, height * 0.65), # Hilt left
			Vector2(width * 0.4, height * 0.65),
			Vector2(width * 0.4, height * 0.6), # Guard left
			Vector2(width * 0.47, height * 0.6),
		])
	elif "shield" in desc_lower or "armor" in desc_lower or "breastplate" in desc_lower or "helm" in desc_lower:
		# Draw a shield crest
		icon_points = PackedVector2Array([
			Vector2(width * 0.5, height * 0.25), # Top center
			Vector2(width * 0.75, height * 0.25), # Top right
			Vector2(width * 0.72, height * 0.55), # Mid right
			Vector2(width * 0.5, height * 0.82), # Bottom point
			Vector2(width * 0.28, height * 0.55), # Mid left
			Vector2(width * 0.25, height * 0.25), # Top left
		])
	elif "potion" in desc_lower or "vial" in desc_lower or "flask" in desc_lower or "elixir" in desc_lower:
		# Draw a potion bottle
		icon_points = PackedVector2Array([
			Vector2(width * 0.42, height * 0.3), # Neck left
			Vector2(width * 0.58, height * 0.3), # Neck right
			Vector2(width * 0.58, height * 0.45), # Shoulder right
			Vector2(width * 0.72, height * 0.55), # Body right
			Vector2(width * 0.65, height * 0.78), # Bottom right
			Vector2(width * 0.35, height * 0.78), # Bottom left
			Vector2(width * 0.28, height * 0.55), # Body left
			Vector2(width * 0.42, height * 0.45), # Shoulder left
		])
	else:
		# Default ring or jewel shape
		var r = width * 0.18
		for i in range(24):
			var angle = i * (PI * 2.0 / 24.0)
			var wobble = 1.0 + 0.08 * sin(angle * 6.0)
			icon_points.append(center + Vector2(cos(angle), sin(angle)) * r * wobble)
			
	icon.polygon = icon_points
	icon.color = colors[2]
	viewport.add_child(icon)
	
	# 3. Outer border frame
	var frame = ReferenceRect.new()
	frame.size = Vector2(width, height)
	frame.editor_only = false
	frame.border_color = colors[3]
	frame.border_width = width * 0.03
	viewport.add_child(frame)
	
	await get_tree().process_frame
	await RenderingServer.frame_post_draw
	
	var img = viewport.get_texture().get_image()
	remove_child(viewport)
	viewport.queue_free()
	return img

## Generates a procedural character avatar placeholder with initials and seed color
func generate_avatar_placeholder(char_name: String, width: int = 256, height: int = 256) -> Image:
	if DisplayServer.get_name() == "headless":
		var mock = Image.create(width, height, false, Image.FORMAT_RGBA8)
		mock.fill(Color(0.5, 0.4, 0.3))
		return mock
		
	var viewport = SubViewport.new()
	viewport.size = Vector2i(width, height)
	viewport.transparent_bg = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(viewport)
	
	# Determine background color from seed
	var colors = get_colors_from_seed(char_name, 1)
	var bg_color = colors[0]
	
	# Circle Background
	var circle = Polygon2D.new()
	var points = PackedVector2Array()
	var center = Vector2(width / 2.0, height / 2.0)
	var radius = min(width, height) / 2.0
	for i in range(64):
		var angle = i * (PI * 2.0 / 64.0)
		points.append(center + Vector2(cos(angle), sin(angle)) * radius)
	circle.polygon = points
	circle.color = bg_color
	viewport.add_child(circle)
	
	# Get initials
	var initials = ""
	var parts = char_name.split(" ", false)
	for part in parts:
		if not part.is_empty():
			initials += part[0].to_upper()
			if initials.length() >= 2:
				break
	if initials.is_empty():
		initials = "?"
		
	# Draw initials in center
	var label = Label.new()
	label.text = initials
	label.size = Vector2(width, height)
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	
	# Set a large, bold font size
	var font_size = int(min(width, height) * 0.4)
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", Color.WHITE)
	
	# Add shadow/outline for readability
	label.add_theme_color_override("font_outline_color", Color.BLACK)
	label.add_theme_constant_override("outline_size", int(font_size * 0.15))
	
	viewport.add_child(label)
	
	await get_tree().process_frame
	await RenderingServer.frame_post_draw
	
	var img = viewport.get_texture().get_image()
	remove_child(viewport)
	viewport.queue_free()
	return img

## Generates a procedural scene placeholder (themed gradient pattern)
func generate_scene_placeholder(description: String, width: int = 768, height: int = 512) -> Image:
	if DisplayServer.get_name() == "headless":
		var mock = Image.create(width, height, false, Image.FORMAT_RGBA8)
		mock.fill(Color(0.2, 0.3, 0.4))
		return mock
		
	var viewport = SubViewport.new()
	viewport.size = Vector2i(width, height)
	viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	add_child(viewport)
	
	var colors = get_colors_from_seed(description, 3)
	
	var bg = TextureRect.new()
	bg.size = Vector2(width, height)
	
	var grad = Gradient.new()
	grad.colors = [colors[0], colors[1], colors[2]]
	grad.offsets = [0.0, 0.5, 1.0]
	
	var grad_tex = GradientTexture2D.new()
	grad_tex.gradient = grad
	grad_tex.fill = GradientTexture2D.FILL_LINEAR
	grad_tex.fill_from = Vector2(0.0, 0.0)
	grad_tex.fill_to = Vector2(1.0, 1.0)
	bg.texture = grad_tex
	viewport.add_child(bg)
	
	await get_tree().process_frame
	await RenderingServer.frame_post_draw
	
	var img = viewport.get_texture().get_image()
	remove_child(viewport)
	viewport.queue_free()
	return img

