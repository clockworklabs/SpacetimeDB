## A Rust-style Option type for representing values that may or may not exist.
##
## Wraps a single value in an array-backed store. Use [method some] and [method none]
## static constructors to create instances, and [method unwrap] / [method unwrap_or]
## to safely retrieve the inner value.
##
## [b]Usage:[/b]
## [codeblock]
## var opt: Option = Option.some(42)
## if opt.is_some():
##     print(opt.unwrap())  # 42
##
## var empty: Option = Option.none()
## print(empty.unwrap_or(0))  # 0
## [/codeblock]
##
## The internal array is clamped to 0 or 1 elements — assigning a multi-element
## array via the [member data] setter silently truncates to the first element.
@tool
class_name Option
extends Resource

## Exported accessor for the internal data array. Setting this with an array
## of 1+ elements stores only the first element (Some); an empty array means None.
@export var data: Array = []:
	set(value):
		if value is Array:
			if not value.is_empty():
				_internal_data = value.slice(0, 1)
			else:
				_internal_data = []
		else:
			push_error("Optional data must be an Array.")
			_internal_data = []
	get():
		return _internal_data

var _internal_data: Array = []


## Creates an Option containing [param value].
static func some(value: Variant) -> Option:
	var result = Option.new()
	result.set_some(value)
	return result


## Creates an empty Option (None).
static func none() -> Option:
	var result = Option.new()
	result.set_none()
	return result


## Returns [code]true[/code] if this Option contains a value.
func is_some() -> bool:
	return not _internal_data.is_empty()


## Returns [code]true[/code] if this Option is empty.
func is_none() -> bool:
	return _internal_data.is_empty()


## Returns the contained value. Pushes an error if this Option is None.
func unwrap() -> Variant:
	if is_some():
		return _internal_data[0]
	push_error("Attempted to unwrap a None Optional value!")
	return null


## Returns the contained value, or [param default_value] if None.
func unwrap_or(default_value: Variant) -> Variant:
	if is_some():
		return _internal_data[0]
	return default_value


## Returns the contained value, or calls [param fn] and returns its result if None.
func unwrap_or_else(fn: Callable) -> Variant:
	if is_some():
		return _internal_data[0]
	if fn.is_valid():
		return fn.call()
	return null


## Returns the contained value if it matches [param type], otherwise pushes
## [param err_msg] (or a default message) as an error and returns [code]null[/code].
func expect(type: Variant.Type, err_msg: String = "") -> Variant:
	if is_some():
		if typeof(_internal_data[0]) != type:
			err_msg = "Expected type %s, got %s" % [type, typeof(_internal_data[0])] if err_msg.is_empty() else err_msg
			push_error(err_msg)
			return null
		return _internal_data[0]
	err_msg = "Expected type %s, got None" % type if err_msg.is_empty() else err_msg
	push_error(err_msg)
	return null


## Sets this Option to contain [param value].
func set_some(value: Variant) -> void:
	self.data = [value]


## Clears this Option to None.
func set_none() -> void:
	self.data = []


## Returns a debug string: [code]"Some(value [type: N])"[/code] or [code]"None"[/code].
## Hooks into Godot's [code]str()[/code]/print conversion via [method Object._to_string].
func _to_string() -> String:
	if is_some():
		return "Some(%s [type: %s])" % [_internal_data[0], typeof(_internal_data[0])]
	else:
		return "None"
