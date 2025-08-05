class_name LocalDatabase extends Node

var _tables: Dictionary[String, Dictionary] = {}
var _primary_key_cache: Dictionary = {}
var _schema: SpacetimeDBSchema

var _cached_normalized_table_names: Dictionary = {} 
var _cached_pk_fields: Dictionary = {}
var _insert_listeners_by_table: Dictionary = {}
var _update_listeners_by_table: Dictionary = {}
var _delete_listeners_by_table: Dictionary = {} 
var _delete_key_listeners_by_table: Dictionary = {} 
var _transactions_completed_listeners_by_table: Dictionary = {}

signal row_inserted(table_name: String, row: _ModuleTableType)
signal row_updated(table_name: String, old_row: _ModuleTableType, new_row: _ModuleTableType)
signal row_deleted(table_name: String, row: _ModuleTableType) 
signal row_transactions_completed(table_name: String)

func _init(p_schema: SpacetimeDBSchema):
    # Initialize _tables dictionary with known table names
    _schema = p_schema
    for table_name_lower in _schema.tables.keys():
        _tables[table_name_lower] = {}

func subscribe_to_inserts(table_name: StringName, callable: Callable):
    if not _insert_listeners_by_table.has(table_name):
        _insert_listeners_by_table[table_name] = []
    if not _insert_listeners_by_table[table_name].has(callable):
        _insert_listeners_by_table[table_name].append(callable)

func unsubscribe_from_inserts(table_name: StringName, callable: Callable):
    if _insert_listeners_by_table.has(table_name):
        _insert_listeners_by_table[table_name].erase(callable)
        if _insert_listeners_by_table[table_name].is_empty():
            _insert_listeners_by_table.erase(table_name)

func subscribe_to_updates(table_name: StringName, callable: Callable):
    if not _update_listeners_by_table.has(table_name):
        _update_listeners_by_table[table_name] = []
    if not _update_listeners_by_table[table_name].has(callable):
        _update_listeners_by_table[table_name].append(callable)

func unsubscribe_from_updates(table_name: StringName, callable: Callable):
    if _update_listeners_by_table.has(table_name):
        _update_listeners_by_table[table_name].erase(callable)
        if _update_listeners_by_table[table_name].is_empty():
            _update_listeners_by_table.erase(table_name)

func subscribe_to_deletes(table_name: StringName, callable: Callable):
    if not _delete_listeners_by_table.has(table_name):
        _delete_listeners_by_table[table_name] = []
    if not _delete_listeners_by_table[table_name].has(callable):
        _delete_listeners_by_table[table_name].append(callable)

func unsubscribe_from_deletes(table_name: StringName, callable: Callable):
    if _delete_listeners_by_table.has(table_name):
        _delete_listeners_by_table[table_name].erase(callable)
        if _delete_listeners_by_table[table_name].is_empty():
            _delete_listeners_by_table.erase(table_name)

func subscribe_to_transactions_completed(table_name: StringName, callable: Callable):
    if not _transactions_completed_listeners_by_table.has(table_name):
        _transactions_completed_listeners_by_table[table_name] = []
    if not _transactions_completed_listeners_by_table[table_name].has(callable):
        _transactions_completed_listeners_by_table[table_name].append(callable)

func unsubscribe_from_transactions_completed(table_name: StringName, callable: Callable):
    if _transactions_completed_listeners_by_table.has(table_name):
        _transactions_completed_listeners_by_table[table_name].erase(callable)
        if _transactions_completed_listeners_by_table[table_name].is_empty():
            _transactions_completed_listeners_by_table.erase(table_name)

# --- Primary Key Handling ---
# Finds and caches the primary key field name for a given schema
func _get_primary_key_field(table_name_lower: String) -> StringName:
    if _primary_key_cache.has(table_name_lower):
        return _primary_key_cache[table_name_lower]

    if not _schema.types.has(table_name_lower):
        printerr("LocalDatabase: No schema found for table '", table_name_lower, "' to determine PK.")
        return &"" # Return empty StringName

    var schema := _schema.get_type(table_name_lower)
    var instance = schema.new() # Need instance for metadata/properties

    # 1. Check metadata (preferred)
    if instance and instance.has_meta("primary_key"):
        var pk_field : StringName = instance.get_meta("primary_key")
        _primary_key_cache[table_name_lower] = pk_field
        return pk_field

    # 2. Convention: Check for "identity" or "id" field
    var properties = schema.get_script_property_list()
    for prop in properties:
        if prop.usage & PROPERTY_USAGE_STORAGE:
            if prop.name == &"identity" or prop.name == &"id":
                _primary_key_cache[table_name_lower] = prop.name
                return prop.name
            # 3. Fallback: Assume first exported property (less reliable)
            # Uncomment if this is your desired convention
            # _primary_key_cache[table_name_lower] = prop.name
            # return prop.name

    printerr("LocalDatabase: Could not determine primary key for table '", table_name_lower, "'. Add metadata or use convention.")
    _primary_key_cache[table_name_lower] = &"" # Cache failure
    return &""


# --- Applying Updates ---

func apply_database_update(db_update: DatabaseUpdateData):
    if not db_update: return
    for table_update: TableUpdateData in db_update.tables:
        apply_table_update(table_update)

func apply_table_update(table_update: TableUpdateData):
    var table_name_original := StringName(table_update.table_name)
    var table_name_lower: String

    if _cached_normalized_table_names.has(table_name_original):
        table_name_lower = _cached_normalized_table_names[table_name_original]
    else:
        table_name_lower = table_update.table_name.to_lower().replace("_", "")
        _cached_normalized_table_names[table_name_original] = table_name_lower

    if not _tables.has(table_name_lower):
        printerr("LocalDatabase: Received update for unknown table '", table_name_original, "' (normalized: '", table_name_lower, "')")
        return
        
    var pk_field: StringName
    if _cached_pk_fields.has(table_name_lower):
        pk_field = _cached_pk_fields[table_name_lower]
    else:
        pk_field = _get_primary_key_field(table_name_lower)
        if pk_field == &"":
            printerr("LocalDatabase: Cannot apply update for table '", table_name_original, "' without primary key.")
            return
        _cached_pk_fields[table_name_lower] = pk_field

    var table_dict := _tables[table_name_lower]
    
    var inserted_pks_set: Dictionary = {} # { pk_value: true }
    
    for inserted_row: _ModuleTableType in table_update.inserts:
        var pk_value = inserted_row.get(pk_field)
        if pk_value == null:
            push_error("LocalDatabase: Inserted row for table '", table_name_original, "' has null PK value for field '", pk_field, "'. Skipping.")
            continue

        inserted_pks_set[pk_value] = true

        var prev_row_resource: _ModuleTableType = table_dict.get(pk_value, null)
        
        table_dict[pk_value] = inserted_row
        if prev_row_resource != null:
            if _update_listeners_by_table.has(table_name_original):
                for listener: Callable in _update_listeners_by_table[table_name_original]:
                    listener.call(prev_row_resource, inserted_row) 
                row_updated.emit(table_name_original, prev_row_resource, inserted_row)
        else:
            if _insert_listeners_by_table.has(table_name_original):
                for listener: Callable in _insert_listeners_by_table[table_name_original]:
                    listener.call(inserted_row)
                row_inserted.emit(table_name_original, inserted_row)
                
    for deleted_row: _ModuleTableType in table_update.deletes:
        var pk_value = deleted_row.get(pk_field)
        if pk_value == null:
            push_warning("LocalDatabase: Deleted row for table '", table_name_original, "' has null PK value for field '", pk_field, "'. Skipping.")
            continue
            
        if not inserted_pks_set.has(pk_value):
            if table_dict.erase(pk_value):
                if _delete_listeners_by_table.has(table_name_original):
                    for listener: Callable in _delete_listeners_by_table[table_name_original]:
                        listener.call(deleted_row)
                    row_deleted.emit(table_name_original, deleted_row)

    if _transactions_completed_listeners_by_table.has(table_name_original):
        for listener: Callable in _transactions_completed_listeners_by_table[table_name_original]:
            listener.call()
        row_transactions_completed.emit(table_name_original)
            
# --- Access Methods ---
func get_row_by_pk(table_name: String, primary_key_value) -> _ModuleTableType:
    var table_name_lower := table_name.to_lower().replace("_","")
    if _tables.has(table_name_lower):
        return _tables[table_name_lower].get(primary_key_value) 
    return null
    
func get_all_rows(table_name: String) -> Array[_ModuleTableType]:
    var rows = _get_all_rows_untyped(table_name)
    var typed_result_array: Array[_ModuleTableType] = []
    typed_result_array.assign(rows)
    
    return typed_result_array

func count_all_rows(table_name: String) -> int:
    var rows = _get_all_rows_untyped(table_name)
    return rows.size()
        
func _get_all_rows_untyped(table_name: String) -> Array:
    var table_name_lower := table_name.to_lower().replace("_","")
    if _tables.has(table_name_lower):
        var table_dict := _tables[table_name_lower]
        return table_dict.values()
    
    return []
