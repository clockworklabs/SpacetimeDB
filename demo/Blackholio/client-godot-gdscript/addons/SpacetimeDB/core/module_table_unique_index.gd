## Base class for codegen'd unique index accessors.
##
## Each generated unique index (e.g. [code]WorldPawnStatsPawnIdUniqueIndex[/code])
## extends this and exposes a typed [code]find()[/code] method. Internally keeps
## a dictionary cache that stays in sync with [LocalDatabase] via insert/update/delete
## listeners.
class_name _ModuleTableUniqueIndex
extends Resource

## Normalized table name this index belongs to.
var _table_name: StringName
## The field name used as the unique key.
var _field_name: StringName
## Reference to the subclass-owned cache, captured in [method _connect_cache_to_db]
## so the named listeners below can read it (the typed cache lives on the subclass).
var _cache_ref: Dictionary = { }


## Wires [param cache] to live insert/update/delete callbacks on [param db]
## so the dictionary stays current without manual polling. The callbacks read the
## cache via [member _cache_ref] (set here), so they're named methods rather than
## capturing lambdas.
func _connect_cache_to_db(cache: Dictionary, db: LocalDatabase) -> void:
	_cache_ref = cache
	db.subscribe_to_inserts(_table_name, _on_insert)
	db.subscribe_to_updates(_table_name, _on_update)
	db.subscribe_to_deletes(_table_name, _on_delete)


## Insert listener — maps the row's unique key to the row.
func _on_insert(r: _ModuleTableType) -> void:
	var col_val: Variant = r[_field_name]
	_cache_ref[col_val] = r


## Update listener — moves the row to its new key when the key changed.
func _on_update(p: _ModuleTableType, r: _ModuleTableType) -> void:
	var previous_col_val: Variant = p[_field_name]
	var col_val: Variant = r[_field_name]
	if previous_col_val != col_val:
		_cache_ref.erase(previous_col_val)
	_cache_ref[col_val] = r


## Delete listener — drops the row's key from the cache.
func _on_delete(r: _ModuleTableType) -> void:
	var col_val: Variant = r[_field_name]
	_cache_ref.erase(col_val)
