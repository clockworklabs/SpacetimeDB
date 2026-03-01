class_name _ModuleTable extends RefCounted

var _db: LocalDatabase

func _init(db: LocalDatabase) -> void:
    _db = db

func count() -> int:
    return _db.count_all_rows(get_meta("table_name", ""))

func iter() -> Array:
    return _db.get_all_rows(get_meta("table_name", ""))

func on_insert(listener: Callable) -> void:
    _db.subscribe_to_inserts(get_meta("table_name", ""), listener)
    
func remove_on_insert(listener: Callable) -> void:
    _db.unsubscribe_from_inserts(get_meta("table_name", ""), listener)

func on_update(listener: Callable) -> void:
    _db.subscribe_to_updates(get_meta("table_name", ""), listener)
    
func remove_on_update(listener: Callable) -> void:
    _db.unsubscribe_from_updates(get_meta("table_name", ""), listener)

func on_delete(listener: Callable) -> void:
    _db.subscribe_to_deletes(get_meta("table_name", ""), listener)
    
func remove_on_delete(listener: Callable) -> void:
    _db.unsubscribe_from_deletes(get_meta("table_name", ""), listener)
