## Handle returned by [method SpacetimeDBClient.call_procedure].
##
## Tracks the lifecycle of a single stored-procedure call. Poll [member outcome]
## or [code]await[/code] [method wait_for_response] to get the result bytes.
## Use [method decode] to BSATN-decode the return value.
class_name SpacetimeDBProcedureCall
extends RefCounted

## Lifecycle states of a procedure call.
enum Outcome {
	## Waiting for the server to respond.
	PENDING,
	## Procedure returned successfully.
	RETURNED,
	## Procedure returned an application-level error.
	ERROR,
	## Server encountered an internal error.
	INTERNAL_ERROR,
	## Client timed out waiting for a response.
	TIMEOUT,
	## Connection was lost before a response arrived.
	DISCONNECTED,
}

## Client-assigned request id for correlation.
var request_id: int = -1
## Immediate serialization or send error, or [constant OK].
var error: Error = OK
## Current lifecycle state.
var outcome: Outcome = Outcome.PENDING
## Human-readable error description.
var error_message: String = ""
## BSATN-encoded return value (populated on [constant Outcome.RETURNED]).
var return_bytes: PackedByteArray
var _return_bsatn_type: StringName = &""
var _client: SpacetimeDBClient


static func create(
		p_client: SpacetimeDBClient,
		p_request_id: int,
		p_return_bsatn_type: StringName = &"",
) -> SpacetimeDBProcedureCall:
	var call: SpacetimeDBProcedureCall = SpacetimeDBProcedureCall.new()
	call._client = p_client
	call.request_id = p_request_id
	call._return_bsatn_type = p_return_bsatn_type
	return call


## Creates a pre-failed handle for an immediate client-side error.
static func fail(p_error: Error) -> SpacetimeDBProcedureCall:
	var call: SpacetimeDBProcedureCall = SpacetimeDBProcedureCall.new()
	call.error = p_error
	call.outcome = Outcome.ERROR
	call.error_message = error_string(p_error)
	return call


## Awaits the server response for up to [param timeout_sec] seconds, then returns this
## handle so the unambiguous outcome is available in one step:[br]
## [code]var call := await procedures.foo(args).wait_for_response()[/code][br]
## then inspect [member outcome] / [method is_ok] / [method is_error] / [method decode] /
## [member return_bytes] / [member error_message]. Distinguishes RETURNED / ERROR /
## INTERNAL_ERROR / TIMEOUT / DISCONNECTED instead of an ambiguous empty-array return.
func wait_for_response(timeout_sec: float = 10) -> SpacetimeDBProcedureCall:
	if error:
		return self
	await _client.wait_for_procedure_response(request_id, timeout_sec)
	if outcome == Outcome.PENDING:
		outcome = Outcome.TIMEOUT
		error_message = "Timeout waiting for procedure response"
	return self


## Decodes [member return_bytes] using the BSATN type provided at call time.[br]
## Returns [code]null[/code] if the bytes are empty or no type was specified.
func decode() -> Variant:
	if return_bytes.is_empty() or _return_bsatn_type.is_empty():
		return null
	var spb: StreamPeerBuffer = StreamPeerBuffer.new()
	spb.data_array = return_bytes
	spb.big_endian = false
	spb.seek(0)
	return _client._deserializer._read_value_from_bsatn_type(spb, _return_bsatn_type, &"procedure_return")


## Returns [code]true[/code] if the procedure returned successfully.
func is_ok() -> bool:
	return outcome == Outcome.RETURNED


## Returns [code]true[/code] if the procedure ended in any error state.
func is_error() -> bool:
	return outcome == Outcome.ERROR or outcome == Outcome.INTERNAL_ERROR or outcome == Outcome.DISCONNECTED


## Returns [code]true[/code] if the call has received a terminal outcome.
func is_completed() -> bool:
	return outcome != Outcome.PENDING
