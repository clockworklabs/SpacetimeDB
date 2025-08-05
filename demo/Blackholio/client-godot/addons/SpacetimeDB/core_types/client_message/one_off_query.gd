class_name OneOffQueryMessage extends Resource

## The query string to execute once on the server.
@export var query: String

func _init(p_query: String = ""):
    query = p_query
