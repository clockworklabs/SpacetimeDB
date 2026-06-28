## Server message delivering database changes triggered by a reducer or subscription update.
##
## Contains one [DatabaseUpdateData] per affected query set, each holding the
## per-table inserts and deletes that the SDK applies to its local database.
@tool
class_name TransactionUpdateMessage
extends SpacetimeDBServerMessage

## Database changes grouped by query set.
var query_sets: Array[DatabaseUpdateData]
