# res://eval/EvalRunnerNode.gd
extends Node

## Orison evaluation harness (migration plan Phase 1).
##
## The unit suite in tests/ proves the parsers work. This proves the *storytelling*
## works, which is the thing players actually experience and the thing that has
## never been measured. Every prompt change before this was a guess.
##
## Run:
##   godot --headless --path . res://eval/EvalRunner.tscn -- --fixture=all
##
## Options (after the bare `--`):
##   --fixture=<name>[,<name>...]|all    default: all
##                                       e.g. --fixture=minimal,messy skips the
##                                       170-character `large` fixture, which is
##                                       170 sequential model calls in live mode
##   --cassette=<name>                   default: synthetic
##   --live                              call a real Ollama and record a cassette
##   --selftest                          assert the harness detects planted defects
##   --out=<res:// or user:// path>      default: user://eval_results.json
##
## MODES. Narrative metrics need model output. Rather than require Ollama for
## every run, responses are recorded once in --live mode into a cassette and
## replayed deterministically thereafter. This is the standard record/replay
## pattern and it is what lets the suite run in CI at all.
##
##   synthetic cassette -> proves the HARNESS works. Not a quality measurement.
##   recorded cassette  -> proves the ENGINE works. This is the real baseline.
##
## The distinction matters and the report labels it explicitly, because a green
## synthetic run says nothing whatsoever about narrative quality.

const VaultCompilerScript = preload("res://src/core/VaultCompiler.gd")

const FIXTURE_ROOT := "res://fixtures/vaults/"
const CASSETTE_ROOT := "res://eval/cassettes/"

var _opts := {
	"fixture": "all",
	"cassette": "synthetic",
	"live": false,
	"selftest": false,
	"out": "user://eval_results.json",
}

var _cassette: Dictionary = {}
var _recorded: Array = []
var _results: Array = []
var _latencies: Array[float] = []


func _ready() -> void:
	_parse_args()
	_print_header()

	ImageGenClient.mock_handler = func(params: Dictionary):
		var cb: Callable = params.get("callback", Callable())
		if not cb.is_valid():
			return
		match params.get("method", ""):
			"test_connection":
				cb.call(true, "")
			_:
				var img := Image.create(64, 64, false, Image.FORMAT_RGBA8)
				img.fill(Color(0.2, 0.3, 0.4, 1.0))
				cb.call(true, img, "")

	if _opts["live"]:
		_install_recording_proxy()
	else:
		_load_cassette()
		_install_replay_mock()

	var fixtures := _fixtures_to_run()
	for name in fixtures:
		await _run_fixture(name)
		# Flush after each fixture. A live run over `all` is long enough that an
		# interrupt is likely, and writing only at the end meant Ctrl-C threw away
		# every response recorded so far.
		if _opts["live"]:
			_write_cassette(true)

	if _opts["selftest"]:
		_run_selftest()

	var ok := _report()
	if _opts["live"]:
		_write_cassette(false)
	get_tree().quit(0 if ok else 1)


# ==============================================================================
# Setup
# ==============================================================================

func _parse_args() -> void:
	for arg in OS.get_cmdline_user_args():
		if arg == "--live":
			_opts["live"] = true
		elif arg == "--selftest":
			_opts["selftest"] = true
		elif arg.begins_with("--fixture="):
			_opts["fixture"] = arg.split("=", true, 1)[1]
		elif arg.begins_with("--cassette="):
			_opts["cassette"] = arg.split("=", true, 1)[1]
		elif arg.begins_with("--out="):
			_opts["out"] = arg.split("=", true, 1)[1]


func _print_header() -> void:
	print("=================================================================")
	print("             ORISON EVALUATION HARNESS (Phase 1)")
	print("=================================================================")
	var mode := "LIVE (real model, recording cassette)" if _opts["live"] else "REPLAY (cassette: %s)" % _opts["cassette"]
	print("Mode:     %s" % mode)
	print("Fixtures: %s" % _opts["fixture"])
	if not _opts["live"] and _opts["cassette"] == "synthetic":
		print("")
		print("NOTE: the synthetic cassette exercises the harness, it does NOT measure")
		print("      narrative quality. For a real baseline, record one first:")
		print("        godot --headless --path . res://eval/EvalRunner.tscn -- --live")
	print("")


func _fixtures_to_run() -> Array[String]:
	if _opts["fixture"] == "all":
		return ["minimal", "messy", "large"]
	# Comma-separated, so a live run can skip `large`. That fixture alone is 170
	# character files and therefore 170 sequential model calls; the narrative
	# metrics all come from minimal and messy, which are 7 between them.
	var out: Array[String] = []
	for part in str(_opts["fixture"]).split(",", false):
		var name := part.strip_edges()
		if not name.is_empty():
			out.append(name)
	return out


# ==============================================================================
# Cassette: record and replay
# ==============================================================================

func _load_cassette() -> void:
	var path: String = CASSETTE_ROOT + str(_opts["cassette"]) + ".json"
	if not FileAccess.file_exists(path):
		printerr("[eval] Cassette not found: %s" % path)
		_cassette = {"entries": [], "default": "{}"}
		return
	var f := FileAccess.open(path, FileAccess.READ)
	var json := JSON.new()
	if json.parse(f.get_as_text()) == OK and json.data is Dictionary:
		_cassette = json.data
	f.close()
	var meta: Dictionary = _cassette.get("meta", {})
	print("[eval] Cassette '%s' loaded (kind: %s, %d entries)" % [
		_opts["cassette"], meta.get("kind", "unknown"), (_cassette.get("entries", []) as Array).size()])


## Matches a prompt to a cassette response. Substring rules rather than prompt
## hashing, because hashes break the moment a prompt is reworded and the whole
## point of this harness is to let prompts be reworded and remeasured.
func _cassette_response(prompt: String) -> String:
	for entry in _cassette.get("entries", []):
		var needle := str(entry.get("match", ""))
		if not needle.is_empty() and prompt.contains(needle):
			var resp = entry.get("response", "")
			return resp if resp is String else JSON.stringify(resp)
	var d = _cassette.get("default", "{}")
	return d if d is String else JSON.stringify(d)


## In live mode the harness must both call the real model AND capture what came
## back, but mock_response_handler short-circuits before the real request. So the
## recording handler re-enters at _raw_send_custom_request, which does not consult
## the mock, and captures the pair on the way out.
##
## The full prompt is stored as the match key, so replay is exact-match. That is
## deliberately brittle: if a prompt is reworded the cassette stops matching and
## must be re-recorded, which is correct, because a cassette recorded against a
## different prompt is not evidence about the current one.
func _install_recording_proxy() -> void:
	LLMClient.mock_response_handler = func(prompt: String, model: String, callback: Callable, timeout: float):
		var wrapped := func(success: bool, text: String, err: String):
			if success:
				_recorded.append({"match": prompt, "response": text})
			if callback.is_valid():
				callback.call(success, text, err)
		LLMClient._raw_send_custom_request(prompt, model, wrapped, timeout, false)


func _install_replay_mock() -> void:
	LLMClient.mock_response_handler = func(prompt: String, _model: String, callback: Callable, _timeout: float):
		if callback.is_valid():
			callback.call(true, _cassette_response(prompt), "")
	# No embedding model in replay. This is deliberate: it isolates the lexical
	# retrieval path, which is precisely what defect B-13 concerns.
	LLMClient.mock_embedding_handler = func(_text: String): return []


func _write_cassette(interim: bool = false) -> void:
	var path := "user://recorded_cassette.json"
	var f := FileAccess.open(path, FileAccess.WRITE)
	if not f:
		printerr("[eval] Could not write cassette to %s" % path)
		return
	f.store_string(JSON.stringify({
		"meta": {
			"kind": "recorded",
			"model_dm": LLMClient.world_builder_model,
			"model_npc": LLMClient.character_model,
			"recorded_at": Time.get_datetime_string_from_system(true),
		},
		"entries": _recorded,
		"default": "{}",
	}, "\t"))
	f.close()
	if interim:
		print("[eval] Cassette flushed (%d responses so far) -> %s"
			% [_recorded.size(), ProjectSettings.globalize_path(path)])
	else:
		print("\n[eval] Recorded cassette written to %s (%d responses)"
			% [ProjectSettings.globalize_path(path), _recorded.size()])
		print("[eval] Copy it to eval/cassettes/baseline.json and commit it.")


# ==============================================================================
# Result recording
# ==============================================================================

func _record(suite: String, metric: String, passed: bool, detail: String, value = null) -> void:
	_results.append({
		"suite": suite, "metric": metric, "passed": passed, "detail": detail, "value": value,
	})
	print("  %s  %-34s %s" % ["PASS" if passed else "FAIL", metric, detail])


func _skip(suite: String, metric: String, why: String) -> void:
	_results.append({"suite": suite, "metric": metric, "skipped": true, "detail": why, "passed": true})
	print("  SKIP  %-34s %s" % [metric, why])


# ==============================================================================
# Fixture suites
# ==============================================================================

func _load_ground_truth(fixture: String) -> Dictionary:
	var path := FIXTURE_ROOT + fixture + "/ground_truth.json"
	if not FileAccess.file_exists(path):
		printerr("[eval] Missing ground truth: %s" % path)
		return {}
	var f := FileAccess.open(path, FileAccess.READ)
	var json := JSON.new()
	var ok := json.parse(f.get_as_text()) == OK
	f.close()
	return json.data if ok and json.data is Dictionary else {}


func _run_fixture(fixture: String) -> void:
	print("-----------------------------------------------------------------")
	print("FIXTURE: %s" % fixture)
	print("-----------------------------------------------------------------")

	var gt := _load_ground_truth(fixture)
	if gt.is_empty():
		_record(fixture, "ground_truth_loads", false, "could not read ground_truth.json")
		return

	var vault_path := ProjectSettings.globalize_path(FIXTURE_ROOT + fixture)
	var t0 := Time.get_ticks_msec()
	var compiled: Dictionary = await VaultCompilerScript.compile_vault(vault_path, {}, Callable())
	var compile_ms := Time.get_ticks_msec() - t0

	if compiled.is_empty():
		_record(fixture, "compile", false, "compile_vault returned empty")
		return
	_record(fixture, "compile", true, "%d ms" % compile_ms, compile_ms)

	var graph: Dictionary = compiled.get("knowledge_graph", {})
	var nodes: Dictionary = graph.get("nodes", {})
	var labels := {}
	for id in nodes:
		labels[str(nodes[id].get("label", "")).to_lower()] = id

	_check_entities(fixture, gt, nodes, labels)
	_check_absent(fixture, gt, labels)
	_check_retained(fixture, gt, nodes)
	_check_edges(fixture, gt, graph, labels)
	await _check_retrieval(fixture, gt, compiled)
	await _run_transcript_suite(fixture, compiled)


func _check_entities(fixture: String, gt: Dictionary, nodes: Dictionary, labels: Dictionary) -> void:
	var expected: Array = gt.get("entities", [])
	if expected.is_empty():
		return
	var missing: Array[String] = []
	var missing_fields: Array[String] = []
	for e in expected:
		var name := str(e.get("name", ""))
		var key := name.to_lower()
		if not labels.has(key):
			missing.append(name)
			continue
		var node: Dictionary = nodes[labels[key]]
		for field in e.get("required_fields", []):
			if _field_value(node, str(field)).strip_edges().is_empty():
				missing_fields.append("%s.%s" % [name, field])

	_record(fixture, "entity_presence", missing.is_empty(),
		"%d/%d present%s" % [expected.size() - missing.size(), expected.size(),
			"" if missing.is_empty() else " | missing: " + ", ".join(missing)],
		expected.size() - missing.size())

	# Ingest completeness by field. rag_architecture.md Bug 1 and Bug 2 both
	# manifest here: the entity exists but its fields are empty.
	_record(fixture, "entity_fields_populated", missing_fields.is_empty(),
		"all required fields populated" if missing_fields.is_empty()
			else "%d empty: %s" % [missing_fields.size(), ", ".join(missing_fields)],
		missing_fields.size())


## Canonical field name -> where the compiler actually stores it on a KG node.
## `biography` is the notable one: VaultCompiler maps it onto the node's `desc`
## (VaultCompiler.gd:66), so a naive lookup for "biography" reports every
## character as having an empty biography when they do not.
const FIELD_ALIASES := {
	"biography": ["biography", "desc", "backstory", "history"],
	"personality": ["personality", "traits"],
	"appearance": ["appearance", "looks"],
	"goals": ["goals", "motivation"],
	"gender": ["gender", "pronouns"],
}


func _field_value(node: Dictionary, field: String) -> String:
	var props: Dictionary = node.get("properties", {})
	var candidates: Array = FIELD_ALIASES.get(field, [field])
	for c in candidates:
		var key := str(c)
		if props.has(key) and not str(props[key]).strip_edges().is_empty():
			return str(props[key])
		if node.has(key) and not str(node[key]).strip_edges().is_empty():
			return str(node[key])
	return ""


func _check_absent(fixture: String, gt: Dictionary, labels: Dictionary) -> void:
	var forbidden: Array = gt.get("must_not_exist", [])
	if forbidden.is_empty():
		return
	var wrongly_present: Array[String] = []
	for f in forbidden:
		if labels.has(str(f.get("name", "")).to_lower()):
			wrongly_present.append(str(f.get("name", "")))
	_record(fixture, "dangling_link_tolerance", wrongly_present.is_empty(),
		"no phantom entities" if wrongly_present.is_empty()
			else "invented: " + ", ".join(wrongly_present))


## The data-lake principle from rag_architecture.md 1.1: no source text is ever
## silently discarded. Searches every node's serialised content for text the
## fixture says must survive.
func _check_retained(fixture: String, gt: Dictionary, nodes: Dictionary) -> void:
	var required: Array = gt.get("ingest_must_retain", [])
	if required.is_empty():
		return
	var haystack := ""
	for id in nodes:
		haystack += JSON.stringify(nodes[id]).to_lower()

	var lost: Array[String] = []
	for item in required:
		var needle := str(item.get("substring", "")).to_lower()
		if not needle.is_empty() and not haystack.contains(needle):
			lost.append('"%s"' % item.get("substring", ""))

	_record(fixture, "ingest_completeness", lost.is_empty(),
		"%d/%d retained" % [required.size() - lost.size(), required.size()]
			+ ("" if lost.is_empty() else " | DROPPED: " + ", ".join(lost)),
		required.size() - lost.size())


func _check_edges(fixture: String, gt: Dictionary, graph: Dictionary, labels: Dictionary) -> void:
	var expected: Array = gt.get("edges", [])
	if expected.is_empty():
		return
	var edges: Array = graph.get("edges", [])
	var missing: Array[String] = []
	for e in expected:
		var a: String = labels.get(str(e.get("from", "")).to_lower(), "")
		var b: String = labels.get(str(e.get("to", "")).to_lower(), "")
		var found := false
		if a != "" and b != "":
			for edge in edges:
				var f := str(edge.get("from", ""))
				var t := str(edge.get("to", ""))
				if (f == a and t == b) or (f == b and t == a):
					found = true
					break
		if not found:
			missing.append("%s->%s" % [e.get("from", "?"), e.get("to", "?")])
	_record(fixture, "edge_extraction", missing.is_empty(),
		"%d/%d edges" % [expected.size() - missing.size(), expected.size()]
			+ ("" if missing.is_empty() else " | missing: " + ", ".join(missing)),
		expected.size() - missing.size())


## Recall@k against labelled ground truth.
##
## retrieve_context() returns formatted prose rather than ranked ids, so the
## labels are parsed back out of its output. That is slightly awkward but it
## measures what the model is actually handed, which is the honest thing to
## measure, and it keeps the engine untouched under feature freeze.
func _check_retrieval(fixture: String, gt: Dictionary, compiled: Dictionary) -> void:
	var cases: Array = gt.get("retrieval", [])
	if cases.is_empty():
		return

	# Construct the manager over the compiled graph directly rather than routing
	# through CampaignState. set_knowledge_graph_data() assumes an initialised
	# campaign (CampaignState.gd:342 indexes state["knowledge_graph"] without
	# creating it) and throws otherwise, which silently left the graph empty and
	# made every retrieval query return nothing for the wrong reason.
	var kg_data: Dictionary = compiled.get("knowledge_graph", {})
	var kg := KnowledgeGraphManager.new(kg_data)

	var total_recall := 0.0
	var zero_hits := 0
	var details: Array[String] = []

	for c in cases:
		var query := str(c.get("query", ""))
		var relevant: Array = c.get("relevant", [])
		var k := int(c.get("k", 5))

		var t0 := Time.get_ticks_msec()
		var ctx: String = await kg.retrieve_context(query, -1, 0)
		_latencies.append(float(Time.get_ticks_msec() - t0))

		var got := _labels_from_context(ctx)
		var hits := 0
		for r in relevant:
			if got.has(str(r).to_lower()):
				hits += 1
		var recall := float(hits) / float(max(relevant.size(), 1))
		total_recall += recall
		if hits == 0:
			zero_hits += 1
		details.append('  q="%s" recall=%.2f (%d/%d, %d retrieved)' % [
			query, recall, hits, relevant.size(), got.size()])

	var mean_recall := total_recall / float(cases.size())
	for d in details:
		print(d)

	# No pass threshold on the first run: this establishes the baseline rather
	# than judging it. migration_plan.md Phase 3 must beat whatever lands here.
	_record(fixture, "retrieval_recall_mean", true,
		"%.3f across %d queries (%d returned nothing at all)" % [mean_recall, cases.size(), zero_hits],
		mean_recall)


func _labels_from_context(ctx: String) -> Dictionary:
	var out := {}
	for line in ctx.split("\n"):
		var s := line.strip_edges()
		if not s.begins_with("- **"):
			continue
		var rest := s.substr(4)
		var end := rest.find("**")
		if end > 0:
			out[rest.substr(0, end).to_lower()] = true
	return out


# ==============================================================================
# Transcript suite: metrics over model output
# ==============================================================================

## Drives real scripted turns and measures the model's actual output.
##
## Crucially this builds the prompt with the real PromptBuilder and sends it
## through the real LLMClient, so the same code path runs whether a cassette or a
## live model is behind it. An earlier version scored a hand-authored array of
## responses, which measured nothing about the engine: the prompt assembly, the
## character card, the history window and the schema instructions were all
## bypassed. This is what makes a recorded cassette a baseline rather than a prop.
func _run_transcript_suite(fixture: String, compiled: Dictionary) -> void:
	print("-----------------------------------------------------------------")
	print("TRANSCRIPT SUITE (%s)" % fixture)
	print("-----------------------------------------------------------------")

	var gt := _load_ground_truth(fixture)
	var script: Array = gt.get("transcript_script", [])
	if script.is_empty():
		_skip(fixture, "transcript", "fixture defines no transcript_script")
		return

	var char_name := str(gt.get("transcript_character", ""))
	var kg_data: Dictionary = compiled.get("knowledge_graph", {})
	var nodes: Dictionary = kg_data.get("nodes", {})

	var char_id := ""
	for id in nodes:
		if str(nodes[id].get("label", "")).to_lower() == char_name.to_lower():
			char_id = str(id)
			break
	if char_id.is_empty():
		_record(fixture, "transcript_setup", false,
			"transcript_character '%s' not found in the compiled graph" % char_name)
		return

	# A real campaign, so PromptBuilder sees the state it expects.
	var campaign_id := "eval_" + fixture
	var save_data := SaveManager.create_campaign(campaign_id, "Eval " + fixture)
	CampaignState.initialize(campaign_id, save_data)
	CampaignState.set_knowledge_graph_data(nodes, kg_data.get("edges", []))

	var node: Dictionary = nodes[char_id]
	var node_props: Dictionary = node.get("properties", {})
	CampaignState.init_character(char_id, char_name, str(node.get("desc", "")))
	CampaignState.update_character_properties(char_id, node_props)

	var graph := KnowledgeGraphManager.new()
	var prompt_builder := PromptBuilder.new(graph, EmotionPromptBuilder.new())

	var expected_pronouns: Array = gt.get("transcript_forbidden_pronouns", [])
	var bad_schema: Array[String] = []
	var pronoun_errors: Array[String] = []
	var forbidden: Array[String] = []
	var repeats: Array[String] = []
	var seen := {}
	var turn_latencies: Array[float] = []

	for i in range(script.size()):
		var player_input := str(script[i])
		CampaignState.add_history_log("user", player_input)

		var prompt: String = await prompt_builder.build_prompt(char_id, player_input, false)

		var t0 := Time.get_ticks_msec()
		var res := await _send_async(prompt)
		turn_latencies.append(float(Time.get_ticks_msec() - t0))

		var raw := str(res[1])
		if not bool(res[0]):
			bad_schema.append("turn %d (request failed)" % i)
			continue

		# 1. Schema validity. Parsed strictly: JsonRepair is deliberately NOT used
		#    here, because the whole point of migration plan 2.4 is that the model
		#    should be incapable of emitting anything that needs repairing. Scoring
		#    post-repair output would hide exactly the defect being measured.
		var parsed = JSON.parse_string(raw)
		if not (parsed is Dictionary) or not parsed.has("dialogue"):
			bad_schema.append("turn %d" % i)
			continue

		var dialogue := str(parsed.get("dialogue", ""))
		var narration := str(parsed.get("narration", ""))
		var blob := (dialogue + " " + narration).to_lower()

		# 2. Pronoun consistency (rag_architecture.md Bug 3). Word-bounded, since
		#    "she was" contains "he ".
		for rule in expected_pronouns:
			if _contains_word(blob, str(rule)):
				pronoun_errors.append("turn %d: forbidden '%s'" % [i, str(rule).strip_edges()])

		# 3. Loop detection.
		var norm := dialogue.strip_edges().to_lower()
		if not norm.is_empty():
			if seen.has(norm):
				repeats.append("turn %d repeats turn %d" % [i, seen[norm]])
			else:
				seen[norm] = i

		# 4. Forbidden phrasing.
		if _contains_word(dialogue.to_lower(), char_name.to_lower() + " says"):
			forbidden.append("turn %d: third-person self-reference" % i)
		if dialogue.to_lower().contains("affinity"):
			forbidden.append("turn %d: leaks affinity score" % i)

		CampaignState.add_history_log("assistant", dialogue, char_name, char_id)

	var n := script.size()
	_record(fixture, "schema_validity", bad_schema.is_empty(),
		"%d/%d parsed" % [n - bad_schema.size(), n]
			+ ("" if bad_schema.is_empty() else " | bad: " + ", ".join(bad_schema)),
		float(n - bad_schema.size()) / float(n))
	_record(fixture, "pronoun_consistency", pronoun_errors.is_empty(),
		"clean across %d turns" % n if pronoun_errors.is_empty() else ", ".join(pronoun_errors),
		pronoun_errors.size())
	_record(fixture, "loop_detection", repeats.is_empty(),
		"no verbatim repeats" if repeats.is_empty() else ", ".join(repeats), repeats.size())
	_record(fixture, "forbidden_phrasing", forbidden.is_empty(),
		"clean" if forbidden.is_empty() else ", ".join(forbidden), forbidden.size())

	if not turn_latencies.is_empty():
		turn_latencies.sort()
		var p50: float = turn_latencies[int(turn_latencies.size() * 0.5)]
		_record(fixture, "turn_latency_p50_ms", true, "%.0f ms" % p50, p50)


## Wraps the callback-style LLM API in something awaitable.
func _send_async(prompt: String) -> Array:
	var carrier := EvalSignalCarrier.new()
	LLMClient.send_custom_request(prompt, LLMClient.character_model,
		func(success: bool, text: String, err: String):
			carrier.done.call_deferred(success, text, err),
		300.0, LLMClient.RequestPriority.HIGH, false, LLMClient.ROLE_CHARACTER)
	return await carrier.finished


class EvalSignalCarrier:
	extends RefCounted
	signal finished(result: Array)

	func done(success: bool, text: String, err: String) -> void:
		finished.emit([success, text, err])


## Word-bounded containment. The reason this exists rather than String.contains:
## "he" is a substring of "she", "the", "there" and "when", so substring matching
## makes the pronoun metric fire constantly on correct output.
func _contains_word(haystack: String, word: String) -> bool:
	var w := word.strip_edges()
	if w.is_empty():
		return false
	var re := RegEx.new()
	if re.compile("\\b" + _escape_regex(w) + "\\b") != OK:
		return haystack.contains(w)
	return re.search(haystack) != null


func _escape_regex(s: String) -> String:
	var out := ""
	for ch in s:
		if "\\^$.|?*+()[]{}".contains(ch):
			out += "\\" + ch
		else:
			out += ch
	return out


# ==============================================================================
# Self-test: does the harness actually catch anything?
# ==============================================================================

## A suite that cannot fail is worthless. The synthetic cassette plants one of
## each defect class; this asserts every one was caught. Run it in CI so the
## harness cannot silently rot into a rubber stamp.
func _run_selftest() -> void:
	print("-----------------------------------------------------------------")
	print("SELF-TEST (harness must detect the planted defects)")
	print("-----------------------------------------------------------------")
	var expected := ["schema_validity", "pronoun_consistency", "loop_detection", "forbidden_phrasing"]
	var caught := {}
	for r in _results:
		if not r.get("passed", true) and not r.get("skipped", false):
			caught[r.get("metric", "")] = true

	var undetected: Array[String] = []
	for metric in expected:
		if not caught.has(metric):
			undetected.append(metric)

	if undetected.is_empty():
		print("  PASS  harness detected all %d planted defect classes" % expected.size())
	else:
		print("  FAIL  harness MISSED: %s" % ", ".join(undetected))
		print("        A metric that never fires is not a metric.")
	_results.append({
		"suite": "selftest", "metric": "detects_planted_defects",
		"passed": undetected.is_empty(),
		"detail": "missed: " + ", ".join(undetected) if not undetected.is_empty() else "all caught",
	})


# ==============================================================================
# Report
# ==============================================================================

func _report() -> bool:
	var passed := 0
	var failed := 0
	var skipped := 0
	for r in _results:
		if r.get("skipped", false):
			skipped += 1
		elif r.get("passed", false):
			passed += 1
		else:
			failed += 1

	# In self-test mode the transcript failures are the point, so only the
	# self-test verdict decides the exit code.
	var ok := failed == 0
	if _opts["selftest"]:
		ok = true
		for r in _results:
			if r.get("suite", "") == "selftest" and not r.get("passed", false):
				ok = false

	print("")
	print("=================================================================")
	print("Eval Results: %d passed, %d failed, %d skipped" % [passed, failed, skipped])
	if not _latencies.is_empty():
		_latencies.sort()
		var p50: float = _latencies[int(_latencies.size() * 0.5)]
		var p95: float = _latencies[mini(int(_latencies.size() * 0.95), _latencies.size() - 1)]
		print("Retrieval latency: p50 %.0f ms, p95 %.0f ms (%d queries)" % [p50, p95, _latencies.size()])
	if not _opts["live"] and _opts["cassette"] == "synthetic":
		print("")
		print("REMINDER: synthetic cassette. Narrative numbers above are harness")
		print("          self-checks, NOT a measurement of Orison's quality.")
	print("=================================================================")

	var f := FileAccess.open(str(_opts["out"]), FileAccess.WRITE)
	if f:
		f.store_string(JSON.stringify({
			"mode": "live" if _opts["live"] else "replay",
			"cassette": _opts["cassette"],
			"synthetic": (not _opts["live"]) and _opts["cassette"] == "synthetic",
			"generated_at": Time.get_datetime_string_from_system(true),
			"summary": {"passed": passed, "failed": failed, "skipped": skipped},
			"results": _results,
		}, "\t"))
		f.close()
		print("Machine-readable results: %s" % ProjectSettings.globalize_path(str(_opts["out"])))

	return ok
