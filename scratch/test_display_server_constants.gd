# res://tests/test_display_server_constants.gd
extends SceneTree

func _init() -> void:
	print("DisplayServer.FILE_DIALOG_MODE_OPEN_FILE = ", DisplayServer.FILE_DIALOG_MODE_OPEN_FILE)
	print("DisplayServer.FILE_DIALOG_MODE_OPEN_DIR = ", DisplayServer.FILE_DIALOG_MODE_OPEN_DIR)
	
	# Let's also check if DisplayServer supports native dialogs
	print("DisplayServer name: ", DisplayServer.get_name())
	print("DisplayServer supports native dialogs: ", DisplayServer.has_feature(DisplayServer.FEATURE_NATIVE_DIALOG))
	
	quit(0)
