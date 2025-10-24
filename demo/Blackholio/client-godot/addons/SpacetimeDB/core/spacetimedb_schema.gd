class_name SpacetimeDBSchema extends Resource

var types: Dictionary[String, GDScript] = {}
var tables: Dictionary[String, GDScript] = {}

var debug_mode: bool = false # Controls verbose debug printing

func _init(p_module_name: String, p_schema_path: String = "res://spacetime_bindings/schema", p_debug_mode: bool = false) -> void:
    debug_mode = p_debug_mode
    
    # Load table row schemas and spacetime types
    _load_types("%s/types" % p_schema_path, p_module_name.to_snake_case())
    # Load core types if they are defined as Resources with scripts
    _load_types("res://addons/SpacetimeDB/core_types/**")

func _load_types(raw_path: String, prefix: String = "") -> void:
    var path := raw_path
    if path.ends_with("/**"):
        path = path.left(-3)
    
    var dir := DirAccess.open(path)
    if not DirAccess.dir_exists_absolute(path):
        printerr("SpacetimeDBSchema: Schema directory does not exist: ", path)
        return

    dir.list_dir_begin()
    while true:
        var file_name_raw := dir.get_next()
        if file_name_raw == "":
            break
        
        if dir.current_is_dir():
            var dir_name := file_name_raw
            if dir_name != "." and dir_name != ".." and raw_path.ends_with("/**"):
                var dir_path := path.path_join(dir_name)
                _load_types(dir_path.path_join("/**"), prefix)
            continue

        var file_name := file_name_raw

        # Handle potential remapping on export
        if file_name.ends_with(".remap"):
            file_name = file_name.replace(".remap", "")
            if not file_name.ends_with(".gd"):
                file_name += ".gd"

        if not file_name.ends_with(".gd"):
            continue
            
        if prefix != "" and not file_name.begins_with(prefix):
            continue

        var script_path := path.path_join(file_name)
        if not ResourceLoader.exists(script_path):
            printerr("SpacetimeDBSchema: Script file not found or inaccessible: ", script_path, " (Original name: ", file_name_raw, ")")
            continue

        var script := ResourceLoader.load(script_path, "GDScript") as GDScript

        if script and script.can_instantiate():
            var instance = script.new()
            if instance is Resource: # Ensure it's a resource to get metadata
                var fallback_table_names: Array[String] = [file_name.get_basename().get_file()]
                
                var constants := script.get_script_constant_map()
                var table_names: Array[String]
                var is_table := false
                if constants.has('table_names'):
                    is_table = true
                    table_names = constants['table_names'] as Array[String]
                else:
                    table_names = fallback_table_names

                for table_name in table_names:
                    var lower_table_name := table_name.to_lower().replace("_", "")
                    if types.has(lower_table_name) and debug_mode:
                        push_warning("SpacetimeDBSchema: Overwriting schema for table '%s' (from %s)" % [table_name, script_path])
                        
                    if is_table:
                        tables[lower_table_name] = script
                    types[lower_table_name] = script
            
    dir.list_dir_end()

func get_type(type_name: String) -> GDScript:
    return types.get(type_name)
