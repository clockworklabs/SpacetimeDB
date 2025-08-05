@tool
class_name TableIdData extends Resource

@export var pascal_case: String
@export var snake_case: String

func _init(p_pascal: String = "", p_snake: String = ""):
    pascal_case = p_pascal
    snake_case = p_snake
