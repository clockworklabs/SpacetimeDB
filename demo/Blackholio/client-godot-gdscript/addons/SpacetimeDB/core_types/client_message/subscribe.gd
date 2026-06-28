## Client message that subscribes to one or more SQL queries.
##
## Serialized with variant tag [constant SpacetimeDBClientMessage.SUBSCRIBE].
## The server will push matching rows immediately via [SubscribeAppliedMessage]
## and continue sending [TransactionUpdateMessage]s as the subscribed data changes.
class_name SubscribeMessage
extends SpacetimeDBClientMessage

## BSATN type hints used by the SDK's binary serializer.
const BSATN_TYPES: Dictionary[StringName, StringName] = { &"request_id": &"u32", &"query_id": &"u32" }

## Client-assigned id used to match this request to its [SubscribeAppliedMessage].
@export var request_id: int
## Client-assigned query set id that groups these queries for later unsubscription.
@export var query_id: int
## SQL query strings to subscribe to (e.g. [code]"SELECT * FROM player"[/code]).
@export var queries: Array[String]


func _init(p_request_id: int = 0, p_query_id: int = 0, p_queries: Array[String] = []):
	request_id = p_request_id
	query_id = p_query_id
	queries = p_queries
