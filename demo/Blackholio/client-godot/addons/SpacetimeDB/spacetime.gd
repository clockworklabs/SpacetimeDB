@tool
class_name SpacetimePlugin extends EditorPlugin

const LEGACY_DATA_PATH := "res://spacetime_data"
const BINDINGS_PATH := "res://spacetime_bindings"
const BINDINGS_SCHEMA_PATH := BINDINGS_PATH + "/schema"
const AUTOLOAD_NAME := "SpacetimeDB"
const AUTOLOAD_FILE_NAME := "spacetime_autoload.gd"
const AUTOLOAD_PATH := BINDINGS_SCHEMA_PATH + "/" + AUTOLOAD_FILE_NAME
const SAVE_PATH := BINDINGS_PATH + "/codegen_data.json"
const CONFIG_PATH := "res://addons/SpacetimeDB/plugin.cfg"
const UI_PANEL_NAME := "SpacetimeDB"
const UI_PATH := "res://addons/SpacetimeDB/ui/ui.tscn"

var http_request := HTTPRequest.new()
var codegen_data: Dictionary
var ui: SpacetimePluginUI

static var instance: SpacetimePlugin

func _enter_tree():    
    instance = self
    
    if not is_instance_valid(ui):
        var scene = load(UI_PATH)
        if scene:
            ui = scene.instantiate() as SpacetimePluginUI
        else:
            printerr("SpacetimePlugin: Failed to load UI scene: ", UI_PATH)
            return
            
    if is_instance_valid(ui):
        add_control_to_bottom_panel(ui, UI_PANEL_NAME)
    else:
        printerr("SpacetimePlugin: UI panel is not valid after instantiation")
        return
        
    ui.module_added.connect(_on_module_added)
    ui.module_updated.connect(_on_module_updated)
    ui.module_removed.connect(_on_module_removed)
    ui.check_uri.connect(_on_check_uri)
    ui.generate_schema.connect(_on_generate_schema)
    ui.clear_logs()
    
    http_request.timeout = 4;
    add_child(http_request)
    
    var config_file = ConfigFile.new()
    config_file.load(CONFIG_PATH)
    
    var version: String = config_file.get_value("plugin", "version", "0.0.0")
    var author: String = config_file.get_value("plugin", "author", "??")
    
    print_log("SpacetimeDB SDK v%s (c) 2025-present %s & Contributors" % [version, author])
    load_codegen_data()

func add_module(name: String, fromLoad: bool = false):
    ui.add_module(name)
    
    if not fromLoad:
        codegen_data.modules.append(name)
        save_codegen_data()

func load_codegen_data() -> void:
    var load_data = FileAccess.open(SAVE_PATH, FileAccess.READ)
    if load_data:
        print_log("Loading codegen data from %s" % [SAVE_PATH])
        codegen_data = JSON.parse_string(load_data.get_as_text())
        load_data.close()
        ui.set_uri(codegen_data.uri)
        
        for module in codegen_data.modules.duplicate():
            add_module(module, true)
            print_log("Loaded module: %s" % [module])
    else:
        codegen_data = {
            "uri": "http://127.0.0.1:3000",
            "modules": []
        }
        save_codegen_data()

func save_codegen_data() -> void:
    if not FileAccess.file_exists(BINDINGS_PATH):
        DirAccess.make_dir_absolute(BINDINGS_PATH)
        get_editor_interface().get_resource_filesystem().scan()

    var save_file = FileAccess.open(SAVE_PATH, FileAccess.WRITE)
    if not save_file:
        print_err("Failed to open codegen_data.json for writing")
        return
    save_file.store_string(JSON.stringify(codegen_data))
    save_file.close()

func _on_module_added(name: String) -> void:
    codegen_data.modules.append(name)
    save_codegen_data()

func _on_module_updated(index: int, name: String) -> void:
    codegen_data.modules.set(index, name)
    save_codegen_data()

func _on_module_removed(index: int) -> void:
    codegen_data.modules.remove_at(index)
    save_codegen_data()

func _on_check_uri(uri: String):
    if codegen_data.uri != uri:
        codegen_data.uri = uri
        save_codegen_data()
    
    if uri.ends_with("/"):
        uri = uri.left(-1)
    uri += "/v1/ping"
    
    print_log("Pinging... " + uri)
    http_request.request(uri)
    
    var result = await http_request.request_completed
    if result[1] == 0:
        print_err("Request timeout - " + uri)
    else:
        print_log("Response code: " + str(result[1]))

func _on_generate_schema(uri: String, module_names: Array[String]):
    if uri.ends_with("/"):
        uri = uri.left(-1)
            
    print_log("Starting code generation...")
    
    print_log("Fetching module schemas...")
    var module_schemas: Dictionary[String, String] = {}
    var failed = false
    for module_name in module_names:
        var schema_uri := "%s/v1/database/%s/schema?version=9" % [uri, module_name]
        http_request.request(schema_uri)
        var result = await http_request.request_completed
        if result[1] == 200:
            var json = PackedByteArray(result[3]).get_string_from_utf8()
            var snake_module_name = module_name.replace("-", "_")
            module_schemas[snake_module_name] = json
            print_log("Fetched schema for module: %s" % [module_name])
            continue
        
        if result[1] == 404:
            print_err("Module not found - %s" % [schema_uri])
        elif result[1] == 0:
            print_err("Request timeout - %s" % [schema_uri])
        else:
            print_err("Failed to fetch module schema: %s - Response code %s" % [module_name, result[1]])
        failed = true
    
    if failed:
        print_err("Code generation failed!")
        return
    
    var codegen := SpacetimeCodegen.new(BINDINGS_SCHEMA_PATH)
    var generated_files := codegen.generate_bindings(module_schemas)
    
    _cleanup_unused_classes(BINDINGS_SCHEMA_PATH, generated_files)
    
    if DirAccess.dir_exists_absolute(LEGACY_DATA_PATH):
        print_log("Removing legacy data directory: %s" % LEGACY_DATA_PATH)
        DirAccess.remove_absolute(LEGACY_DATA_PATH)
    
    var setting_name := "autoload/" + AUTOLOAD_NAME
    if ProjectSettings.has_setting(setting_name):
        var current_autoload: String = ProjectSettings.get_setting(setting_name)
        if current_autoload != "*%s" % AUTOLOAD_PATH:
            print_log("Removing old autoload path: %s" % current_autoload)
            ProjectSettings.set_setting(setting_name, null)
    
    if not ProjectSettings.has_setting(setting_name):
        add_autoload_singleton(AUTOLOAD_NAME, AUTOLOAD_PATH)
    
    get_editor_interface().get_resource_filesystem().scan()
    print_log("Code generation complete!")

func _cleanup_unused_classes(dir_path: String = "res://schema", files: Array[String] = []) -> void:
    var dir = DirAccess.open(dir_path)
    if not dir: return
    print_log("File Cleanup: Scanning folder: " + dir_path)
    for file in dir.get_files():
        if not file.ends_with(".gd"): continue
        var full_path = "%s/%s" % [dir_path, file]
        if not full_path in files:
            print_log("Removing file: %s" % [full_path])
            DirAccess.remove_absolute(full_path)
            if FileAccess.file_exists("%s.uid" % [full_path]):
                DirAccess.remove_absolute("%s.uid" % [full_path])
    var subfolders = dir.get_directories()
    for folder in subfolders:
        _cleanup_unused_classes(dir_path + "/" + folder, files)

static func clear_logs():
    instance.ui.clear_logs()

static func print_log(text: Variant) -> void:
    instance.ui.add_log(text)

static func print_err(text: Variant) -> void:
    instance.ui.add_err(text)

func _exit_tree():
    ui.destroy()
    ui = null
    http_request.queue_free()
    http_request = null
        
    if ProjectSettings.has_setting("autoload/" + AUTOLOAD_NAME):
        remove_autoload_singleton(AUTOLOAD_NAME)
