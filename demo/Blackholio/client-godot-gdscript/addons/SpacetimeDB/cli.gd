## Headless codegen entry point.
##
## Run with:
##   godot --headless --path <project> --script res://addons/SpacetimeDB/cli.gd
##
## Reads plugin config from disk, fetches schemas, regenerates bindings.
## Exits 0 on success, 1 on failure.
extends SceneTree

func _initialize() -> void:
	if not ResourceLoader.exists(SpacetimePlugin.SAVE_PATH, "SpacetimeDBPluginConfig"):
		printerr("Plugin config not found at %s" % [SpacetimePlugin.SAVE_PATH])
		quit(1)
		return

	var plugin_config: SpacetimeDBPluginConfig = ResourceLoader.load(SpacetimePlugin.SAVE_PATH)
	if plugin_config == null or plugin_config.module_configs.is_empty():
		printerr("Plugin config has no modules configured")
		quit(1)
		return

	var http_request: HTTPRequest = HTTPRequest.new()
	http_request.timeout = 4
	root.add_child(http_request)
	# Unbounded wait — one frame for the HTTPRequest node to enter the tree.
	# Headless context: process_frame always fires, no deadline needed.
	await process_frame

	var ok: bool = await SpacetimePlugin.generate_schema(http_request, plugin_config)
	if not ok:
		printerr("Codegen failed")
		quit(1)
		return

	print("OK!")
	quit(0)
