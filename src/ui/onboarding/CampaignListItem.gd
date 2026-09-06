# res://src/ui/onboarding/CampaignListItem.gd
extends HBoxContainer

signal selected(campaign_id: String)
signal delete_requested(campaign_id: String)

@onready var thumbnail: TextureRect = %Thumbnail
@onready var select_button: Button = %SelectButton
@onready var delete_button: Button = %DeleteButton

var campaign_id: String = ""

func _ready() -> void:
	select_button.pressed.connect(func(): selected.emit(campaign_id))
	delete_button.pressed.connect(func(): delete_requested.emit(campaign_id))

func setup(save) -> void:
	campaign_id = save.id
	
	# Thumbnail preview
	var base64_thumb = save.get("thumbnail", "")
	if base64_thumb and not base64_thumb.is_empty():
		var thumb_texture = _get_texture_from_base64(base64_thumb)
		if thumb_texture:
			thumbnail.texture = thumb_texture
			thumbnail.visible = true
		else:
			thumbnail.visible = false
	else:
		thumbnail.visible = false
		
	var playtime_str = _format_playtime(save.get("playtime_seconds", 0.0))
	select_button.text = "%s\nLast Played: %s | Playtime: %s" % [
		save.get("title", campaign_id),
		save.get("last_played", "Unknown Date"),
		playtime_str
	]

func _get_texture_from_base64(base64_str: String) -> Texture2D:
	if base64_str.is_empty():
		return null
	var buffer = Marshalls.base64_to_raw(base64_str)
	if buffer.is_empty():
		return null
	var img = Image.new()
	var err = img.load_png_from_buffer(buffer)
	if err == OK:
		return ImageTexture.create_from_image(img)
	return null

func _format_playtime(seconds: float) -> String:
	var total_seconds = int(seconds)
	var hrs = total_seconds / 3600
	var mins = (total_seconds % 3600) / 60
	if hrs > 0:
		return "%dh %dm" % [hrs, mins]
	return "%dm" % [mins]
