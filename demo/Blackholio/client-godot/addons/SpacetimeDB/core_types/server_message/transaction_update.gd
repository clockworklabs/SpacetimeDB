@tool
class_name TransactionUpdateMessage extends Resource

@export var status: UpdateStatusData
@export var timestamp_ns: int # i64 (Timestamp)
@export var caller_identity: PackedByteArray # 32 bytes
@export var caller_connection_id: PackedByteArray # 16 bytes
@export var reducer_call: ReducerCallInfoData
@export var energy_quanta_used: int # u64
@export var total_host_execution_duration_ns: int # i64 (TimeDuration)

func _init():
    set_meta("bsatn_type_timestamp_ns", "i64")
    set_meta("bsatn_type_caller_identity", "identity")
    set_meta("bsatn_type_caller_connection_id", "connection_id")
    set_meta("bsatn_type_energy_quanta_used", "u64")
    set_meta("bsatn_type_energy_total_host_execution_duration_ns", "i64")
