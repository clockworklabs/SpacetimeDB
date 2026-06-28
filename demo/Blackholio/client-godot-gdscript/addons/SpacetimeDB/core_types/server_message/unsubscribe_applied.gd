## Server message confirming that an unsubscription has been applied.
##
## Sent in response to an [UnsubscribeMessage]. If the unsubscribe used
## [constant UnsubscribeMessage.UnsubscribeFlags.SendDroppedRows], the
## [member tables] array contains the rows that were removed.
@tool
class_name UnsubscribeAppliedMessage
extends SpacetimeDBServerMessage

## The [member UnsubscribeMessage.request_id] echoed back by the server.
var request_id: int
## The query set id that was unsubscribed.
var query_id: QueryIdData
## Rows dropped from the local database (empty unless [code]SendDroppedRows[/code] was set).
var tables: Array[TableUpdateData]


func _init():
	query_id = QueryIdData.new()
