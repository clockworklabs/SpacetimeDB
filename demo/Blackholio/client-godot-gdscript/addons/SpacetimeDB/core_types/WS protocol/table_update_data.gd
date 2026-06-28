## Row-level changes for a single table within a [DatabaseUpdateData] batch.
##
## Maps the protocol's [code]TableUpdate[/code] structure into a flat
## inserts/deletes representation. The server sends either
## [code]PersistentTable(inserts, deletes)[/code] or [code]EventTable(events)[/code]
## the SDK's parser flattens both forms into [member inserts] and [member deletes].
## [member is_event] distinguishes the two so the cache can skip storing ephemeral rows.
@tool
class_name TableUpdateData
extends RefCounted

## The table's identifier as a [StringName] (snake_case form).
var table_name: StringName
## Rows removed from the table. Each element is a typed [code]_ModuleTableType[/code] [Resource].
var deletes: Array[Resource]
## Rows added to the table. Each element is a typed [code]_ModuleTableType[/code] [Resource].
var inserts: Array[Resource]
## True when these rows came from an [code]EventTable[/code] (ephemeral). Event-table
## rows fire [code]on_insert[/code] but are never stored in the local cache.
var is_event: bool = false
