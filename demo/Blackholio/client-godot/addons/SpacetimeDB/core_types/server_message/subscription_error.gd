@tool
class_name SubscriptionErrorMessage extends Resource

@export var total_host_execution_duration_micros: int # u64
@export var request_id: int # u32 or -1 for None
@export var query_id: int # u32 or -1 for None
@export var table_id_resource: TableIdData # TableIdData or null for None
@export var error_message: String

func _init():
    request_id = -1 # Default to None
    query_id = -1
    table_id_resource = null # Default to None
    set_meta("bsatn_type_total_host_execution_duration_micros", "u64")
    
func has_request_id() -> bool: return request_id != -1
func has_query_id() -> bool: return query_id != -1
func has_table_id() -> bool: return table_id_resource != null
func get_table_id() -> TableIdData: return table_id_resource
