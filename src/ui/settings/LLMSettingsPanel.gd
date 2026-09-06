# res://src/ui/settings/LLMSettingsPanel.gd
extends VBoxContainer
class_name LLMSettingsPanel

signal settings_changed

@onready var ollama_url_input: LineEdit = %OllamaUrlInput
@onready var world_builder_model_input: LineEdit = %WorldBuilderModelInput
@onready var character_model_input: LineEdit = %CharacterModelInput
@onready var autosave_input: SpinBox = %AutosaveInput
@onready var test_connection_btn: Button = %TestConnectionButton
@onready var connection_status_label: Label = %ConnectionStatusLabel
@onready var apply_btn: Button = %ApplyButton

@export var show_apply_button: bool = false:
	set(val):
		show_apply_button = val
		if apply_btn:
			apply_btn.visible = val

func _ready() -> void:
	test_connection_btn.pressed.connect(_on_test_connection_pressed)
	apply_btn.pressed.connect(apply_settings)
	apply_btn.visible = show_apply_button
	
	# Load active LLM config into inputs
	ollama_url_input.text = LLMClient.api_url
	world_builder_model_input.text = LLMClient.world_builder_model
	character_model_input.text = LLMClient.character_model
	autosave_input.value = LLMClient.autosave_interval
	
	# Connect text changed signals to emit settings_changed
	ollama_url_input.text_changed.connect(func(_t): settings_changed.emit())
	world_builder_model_input.text_changed.connect(func(_t): settings_changed.emit())
	character_model_input.text_changed.connect(func(_t): settings_changed.emit())
	autosave_input.value_changed.connect(func(_v): settings_changed.emit())

func test_connection() -> void:
	_on_test_connection_pressed()

func apply_settings() -> void:
	LLMClient.api_url = ollama_url_input.text.strip_edges()
	LLMClient.world_builder_model = world_builder_model_input.text.strip_edges()
	LLMClient.character_model = character_model_input.text.strip_edges()
	LLMClient.autosave_interval = int(autosave_input.value)
	LLMClient.save_config()
	
	if show_apply_button:
		var prev_text = apply_btn.text
		apply_btn.text = "Saved!"
		apply_btn.disabled = true
		await get_tree().create_timer(1.5).timeout
		apply_btn.text = prev_text
		apply_btn.disabled = false

func _on_test_connection_pressed() -> void:
	var test_url = ollama_url_input.text.strip_edges()
	if test_url.is_empty():
		connection_status_label.add_theme_color_override("font_color", Color.html("#e05353"))
		connection_status_label.text = "URL is empty"
		return
		
	connection_status_label.add_theme_color_override("font_color", Color.html("#611765"))
	connection_status_label.text = "Testing connection..."
	test_connection_btn.disabled = true
	
	LLMClient.test_connection(test_url, _on_connection_test_completed)

func _on_connection_test_completed(success: bool, error_msg: String, models: Array[String]) -> void:
	test_connection_btn.disabled = false
	
	if success:
		var wb_model = world_builder_model_input.text.strip_edges()
		var char_model = character_model_input.text.strip_edges()
		
		var wb_found = wb_model in models
		var char_found = char_model in models
		var embed_found = "nomic-embed-text" in models
		
		var status_text = "✅ Connected to Ollama server."
		var color = Color.html("#52cc7a") # Green
		
		if models.is_empty():
			status_text += "\n⚠️ Warning: No models are pulled yet."
			color = Color.html("#e6c229") # Yellow
		else:
			var unique_models: Array[String] = []
			for m in models:
				if not unique_models.has(m):
					unique_models.append(m)
			status_text += "\nModels found: " + ", ".join(unique_models.slice(0, 4))
			if unique_models.size() > 4:
				status_text += " (and %d more)" % (unique_models.size() - 4)
				
			if not wb_found or not char_found or not embed_found:
				color = Color.html("#e6c229") # Yellow
				if not wb_found:
					status_text += "\n⚠️ DM Model '%s' not found locally." % wb_model
				if not char_found:
					status_text += "\n⚠️ NPC Model '%s' not found locally." % char_model
				if not embed_found:
					status_text += "\n⚠️ Embedding model 'nomic-embed-text' not found locally."
					
		connection_status_label.add_theme_color_override("font_color", color)
		connection_status_label.text = status_text
	else:
		connection_status_label.add_theme_color_override("font_color", Color.html("#e05353")) # Red
		connection_status_label.text = "❌ Error: %s" % error_msg
