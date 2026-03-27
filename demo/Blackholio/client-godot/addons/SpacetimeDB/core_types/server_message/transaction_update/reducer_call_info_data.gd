@tool
class_name ReducerCallInfoData extends Resource

@export var reducer_name: String
@export var reducer_id: int # u32
@export var args: PackedByteArray # Raw BSATN bytes for arguments
@export var request_id: int # u32
@export var execution_time: int

func _init(): 
    set_meta("bsatn_type_reducer_id", "u32")
    set_meta("bsatn_type_request_id", "u32")
    set_meta("bsatn_type_execution_time", "i64")
