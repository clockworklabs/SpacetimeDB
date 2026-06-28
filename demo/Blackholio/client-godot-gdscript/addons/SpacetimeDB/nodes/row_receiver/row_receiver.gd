@tool
@icon("res://addons/SpacetimeDB/nodes/row_receiver/icon.svg")
class_name RowReceiver
extends Node

signal insert(row: _ModuleTableType)
signal update(prev: _ModuleTableType, row: _ModuleTableType)
signal delete(row: _ModuleTableType)
signal transactions_completed

@export var debug_mode: bool = false
@export var table_to_receive: _ModuleTableType:
	set = on_set

var selected_table_name: StringName:
	set = set_selected_table_name
var _derived_table_names: Array[StringName] = []
var _current_db_instance: LocalDatabase = null


func _ready() -> void:
	if Engine.is_editor_hint():
		return

	if not table_to_receive:
		push_error("The table_to_receive is not set on %s" % get_path())
		return

	call_deferred(&"_init_subscription")


func _init_subscription() -> void:
	var db: LocalDatabase = await _get_db(true)
	if not is_instance_valid(self) or not is_inside_tree():
		return
	_subscribe_to_table(db, selected_table_name)


func _exit_tree() -> void:
	_unsubscribe_from_table(selected_table_name)


func _get_property_list() -> Array[Dictionary]:
	var properties: Array[Dictionary] = []
	if not _derived_table_names.is_empty():
		var hint_string_for_enum: String = ",".join(_derived_table_names)
		properties.append(
			{
				"name": "selected_table_name",
				"type": TYPE_STRING_NAME,
				"hint": PROPERTY_HINT_ENUM,
				"hint_string": hint_string_for_enum,
			},
		)
	return properties


func on_set(schema: _ModuleTableType) -> void:
	_derived_table_names.clear()

	if schema == null:
		name = "Receiver [EMPTY]"
		table_to_receive = schema
		if not selected_table_name.is_empty():
			set_selected_table_name("")
	else:
		var script_resource: Script = schema.get_script()

		if script_resource is Script:
			var global_name: String = script_resource.get_global_name()
			global_name = global_name.replace("_gd", "")
			if global_name == "_ModuleTableType":
				push_error("_ModuleTableType is the base class for table types, not a reciever table. Selection is not changed.")
				return
			table_to_receive = schema
			name = "Receiver [%s]" % global_name

			var constant_map: Dictionary = script_resource.get_script_constant_map()
			if constant_map.has("table_names"):
				var names_value: Variant = constant_map["table_names"]
				if names_value is Array:
					for item: Variant in names_value:
						if item is StringName or item is String:
							_derived_table_names.push_back(StringName(item))
		else:
			name = "Receiver [Unknown Schema Type]"

	var current_selection_still_valid: bool = _derived_table_names.has(selected_table_name)
	if not current_selection_still_valid:
		if not _derived_table_names.is_empty():
			set_selected_table_name(_derived_table_names[0])
		else:
			if not selected_table_name.is_empty():
				set_selected_table_name("")

	if Engine.is_editor_hint():
		property_list_changed.emit()


func set_selected_table_name(value: StringName) -> void:
	if selected_table_name == value:
		return
	selected_table_name = value


func get_table_data() -> Array[_ModuleTableType]:
	var db: LocalDatabase = await _get_db()
	if db:
		return db.get_all_rows(selected_table_name)
	return []


func _print_log(log_message: String) -> void:
	if debug_mode:
		print("%s: %s" % [get_path(), log_message])


func _get_db(wait_for_init: bool = false) -> LocalDatabase:
	if _current_db_instance == null or not is_instance_valid(_current_db_instance):
		var constants: Dictionary = (table_to_receive.get_script() as GDScript).get_script_constant_map()
		var module_name: String = constants.get("module_name", "").to_pascal_case()
		var client: SpacetimeDBClient = SpacetimeDB[module_name]
		_current_db_instance = client.get_local_database()

		if _current_db_instance == null and wait_for_init:
			_print_log("Waiting for db to be initialized...")
			await client.database_initialized
			_current_db_instance = client.get_local_database()
			_print_log("Db initialized")
	return _current_db_instance


func _subscribe_to_table(db: LocalDatabase, table_name_sn: StringName) -> void:
	if Engine.is_editor_hint() or table_name_sn.is_empty():
		return

	_print_log("Subscribing to table: %s" % table_name_sn)

	if get_parent() and not get_parent().is_node_ready():
		_print_log("Waiting for parent before subscribing")
		await get_parent().ready
		if not is_instance_valid(self) or not is_inside_tree():
			return

	# Emit data that was inserted before we subscribed
	var existing_data: Array[_ModuleTableType] = await get_table_data()
	if not is_instance_valid(self) or not is_inside_tree():
		return
	if not existing_data.is_empty():
		for row: _ModuleTableType in existing_data:
			_on_insert(row)
		_on_transactions_completed()

	db.subscribe_to_inserts(table_name_sn, _on_insert)
	db.subscribe_to_updates(table_name_sn, _on_update)
	db.subscribe_to_deletes(table_name_sn, _on_delete)
	db.subscribe_to_transactions_completed(table_name_sn, _on_transactions_completed)

	_print_log("Successfully subscribed to table: %s" % table_name_sn)


func _unsubscribe_from_table(table_name_sn: StringName) -> void:
	if Engine.is_editor_hint() or table_name_sn.is_empty():
		return

	_print_log("Unsubscribing from table: %s" % table_name_sn)

	var db: LocalDatabase = await _get_db()
	if not is_instance_valid(db):
		return

	db.unsubscribe_from_inserts(table_name_sn, _on_insert)
	db.unsubscribe_from_updates(table_name_sn, _on_update)
	db.unsubscribe_from_deletes(table_name_sn, _on_delete)
	db.unsubscribe_from_transactions_completed(table_name_sn, _on_transactions_completed)


func _on_insert(row: _ModuleTableType) -> void:
	insert.emit(row)


func _on_update(previous: _ModuleTableType, row: _ModuleTableType) -> void:
	update.emit(previous, row)


func _on_delete(row: _ModuleTableType) -> void:
	delete.emit(row)


func _on_transactions_completed() -> void:
	transactions_completed.emit()
