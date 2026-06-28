## High-level accessor for a single SpacetimeDB table.
##
## Generated table classes (e.g. [code]WorldPawnStatsTable[/code]) extend this.
## Delegates all reads and listener management to [LocalDatabase], providing a
## typed, table-scoped API for game code.
class_name _ModuleTable
extends RefCounted

var _db: LocalDatabase
var _table_name: StringName


func _init(db: LocalDatabase) -> void:
	_db = db


## Returns the total number of rows in this table.
func count() -> int:
	return _db.count_all_rows(_table_name)


## Returns all rows in this table as an untyped [Array].
func iter() -> Array:
	return _db.get_all_rows(_table_name)


## Registers [param listener] to be called with the new row whenever a row is inserted.
func on_insert(listener: Callable) -> void:
	_db.subscribe_to_inserts(_table_name, listener)


## Removes a previously registered insert [param listener].
func remove_on_insert(listener: Callable) -> void:
	_db.unsubscribe_from_inserts(_table_name, listener)


## Registers [param listener] to be called with [code](old_row, new_row)[/code] on updates.
func on_update(listener: Callable) -> void:
	_db.subscribe_to_updates(_table_name, listener)


## Removes a previously registered update [param listener].
func remove_on_update(listener: Callable) -> void:
	_db.unsubscribe_from_updates(_table_name, listener)


## Registers [param listener] to be called with a row just before it is deleted,
## while it is still queryable in the cache.
func on_before_delete(listener: Callable) -> void:
	_db.subscribe_to_before_deletes(_table_name, listener)


## Removes a previously registered before-delete [param listener].
func remove_on_before_delete(listener: Callable) -> void:
	_db.unsubscribe_from_before_deletes(_table_name, listener)


## Registers [param listener] to be called with the deleted row on deletes.
func on_delete(listener: Callable) -> void:
	_db.subscribe_to_deletes(_table_name, listener)


## Removes a previously registered delete [param listener].
func remove_on_delete(listener: Callable) -> void:
	_db.unsubscribe_from_deletes(_table_name, listener)


## Returns all rows matching [param predicate].
func find_where(predicate: Callable) -> Array:
	return _db.find_where(_table_name, predicate)


## Returns the first row matching [param predicate], or [code]null[/code].
func first_where(predicate: Callable) -> _ModuleTableType:
	return _db.first_where(_table_name, predicate)


## Returns all rows where [param field] equals [param value].
func find_by(field: StringName, value: Variant) -> Array:
	return _db.find_by(_table_name, field, value)


## Returns the first row where [param field] equals [param value], or [code]null[/code].
func first_by(field: StringName, value: Variant) -> _ModuleTableType:
	return _db.first_by(_table_name, field, value)


## Returns the count of rows matching [param predicate].
func count_where(predicate: Callable) -> int:
	return _db.count_where(_table_name, predicate)
