## Server message containing the outcome of a [CallReducerMessage].
##
## The [member reducer_result] holds a [ReducerOutcomeEnum] whose active variant
## indicates success (with or without changes) or failure.
@tool
class_name ReducerResultMessage
extends SpacetimeDBServerMessage

## BSATN type hints used by the SDK's binary deserializer.
const BSATN_TYPES: Dictionary[StringName, StringName] = { &"request_id": &"u32", &"timestamp": &"timestamp", &"reducer_result": &"ReducerOutcomeEnum" }

## Client-assigned id from the originating [CallReducerMessage].
@export var request_id: int
## Server-side timestamp (microseconds since Unix epoch) when the reducer ran.
@export var timestamp: int

## The reducer's outcome. See [ReducerOutcomeEnum] for variant details.
var reducer_result: ReducerOutcomeEnum
## Raw BSATN bytes of the reducer's return value (populated on OK outcomes).
var ret_value: PackedByteArray = PackedByteArray()
