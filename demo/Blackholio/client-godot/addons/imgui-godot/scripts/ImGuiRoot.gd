extends Node

signal imgui_layout

const csharp_controller := "res://addons/imgui-godot/ImGuiGodot/ImGuiController.cs"
const csharp_sync := "res://addons/imgui-godot/ImGuiGodot/ImGuiSync.cs"

func _enter_tree():
    var has_csharp := false
    if ClassDB.class_exists("CSharpScript"):
        var script := load(csharp_sync)
        has_csharp = script.get_instance_base_type() == "Object"

    if ClassDB.class_exists("ImGuiController"):
        # native
        add_child(ClassDB.instantiate("ImGuiController"))
        if has_csharp:
            var obj: Object = load(csharp_sync).new()
            obj.SyncPtrs()
            obj.free()
    else:
        # C# only
        if has_csharp:
            add_child(load(csharp_controller).new())
