## Client message that invokes a stored procedure on the SpacetimeDB server.
##
## Serialized with variant tag [constant SpacetimeDBClientMessage.CALL_PROCEDURE].
## The [member procedure_name] identifies the procedure and [member args] carries
## the BSATN-encoded argument payload.
class_name CallProcedureMessage
extends SpacetimeDBClientMessage

## Flags modifying procedure call behaviour.
enum CallProcedureFlags {
	## No special flags.
	Default,
}

## BSATN type hints used by the SDK's binary serializer.
const BSATN_TYPES: Dictionary[StringName, StringName] = { &"request_id": &"u32", &"flags": &"u8" }

## Client-assigned id used to match this request to its [ProcedureResultData].
@export var request_id: int
## Procedure call flags. Currently only [constant CallProcedureFlags.Default].
@export var flags: CallProcedureFlags
## The name of the stored procedure to invoke.
@export var procedure_name: String
## BSATN-encoded procedure arguments.
@export var args: PackedByteArray


func _init(p_procedure_name: String = "", p_args: PackedByteArray = PackedByteArray(), p_request_id: int = 0, p_flags: CallProcedureFlags = CallProcedureFlags.Default):
	procedure_name = p_procedure_name
	args = p_args
	request_id = p_request_id
	flags = p_flags
