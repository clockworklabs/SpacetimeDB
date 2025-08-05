class_name UnsubscribeMultiMessage extends Resource

## Client request ID used during the original multi-subscription.
@export var request_id: int # u32
@export var query_id: QueryIdData

func _init(p_query_id: int = 0, p_request_id: int = 0):
    request_id = p_request_id
    query_id = QueryIdData.new(p_query_id)
    set_meta("bsatn_type_request_id", "u32")
