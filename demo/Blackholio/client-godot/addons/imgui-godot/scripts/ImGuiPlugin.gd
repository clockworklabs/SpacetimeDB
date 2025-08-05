@tool
extends EditorPlugin

var _exporter: ImGuiExporter
const imgui_root := "res://addons/imgui-godot/data/ImGuiRoot.tscn"

func _enter_tree():
    Engine.register_singleton("ImGuiPlugin", self)
    add_autoload_singleton("ImGuiRoot", imgui_root)

    # register export plugin
    _exporter = ImGuiExporter.new()
    _exporter.plugin = self
    add_export_plugin(_exporter)

    # add project setting
    var setting_name = "addons/imgui/config"
    if not ProjectSettings.has_setting(setting_name):
        ProjectSettings.set_setting(setting_name, String())
    ProjectSettings.add_property_info({
        "name": setting_name,
        "type": TYPE_STRING,
        "hint": PROPERTY_HINT_FILE,
        "hint_string": "*.tres,*.res",
        })
    ProjectSettings.set_initial_value(setting_name, String())
    ProjectSettings.set_as_basic(setting_name, true)

    # remove obsolete ImGuiLayer autoload
    if ProjectSettings.has_setting("autoload/ImGuiLayer"):
        remove_autoload_singleton("ImGuiLayer")

    # warn user if csproj will fail to build
    if "C#" in ProjectSettings.get_setting("application/config/features"):
        var projPath: String = ProjectSettings.get_setting("dotnet/project/solution_directory")
        var fn: String = "%s.csproj" % ProjectSettings.get_setting("dotnet/project/assembly_name")
        check_csproj(projPath.path_join(fn))

func check_csproj(fn):
    var fi := FileAccess.open(fn, FileAccess.READ)
    if !fi:
        return

    var changesNeeded := ""
    var data := fi.get_as_text()
    var idx := data.find("<TargetFramework>net")
    if idx != -1:
        idx += len("<TargetFramework>net")
        var idx_dot := data.find(".", idx)
        var netVer := data.substr(idx, idx_dot - idx).to_int()
        if netVer < 8:
            changesNeeded += "- Set target framework to .NET 8 or later\n"

    if !data.contains("<AllowUnsafeBlocks>"):
        changesNeeded += "- Allow unsafe blocks\n"

    if !data.contains("<PackageReference Include=\"ImGui.NET\""):
        changesNeeded += "- Add NuGet package ImGui.NET\n"

    if changesNeeded != "":
        var text := "Your .csproj requires the following changes:\n\n%s" % changesNeeded
        push_warning("imgui-godot\n\n%s" % text)

func _exit_tree():
    remove_export_plugin(_exporter)
    remove_autoload_singleton("ImGuiRoot")
    Engine.unregister_singleton("ImGuiPlugin")


class ImGuiExporter extends EditorExportPlugin:
    var export_imgui := true
    var extension_list_file := PackedByteArray()
    var gdext_file := PackedByteArray()
    var plugin: EditorPlugin = null
    const gdext_resource := "res://addons/imgui-godot/imgui-godot-native.gdextension"

    func _get_name() -> String:
        return "ImGui"

    func _get_export_options(platform: EditorExportPlatform) -> Array[Dictionary]:
        var rv: Array[Dictionary] = []
        var desktop_platform := platform.get_os_name() in ["Windows", "macOS", "Linux"]

        rv.append({
            "option": {
                "name": "imgui/debug",
                "type": TYPE_BOOL,
            },
            "default_value": desktop_platform,
        })
        rv.append({
            "option": {
                "name": "imgui/release",
                "type": TYPE_BOOL,
            },
            "default_value": false,
        })
        return rv

    func _export_begin(features: PackedStringArray, is_debug: bool, path: String, flags: int) -> void:
        extension_list_file = PackedByteArray()
        gdext_file = PackedByteArray()

        if is_debug:
            export_imgui = get_option("imgui/debug")
        else:
            export_imgui = get_option("imgui/release")

        if not export_imgui:
            print("imgui-godot: not exporting (ignore 'failed to load GDExtension' error)")

            # disable autoload
            if ProjectSettings.has_setting("autoload/ImGuiRoot"):
                plugin.remove_autoload_singleton("ImGuiRoot")

            # prevent copying of GDExtension library (causes printed error)
            var da := DirAccess.open("res://addons/imgui-godot")
            if da.file_exists("imgui-godot-native.gdextension"):
                gdext_file = FileAccess.get_file_as_bytes(gdext_resource)
                da.remove("imgui-godot-native.gdextension")

            # prevent attempt to load .gdextension resource
            extension_list_file = FileAccess.get_file_as_bytes("res://.godot/extension_list.cfg")
            var extension_list := extension_list_file.get_string_from_utf8()
            var idx := extension_list.find(gdext_resource)
            if idx != -1:
                var buf := extension_list.erase(idx, gdext_resource.length())
                var fi := FileAccess.open("res://.godot/extension_list.cfg", FileAccess.WRITE)
                fi.store_string(buf)
                fi.close()

    func _export_end() -> void:
        if not export_imgui:
            # restore autoload
            plugin.add_autoload_singleton("ImGuiRoot", imgui_root)

            # restore GDExtension
            if extension_list_file.size() > 0:
                var fi := FileAccess.open("res://.godot/extension_list.cfg", FileAccess.WRITE)
                fi.store_buffer(extension_list_file)
                fi.close()
            if gdext_file.size() > 0:
                var fi := FileAccess.open(gdext_resource, FileAccess.WRITE)
                fi.store_buffer(gdext_file)
                fi.close()

    func _export_file(path: String, type: String, features: PackedStringArray) -> void:
        if not export_imgui:
            if path.contains("res://addons/imgui-godot"):
                skip()
