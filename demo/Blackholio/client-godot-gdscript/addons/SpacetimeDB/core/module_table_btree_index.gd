## Base class for codegen'd btree (non-unique) index accessors.
##
## Each generated btree index (e.g. [code]BlackholioCirclePlayerIdBTreeIndex[/code])
## extends this and exposes a typed [code]filter(col_val) -> Array[Row][/code]
## returning every row whose indexed column equals the given value. Backed by a
## multimap cache (one bucket of rows per distinct column value) kept in sync with
## [LocalDatabase] via insert/update/delete listeners, so a lookup is O(1) on the
## value plus O(k) over the k matching rows, not a linear scan of the whole table.
class_name _ModuleTableBTreeIndex
extends Resource

## Normalized table name this index belongs to.
var _table_name: StringName
## The field name used as the (non-unique) index key.
var _field_name: StringName
## Ascending list of the distinct keys currently in the cache — a mirror of the
## multimap's keys kept sorted so range queries binary-search instead of scanning.
## Maintained only at bucket create/empty edges (distinct-key churn), not per row.
## Populated for every key type; only orderable types get a [code]filter_range[/code]
## accessor from codegen, since [method Array.bsearch] needs a defined [code]<[/code].
var _sorted_keys: Array = []
## Reference to the subclass-owned multimap, captured in [method _connect_cache_to_db]
## so base-class range queries can read it (the typed [code]_cache[/code] lives on the
## generated subclass).
var _cache_ref: Dictionary = { }


## Inserts [param k] into [member _sorted_keys] at its sorted position.
func _key_added(k: Variant) -> void:
	var i: int = _sorted_keys.bsearch(k, true)
	_sorted_keys.insert(i, k)


## Removes [param k] from [member _sorted_keys] if present.
func _key_removed(k: Variant) -> void:
	var i: int = _sorted_keys.bsearch(k, true)
	if i < _sorted_keys.size() and _sorted_keys[i] == k:
		_sorted_keys.remove_at(i)


## Gathers every cached row in the sorted-key window [code][lo, hi)[/code] (indices
## into [member _sorted_keys]). Shared by all of the range/bound queries below.
func _gather(lo: int, hi: int) -> Array:
	var out: Array = []
	for i: int in range(lo, hi):
		out.append_array(_cache_ref[_sorted_keys[i]])
	return out


## Every cached row whose key lies in [code][from_val, to_val][/code] inclusive.
## Returns an untyped [Array]; the codegen'd subclass assigns it into a typed
## [code]Array[Row][/code]. O(log d) to locate the window over d distinct keys,
## plus O(k) to gather the k matching rows. The one-sided variants below share the
## same cost profile.
func _range_rows(from_val: Variant, to_val: Variant) -> Array:
	# [first key >= from_val, first key > to_val)
	return _gather(_sorted_keys.bsearch(from_val, true), _sorted_keys.bsearch(to_val, false))


## Rows whose key is [code]>= v[/code].
func _gte_rows(v: Variant) -> Array:
	return _gather(_sorted_keys.bsearch(v, true), _sorted_keys.size())


## Rows whose key is [code]> v[/code].
func _gt_rows(v: Variant) -> Array:
	return _gather(_sorted_keys.bsearch(v, false), _sorted_keys.size())


## Rows whose key is [code]<= v[/code].
func _lte_rows(v: Variant) -> Array:
	return _gather(0, _sorted_keys.bsearch(v, false))


## Rows whose key is [code]< v[/code].
func _lt_rows(v: Variant) -> Array:
	return _gather(0, _sorted_keys.bsearch(v, true))


## First cached row whose key equals [param col_val], or null if the bucket is absent
## or empty. Backs the generated [code]first_by_<field>[/code] without the typed
## bucket-copy that [code]filter()[/code] builds — only row 0 is needed.
func _first_row(col_val: Variant) -> _ModuleTableType:
	var bucket: Variant = _cache_ref.get(col_val)
	if bucket == null or bucket.is_empty():
		return null
	return bucket[0]


## Wires [param cache] (a [code]Dictionary[value, Array[Row]][/code] multimap) to live
## insert/update/delete callbacks on [param db] so each per-value bucket stays current
## without manual polling. Mirrors [_ModuleTableUniqueIndex] but keeps a bucket of rows
## per key instead of a single row, and keeps [member _sorted_keys] aligned at the
## bucket create/empty edges. The callbacks read the multimap via [member _cache_ref]
## (set here), so they're named methods rather than capturing lambdas.
func _connect_cache_to_db(cache: Dictionary, db: LocalDatabase) -> void:
	_cache_ref = cache
	db.subscribe_to_inserts(_table_name, _on_insert)
	db.subscribe_to_updates(_table_name, _on_update)
	db.subscribe_to_deletes(_table_name, _on_delete)


## Insert listener — appends the row to its key's bucket, creating the bucket (and
## registering the key in [member _sorted_keys]) on first sight of the value.
func _on_insert(r: _ModuleTableType) -> void:
	var col_val: Variant = r[_field_name]
	if not _cache_ref.has(col_val):
		_cache_ref[col_val] = []
		_key_added(col_val)
	_cache_ref[col_val].append(r)


## Update listener — moves the row between buckets when its key changed, else swaps
## the stale instance in place. Empties/creates buckets at the key edges.
func _on_update(p: _ModuleTableType, r: _ModuleTableType) -> void:
	var previous_col_val: Variant = p[_field_name]
	var col_val: Variant = r[_field_name]

	if previous_col_val != col_val:
		if _cache_ref.has(previous_col_val):
			_cache_ref[previous_col_val].erase(p)
			if _cache_ref[previous_col_val].is_empty():
				_cache_ref.erase(previous_col_val)
				_key_removed(previous_col_val)
		if not _cache_ref.has(col_val):
			_cache_ref[col_val] = []
			_key_added(col_val)
		_cache_ref[col_val].append(r)
	elif _cache_ref.has(col_val):
		# Same key — swap the stale instance for the new one in place.
		var idx: int = _cache_ref[col_val].find(p)
		if idx != -1:
			_cache_ref[col_val][idx] = r
		else:
			_cache_ref[col_val].append(r)
	else:
		_cache_ref[col_val] = [r]
		_key_added(col_val)


## Delete listener — drops the row from its bucket, emptying the bucket (and
## unregistering the key) when it was the last row for that value.
func _on_delete(r: _ModuleTableType) -> void:
	var col_val: Variant = r[_field_name]
	if _cache_ref.has(col_val):
		_cache_ref[col_val].erase(r)
		if _cache_ref[col_val].is_empty():
			_cache_ref.erase(col_val)
			_key_removed(col_val)
