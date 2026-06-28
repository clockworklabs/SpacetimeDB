class_name SpacetimeParsedSchema
extends Resource

var module: String = ""
var types: Array[Dictionary] = []
var reducers: Array[Dictionary] = []
var procedures: Array[Dictionary] = []
var tables: Array[Dictionary] = []
var type_map: Dictionary[String, String] = { }
var meta_type_map: Dictionary[String, String] = { }
var typespace: Array = []


func is_empty() -> bool:
	return types.is_empty() and reducers.is_empty()


func to_dictionary() -> Dictionary:
	return {
		"module": module,
		"types": types,
		"reducers": reducers,
		"procedures": procedures,
		"tables": tables,
		"type_map": type_map,
		"meta_type_map": meta_type_map,
		"typespace": typespace,
	}
