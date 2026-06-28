@tool
class_name SpacetimePluginUI
extends Control

const ERROR_LOG_ICON: String = SpacetimePlugin.ADDON_PATH + "/ui/icons/Error.svg"

signal plugin_config_changed()
signal check_uri()
signal generate_schema()

var _uri_input: LineEdit
var _modules_container: VBoxContainer
var _logs_label: RichTextLabel
var _add_module_hint_label: RichTextLabel
var _new_module_name_input: LineEdit
var _new_module_alias_input: LineEdit
var _new_module_reducer_checkbox: CheckBox
var _new_module_table_checkbox: CheckBox
var _generate_button: Button
var _plugin_config: SpacetimeDBPluginConfig


func _enter_tree() -> void:
	_uri_input = %UriInput
	_modules_container = %ModulesContainer
	_logs_label = %Logs
	_add_module_hint_label = %AddModuleHint
	_new_module_name_input = %ModuleNameInput
	_new_module_alias_input = %ModuleAliasInput
	_new_module_reducer_checkbox = %ReducerCheckbox
	_new_module_table_checkbox = %TablesCheckbox
	_generate_button = %GenerateButton


func _input(event: InputEvent) -> void:
	if not visible:
		return

	if event is InputEventKey:
		if event.pressed and event.keycode == KEY_C and event.ctrl_pressed:
			copy_selected_logs()
		elif event.pressed and event.keycode == KEY_K and event.ctrl_pressed and event.alt_pressed:
			clear_logs()


func set_uri(uri: String) -> void:
	_uri_input.text = uri


func update_module_ui() -> void:
	for child in _modules_container.get_children():
		_modules_container.remove_child(child)
		child.queue_free()
	set_uri(_plugin_config.uri)
	for module_id: String in _plugin_config.module_configs:
		var module_config: SpacetimeDBModuleConfig = _plugin_config.module_configs[module_id]
		var new_module: Control = $"Prefabs/ModulePrefab".duplicate() as Control
		var name_input: LineEdit = new_module.get_node("VBoxContainer/HBoxContainer/VBoxContainer/ModuleNameInput") as LineEdit
		var alias_input: LineEdit = new_module.get_node("VBoxContainer/HBoxContainer/VBoxContainer/ModuleAliasInput") as LineEdit
		var reducer_config_box: CheckBox = new_module.get_node("VBoxContainer/ReducerCheckbox") as CheckBox
		var table_config_box: CheckBox = new_module.get_node("VBoxContainer/TableCheckbox") as CheckBox
		name_input.text = module_config.name
		alias_input.text = module_config.alias
		reducer_config_box.button_pressed = module_config.hide_scheduled_reducers
		table_config_box.button_pressed = module_config.hide_private_tables
		_modules_container.add_child(new_module)

		var remove_button: Button = new_module.get_node("VBoxContainer/HBoxContainer/RemoveButton") as Button
		remove_button.button_down.connect(
			func():
				_plugin_config.module_configs.erase(module_config.alias)
				plugin_config_changed.emit()
				update_module_ui()
		)
		new_module.show()
		reducer_config_box.toggled.connect(
			func(on: bool):
				module_config.hide_scheduled_reducers = on
				plugin_config_changed.emit()
		)
		table_config_box.toggled.connect(
			func(on: bool):
				module_config.hide_private_tables = on
				plugin_config_changed.emit()
		)
		name_input.text_changed.connect(
			func(text: String):
				module_config.name = text
				plugin_config_changed.emit()
		)
		alias_input.text_changed.connect(
			func(text: String):
				_plugin_config.module_configs.erase(module_config.alias)
				module_config.alias = text
				_plugin_config.module_configs.set(module_config.alias, module_config)
				plugin_config_changed.emit()
		)
	if _modules_container.get_child_count() == 0:
		_add_module_hint_label.show()
		_generate_button.disabled = true
	else:
		_add_module_hint_label.hide()
		_generate_button.disabled = false


func clear_logs() -> void:
	_logs_label.text = ""


func copy_selected_logs() -> void:
	var selected_text: String = _logs_label.get_selected_text()
	if selected_text:
		DisplayServer.clipboard_set(selected_text)


func add_log(text: Variant) -> void:
	var text_type: int = typeof(text)
	if text_type == TYPE_STRING:
		_logs_label.text += "%s\n" % [text]
	elif text_type == TYPE_ARRAY:
		for i in text as Array:
			_logs_label.text += str(i) + " "
		_logs_label.text += "\n"
	else:
		_logs_label.text += "%s\n" % [str(text)]


func add_err(text: Variant) -> void:
	var text_type: int = typeof(text)
	if text_type == TYPE_STRING:
		_logs_label.text += "[img]%s[/img] [color=#FF786B][b]ERROR:[/b] %s[/color]\n" % [ERROR_LOG_ICON, text]
	elif text_type == TYPE_ARRAY:
		_logs_label.text += "[img]%s[/img] [color=#FF786B][b]ERROR:[/b] " % [ERROR_LOG_ICON]
		for i in text as Array:
			_logs_label.text += str(i) + " "
		_logs_label.text += "[/color]\n"
	else:
		_logs_label.text += "[img]%s[/img] [color=#FF786B][b]ERROR:[/b] %s[/color]\n" % [ERROR_LOG_ICON, str(text)]


func destroy() -> void:
	_uri_input = null
	_modules_container = null
	_logs_label = null
	_add_module_hint_label = null
	_new_module_name_input = null
	_new_module_alias_input = null
	_generate_button = null


func _on_check_uri() -> void:
	_plugin_config.uri = _uri_input.text
	update_module_ui()
	check_uri.emit()


func _on_generate_code() -> void:
	generate_schema.emit()


func _on_new_module() -> void:
	var name: String = _new_module_name_input.text
	var alias: String = _new_module_alias_input.text
	var table_config: bool = _new_module_table_checkbox.button_pressed
	var reducer_config: bool = _new_module_reducer_checkbox.button_pressed
	if alias.is_empty():
		alias = name
	var module_config: SpacetimeDBModuleConfig = _plugin_config.module_configs.get(alias, SpacetimeDBModuleConfig.new())
	module_config.name = name
	module_config.alias = alias
	module_config.hide_private_tables = table_config
	module_config.hide_scheduled_reducers = reducer_config
	_plugin_config.module_configs.set(alias, module_config)
	_new_module_name_input.text = ""
	_new_module_alias_input.text = ""
	plugin_config_changed.emit()
	update_module_ui()


func _on_clear_logs() -> void:
	clear_logs()


func _on_copy_selected_logs() -> void:
	copy_selected_logs()
