## Client message that executes a single SQL query without creating a subscription.
##
## Serialized with variant tag [constant SpacetimeDBClientMessage.ONEOFF_QUERY].
## The server replies with a [OneOffQueryResponseMessage] containing the result rows.
class_name OneOffQueryMessage
extends SpacetimeDBClientMessage

## BSATN type hints for property serialization order.
const BSATN_TYPES: Dictionary[StringName, StringName] = { &"request_id": &"u32", &"query_string": &"string" }

## Client-assigned request id for correlation.
@export var request_id: int
## The SQL query string to execute once on the server.
@export var query_string: String


func _init(p_request_id: int = 0, p_query_string: String = ""):
	request_id = p_request_id
	query_string = p_query_string
