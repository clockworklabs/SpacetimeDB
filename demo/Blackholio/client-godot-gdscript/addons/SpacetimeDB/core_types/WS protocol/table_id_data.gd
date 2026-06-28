## Stores the PascalCase and snake_case forms of a table name.
##
## Used during message parsing and codegen to map between the server's table
## identifier format and GDScript naming conventions.
@tool
class_name TableIdData
extends RefCounted

## The table name in PascalCase (e.g. [code]"PlayerState"[/code]).
var pascal_case: String
## The table name in snake_case (e.g. [code]"player_state"[/code]).
var snake_case: String


func _init(p_pascal: String = "", p_snake: String = ""):
	pascal_case = p_pascal
	snake_case = p_snake
