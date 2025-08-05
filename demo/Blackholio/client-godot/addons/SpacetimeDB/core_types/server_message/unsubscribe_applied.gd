@tool
class_name UnsubscribeAppliedMessage extends Resource

@export var request_id: int # u32
@export var total_host_execution_duration_micros: int # u64
@export var query_id: QueryIdData # Nested Resource
@export var rows: SubscribeRowsData # Nested Resource

func _init():
    set_meta("bsatn_type_request_id", "u32")
    set_meta("bsatn_type_total_host_execution_duration_micros", "u64")
