## Rust-style enum representing the outcome of a reducer call in the BSATN WS protocol.
##
## Variants:[br]
## [b]ok[/b] — Reducer succeeded; payload is a [TransactionUpdateMessage].[br]
## [b]okEmpty[/b] — Reducer succeeded with no database changes.[br]
## [b]err[/b] — Reducer returned an error; payload is a [PackedByteArray].[br]
## [b]internalError[/b] — Server-side internal error; payload is a [String].[br]
##
## Use [method get_ok], [method get_err], or [method get_internal_error] to
## retrieve the typed payload for the active variant.
@tool
class_name ReducerOutcomeEnum
extends RustEnum

## Discriminant values for each reducer outcome variant.
enum Options {
	## Reducer succeeded with database changes.
	ok,
	## Reducer succeeded with no database changes.
	okEmpty,
	## Reducer returned an application-level error.
	err,
	## Server encountered an internal error while running the reducer.
	internalError,
}

## BSATN type names for each variant's payload, indexed by [enum Options].[br]
## [code]ReducerOk[/code] is parsed manually into a [TransactionUpdateMessage].
## Empty string means no payload. [code]vec_u8[/code] for raw bytes.
const ENUM_OPTIONS: Array[StringName] = [&'ReducerOk', &'', &'vec_u8', &'string']


## Returns the variant name for discriminant [param i], or [code]&"Unknown"[/code] if out of range.
static func parse_enum_name(i: int) -> StringName:
	if i == 0:
		return &'ok'
	elif i == 1:
		return &'okEmpty'
	elif i == 2:
		return &'err'
	elif i == 3:
		return &'internalError'
	else:
		printerr("Enum does not have value for %d. This is out of bounds." % i)
		return &'Unknown'


## Returns the TransactionUpdateMessage from the Ok variant.
func get_ok() -> TransactionUpdateMessage:
	return data


## Returns the raw error bytes from the [code]err[/code] variant.
func get_err() -> PackedByteArray:
	return data


## Returns the error string from the [code]internalError[/code] variant.
func get_internal_error() -> String:
	return data


## Factory method. Creates a [ReducerOutcomeEnum] with the given variant [param p_type] and optional [param p_data].
static func create(p_type: int, p_data: Variant = null) -> ReducerOutcomeEnum:
	var result: ReducerOutcomeEnum = ReducerOutcomeEnum.new()
	result.value = p_type
	result.data = p_data
	return result
