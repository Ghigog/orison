# res://src/resources/InventoryItem.gd
extends Resource
class_name InventoryItem

@export var item_name: String = ""
@export var quantity: int = 1
@export var properties: Dictionary = {}

func to_dict() -> Dictionary:
	return {
		"item": item_name,
		"quantity": quantity,
		"properties": properties
	}

static func from_dict(d: Dictionary) -> InventoryItem:
	var instance = InventoryItem.new()
	if d.has("item"):
		instance.item_name = d["item"]
	elif d.has("item_name"):
		instance.item_name = d["item_name"]
	instance.quantity = d.get("quantity", 1)
	instance.properties = d.get("properties", {})
	return instance
