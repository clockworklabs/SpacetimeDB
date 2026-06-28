## Server message containing the result of a [OneOffQueryMessage].
##
## If [member is_error] is [code]false[/code], [member tables] contains the
## matching rows (inserts only). If [code]true[/code], [member error_message]
## describes what went wrong.
@tool
class_name OneOffQueryResponseMessage
extends SpacetimeDBServerMessage

## Client-assigned id from the originating [OneOffQueryMessage].
var request_id: int
## [code]true[/code] if the query failed.
var is_error: bool = false
## Human-readable error (valid only when [member is_error] is [code]true[/code]).
var error_message: String = ""
## Query result rows, one [TableUpdateData] per table (inserts only).
var tables: Array[TableUpdateData] = []
