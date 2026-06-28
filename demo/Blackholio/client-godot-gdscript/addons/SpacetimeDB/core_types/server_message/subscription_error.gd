## Server message indicating that a subscription or query failed.
##
## Either or both of [member request_id] and [member query_id] may be absent
## (represented as [code]-1[/code] and [code]null[/code] respectively). Use
## [method has_request_id] and [method has_query_id] to check presence.
@tool
class_name SubscriptionErrorMessage
extends SpacetimeDBServerMessage

## The request id from the originating message, or [code]-1[/code] if absent.
var request_id: int
## The query set id related to the error, or [code]null[/code] if absent.
var query_id: QueryIdData
## Human-readable description of the error.
var error_message: String


func _init():
	request_id = -1
	query_id = null


## Returns [code]true[/code] if the server included a request id in this error.
func has_request_id() -> bool:
	return request_id != -1


## Returns [code]true[/code] if the server included a query id in this error.
func has_query_id() -> bool:
	return query_id != null
