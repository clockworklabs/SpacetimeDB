class_name UnsubscribeMessage extends Resource

## Client request ID used during the original subscription.
@export var request_id: int # u32

## Identifier of the query being unsubscribed from (as a Resource).
@export var query_id: QueryIdData

func _init(p_request_id: int = 0, p_query_id_resource: QueryIdData = null):
    request_id = p_request_id
    query_id = p_query_id_resource if p_query_id_resource != null else QueryIdData.new()
    set_meta("bsatn_type_request_id", "u32")
