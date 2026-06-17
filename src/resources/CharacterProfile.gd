# res://src/resources/CharacterProfile.gd
extends Resource
class_name CharacterProfile

@export var id: String = ""
@export var name: String = ""
@export var biography: String = ""
@export var affinity: float = 0.0
@export var inventory: Array[InventoryItem] = []
@export var emotions: Array[EmotionEvent] = []
@export var writing_style: String = ""

func to_dict() -> Dictionary:
	var inv_list: Array[Dictionary] = []
	for item in inventory:
		inv_list.append(item.to_dict())
		
	var emo_list: Array[Dictionary] = []
	for event in emotions:
		emo_list.append(event.to_dict())
		
	return {
		"name": name,
		"biography": biography,
		"affinity": affinity,
		"inventory": inv_list,
		"emotions": emo_list,
		"writing_style": writing_style
	}

static func from_dict(char_id: String, d: Dictionary) -> CharacterProfile:
	var instance = CharacterProfile.new()
	instance.id = char_id
	instance.name = d.get("name", char_id.capitalize())
	instance.biography = d.get("biography", "")
	instance.affinity = d.get("affinity", 0.0)
	instance.writing_style = d.get("writing_style", "")
	
	var raw_inv = d.get("inventory", [])
	instance.inventory = []
	for item_dict in raw_inv:
		if item_dict is Dictionary:
			instance.inventory.append(InventoryItem.from_dict(item_dict))
			
	var raw_emo = d.get("emotions", [])
	instance.emotions = []
	for emo_dict in raw_emo:
		if emo_dict is Dictionary:
			instance.emotions.append(EmotionEvent.from_dict(emo_dict))
			
	return instance
