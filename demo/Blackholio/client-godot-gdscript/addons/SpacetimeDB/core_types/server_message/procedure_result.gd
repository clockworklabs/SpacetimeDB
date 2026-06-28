## Server message containing the result of a [CallProcedureMessage].
##
## If [member status_tag] is [code]0[/code] (Returned), [member return_bytes]
## holds the BSATN-encoded return value. If [code]1[/code] (InternalError),
## [member error_message] describes what went wrong.
class_name ProcedureResultData
extends SpacetimeDBServerMessage

## Client-assigned id from the originating [CallProcedureMessage].
var request_id: int
## Server-side timestamp (microseconds since Unix epoch) when the procedure ran.
var timestamp: int
## Execution duration in microseconds.
var duration: int
## Status discriminant: [code]0[/code] = Returned, [code]1[/code] = InternalError.
var status_tag: int
## BSATN-encoded return value (valid only when [member status_tag] is [code]0[/code]).
var return_bytes: PackedByteArray
## Human-readable error (valid only when [member status_tag] is [code]1[/code]).
var error_message: String
