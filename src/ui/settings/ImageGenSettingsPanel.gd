# res://src/ui/settings/ImageGenSettingsPanel.gd
extends VBoxContainer
class_name ImageGenSettingsPanel

signal settings_changed

@onready var image_gen_enable_check: CheckBox = %ImageGenEnableCheck
@onready var image_gen_options_container: VBoxContainer = %ImageGenOptionsContainer
@onready var image_gen_provider_dropdown: OptionButton = %ImageGenProviderDropdown
@onready var image_gen_url_input: LineEdit = %ImageGenUrlInput
@onready var image_gen_denoising_input: LineEdit = %ImageGenDenoisingInput
@onready var campaign_art_style_dropdown: OptionButton = %CampaignArtStyleDropdown
@onready var image_gen_help_btn: Button = %ImageGenHelpButton
@onready var test_connection_btn: Button = %TestConnectionButton
@onready var connection_status_label: Label = %ConnectionStatusLabel
@onready var apply_btn: Button = %ApplyButton

@export var show_apply_button: bool = false:
	set(val):
		show_apply_button = val
		if apply_btn:
			apply_btn.visible = val

func _ready() -> void:
	image_gen_help_btn.pressed.connect(_open_draw_things_tutorial)
	test_connection_btn.pressed.connect(_on_test_connection_pressed)
	apply_btn.pressed.connect(apply_settings)
	apply_btn.visible = show_apply_button
	
	# Load active image gen config into inputs
	image_gen_enable_check.button_pressed = LLMClient.image_gen_enabled
	image_gen_options_container.visible = LLMClient.image_gen_enabled
	
	image_gen_provider_dropdown.selected = 0 if LLMClient.image_gen_provider == "automatic1111" else 1
	image_gen_url_input.text = LLMClient.image_gen_url
	image_gen_denoising_input.text = str(LLMClient.image_gen_denoising_strength)
	
	if CampaignState and not CampaignState.campaign_id.is_empty() and campaign_art_style_dropdown:
		var active_style = CampaignState.state.adventure_meta.get("art_style", "Digital Anime Art")
		for i in range(campaign_art_style_dropdown.item_count):
			if campaign_art_style_dropdown.get_item_text(i) == active_style:
				campaign_art_style_dropdown.selected = i
				break
		campaign_art_style_dropdown.disabled = false
	elif campaign_art_style_dropdown:
		campaign_art_style_dropdown.selected = 0
		campaign_art_style_dropdown.disabled = true
	
	image_gen_enable_check.toggled.connect(func(pressed):
		image_gen_options_container.visible = pressed
		settings_changed.emit()
	)
	
	image_gen_provider_dropdown.item_selected.connect(func(_idx): settings_changed.emit())
	image_gen_url_input.text_changed.connect(func(_t): settings_changed.emit())
	image_gen_denoising_input.text_changed.connect(func(_t): settings_changed.emit())
	if campaign_art_style_dropdown:
		campaign_art_style_dropdown.item_selected.connect(func(_idx): settings_changed.emit())

func apply_settings() -> void:
	LLMClient.image_gen_enabled = image_gen_enable_check.button_pressed
	LLMClient.image_gen_provider = "automatic1111" if image_gen_provider_dropdown.selected == 0 else "comfyui"
	LLMClient.image_gen_url = image_gen_url_input.text.strip_edges()
	
	var val = float(image_gen_denoising_input.text)
	if val <= 0.0:
		val = 0.7
	LLMClient.image_gen_denoising_strength = clampf(val, 0.1, 1.0)
	LLMClient.save_config()
	
	if CampaignState and not CampaignState.campaign_id.is_empty() and campaign_art_style_dropdown:
		var style = campaign_art_style_dropdown.get_item_text(campaign_art_style_dropdown.selected)
		CampaignState.set_campaign_meta("art_style", style)
		CampaignState.save()
	
	if show_apply_button:
		var prev_text = apply_btn.text
		apply_btn.text = "Saved!"
		apply_btn.disabled = true
		await get_tree().create_timer(1.5).timeout
		apply_btn.text = prev_text
		apply_btn.disabled = false

func _open_draw_things_tutorial() -> void:
	var tutorial_scene = load("res://scenes/ui/onboarding/DrawThingsTutorial.tscn")
	if tutorial_scene:
		var tutorial = tutorial_scene.instantiate()
		var main_root = get_tree().root
		main_root.add_child(tutorial)

func test_connection() -> void:
	_on_test_connection_pressed()

func _on_test_connection_pressed() -> void:
	var gen_url = image_gen_url_input.text.strip_edges()
	var is_enabled = image_gen_enable_check.button_pressed
	
	if not is_enabled:
		connection_status_label.add_theme_color_override("font_color", Color.html("#e6c229"))
		connection_status_label.text = "🎨 Image generator is disabled."
		return
		
	if gen_url.is_empty():
		connection_status_label.add_theme_color_override("font_color", Color.html("#e05353"))
		connection_status_label.text = "URL is empty"
		return
		
	connection_status_label.add_theme_color_override("font_color", Color.html("#611765"))
	connection_status_label.text = "Testing connection..."
	test_connection_btn.disabled = true
	
	var probe_url = gen_url.rstrip("/") + "/sdapi/v1/options"
	var probe_http = HTTPRequest.new()
	probe_http.timeout = 4.0
	add_child(probe_http)
	probe_http.request_completed.connect(func(result: int, response_code: int, _headers: PackedStringArray, _body: PackedByteArray):
		test_connection_btn.disabled = false
		if result == HTTPRequest.RESULT_SUCCESS and response_code == 200:
			connection_status_label.add_theme_color_override("font_color", Color.html("#52cc7a")) # Green
			connection_status_label.text = "✅ Local AI image generator detected and ready!"
		else:
			connection_status_label.add_theme_color_override("font_color", Color.html("#e05353")) # Red
			connection_status_label.text = "❌ Not found at %s. Is Draw Things/A1111 running?" % gen_url
		probe_http.queue_free()
	)
	var err = probe_http.request(probe_url, [], HTTPClient.METHOD_GET)
	if err != OK:
		test_connection_btn.disabled = false
		connection_status_label.add_theme_color_override("font_color", Color.html("#e05353")) # Red
		connection_status_label.text = "❌ Could not reach %s." % gen_url
		probe_http.queue_free()
