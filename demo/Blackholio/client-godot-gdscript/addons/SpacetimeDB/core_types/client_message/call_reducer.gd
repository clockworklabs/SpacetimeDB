## Client message that invokes a reducer function on the SpacetimeDB server.
##
## Serialized with variant tag [constant SpacetimeDBClientMessage.CALL_REDUCER].
## The [member reducer_name] identifies the reducer and [member args] carries
## the BSATN-encoded argument payload.
class_name CallReducerMessage
extends SpacetimeDBClientMessage

## Flags modifying reducer call behaviour.
enum CallReducerFlags {
	## No special flags.
	Default,
}

## BSATN type hints used by the SDK's binary serializer.
const BSATN_TYPES: Dictionary[StringName, StringName] = { &"request_id": &"u32", &"flags": &"u8" }

## Client-assigned id used to match this request to its [ReducerResultMessage].
@export var request_id: int
## Reducer call flags. Currently only [constant CallReducerFlags.Default].
@export var flags: CallReducerFlags
## The name of the reducer function to invoke.
@export var reducer_name: String
## BSATN-encoded reducer arguments.
@export var args: PackedByteArray


func _init(p_reducer_name: String = "", p_args: PackedByteArray = PackedByteArray(), p_request_id: int = 0, p_flags: CallReducerFlags = CallReducerFlags.Default):
	reducer_name = p_reducer_name
	args = p_args
	request_id = p_request_id
	flags = p_flags
