## A set of table updates associated with a single subscription query.
##
## Received inside [SubscribeAppliedMessage], [UnsubscribeAppliedMessage],
## and [TransactionUpdateMessage]. Groups a [member query_id] with the
## [TableUpdateData] rows that changed for that query.
@tool
class_name DatabaseUpdateData
extends RefCounted

## The query that produced this batch of updates.
var query_id: QueryIdData
## Per-table insert/delete row lists included in this update.
var tables: Array[TableUpdateData]


func _init():
	query_id = QueryIdData.new()
