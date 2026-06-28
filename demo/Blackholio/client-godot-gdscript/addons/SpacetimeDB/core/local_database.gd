## Client-side in-memory mirror of SpacetimeDB tables.
##
## Stores rows keyed by primary key (or in flat arrays for PK-less tables).
## Processes [TableUpdateData] batches from the server, resolves inserts vs
## updates via PK matching, and dispatches per-table listener callbacks and
## signals. Game code normally interacts via [_ModuleTable] wrappers rather
## than calling [LocalDatabase] directly.
class_name LocalDatabase
extends Node

var _tables: Dictionary[StringName, Dictionary] = { }
var _primary_key_cache: Dictionary[StringName, StringName] = { }
var _schema: SpacetimeDBSchema
var _cached_normalized_table_names: Dictionary[StringName, StringName] = { }
var _insert_listeners_by_table: Dictionary[StringName, Array] = { } ## Array[Callable]
var _update_listeners_by_table: Dictionary[StringName, Array] = { } ## Array[Callable]
var _before_delete_listeners_by_table: Dictionary[StringName, Array] = { } ## Array[Callable]
var _delete_listeners_by_table: Dictionary[StringName, Array] = { } ## Array[Callable]
var _transactions_completed_listeners_by_table: Dictionary[StringName, Array] = { } ## Array[Callable]
## Shared read-only sentinel returned by [method _listener_snapshot] when a table
## has no listeners — avoids allocating an empty Array per snapshot on the common
## no-listener path. Read-only so a stray mutation fails loud (C2a).
static var _EMPTY_LISTENERS: Array = []
var _pk_less_tables: Dictionary[StringName, Array] = { } ## Array[_ModuleTableType]
var _row_property_cache: Dictionary[StringName, Array] = { } ## Array[StringName] — storage props per table
## Per-table refcount of cached PK rows: table -> { pk -> int }. A row shared by N
## overlapping query sets has count N; on_insert fires on 0->positive, on_delete on
## positive->0. Lets an unsubscribe drop only rows no longer held by another query.
var _ref_counts: Dictionary[StringName, Dictionary] = { }
## PK-less analogue of _ref_counts. Rows have no key, so they're refcounted by value:
## table -> { row_hash -> Array of [row, count] } (hash bucket + _rows_equal tiebreak).
## A distinct row value held by N overlapping subscriptions has count N; on_insert fires
## on 0->1, on_delete on 1->0. Mirrors the per-row entries in _pk_less_tables.
var _pk_less_counts: Dictionary[StringName, Dictionary] = { }
## Per-query row membership: query_id -> { table -> (PK: { pk -> row }) | (PK-less: { hash -> [[row, count]] }) }.
## Records which rows each subscription contributes so a SubscriptionError on an already-
## applied query can be pruned precisely (decrement those rows' refcounts, evict any that
## no other query holds) — the server sends no dropped rows on an error, unlike unsubscribe.
var _query_rows: Dictionary[int, Dictionary] = { }

## Emitted after a row is inserted into a table.
signal row_inserted(table_name: StringName, row: _ModuleTableType)
## Emitted after a row is updated (PK match found in inserts + existing data).
signal row_updated(table_name: StringName, old_row: _ModuleTableType, new_row: _ModuleTableType)
## Emitted just before a row is removed from the cache (row still queryable).
signal row_before_delete(table_name: StringName, row: _ModuleTableType)
## Emitted after a row is deleted from a table.
signal row_deleted(table_name: StringName, row: _ModuleTableType)
## Emitted once after all inserts/deletes in a single [TableUpdateData] are processed.
signal row_transactions_completed(table_name: StringName)


static func _static_init() -> void:
	if not _EMPTY_LISTENERS.is_read_only():
		_EMPTY_LISTENERS.make_read_only()


func _init(p_schema: SpacetimeDBSchema) -> void:
	_schema = p_schema
	for raw_name: StringName in p_schema.raw_table_names:
		_tables[raw_name.to_lower()] = { }
	p_schema.raw_table_names.clear() # consumed — free the memory


## Snapshot a table's listener list for safe iteration during dispatch. A listener
## may unsubscribe inside its own callback, so the list it mutates must not be the
## one being iterated — hence duplicate. Duplicate only when non-empty; the common
## no-listener case returns the shared read-only empty (zero alloc).
func _listener_snapshot(by_table: Dictionary, key: StringName) -> Array:
	var live: Array = by_table.get(key, _EMPTY_LISTENERS)
	return live.duplicate() if not live.is_empty() else _EMPTY_LISTENERS


# --- Normalization helper (#2) ---
# Single shared cache for both apply_table_update and access methods
func _normalize(table_name: StringName) -> StringName:
	if _cached_normalized_table_names.has(table_name):
		return _cached_normalized_table_names[table_name]
	var normalized: StringName = table_name.to_lower()
	_cached_normalized_table_names[table_name] = normalized
	return normalized


## Registers [param callable] to be called with the inserted row for [param table_name].
func subscribe_to_inserts(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if not _insert_listeners_by_table.has(key):
		_insert_listeners_by_table[key] = []
	if not _insert_listeners_by_table[key].has(callable):
		_insert_listeners_by_table[key].append(callable)


## Removes an insert listener for [param table_name].
func unsubscribe_from_inserts(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if _insert_listeners_by_table.has(key):
		_insert_listeners_by_table[key].erase(callable)
		if _insert_listeners_by_table[key].is_empty():
			_insert_listeners_by_table.erase(key)


## Registers [param callable] to be called with [code](old_row, new_row)[/code] for [param table_name].
func subscribe_to_updates(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if not _update_listeners_by_table.has(key):
		_update_listeners_by_table[key] = []
	if not _update_listeners_by_table[key].has(callable):
		_update_listeners_by_table[key].append(callable)


## Removes an update listener for [param table_name].
func unsubscribe_from_updates(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if _update_listeners_by_table.has(key):
		_update_listeners_by_table[key].erase(callable)
		if _update_listeners_by_table[key].is_empty():
			_update_listeners_by_table.erase(key)


## Registers [param callable] to be called with the row about to be deleted for
## [param table_name]. Fires before the row leaves the cache, so the callback can
## still read it (and related rows) at their pre-delete state.
func subscribe_to_before_deletes(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if not _before_delete_listeners_by_table.has(key):
		_before_delete_listeners_by_table[key] = []
	if not _before_delete_listeners_by_table[key].has(callable):
		_before_delete_listeners_by_table[key].append(callable)


## Removes a before-delete listener for [param table_name].
func unsubscribe_from_before_deletes(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if _before_delete_listeners_by_table.has(key):
		_before_delete_listeners_by_table[key].erase(callable)
		if _before_delete_listeners_by_table[key].is_empty():
			_before_delete_listeners_by_table.erase(key)


## Registers [param callable] to be called with the deleted row for [param table_name].
func subscribe_to_deletes(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if not _delete_listeners_by_table.has(key):
		_delete_listeners_by_table[key] = []
	if not _delete_listeners_by_table[key].has(callable):
		_delete_listeners_by_table[key].append(callable)


## Removes a delete listener for [param table_name].
func unsubscribe_from_deletes(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if _delete_listeners_by_table.has(key):
		_delete_listeners_by_table[key].erase(callable)
		if _delete_listeners_by_table[key].is_empty():
			_delete_listeners_by_table.erase(key)


## Registers [param callable] to be called (no args) after all changes in a batch for [param table_name].
func subscribe_to_transactions_completed(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if not _transactions_completed_listeners_by_table.has(key):
		_transactions_completed_listeners_by_table[key] = []
	if not _transactions_completed_listeners_by_table[key].has(callable):
		_transactions_completed_listeners_by_table[key].append(callable)


## Removes a transactions-completed listener for [param table_name].
func unsubscribe_from_transactions_completed(table_name: StringName, callable: Callable) -> void:
	var key: StringName = _normalize(table_name)
	if _transactions_completed_listeners_by_table.has(key):
		_transactions_completed_listeners_by_table[key].erase(callable)
		if _transactions_completed_listeners_by_table[key].is_empty():
			_transactions_completed_listeners_by_table.erase(key)


# --- Primary Key Handling (#5) ---
# _primary_key_cache now serves both roles — _cached_pk_fields removed
func _get_primary_key_field(table_name_lower: StringName) -> StringName:
	if _primary_key_cache.has(table_name_lower):
		return _primary_key_cache[table_name_lower]

	# schema.types is still keyed with underscore-stripped names for Rust/filename compat
	var schema_key: StringName = table_name_lower.replace("_", "")
	if not _schema.types.has(schema_key):
		printerr("LocalDatabase: No schema found for table '", table_name_lower, "' to determine PK.")
		return &""

	var schema: GDScript = _schema.get_type(schema_key)
	var constants: Dictionary = schema.get_script_constant_map()
	if constants.has(&"PRIMARY_KEY"):
		var pk_field: StringName = constants[&"PRIMARY_KEY"]
		_primary_key_cache[table_name_lower] = pk_field
		return pk_field

	var properties: Array = schema.get_script_property_list()
	for prop: Dictionary in properties:
		if (prop.usage & PROPERTY_USAGE_STORAGE):
			if prop.name == &"identity" or prop.name == &"id":
				_primary_key_cache[table_name_lower] = prop.name
				return prop.name

	_primary_key_cache[table_name_lower] = &""
	return &""


# --- PK-less Row Helpers ---
func _get_row_properties(table_name_lower: StringName) -> Array[StringName]:
	if _row_property_cache.has(table_name_lower):
		return _row_property_cache[table_name_lower]
	var schema_key: StringName = table_name_lower.replace("_", "")
	if not _schema.types.has(schema_key):
		return []
	var schema: GDScript = _schema.get_type(schema_key)
	var props: Array[StringName] = []
	for prop: Dictionary in schema.get_script_property_list():
		if prop.usage & PROPERTY_USAGE_STORAGE:
			props.append(prop.name)
	_row_property_cache[table_name_lower] = props
	return props


func _rows_equal(a: _ModuleTableType, b: _ModuleTableType, props: Array[StringName]) -> bool:
	for prop_name: StringName in props:
		if a.get(prop_name) != b.get(prop_name):
			return false
	return true


func _row_hash(row: _ModuleTableType, props: Array[StringName]) -> int:
	var h: int = 0
	for prop_name: StringName in props:
		h = h * 31 + hash(row.get(prop_name))
	return h


# --- PK-less refcount helpers (counts: { hash -> Array of [row, count] }) ---
# Finds the [row, count] entry for a value, or returns an empty Array if absent
# (a real entry is always [row, count], size 2 — so .is_empty() means "not found").
func _pk_less_find(counts: Dictionary, h: int, row: _ModuleTableType, props: Array[StringName]) -> Array:
	if not counts.has(h):
		return []
	for entry: Array in counts[h]:
		if _rows_equal(entry[0], row, props):
			return entry
	return []


func _pk_less_add(counts: Dictionary, h: int, row: _ModuleTableType) -> void:
	if not counts.has(h):
		counts[h] = []
	counts[h].append([row, 1])


func _pk_less_remove(counts: Dictionary, h: int, entry: Array) -> void:
	if not counts.has(h):
		return
	counts[h].erase(entry)
	if counts[h].is_empty():
		counts.erase(h)


# --- Per-query membership (for prune_query) ---
func _query_table_pk_mem(query_id: int, table: StringName) -> Dictionary:
	if not _query_rows.has(query_id):
		_query_rows[query_id] = { }
	var tables: Dictionary = _query_rows[query_id]
	if not tables.has(table):
		tables[table] = { } # pk -> row
	return tables[table]


func _query_table_pkless_mem(query_id: int, table: StringName) -> Dictionary:
	if not _query_rows.has(query_id):
		_query_rows[query_id] = { }
	var tables: Dictionary = _query_rows[query_id]
	if not tables.has(table):
		tables[table] = { } # hash -> [[row, count]] (same shape as _pk_less_counts)
	return tables[table]


## Drops every row contributed by [param query_id] from the cache. Used on a
## SubscriptionError for an already-applied subscription (the server sends no dropped
## rows on an error): decrements each row's refcount via the normal delete path and
## evicts only rows no other subscription holds — the same effect as an unsubscribe,
## reconstructed from locally-tracked per-query membership.
func prune_query(query_id: int) -> void:
	if not _query_rows.has(query_id):
		return
	var tables: Dictionary = _query_rows[query_id]
	# Direct key iteration (no .keys() alloc). apply_table_update below mutates the inner
	# membership containers but never adds/removes a table key here, so this is safe.
	for table_name_lower: StringName in tables:
		var membership: Dictionary = tables[table_name_lower]
		var drop: TableUpdateData = TableUpdateData.new()
		drop.table_name = table_name_lower
		if _get_primary_key_field(table_name_lower).is_empty():
			# PK-less membership { hash -> [[row, count]] }: emit `count` deletes per value.
			for h: int in membership:
				for entry: Array in membership[h]:
					for _i: int in entry[1]:
						drop.deletes.append(entry[0])
		else:
			# PK membership { pk -> row }.
			drop.deletes.assign(membership.values())
		if not drop.deletes.is_empty():
			apply_table_update(drop, query_id)
	_query_rows.erase(query_id)


## Drops the per-query membership index for [param query_id] without touching the cache
## (the rows were already removed via the normal delete path, e.g. an unsubscribe whose
## dropped rows the server echoed). Prevents the index from growing unbounded.
func forget_query(query_id: int) -> void:
	_query_rows.erase(query_id)


## Applies all table updates from a [SubscribeAppliedMessage] to the local store.
func apply_database_subscription_applied(db_update: SubscribeAppliedMessage) -> void:
	if not db_update:
		return
	for table_update: TableUpdateData in db_update.tables:
		apply_table_update(table_update, db_update.query_set_id.id)


## Applies all table updates from a [DatabaseUpdateData] to the local store.
func apply_database_update(db_update: DatabaseUpdateData) -> void:
	if not db_update:
		return
	for table_update: TableUpdateData in db_update.tables:
		apply_table_update(table_update, db_update.query_id.id)


## Applies a single [TableUpdateData] — processes inserts then deletes, dispatches
## listener callbacks and signals, and handles both PK-keyed and PK-less tables.
## [param query_id] (>= 0) records which subscription contributed each row, so a
## [method prune_query] can later drop exactly that query's rows on a SubscriptionError.
func apply_table_update(table_update: TableUpdateData, query_id: int = -1) -> void:
	var table_name_lower: StringName = _normalize(table_update.table_name)

	if not _tables.has(table_name_lower):
		printerr("LocalDatabase: Received update for unknown table '", table_update.table_name, "' (normalized: '", table_name_lower, "')")
		return

	var pk_field: StringName = _get_primary_key_field(table_name_lower)

	# Hoist listener array lookups once per table_update, not per row. Snapshot
	# guards against a listener unsubscribing mid-dispatch; the no-listener case
	# allocs nothing (shared read-only empty).
	var insert_listeners: Array = _listener_snapshot(_insert_listeners_by_table, table_name_lower)
	var update_listeners: Array = _listener_snapshot(_update_listeners_by_table, table_name_lower)
	var before_delete_listeners: Array = _listener_snapshot(_before_delete_listeners_by_table, table_name_lower)
	var delete_listeners: Array = _listener_snapshot(_delete_listeners_by_table, table_name_lower)
	var tx_listeners: Array = _listener_snapshot(_transactions_completed_listeners_by_table, table_name_lower)
	var has_insert_listeners: bool = not insert_listeners.is_empty()
	var has_update_listeners: bool = not update_listeners.is_empty()
	var has_before_delete_listeners: bool = not before_delete_listeners.is_empty()
	var has_delete_listeners: bool = not delete_listeners.is_empty()

	# Event tables carry ephemeral rows: fire on_insert, never store. count()/iter()
	# stay empty and there is no update/delete/refcount tracking. The server only
	# sends these as EventTable row lists, which the deserializer flattens into
	# inserts with is_event set.
	if table_update.is_event:
		var fired_event: bool = false
		for event_row: _ModuleTableType in table_update.inserts:
			fired_event = true
			if has_insert_listeners:
				for listener: Callable in insert_listeners:
					listener.call(event_row)
			row_inserted.emit(table_name_lower, event_row)
		if fired_event:
			for listener: Callable in tx_listeners:
				listener.call()
			row_transactions_completed.emit(table_name_lower)
		return

	var table_dict: Dictionary = _tables[table_name_lower]
	var had_any_change: bool = false

	if pk_field.is_empty():
		# PK-less table: refcounted by row value (rows have no key). A distinct value held
		# by N overlapping subscriptions has count N; on_insert fires only on 0->1 and
		# on_delete only on 1->0, so a shared row survives one subscription's unsubscribe.
		# _pk_less_tables holds each present row once (for iteration/queries); _pk_less_counts
		# holds the multiplicity, keyed by row hash with an _rows_equal tiebreak.
		if not _pk_less_tables.has(table_name_lower):
			_pk_less_tables[table_name_lower] = []
		if not _pk_less_counts.has(table_name_lower):
			_pk_less_counts[table_name_lower] = { }
		var rows_array: Array = _pk_less_tables[table_name_lower]
		var counts: Dictionary = _pk_less_counts[table_name_lower]
		var props: Array[StringName] = _get_row_properties(table_name_lower)
		# Per-query membership for pk-less is itself a hash-bucket count map (same shape as
		# _pk_less_counts), hoisted out of the row loops. O(bucket) add/remove instead of an
		# O(n) array scan per delete.
		var track_pkless_query: bool = query_id >= 0
		var pkless_qmem: Dictionary = _query_table_pkless_mem(query_id, table_name_lower) if track_pkless_query else { }

		for inserted_row: _ModuleTableType in table_update.inserts:
			var ins_hash: int = _row_hash(inserted_row, props)
			var ins_entry: Array = _pk_less_find(counts, ins_hash, inserted_row, props)
			if ins_entry.is_empty():
				# Globally new value (0->1). It's therefore also new to this query's
				# membership, so add directly — no membership find needed.
				_pk_less_add(counts, ins_hash, inserted_row)
				rows_array.append(inserted_row)
				had_any_change = true
				if has_insert_listeners:
					for listener: Callable in insert_listeners:
						listener.call(inserted_row)
				row_inserted.emit(table_name_lower, inserted_row)
				if track_pkless_query:
					_pk_less_add(pkless_qmem, ins_hash, inserted_row)
			else:
				ins_entry[1] += 1 # already present (overlap / multiplicity) — bump silently
				# Already present globally; this query may or may not hold it yet (overlap).
				if track_pkless_query:
					var mem_ins: Array = _pk_less_find(pkless_qmem, ins_hash, inserted_row, props)
					if mem_ins.is_empty():
						_pk_less_add(pkless_qmem, ins_hash, inserted_row)
					else:
						mem_ins[1] += 1

		if not table_update.deletes.is_empty():
			var evicted: Dictionary = { } # instance_id -> true, for a single-pass array compact
			for deleted_row: _ModuleTableType in table_update.deletes:
				var del_hash: int = _row_hash(deleted_row, props)
				var del_entry: Array = _pk_less_find(counts, del_hash, deleted_row, props)
				if del_entry.is_empty() or del_entry[1] <= 0:
					continue
				del_entry[1] -= 1
				if track_pkless_query:
					var mem_del: Array = _pk_less_find(pkless_qmem, del_hash, deleted_row, props)
					if not mem_del.is_empty():
						mem_del[1] -= 1
						if mem_del[1] == 0:
							_pk_less_remove(pkless_qmem, del_hash, mem_del)
				if del_entry[1] == 0:
					var cached_row: _ModuleTableType = del_entry[0]
					_pk_less_remove(counts, del_hash, del_entry)
					evicted[cached_row.get_instance_id()] = true
					had_any_change = true
					if has_before_delete_listeners:
						for listener: Callable in before_delete_listeners:
							listener.call(cached_row)
					row_before_delete.emit(table_name_lower, cached_row)
					if has_delete_listeners:
						for listener: Callable in delete_listeners:
							listener.call(cached_row)
					row_deleted.emit(table_name_lower, cached_row)
			if not evicted.is_empty():
				# Single pass compact — the stored row is the same instance appended on 0->1.
				var write_idx: int = 0
				for read_idx: int in rows_array.size():
					var row: _ModuleTableType = rows_array[read_idx]
					if evicted.has(row.get_instance_id()):
						continue
					rows_array[write_idx] = row
					write_idx += 1
				rows_array.resize(write_idx)

		if had_any_change:
			for listener: Callable in tx_listeners:
				listener.call()
			row_transactions_completed.emit(table_name_lower)
		return

	# PK table: refcounted single pass. Within one update the server sends each pk at
	# most once per list, so no per-pk accumulation is needed. on_insert fires on
	# refcount 0->1, on_delete on 1->0; a delete+insert of the same pk in one update is
	# an update (net refcount 0, value may change). A row delivered by N overlapping
	# query sets has refcount N; an identical re-delivery bumps it silently. When
	# query_id >= 0, membership records this query's pks for precise SubscriptionError
	# pruning (dict lookup hoisted out of the row loops).
	if not _ref_counts.has(table_name_lower):
		_ref_counts[table_name_lower] = { }
	var ref_table: Dictionary = _ref_counts[table_name_lower]
	var props: Array[StringName] = _get_row_properties(table_name_lower)
	var track_query: bool = query_id >= 0
	var qmem: Dictionary = _query_table_pk_mem(query_id, table_name_lower) if track_query else { }

	# Update detection (delete+insert of the same pk) only matters when this update has
	# BOTH inserts and deletes. Pure inserts (subscribe) and pure deletes (rows leaving)
	# skip the pk-set build entirely. Null PKs are warned in the delete pass below.
	var detect_updates: bool = not table_update.inserts.is_empty() and not table_update.deletes.is_empty()
	var deleted_pks: Dictionary = { }
	if detect_updates:
		for deleted_row: _ModuleTableType in table_update.deletes:
			var del_pk: Variant = deleted_row.get(pk_field)
			if del_pk != null:
				deleted_pks[del_pk] = true

	for inserted_row: _ModuleTableType in table_update.inserts:
		var pk_value: Variant = inserted_row.get(pk_field)
		if pk_value == null:
			push_error("LocalDatabase: Inserted row for table '%s' has null PK '%s'. Skipping." % [table_name_lower, pk_field])
			continue
		if track_query:
			qmem[pk_value] = inserted_row
		var old_ref: int = ref_table.get(pk_value, 0)
		if detect_updates and deleted_pks.has(pk_value):
			# Update: delete+insert of the same pk. Refcount unchanged; mark handled so
			# the delete pass skips it. Fire on_update only when the value differs.
			deleted_pks.erase(pk_value)
			var prev_u: _ModuleTableType = table_dict.get(pk_value)
			if prev_u == null:
				# No prior cached row → this is an insert, not an update. Firing the
				# update path here would hand listeners a null `prev` (the index
				# listeners dereference it and crash). Refcount stays as the branch
				# intends (the matching delete pass is skipped via deleted_pks).
				table_dict[pk_value] = inserted_row
				had_any_change = true
				if has_insert_listeners:
					for listener: Callable in insert_listeners:
						listener.call(inserted_row)
				row_inserted.emit(table_name_lower, inserted_row)
			elif props.is_empty() or not _rows_equal(prev_u, inserted_row, props):
				table_dict[pk_value] = inserted_row
				had_any_change = true
				if has_update_listeners:
					for listener: Callable in update_listeners:
						listener.call(prev_u, inserted_row)
				row_updated.emit(table_name_lower, prev_u, inserted_row)
		elif old_ref == 0:
			ref_table[pk_value] = 1
			table_dict[pk_value] = inserted_row
			had_any_change = true
			if has_insert_listeners:
				for listener: Callable in insert_listeners:
					listener.call(inserted_row)
			row_inserted.emit(table_name_lower, inserted_row)
		else:
			# Overlapping re-delivery: bump refcount; on_update only if the value differs.
			ref_table[pk_value] = old_ref + 1
			var prev_o: _ModuleTableType = table_dict.get(pk_value)
			if prev_o == null:
				# Refcount bumped above but no cached row (desync / first sight under
				# an existing ref) → insert semantics, not update. Avoids a null `prev`
				# reaching listeners (the index listeners would crash on it).
				table_dict[pk_value] = inserted_row
				had_any_change = true
				if has_insert_listeners:
					for listener: Callable in insert_listeners:
						listener.call(inserted_row)
				row_inserted.emit(table_name_lower, inserted_row)
			elif props.is_empty() or not _rows_equal(prev_o, inserted_row, props):
				table_dict[pk_value] = inserted_row
				had_any_change = true
				if has_update_listeners:
					for listener: Callable in update_listeners:
						listener.call(prev_o, inserted_row)
				row_updated.emit(table_name_lower, prev_o, inserted_row)

	# Delete pass: skip entirely when there are no deletes, or (when detecting updates)
	# when every delete was consumed as an update above.
	if not table_update.deletes.is_empty() and not (detect_updates and deleted_pks.is_empty()):
		for deleted_row2: _ModuleTableType in table_update.deletes:
			var pk_value: Variant = deleted_row2.get(pk_field)
			if pk_value == null:
				push_warning("LocalDatabase: Deleted row for table '%s' has null PK '%s'. Skipping." % [table_name_lower, pk_field])
				continue
			if detect_updates and not deleted_pks.has(pk_value):
				continue # consumed as an update above
			var old_ref: int = ref_table.get(pk_value, 0)
			if old_ref <= 0:
				continue
			if track_query:
				qmem.erase(pk_value)
			if old_ref > 1:
				ref_table[pk_value] = old_ref - 1
				continue
			ref_table.erase(pk_value)
			var cached_row: _ModuleTableType = table_dict.get(pk_value)
			if cached_row != null:
				had_any_change = true
				if has_before_delete_listeners:
					for listener: Callable in before_delete_listeners:
						listener.call(cached_row)
				row_before_delete.emit(table_name_lower, cached_row)
				table_dict.erase(pk_value)
				if has_delete_listeners:
					for listener: Callable in delete_listeners:
						listener.call(cached_row)
				row_deleted.emit(table_name_lower, cached_row)

	if had_any_change:
		for listener: Callable in tx_listeners:
			listener.call()
		row_transactions_completed.emit(table_name_lower)


## Wipes every cached row from all tables, emitting a delete callback per row and a
## transactions-completed callback per non-empty table. Used to reset the mirror
## (e.g. before a fresh subscription after reconnecting).
func clear_local_db() -> void:
	for table_name_lower: StringName in _tables:
		_emit_clear_for_table(table_name_lower, _tables[table_name_lower].values())
		_tables[table_name_lower].clear()
	for table_name_lower: StringName in _pk_less_tables:
		_emit_clear_for_table(table_name_lower, _pk_less_tables[table_name_lower])
		_pk_less_tables[table_name_lower].clear()
	_ref_counts.clear()
	_pk_less_counts.clear()
	_query_rows.clear()


## Emits delete + transactions-completed callbacks for every row in [param rows].
func _emit_clear_for_table(table_name_lower: StringName, rows: Array) -> void:
	if rows.is_empty():
		return
	var before_delete_listeners: Array = _listener_snapshot(_before_delete_listeners_by_table, table_name_lower)
	var delete_listeners: Array = _listener_snapshot(_delete_listeners_by_table, table_name_lower)
	var tx_listeners: Array = _listener_snapshot(_transactions_completed_listeners_by_table, table_name_lower)
	for row: _ModuleTableType in rows:
		for listener: Callable in before_delete_listeners:
			listener.call(row)
		row_before_delete.emit(table_name_lower, row)
		for listener: Callable in delete_listeners:
			listener.call(row)
		row_deleted.emit(table_name_lower, row)
	for listener: Callable in tx_listeners:
		listener.call()
	row_transactions_completed.emit(table_name_lower)


## Returns a single row by its primary key [param primary_key_value], or [code]null[/code].
func get_row_by_pk(table_name: StringName, primary_key_value: Variant) -> _ModuleTableType:
	var key: StringName = _normalize(table_name)
	if not _tables.has(key):
		return null
	return _tables[key].get(primary_key_value, null)


## Returns all rows in [param table_name] as a typed array.
func get_all_rows(table_name: StringName) -> Array[_ModuleTableType]:
	var key: StringName = _normalize(table_name)
	if _pk_less_tables.has(key):
		var result: Array[_ModuleTableType] = []
		result.assign(_pk_less_tables[key])
		return result
	if not _tables.has(key):
		return []
	var pk_result: Array[_ModuleTableType] = []
	pk_result.assign(_tables[key].values())
	return pk_result


## Returns the number of rows in [param table_name].
func count_all_rows(table_name: StringName) -> int:
	var key: StringName = _normalize(table_name)
	if _pk_less_tables.has(key):
		return _pk_less_tables[key].size()
	if not _tables.has(key):
		return 0
	return _tables[key].size()


## Returns all rows in [param table_name] for which [param predicate] returns [code]true[/code].
func find_where(table_name: StringName, predicate: Callable) -> Array[_ModuleTableType]:
	var key: StringName = _normalize(table_name)
	var result: Array[_ModuleTableType] = []
	if _pk_less_tables.has(key):
		for row: _ModuleTableType in _pk_less_tables[key]:
			if predicate.call(row):
				result.append(row)
	elif _tables.has(key):
		var t: Dictionary = _tables[key]
		for pk: Variant in t:
			var row: _ModuleTableType = t[pk]
			if predicate.call(row):
				result.append(row)
	return result


## Returns the first row matching [param predicate], or [code]null[/code].
func first_where(table_name: StringName, predicate: Callable) -> _ModuleTableType:
	var key: StringName = _normalize(table_name)
	if _pk_less_tables.has(key):
		for row: _ModuleTableType in _pk_less_tables[key]:
			if predicate.call(row):
				return row
	elif _tables.has(key):
		var t: Dictionary = _tables[key]
		for pk: Variant in t:
			var row: _ModuleTableType = t[pk]
			if predicate.call(row):
				return row
	return null


## Returns all rows where [param field] equals [param value].
func find_by(table_name: StringName, field: StringName, value: Variant) -> Array[_ModuleTableType]:
	var key: StringName = _normalize(table_name)
	var result: Array[_ModuleTableType] = []
	if _pk_less_tables.has(key):
		for row: _ModuleTableType in _pk_less_tables[key]:
			if row.get(field) == value:
				result.append(row)
	elif _tables.has(key):
		var t: Dictionary = _tables[key]
		for pk: Variant in t:
			var row: _ModuleTableType = t[pk]
			if row.get(field) == value:
				result.append(row)
	return result


## Returns the first row where [param field] equals [param value], or [code]null[/code].
func first_by(table_name: StringName, field: StringName, value: Variant) -> _ModuleTableType:
	var key: StringName = _normalize(table_name)
	if _pk_less_tables.has(key):
		for row: _ModuleTableType in _pk_less_tables[key]:
			if row.get(field) == value:
				return row
	elif _tables.has(key):
		var t: Dictionary = _tables[key]
		for pk: Variant in t:
			var row: _ModuleTableType = t[pk]
			if row.get(field) == value:
				return row
	return null


## Returns the count of rows matching [param predicate].
func count_where(table_name: StringName, predicate: Callable) -> int:
	var key: StringName = _normalize(table_name)
	var c: int = 0
	if _pk_less_tables.has(key):
		for row: _ModuleTableType in _pk_less_tables[key]:
			if predicate.call(row):
				c += 1
	elif _tables.has(key):
		var t: Dictionary = _tables[key]
		for pk: Variant in t:
			var row: _ModuleTableType = t[pk]
			if predicate.call(row):
				c += 1
	return c


## Erases all rows from every table. Used during reconnection to reset state.
func clear_all_tables() -> void:
	for table_name: StringName in _tables:
		_tables[table_name].clear()
	for table_name: StringName in _pk_less_tables:
		_pk_less_tables[table_name].clear()
	_ref_counts.clear()
	_pk_less_counts.clear()
	_query_rows.clear()
