## Handle returned by [method SpacetimeDBClient.call_reducer].
##
## Tracks the lifecycle of a single reducer call from submission through
## server response. Poll [member outcome] or [code]await[/code]
## [method wait_for_response] to determine the result.
class_name SpacetimeDBReducerCall
extends RefCounted

## Lifecycle states of a reducer call.
enum Outcome {
	## Waiting for the server to respond.
	PENDING,
	## Reducer succeeded and produced database changes.
	OK,
	## Reducer succeeded with no database changes.
	OK_EMPTY,
	## Reducer returned an application-level error.
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
## Current lifecycle state. Updated by the client when the server responds.
var outcome: Outcome = Outcome.PENDING
## Human-readable error description (populated on [constant Outcome.ERROR] or [constant Outcome.INTERNAL_ERROR]).
var error_message: String = ""
## The transaction update from a successful reducer (populated on [constant Outcome.OK]).
var transaction_update: TransactionUpdateMessage = null
## Raw BSATN bytes of the reducer's return value (populated only on [constant Outcome.OK]; empty on [constant Outcome.OK_EMPTY] and all other outcomes).
var ret_value: PackedByteArray = PackedByteArray()
var _ret_bsatn_type: StringName = &""
var _client: SpacetimeDBClient


static func create(
		p_client: SpacetimeDBClient,
		p_request_id: int,
		p_ret_bsatn_type: StringName = &"",
) -> SpacetimeDBReducerCall:
	var reducer_call: SpacetimeDBReducerCall = SpacetimeDBReducerCall.new()
	reducer_call._client = p_client
	reducer_call.request_id = p_request_id
	reducer_call._ret_bsatn_type = p_ret_bsatn_type
	return reducer_call


## Creates a pre-failed handle for an immediate client-side error.
static func fail(p_error: Error) -> SpacetimeDBReducerCall:
	var reducer_call: SpacetimeDBReducerCall = SpacetimeDBReducerCall.new()
	reducer_call.error = p_error
	reducer_call.outcome = Outcome.ERROR
	reducer_call.error_message = error_string(p_error)
	return reducer_call


## Awaits the server response for up to [param timeout_sec] seconds, then returns this
## handle so the unambiguous outcome is available in one step:[br]
## [code]var call := await reducers.foo(args).wait_for_response()[/code][br]
## then inspect [member outcome] / [method is_ok] / [method is_error] / [method decode] /
## [member transaction_update] / [member error_message]. Unlike a bare [TransactionUpdateMessage]
## return, this distinguishes OK / OK_EMPTY / ERROR / INTERNAL_ERROR / TIMEOUT / DISCONNECTED.
func wait_for_response(timeout_sec: float = 10) -> SpacetimeDBReducerCall:
	if error:
		return self
	await _client.wait_for_reducer_response(request_id, timeout_sec)
	if outcome == Outcome.PENDING:
		outcome = Outcome.TIMEOUT
		error_message = "Timeout waiting for reducer response"
	return self


## Decodes [member ret_value] using the reducer's ok return type provided at call
## time.[br] Returns [code]null[/code] if the reducer returned no value (unit) or
## no return type was specified (e.g. a hand-written [method SpacetimeDBClient.call_reducer]).
func decode() -> Variant:
	if ret_value.is_empty() or _ret_bsatn_type.is_empty():
		return null
	var spb: StreamPeerBuffer = StreamPeerBuffer.new()
	spb.data_array = ret_value
	spb.big_endian = false
	spb.seek(0)
	return _client._deserializer._read_value_from_bsatn_type(spb, _ret_bsatn_type, &"reducer_return")


## Returns [code]true[/code] if the reducer succeeded ([constant Outcome.OK] or [constant Outcome.OK_EMPTY]).
func is_ok() -> bool:
	return outcome == Outcome.OK or outcome == Outcome.OK_EMPTY


## Returns [code]true[/code] if the reducer ended in any error state.
func is_error() -> bool:
	return outcome == Outcome.ERROR or outcome == Outcome.INTERNAL_ERROR or outcome == Outcome.DISCONNECTED


## Returns [code]true[/code] if the call has received a terminal outcome (no longer [constant Outcome.PENDING]).
func is_completed() -> bool:
	return outcome != Outcome.PENDING
