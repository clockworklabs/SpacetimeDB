class_name SubscribeMultiMessage extends Resource

## List of subscription query strings for this multi-subscription.
@export var queries: Array[String]

## Client-generated request ID to identify this multi-subscription later.
@export var request_id: int # u32
@export var query_id: QueryIdData

func _init(p_queries: Array[String] = [], p_query_id: int = 0, p_request_id: int = 0):
    var typed_queries: Array[String] = []
    typed_queries.assign(p_queries)
    queries = typed_queries
    request_id = p_request_id
    query_id = QueryIdData.new(p_query_id)
    # Add metadata for correct BSATN integer serialization
    set_meta("bsatn_type_request_id", "u32")
