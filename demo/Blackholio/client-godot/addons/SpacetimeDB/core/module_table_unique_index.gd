class_name _ModuleTableUniqueIndex extends Resource

func _connect_cache_to_db(cache: Dictionary, db: LocalDatabase) -> void:
    var table_name: String = get_meta("table_name", "")
    var field_name: String = get_meta("field_name", "")
    
    db.subscribe_to_inserts(table_name, func(r: _ModuleTableType):
        var col_val = r[field_name]
        cache[col_val] = r
    )
    db.subscribe_to_updates(table_name, func(p: _ModuleTableType, r: _ModuleTableType):
        var previous_col_val = p[field_name]
        var col_val = r[field_name]
        
        if previous_col_val != col_val:
            cache.erase(previous_col_val)
        cache[col_val] = r
    )
    db.subscribe_to_deletes(table_name, func(r: _ModuleTableType):
        var col_val = r[field_name]
        cache.erase(col_val)
    )
