@tool
class_name ImGuiConfig extends Resource

@export_range(0.25, 4.0, 0.001, "or_greater") var Scale: float = 1.0
@export var IniFilename: String = "user://imgui.ini"
@export_enum("RenderingDevice", "Canvas", "Dummy") var Renderer: String = "RenderingDevice"
@export_range(-128, 128) var Layer: int = 128

@export_category("Font Settings")
#@export var Fonts: Array[ImGuiFont]
@export var AddDefaultFont: bool = true

# HACK: workaround for intermittent Godot bug
var _fonts: Array

func _get_property_list() -> Array[Dictionary]:
    return [
        {
            "name": "Fonts",
            "class_name": &"",
            "type": TYPE_ARRAY,
            "hint": PROPERTY_HINT_TYPE_STRING,
            "hint_string": "24/17:ImGuiFont",
            "usage": PROPERTY_USAGE_SCRIPT_VARIABLE | PROPERTY_USAGE_EDITOR | PROPERTY_USAGE_STORAGE
        }
    ]

func _get(property: StringName) -> Variant:
    if property == &"Fonts":
        return _fonts
    return null

func _set(property: StringName, value: Variant) -> bool:
    if property == &"Fonts":
        _fonts = value
        return true
    return false
