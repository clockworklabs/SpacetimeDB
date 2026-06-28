## Base class for all server-to-client messages in the SpacetimeDB BSATN WS protocol.
##
## Each message type corresponds to a variant tag on the wire ([member Type]).
## The SDK's message parser reads the tag byte, then delegates to the matching
## subclass script resolved via [method get_script_path].
##
## Back-compat [code]const[/code] aliases (e.g. [code]SpacetimeDBServerMessage.SUBSCRIBE_APPLIED[/code])
## are provided so existing code does not need to migrate to the enum form.
class_name SpacetimeDBServerMessage
extends SpacetimeDBMessage

## Server message type tags (wire values must match protocol exactly).
enum Type {
	INITIAL_CONNECTION = 0x00,
	SUBSCRIBE_APPLIED = 0x01,
	UNSUBSCRIBE_APPLIED = 0x02,
	SUBSCRIPTION_ERROR = 0x03,
	TRANSACTION_UPDATE = 0x04,
	ONE_OFF_QUERY_RESPONSE = 0x05,
	REDUCER_RESULT = 0x06,
	PROCEDURE_RESULT = 0x07,
}

## Back-compat consts so existing code referencing SpacetimeDBServerMessage.SUBSCRIBE_APPLIED etc. still works.
const INITIAL_CONNECTION: int = Type.INITIAL_CONNECTION
const SUBSCRIBE_APPLIED: int = Type.SUBSCRIBE_APPLIED
const UNSUBSCRIBE_APPLIED: int = Type.UNSUBSCRIBE_APPLIED
const SUBSCRIPTION_ERROR: int = Type.SUBSCRIPTION_ERROR
const TRANSACTION_UPDATE: int = Type.TRANSACTION_UPDATE
const ONE_OFF_QUERY_RESPONSE: int = Type.ONE_OFF_QUERY_RESPONSE
const REDUCER_RESULT: int = Type.REDUCER_RESULT
const PROCEDURE_RESULT: int = Type.PROCEDURE_RESULT

const _MSG_PATH: String = SpacetimePlugin.ADDON_PATH + "/core_types/server_message/"


## Returns the [code]res://[/code] path to the GDScript file for the given [param msg_type] tag.
## Returns an empty string if the tag is unknown.
static func get_script_path(msg_type: int) -> String:
	if msg_type == Type.INITIAL_CONNECTION:
		return _MSG_PATH + "initial_connection.gd"
	elif msg_type == Type.SUBSCRIBE_APPLIED:
		return _MSG_PATH + "subscribe_applied.gd"
	elif msg_type == Type.UNSUBSCRIBE_APPLIED:
		return _MSG_PATH + "unsubscribe_applied.gd"
	elif msg_type == Type.SUBSCRIPTION_ERROR:
		return _MSG_PATH + "subscription_error.gd"
	elif msg_type == Type.TRANSACTION_UPDATE:
		return _MSG_PATH + "transaction_update.gd"
	elif msg_type == Type.ONE_OFF_QUERY_RESPONSE:
		return _MSG_PATH + "one_off_query_response.gd"
	elif msg_type == Type.REDUCER_RESULT:
		return _MSG_PATH + "reducer_result.gd"
	elif msg_type == Type.PROCEDURE_RESULT:
		return _MSG_PATH + "procedure_result.gd"
	else:
		return ""
