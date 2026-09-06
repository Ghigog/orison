extends Node

# Active player nodes
var bgm_player: AudioStreamPlayer
var _bgm_fade_tween: Tween

# Dynamic generator hooks
var tts_generator_hook: Callable = Callable()
var image_generator_hook: Callable = Callable()
var sound_generator_hook: Callable = Callable()

func _ready() -> void:
	# Initialize BGM player
	bgm_player = AudioStreamPlayer.new()
	bgm_player.name = "BGMPlayer"
	bgm_player.bus = "Music" # Default to Music bus if it exists, otherwise Master
	add_child(bgm_player)
	
	# Connect to EventBus signals
	EventBus.location_changed.connect(_on_location_changed)
	EventBus.character_speaking.connect(_on_character_speaking)
	print("[MediaManager] Initialized and connected to EventBus.")

## Registers a custom TTS generator Callable(text: String, voice_id: String, output_path: String) -> void
func register_tts_generator(hook: Callable) -> void:
	tts_generator_hook = hook
	print("[MediaManager] Custom TTS generator registered.")

## Registers a custom Image generator Callable(prompt: String, category: String, output_path: String) -> void
func register_image_generator(hook: Callable) -> void:
	image_generator_hook = hook
	print("[MediaManager] Custom Image generator registered.")

## Registers a custom Sound generator Callable(sound_id: String, output_path: String) -> void
func register_sound_generator(hook: Callable) -> void:
	sound_generator_hook = hook
	print("[MediaManager] Custom Sound generator registered.")

## Play BGM from a file path (supports res:// and user:// paths)
func play_bgm(file_path: String) -> void:
	if _bgm_fade_tween and _bgm_fade_tween.is_valid():
		_bgm_fade_tween.kill()
		
	var stream = load_audio_file(file_path)
	if not stream:
		print("[MediaManager] BGM file not found or failed to load: ", file_path)
		stop_bgm()
		return
		
	# If already playing this exact stream, do not restart
	if bgm_player.playing and bgm_player.stream and bgm_player.stream.resource_path == file_path:
		return
		
	if bgm_player.playing:
		# Crossfade: fade out, change stream, fade in
		_bgm_fade_tween = create_tween()
		_bgm_fade_tween.tween_property(bgm_player, "volume_db", -80.0, 0.45)\
			.set_trans(Tween.TRANS_QUAD)\
			.set_ease(Tween.EASE_IN)
		_bgm_fade_tween.tween_callback(func():
			bgm_player.stream = stream
			bgm_player.volume_db = -80.0
			bgm_player.play()
		)
		_bgm_fade_tween.tween_property(bgm_player, "volume_db", 0.0, 0.45)\
			.set_trans(Tween.TRANS_QUAD)\
			.set_ease(Tween.EASE_OUT)
	else:
		bgm_player.stream = stream
		bgm_player.volume_db = -80.0
		bgm_player.play()
		_bgm_fade_tween = create_tween()
		_bgm_fade_tween.tween_property(bgm_player, "volume_db", 0.0, 0.5)\
			.set_trans(Tween.TRANS_QUAD)\
			.set_ease(Tween.EASE_OUT)

## Stops BGM with a smooth fade
func stop_bgm(duration: float = 0.5) -> void:
	if _bgm_fade_tween and _bgm_fade_tween.is_valid():
		_bgm_fade_tween.kill()
		
	if bgm_player.playing:
		_bgm_fade_tween = create_tween()
		_bgm_fade_tween.tween_property(bgm_player, "volume_db", -80.0, duration)\
			.set_trans(Tween.TRANS_QUAD)\
			.set_ease(Tween.EASE_IN)
		_bgm_fade_tween.tween_callback(func():
			bgm_player.stop()
			bgm_player.stream = null
		)

## Plays a voice clip on a temporary AudioStreamPlayer node
func play_voice_clip(file_path: String) -> void:
	var stream = load_audio_file(file_path)
	if not stream:
		print("[MediaManager] Voice clip failed to load: ", file_path)
		return
		
	var voice_player = AudioStreamPlayer.new()
	voice_player.name = "VoicePlayer"
	voice_player.bus = "SFX" # Default to SFX bus if it exists
	add_child(voice_player)
	voice_player.stream = stream
	voice_player.finished.connect(voice_player.queue_free)
	voice_player.play()

## Dynamic TTS generation interface with generator registry fallback
func generate_tts(text: String, voice_id: String, output_path: String) -> void:
	if tts_generator_hook.is_valid():
		tts_generator_hook.call(text, voice_id, output_path)
	else:
		# Log stub call and execute fallback
		print("[MediaManager] TTS Stub called. Text: \"%s\" | Voice ID: %s | Output: %s" % [text, voice_id, output_path])
		# Play a procedural speech blip sequence to represent TTS
		play_procedural_speech_blip_sequence()

## Dynamic Image generation interface with generator registry fallback
func generate_image(prompt: String, category: String, output_path: String) -> void:
	if image_generator_hook.is_valid():
		image_generator_hook.call(prompt, category, output_path)
	else:
		print("[MediaManager] Image Gen Stub called. Prompt: \"%s\" | Category: %s | Output: %s" % [prompt, category, output_path])
		# Fallback to local ImageGenManager
		ImageGenManager.generate_asset(prompt.sha256_text(), prompt, category, output_path)

## Helper to dynamically load audio files (OGG, MP3, WAV) from path
func load_audio_file(path: String) -> AudioStream:
	if path.begins_with("res://"):
		var res = load(path)
		if res is AudioStream:
			return res
			
	if not FileAccess.file_exists(path):
		return null
		
	var ext = path.get_extension().to_lower()
	var file = FileAccess.open(path, FileAccess.READ)
	if not file:
		return null
		
	var bytes = file.get_buffer(file.get_length())
	file.close()
	
	if ext == "ogg":
		# Validate OGG magic header 'OggS' ([0x4f, 0x67, 0x67, 0x53]) to avoid native hangs on corrupt files
		if bytes.size() >= 4 and bytes[0] == 0x4f and bytes[1] == 0x67 and bytes[2] == 0x67 and bytes[3] == 0x53:
			return AudioStreamOggVorbis.load_from_file(path)
		else:
			printerr("[MediaManager] Invalid OGG file magic header (missing 'OggS') for: ", path)
			return null
	elif ext == "mp3":
		var stream = AudioStreamMP3.new()
		stream.data = bytes
		return stream
	elif ext == "wav":
		var stream = AudioStreamWAV.new()
		# Simple WAV header bypass to load raw PCM data
		if bytes.size() > 44:
			stream.data = bytes.slice(44)
		else:
			stream.data = bytes
		stream.format = AudioStreamWAV.FORMAT_16_BITS
		stream.mix_rate = 44100
		stream.stereo = true
		return stream
		
	return null

## Dynamically generates a soft sine wave audio stream
func generate_sine_wave_stream(frequency: float, duration: float) -> AudioStreamWAV:
	var stream = AudioStreamWAV.new()
	stream.format = AudioStreamWAV.FORMAT_16_BITS
	stream.mix_rate = 44100
	stream.stereo = false
	
	var sample_count = int(duration * stream.mix_rate)
	var data = PackedByteArray()
	data.resize(sample_count * 2) # 2 bytes per sample for 16-bit
	
	for i in range(sample_count):
		var time = float(i) / stream.mix_rate
		var val = sin(2.0 * PI * frequency * time)
		var int_val = int(val * 16384.0) # Soft volume (0.5 * 32767)
		data.encode_s16(i * 2, int_val)
		
	stream.data = data
	return stream

## Plays a sequence of soft sine wave blips to represent procedural speech
func play_procedural_speech_blip_sequence() -> void:
	# Run a short async sequence of blips
	var timer = get_tree().create_timer(0.0)
	for i in range(3):
		var pitch = randf_range(250.0, 450.0)
		var dur = randf_range(0.06, 0.1)
		var delay = dur + randf_range(0.02, 0.05)
		
		# Spawn blip audio player after delay
		get_tree().create_timer(i * delay).timeout.connect(func():
			var stream = generate_sine_wave_stream(pitch, dur)
			var voice_player = AudioStreamPlayer.new()
			voice_player.name = "ProceduralSpeechBlip"
			voice_player.bus = "SFX"
			add_child(voice_player)
			voice_player.stream = stream
			voice_player.volume_db = -12.0
			voice_player.finished.connect(voice_player.queue_free)
			voice_player.play()
		)

func _on_location_changed(location_id: String) -> void:
	if CampaignState and CampaignState.graph_manager:
		var node = CampaignState.graph_manager.get_node(location_id)
		if not node.is_empty():
			var props = node.get("properties", {})
			var bgm_path = props.get("bgm_path", props.get("bgm", props.get("audio", "")))
			if not bgm_path.is_empty():
				play_bgm(bgm_path)
				return
				
	stop_bgm()

func _on_character_speaking(character_id: String) -> void:
	if CampaignState:
		var character = CampaignState.get_character(character_id)
		if not character.is_empty():
			var voice_path = character.get("voice_path", character.get("voice", ""))
			if not voice_path.is_empty() and FileAccess.file_exists(voice_path):
				play_voice_clip(voice_path)
				return
				
			var voice_id = character.get("voice_id", "default_voice")
			# If no file exists, trigger TTS stub which plays procedural blips
			generate_tts("Speaking turn...", voice_id, "user://temp_voice_clip.wav")
