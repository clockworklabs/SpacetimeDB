class_name SpacetimeCodegenConfig extends RefCounted

const CONFIG_VERSION := 2
const DEFAULT_CONFIG := {
    "config_version": CONFIG_VERSION,
    "hide_scheduled_reducers": true,
    "hide_private_tables": true
}

var hide_private_tables := DEFAULT_CONFIG.hide_private_tables
var hide_scheduled_reducers := DEFAULT_CONFIG.hide_scheduled_reducers

var _codegen_config_path := SpacetimePlugin.BINDINGS_PATH + "/codegen_config.json"

func _init() -> void:
    load_config()

func load_config() -> void:
    var file: FileAccess
    if not FileAccess.file_exists(_codegen_config_path):
        file = FileAccess.open(_codegen_config_path, FileAccess.WRITE_READ)
        file.store_string(JSON.stringify(DEFAULT_CONFIG, "\t", false))
    else:
        file = FileAccess.open(_codegen_config_path, FileAccess.READ)
        
    var config: Dictionary = JSON.parse_string(file.get_as_text()) as Dictionary
    file.close()
    
    var version: int = config.get("config_version", -1) as int
    
    if version < CONFIG_VERSION:
        config = DEFAULT_CONFIG.duplicate() if version == -1 else _migrate_config(config, version)
        save_config(config)
    
    hide_scheduled_reducers = config.get("hide_scheduled_reducers", hide_scheduled_reducers) as bool
    hide_private_tables = config.get("hide_private_tables", hide_private_tables) as bool

func save_config(config: Dictionary) -> void:
    var file = FileAccess.open(_codegen_config_path, FileAccess.WRITE)
    file.store_string(JSON.stringify(config, "\t", false))
    file.close()
    
func _migrate_config(config: Dictionary, version: int) -> Dictionary:
    if version == 1:
        config = {
            "config_version": 2,
            "hide_scheduled_reducers": config.get("hide_scheduled_reducers", hide_scheduled_reducers),
            "hide_private_tables": config.get("hide_private_tables", hide_private_tables)
        }
    
    return config
    
