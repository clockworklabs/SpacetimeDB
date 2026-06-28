## Runtime schema registry that maps table and type names to their [GDScript] classes.
##
## Built at initialization from the codegen'd scripts in [code]spacetime_bindings/[/code]
## and SDK core types. [LocalDatabase] and [BSATNDeserializer] use this to
## instantiate the correct row type when deserializing server messages.
class_name SpacetimeDBSchema
extends Resource

## All known types keyed by normalized name (lowercased, underscores removed).
var types: Dictionary[StringName, GDScript] = { }
## Subset of [member types] that are actual tables (have [code]table_names[/code] const).
var tables: Dictionary[StringName, GDScript] = { }
## Raw wire table names consumed once by [method LocalDatabase._init] then cleared.
var raw_table_names: Array[StringName] = []
## Enables verbose debug printing during schema loading.
var debug_mode: bool = false


func _init(p_module_name: String, p_schema_path: String = "res://spacetime_bindings/schema", p_debug_mode: bool = false) -> void:
	debug_mode = p_debug_mode

	# Load table row schemas and spacetime types
	_load_types("%s/types" % p_schema_path, p_module_name.to_snake_case())
	# Load core types if they are defined as Resources with scripts
	_load_types(SpacetimePlugin.ADDON_PATH + "/core_types/**")


## Returns the [GDScript] for [param type_name] (normalized), or [code]null[/code] if unknown.
func get_type(type_name: StringName) -> GDScript:
	return types.get(type_name)


func _load_types(raw_path: String, prefix: String = "") -> void:
	var path: String = raw_path
	if path.ends_with("/**"):
		path = path.left(-3)

	var dir: DirAccess = DirAccess.open(path)
	if not DirAccess.dir_exists_absolute(path):
		printerr("SpacetimeDBSchema: Schema directory does not exist: ", path)
		return

	dir.list_dir_begin()
	while true:
		var file_name_raw: String = dir.get_next()
		if file_name_raw.is_empty():
			break

		if dir.current_is_dir():
			var dir_name: String = file_name_raw
			if dir_name != "." and dir_name != ".." and raw_path.ends_with("/**"):
				var dir_path: String = path.path_join(dir_name)
				_load_types(dir_path.path_join("/**"), prefix)
			continue

		var file_name: String = file_name_raw

		# Handle potential remapping on export
		if file_name.ends_with(".remap"):
			file_name = file_name.replace(".remap", "")
			if not file_name.ends_with(".gd"):
				file_name += ".gd"

		if not file_name.ends_with(".gd"):
			continue

		if not prefix.is_empty() and not file_name.begins_with(prefix):
			continue

		var script_path: String = path.path_join(file_name)
		if not ResourceLoader.exists(script_path):
			printerr("SpacetimeDBSchema: Script file not found or inaccessible: ", script_path, " (Original name: ", file_name_raw, ")")
			continue

		var script: GDScript = ResourceLoader.load(script_path, "GDScript") as GDScript

		if script and script.can_instantiate():
			var instance: Variant = script.new()
			if instance is RefCounted: # Resource extends RefCounted — one check covers both
				var fallback_table_names: Array[String] = [file_name.get_basename().get_file()]

				var constants: Dictionary = script.get_script_constant_map()

				if constants.has('table_names'):
					_add_table_names(constants['table_names'], true, script, script_path)
				_add_table_names(fallback_table_names, false, script, script_path)

	dir.list_dir_end()


func _add_table_names(table_names: Array, is_table: bool, script: GDScript, script_path: String) -> void:
	for table_name in table_names:
		var sn: StringName = StringName(table_name)
		var lower_table_name: StringName = sn.to_lower().replace("_", "")
		if types.has(lower_table_name) and debug_mode:
			push_warning("SpacetimeDBSchema: Overwriting schema for table '%s' (from %s)" % [table_name, script_path])

		if is_table:
			tables[lower_table_name] = script
			raw_table_names.append(sn)
		types[lower_table_name] = script
