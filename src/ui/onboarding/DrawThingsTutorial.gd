# res://src/ui/onboarding/DrawThingsTutorial.gd
extends Control

@onready var overlay: ColorRect = $Overlay
@onready var modal_card: PanelContainer = %ModalCard
@onready var close_btn: Button = %CloseButton
@onready var content_vbox: VBoxContainer = %ContentVBox

func _ready() -> void:
	# Set pivot and initial states
	modal_card.pivot_offset = modal_card.size / 2.0
	modal_card.item_rect_changed.connect(func():
		modal_card.pivot_offset = modal_card.size / 2.0
	)
	
	overlay.color.a = 0.0
	modal_card.modulate.a = 0.0
	modal_card.scale = Vector2(0.92, 0.92)
	
	# Connect close button and backdrop click
	close_btn.pressed.connect(close)
	overlay.gui_input.connect(func(event: InputEvent):
		if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
			close()
	)
	
	# Populate colors and styles matching ThemeManager dynamically
	_apply_tutorial_styling()
	
	# Entrance tween
	var entrance_tween = create_tween().set_parallel(true)
	entrance_tween.tween_property(overlay, "color:a", 0.55, 0.3).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	entrance_tween.tween_property(modal_card, "modulate:a", 1.0, 0.3).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	entrance_tween.tween_property(modal_card, "scale", Vector2(1.0, 1.0), 0.3).set_trans(Tween.TRANS_BACK).set_ease(Tween.EASE_OUT)

func _apply_tutorial_styling() -> void:
	# Style the modal panel using ThemeManager active theme colors
	var card_style = StyleBoxFlat.new()
	card_style.bg_color = ThemeManager.color_surface
	card_style.border_color = ThemeManager.color_accent
	card_style.set_border_width_all(2)
	card_style.set_corner_radius_all(ThemeManager.radius_lg)
	card_style.set_content_margin_all(24)
	modal_card.add_theme_stylebox_override("panel", card_style)
	
	# Update color overrides on labels and panels in the Scroll container
	var accent_hex = ThemeManager.color_accent.to_html(false)
	var muted_hex = ThemeManager.color_text.darkened(0.3).to_html(false)
	
	# Block 1 Text
	var block1 = %Block1.get_child(0) as RichTextLabel
	block1.text = "By default, Orison draws simple retro artwork automatically — [b]no setup needed at all[/b].\n\n" + \
		"If you want [b]AI-painted portraits, scenes, and character art[/b] instead, you need a free " + \
		"image generation app running in the background on your computer. Orison will silently send it " + \
		"requests and swap in the painted artwork automatically.\n\n" + \
		"[color=#%s]Your data never leaves your computer. Everything runs locally, for free.[/color]" % [muted_hex]
		
	# Block 2 Text (Draw Things Mac)
	var block2 = %Block2.get_child(0) as RichTextLabel
	block2.text = "[b][color=#%s]Step 1 — Install Draw Things[/color][/b]\n" % [accent_hex] + \
		"Open the [b]App Store[/b] on your Mac. Search for [b]Draw Things: AI Generation[/b] and install it. It is completely free.\n\n" + \
		"[b][color=#%s]Step 2 — Enable the API Server[/color][/b]\n" % [accent_hex] + \
		"Open Draw Things. Click the [b]Settings icon (⚙️)[/b] in the top-right corner.\n" + \
		"Scroll down until you find [b]API Server[/b] (sometimes called HTTP Server) and switch it [b]On[/b].\n" + \
		"Make sure the port is set to [color=#%s]7860[/color]. Leave Draw Things open and running.\n\n" % [accent_hex] + \
		"[b][color=#%s]Step 3 — Download a Model[/color][/b]\n" % [accent_hex] + \
		"When you first open Draw Things it asks you to pick a model. Choose [b]'Download a model via Draw Things'[/b], then pick:\n" + \
		"  • [b]DreamShaper 8[/b] or [b]Stable Diffusion 1.5[/b] — fast, works on any Mac with 8 GB+ RAM.\n" + \
		"  • [b]Stable Diffusion XL (SDXL)[/b] — higher quality images, needs 16 GB+ RAM.\n" + \
		"  [color=#%s]Avoid Flux.1 unless you have a Mac Studio or Mac Pro — it is extremely large and slow.[/color]\n\n" % [muted_hex] + \
		"[b][color=#%s]Step 4 — Enable in Orison[/color][/b]\n" % [accent_hex] + \
		"Back in Orison, flip the [b]Enable Local Image Generation[/b] switch on. Press [b]Test Models[/b] — " + \
		"if everything is working you will see [b]🎨 Local AI image generator detected and ready![/b] in the status area."

	# Block 3 Text (WebUI Forge Mac)
	var block3 = %Block3.get_child(0) as RichTextLabel
	block3.text = "WebUI Forge is the same technology that runs inside Draw Things, but as a web page on your own " + \
		"computer. It requires using the [b]Terminal[/b] app. If that sounds unfamiliar, use Option A instead.\n\n" + \
		"[b][color=#%s]Step 1 — Install Homebrew (Mac package manager)[/color][/b]\n" % [accent_hex] + \
		"Open [b]Terminal[/b] (press Cmd+Space, type Terminal, press Enter). Paste this and press Enter:\n" + \
		"[color=#%s]/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"[/color]\n" % [accent_hex] + \
		"Follow any on-screen instructions. It will ask for your Mac password.\n\n" + \
		"[b][color=#%s]Step 2 — Install developer tools[/color][/b]\n" % [accent_hex] + \
		"In the same Terminal window, paste and run:\n" + \
		"[color=#%s]brew install cmake protobuf rust python@3.10 git wget[/color]\n" % [accent_hex] + \
		"This may take several minutes to complete.\n\n" + \
		"[b][color=#%s]Step 3 — Download WebUI Forge[/color][/b]\n" % [accent_hex] + \
		"Still in Terminal, run:\n" + \
		"[color=#%s]git clone https://github.com/lllyasviel/stable-diffusion-webui-forge[/color]\n" % [accent_hex] + \
		"[color=#%s]cd stable-diffusion-webui-forge[/color]\n\n" % [accent_hex] + \
		"[b][color=#%s]Step 4 — Enable the API flag[/color][/b]\n" % [accent_hex] + \
		"Open the file [b]webui-user.sh[/b] in any text editor (e.g. TextEdit). Find the line:\n" + \
		"[color=#%s]#export COMMANDLINE_ARGS=\"\"[/color]\n" % [muted_hex] + \
		"Change it to (remove the # and add --api):\n" + \
		"[color=#%s]export COMMANDLINE_ARGS=\"--api\"[/color]\n" % [accent_hex] + \
		"Save and close the file.\n\n" + \
		"[b][color=#%s]Step 5 — Launch WebUI Forge[/color][/b]\n" % [accent_hex] + \
		"Back in Terminal, run:\n" + \
		"[color=#%s]./webui.sh[/color]\n" % [accent_hex] + \
		"The first launch downloads models and takes a long time. When it is ready, you will see a message like " + \
		"[color=#%s]Running on local URL: http://127.0.0.1:7860[/color]. " % [accent_hex] + \
		"Leave Terminal open. Orison will now connect to it automatically."

	# Block 4 Text (WebUI Forge Windows)
	var block4 = %Block4.get_child(0) as RichTextLabel
	block4.text = "[b][color=#%s]Step 1 — Download WebUI Forge[/color][/b]\n" % [accent_hex] + \
		"Go to [color=#%s]github.com/lllyasviel/stable-diffusion-webui-forge[/color] in your browser.\n" % [accent_hex] + \
		"Click the green [b]Code[/b] button → [b]Download ZIP[/b]. Extract it to a folder on your computer, " + \
		"e.g. [color=#%s]C:\\WebUI-Forge[/color].\n\n" % [accent_hex] + \
		"[b][color=#%s]Step 2 — Enable the API flag[/color][/b]\n" % [accent_hex] + \
		"Open the extracted folder. Find the file [b]webui-user.bat[/b] and right-click it → [b]Edit[/b] (in Notepad).\n" + \
		"Find the line:\n" + \
		"[color=#%s]set COMMANDLINE_ARGS=[/color]\n" % [muted_hex] + \
		"Change it to:\n" + \
		"[color=#%s]set COMMANDLINE_ARGS=--api[/color]\n" % [accent_hex] + \
		"Save the file.\n\n" + \
		"[b][color=#%s]Step 3 — Launch WebUI Forge[/color][/b]\n" % [accent_hex] + \
		"Double-click [b]webui-user.bat[/b] to launch it. A black command window will open and start downloading " + \
		"dependencies — this may take a while on first run. When it is ready you will see:\n" + \
		"[color=#%s]Running on local URL: http://127.0.0.1:7860[/color]\n" % [accent_hex] + \
		"Leave that window open. Orison will automatically connect to it.\n\n" + \
		"[color=#%s]Note: WebUI Forge on Windows works best with an NVIDIA GPU. If you only have an " % [muted_hex] + \
		"Intel/AMD integrated GPU, image generation will be very slow.[/color]"

	# Block 5 Text (FAQs)
	var block5 = %Block5.get_child(0) as RichTextLabel
	block5.text = "[b]Do I need a special graphics card?[/b]\n" + \
		"For [b]Mac (Draw Things)[/b]: No. It uses Apple's M-series chip — works on all modern Macs.\n" + \
		"For [b]Windows (WebUI Forge)[/b]: An NVIDIA GPU is highly recommended. Without one, generation will be very slow.\n\n" + \
		"[b]Does my data go to the internet?[/b]\n" + \
		"No. All image generation runs 100% on your own computer. Nothing is uploaded anywhere.\n\n" + \
		"[b]Orison says it isn't detecting the image generator — what do I do?[/b]\n" + \
		"Make sure your image gen app (Draw Things or WebUI Forge) is still open and running, " + \
		"and that the API server is switched on (port 7860). Then press [b]Test Models[/b] in Orison again.\n\n" + \
		"[b]What is the URL field in Orison for?[/b]\n" + \
		"It tells Orison where to find the image generator. The default [color=#%s]http://127.0.0.1:7860[/color] " % [accent_hex] + \
		"is correct for all options above. Only change it if you are running the image generator on a different computer.\n\n" + \
		"[b]Can I still play without setting any of this up?[/b]\n" + \
		"Yes! Leave the [b]Enable Local Image Generation[/b] switch [b]off[/b] and Orison will generate " + \
		"instant procedural artwork for every scene and character automatically. No installation needed."

	for child in content_vbox.get_children():
		if child is PanelContainer:
			var block_style = StyleBoxFlat.new()
			block_style.bg_color = ThemeManager.color_surface.lerp(ThemeManager.color_bg, 0.15)
			block_style.set_corner_radius_all(ThemeManager.radius_md)
			block_style.set_content_margin_all(12)
			child.add_theme_stylebox_override("panel", block_style)
			
			var rtl = child.get_child(0) as RichTextLabel
			if rtl:
				rtl.theme = ThemeManager.active_theme
				rtl.add_theme_color_override("default_color", ThemeManager.color_text)
		elif child is Label:
			child.theme = ThemeManager.active_theme
			child.add_theme_color_override("font_color", ThemeManager.color_text)

func close() -> void:
	var exit_tween = create_tween().set_parallel(true)
	exit_tween.tween_property(overlay, "color:a", 0.0, 0.25).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_IN)
	exit_tween.tween_property(modal_card, "modulate:a", 0.0, 0.25).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_IN)
	exit_tween.tween_property(modal_card, "scale", Vector2(0.92, 0.92), 0.25).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_IN)
	await exit_tween.finished
	queue_free()
