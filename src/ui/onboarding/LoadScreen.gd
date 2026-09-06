# res://src/ui/onboarding/LoadScreen.gd
extends VBoxContainer

signal back_pressed
signal campaign_selected(campaign_id: String)

@onready var campaign_list: VBoxContainer = %CampaignList
@onready var load_back_btn: Button = %LoadBackButton
@onready var no_saves_label: Label = %NoSavesLabel

@onready var delete_confirm_dialog: ConfirmationDialog = %DeleteConfirmDialog
var _campaign_id_to_delete: String = ""

func _ready() -> void:
	load_back_btn.pressed.connect(func(): back_pressed.emit())
	
	# Setup Delete Confirmation Dialog
	delete_confirm_dialog.confirmed.connect(_on_delete_confirmed)
	
	var delete_label = delete_confirm_dialog.get_label()
	if delete_label:
		delete_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		delete_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER

func on_screen_shown() -> void:
	delete_confirm_dialog.theme = ThemeManager.active_theme
	refresh_campaign_list()

func refresh_campaign_list() -> void:
	# Clear previous list items
	for child in campaign_list.get_children():
		child.queue_free()
		
	var saves = SaveManager.get_campaign_list()
	if saves.is_empty():
		no_saves_label.visible = true
	else:
		no_saves_label.visible = false
		for save in saves:
			var item = preload("res://scenes/ui/onboarding/CampaignListItem.tscn").instantiate()
			campaign_list.add_child(item)
			item.setup(save)
			item.selected.connect(func(campaign_id):
				campaign_selected.emit(campaign_id)
			)
			item.delete_requested.connect(func(campaign_id):
				_prompt_delete_campaign(campaign_id)
			)

func _prompt_delete_campaign(campaign_id: String) -> void:
	_campaign_id_to_delete = campaign_id
	delete_confirm_dialog.popup_centered()

func _on_delete_confirmed() -> void:
	if not _campaign_id_to_delete.is_empty():
		SaveManager.delete_campaign(_campaign_id_to_delete)
		_campaign_id_to_delete = ""
		refresh_campaign_list()
