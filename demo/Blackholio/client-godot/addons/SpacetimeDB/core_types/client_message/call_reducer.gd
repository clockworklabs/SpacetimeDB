class_name CallReducerMessage extends Resource

@export var reducer_name: String
@export var args: PackedByteArray
@export var request_id: int # u32
@export var flags: int # u8

func _init(p_reducer_name: String = "", p_args: PackedByteArray = PackedByteArray(), p_request_id: int = 0, p_flags: int = 0):
    reducer_name = p_reducer_name
    args = p_args
    request_id = p_request_id
    flags = p_flags
    set_meta("bsatn_type_request_id", "u32")
    set_meta("bsatn_type_flags", "u8")
