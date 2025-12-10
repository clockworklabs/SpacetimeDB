class_name SubscribeSingleMessage extends Resource

## The query string for the single subscription.
@export var query_string: String

## Client-generated request ID to identify this subscription later (e.g., for unsubscribe).
@export var request_id: int # u32

func _init(p_query_string: String = "", p_request_id: int = 0):
    query_string = p_query_string
    request_id = p_request_id
    # Add metadata for correct BSATN integer serialization
    set_meta("bsatn_type_request_id", "u32")
