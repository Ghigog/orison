# res://tests/test_file_dialog_show.gd
extends SceneTree

func _init() -> void:
	var result = DisplayServer.file_dialog_show(
		"Test Native Dialog",
		OS.get_system_dir(OS.SYSTEM_DIR_DOCUMENTS),
		"",
		false,
		DisplayServer.FILE_DIALOG_MODE_OPEN_DIR,
		PackedStringArray(),
		func(status, paths, filter_idx):
			print("Callback received: ", status, paths, filter_idx)
	)
	print("DisplayServer.file_dialog_show returned: ", result)
	
	# Quit after a short delay so the dialog has a chance to initialize/fail
	create_timer(1.0).timeout.connect(func():
		quit(0)
	)
