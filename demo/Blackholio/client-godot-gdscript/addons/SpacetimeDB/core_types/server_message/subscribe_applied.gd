## Server message confirming that a subscription has been applied.
##
## Sent in response to a [SubscribeMessage]. Contains the initial snapshot of
## rows matching the subscribed queries, parsed into [TableUpdateData] entries
## that the SDK feeds into its local database.
@tool
class_name SubscribeAppliedMessage
extends SpacetimeDBServerMessage

## The [member SubscribeMessage.request_id] echoed back by the server.
var request_id: int
## Server-assigned query set id for this subscription group.
var query_set_id: QueryIdData
## Initial row snapshot, one [TableUpdateData] per affected table.
var tables: Array[TableUpdateData]


func _init():
	query_set_id = QueryIdData.new()
