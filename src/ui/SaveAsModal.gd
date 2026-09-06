# res://src/ui/SaveAsModal.gd
extends Control
class_name SaveAsModal

signal saved(new_id: String)
signal closed

@onready var new_title_input: LineEdit = %NewTitleInput
@onready var error_label: Label = %ErrorLabel
@onready var save_button: Button = %SaveButton
@onready var close_button: Button = %CloseButton

func _ready() -> void:
	save_button.pressed.connect(_on_save_pressed)
	close_button.pressed.connect(_on_close_pressed)
	new_title_input.text_submitted.connect(func(_text): _on_save_pressed())
	new_title_input.grab_focus()
	
	ThemeManager.theme_changed.connect(_on_global_theme_changed)
	_apply_modal_styles()
	
	# Entrance transition
	var overlay = get_node_or_null("OverlayBG")
	var card = get_node_or_null("ScrollContainer/CenterContainer/CardPanel")
	if overlay and card:
		var target_bg_alpha = overlay.color.a
		overlay.color.a = 0.0
		card.modulate.a = 0.0
		card.scale = Vector2(0.9, 0.9)
		
		card.pivot_offset = card.size / 2.0
		card.item_rect_changed.connect(func():
			card.pivot_offset = card.size / 2.0
		)
		
		var entrance_tween = create_tween().set_parallel(true)
		entrance_tween.tween_property(overlay, "color:a", target_bg_alpha, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(ThemeManager.ease_default)
		entrance_tween.tween_property(card, "modulate:a", 1.0, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(ThemeManager.ease_default)
		entrance_tween.tween_property(card, "scale", Vector2(1.0, 1.0), ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(ThemeManager.ease_default)

func _on_save_pressed() -> void:
	var title_text = new_title_input.text.strip_edges()
	if title_text.is_empty():
		error_label.text = "Campaign title cannot be empty."
		error_label.visible = true
		return
		
	# Build ID: alphanumeric and underscores
	var regex = RegEx.new()
	regex.compile("[^a-zA-Z0-9_]")
	var new_id = title_text.to_lower().replace(" ", "_")
	new_id = regex.sub(new_id, "", true)
	
	if new_id.is_empty():
		error_label.text = "Invalid characters in campaign title."
		error_label.visible = true
		return
		
	var path = SaveManager.SAVE_DIR + new_id + ".json"
	if FileAccess.file_exists(path):
		error_label.text = "A campaign with this name already exists."
		error_label.visible = true
		return
		
	# Update active campaign ID in CampaignState and save
	CampaignState.campaign_id = new_id
	CampaignState.set_campaign_meta("campaign_id", new_id)
	CampaignState.set_campaign_meta("title", title_text)
	
	var err = CampaignState.save()
	if err == OK:
		saved.emit(new_id)
		_on_close_pressed()
	else:
		error_label.text = "Failed to save: Error code " + str(err)
		error_label.visible = true

func _on_close_pressed() -> void:
	closed.emit()
	
	var overlay = get_node_or_null("OverlayBG")
	var card = get_node_or_null("ScrollContainer/CenterContainer/CardPanel")
	if overlay and card:
		card.pivot_offset = card.size / 2.0
		var exit_tween = create_tween().set_parallel(true)
		exit_tween.tween_property(overlay, "color:a", 0.0, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(Tween.EASE_IN)
		exit_tween.tween_property(card, "modulate:a", 0.0, ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(Tween.EASE_IN)
		exit_tween.tween_property(card, "scale", Vector2(0.9, 0.9), ThemeManager.duration_normal)\
			.set_trans(ThemeManager.trans_default)\
			.set_ease(Tween.EASE_IN)
		exit_tween.chain().tween_callback(queue_free)
	else:
		queue_free()

func _on_global_theme_changed() -> void:
	_apply_modal_styles()

func _apply_modal_styles() -> void:
	self.theme = ThemeManager.active_theme
