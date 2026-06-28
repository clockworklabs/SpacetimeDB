## Encodes GDScript values into the BSATN binary format for SpacetimeDB wire messages.
##
## Used internally by [SpacetimeDBClient] to serialize [SpacetimeDBClientMessage]
## payloads (subscribe, call_reducer, call_procedure, etc.) into [PackedByteArray]
## packets. Also provides low-level primitive writers ([method write_u8],
## [method write_i32], [method write_string], etc.) and a plan-based
## resource serializer that walks a [Resource]'s exported properties.
##
## Check [method has_error] after any serialization call; if [code]true[/code],
## retrieve the message via [method get_last_error].
class_name BSATNSerializer
extends RefCounted

# --- Constants ---
const IDENTITY_SIZE: int = 32
const CONNECTION_ID_SIZE: int = 16
const U128_SIZE: int = 16
const I128_SIZE: int = 16
const U256_SIZE: int = 32
const I256_SIZE: int = 32
# Native type handling
const CONTEXT_WRITERS: Dictionary[StringName, bool] = { &"write_array": true, &"write_option": true, &"write_native_arraylike": true, &"write_nested_resource": true }
const NATIVE_ARRAYLIKE_TYPES: Array[Variant.Type] = [
	TYPE_VECTOR2,
	TYPE_VECTOR2I,
	TYPE_VECTOR3,
	TYPE_VECTOR3I,
	TYPE_VECTOR4,
	TYPE_VECTOR4I,
	TYPE_QUATERNION,
	TYPE_COLOR,
]

var debug_mode: bool = false # Controls verbose debug printing
# --- Properties ---
var _last_error: String = ""
var _has_error: bool = false
var _serialization_plan_cache: Dictionary[Script, Array] = { }
var _spb: StreamPeerBuffer # Internal buffer used by writing functions
var _native_arraylike_regex: RegEx = RegEx.new()


# --- Initialization ---
func _init(p_debug_mode: bool = false) -> void:
	debug_mode = p_debug_mode
	_spb = StreamPeerBuffer.new()
	_spb.big_endian = false # Use Little-Endian

	_native_arraylike_regex.compile("^(?<struct>.+)\\[(?<components>.*)\\]$")

# --- Error Handling ---


## Returns [code]true[/code] if the last serialization operation failed.
func has_error() -> bool:
	return _has_error


## Returns and clears the last error message. Resets [method has_error] to [code]false[/code].
func get_last_error() -> String:
	var e: String = _last_error
	_last_error = ""
	_has_error = false
	return e


## Clears the error state without returning the message.
func clear_error() -> void:
	_last_error = ""
	_has_error = false


# --- Primitive Value Writers ---
# These directly write basic types to the internal StreamPeerBuffer.
func write_i8(v: int) -> void:
	if v < -128 or v > 127:
		_set_error("Value %d out of range for i8" % v)
		v = 0
	_spb.put_u8(v if v >= 0 else v + 256)


func write_i16_le(v: int) -> void:
	if v < -32768 or v > 32767:
		_set_error("Value %d out of range for i16" % v)
		v = 0
	_spb.put_u16(v if v >= 0 else v + 65536)


func write_i32_le(v: int) -> void:
	if v < -2147483648 or v > 2147483647:
		_set_error("Value %d out of range for i32" % v)
		v = 0
	_spb.put_u32(v) # put_u32 handles negative i32 correctly via two's complement


func write_i64_le(v: int) -> void:
	_spb.put_u64(v) # put_u64 handles negative i64 correctly via two's complement


func write_u8(v: int) -> void:
	if v < 0 or v > 255:
		_set_error("Value %d out of range for u8" % v)
		v = 0
	_spb.put_u8(v)


func write_u16_le(v: int) -> void:
	if v < 0 or v > 65535:
		_set_error("Value %d out of range for u16" % v)
		v = 0
	_spb.put_u16(v)


func write_u32_le(v: int) -> void:
	if v < 0 or v > 4294967295:
		_set_error("Value %d out of range for u32" % v)
		v = 0
	_spb.put_u32(v)


func write_u64_le(v: int) -> void:
	# No range guard: GDScript int is i64, so every bit pattern is a valid u64 on the
	# wire. A u64 >= 2^63 round-trips through get_u64() as a negative i64; put_u64
	# writes the same 8 bytes back. Rejecting v < 0 here would make those values
	# (large hashes / ids / u64 columns) un-serializable.
	_spb.put_u64(v)


func write_f32_le(v: float) -> void:
	_spb.put_float(v)


func write_f64_le(v: float) -> void:
	_spb.put_double(v)


func write_u128(v: PackedByteArray) -> void:
	_write_fixed_bytes_le(v, U128_SIZE, "U128")


func write_i128(v: PackedByteArray) -> void:
	_write_fixed_bytes_le(v, I128_SIZE, "I128")


func write_u256(v: PackedByteArray) -> void:
	_write_fixed_bytes_le(v, U256_SIZE, "U256")


func write_i256(v: PackedByteArray) -> void:
	_write_fixed_bytes_le(v, I256_SIZE, "I256")


func write_bool(v: bool) -> void:
	_spb.put_u8(1 if v else 0)


func write_bytes(v: PackedByteArray) -> void:
	var result: Error = _spb.put_data(v)
	if result != OK:
		_set_error("StreamPeerBuffer.put_data failed with code %d" % result)


func write_string_with_u32_len(v: String) -> void:
	var str_bytes: PackedByteArray = v.to_utf8_buffer()
	write_u32_le(str_bytes.size())
	if not str_bytes.is_empty():
		write_bytes(str_bytes)


func write_identity(v: PackedByteArray) -> void:
	_write_fixed_bytes_le(v, IDENTITY_SIZE, "Identity")


func write_connection_id(v: PackedByteArray) -> void:
	_write_fixed_bytes_le(v, CONNECTION_ID_SIZE, "ConnectionId")


func write_timestamp(v: int) -> void:
	write_i64_le(v) # Timestamps are typically i64


# ScheduleAt sum: u8 tag (0=Interval, 1=Time) then the i64 microsecond payload.
func write_scheduled_at(v: ScheduleAt) -> void:
	if v == null:
		_set_error("Cannot serialize null ScheduleAt")
		return
	if v.kind != ScheduleAt.Kind.INTERVAL and v.kind != ScheduleAt.Kind.TIME:
		_set_error("Invalid ScheduleAt kind %d" % v.kind)
		return
	write_u8(v.kind)
	write_i64_le(v.micros)


# Writes a PackedByteArray prefixed with its u32 length (Vec<u8> format)
func write_vec_u8(v: PackedByteArray) -> void:
	write_u32_le(v.size())
	if not v.is_empty():
		write_bytes(v) # Avoid calling put_data with empty array if possible


# --- Special Writers ---
## Writes a Rust sumtype enum
func write_rust_enum(rust_enum: RustEnum) -> void:
	write_u8(rust_enum.value)
	var enum_options: Array = rust_enum.get_script().get_script_constant_map().get(&"ENUM_OPTIONS", [])
	if rust_enum.value < 0 or rust_enum.value >= enum_options.size():
		_set_error("RustEnum value %d out of range for ENUM_OPTIONS (size %d)." % [rust_enum.value, enum_options.size()])
		return
	var sub_class: StringName = enum_options[rust_enum.value]
	var data: Variant = rust_enum.data
	if sub_class.begins_with("vec_"):
		if data is not Array:
			_set_error("Sum type of rust enum is Vec<T> but the godot type is not an array.")
			return
		var vec_type: StringName = sub_class.right(-4)
		# If it's an Option type, we need to remove the opt prefix for the serializer
		# This is a special case, the enum needs more info for the deserializer
		if vec_type.begins_with("opt_"):
			vec_type = vec_type.right(-4)
		_write_value_from_bsatn_type(data, vec_type, &"")
		return
	if sub_class.begins_with("opt_"):
		if data is not Option:
			_set_error("Sum type of rust enum is Option<T> but the godot type is not an Option.")
			return
		var opt_type: StringName = sub_class.right(-4)
		# If it's a Vec type, we need to remove the vec prefix for the serializer
		# This is a special case, the enum needs more info for the deserializer
		if opt_type.begins_with("vec_"):
			opt_type = opt_type.right(-4)
		_write_value_from_bsatn_type(data, opt_type, &"")
		return
	if not sub_class.is_empty():
		if data == null:
			data = _generate_default_type(sub_class)
		_write_value_from_bsatn_type(data, sub_class, &"")


## Writes an option value
func write_option(option_value: Option, bsatn_type: StringName, prop: Dictionary) -> bool:
	var prop_name: StringName = prop.name

	if not option_value is Option:
		_set_error("Value provided to write_option is not an Option instance (type: %s) for property '%s'." % [typeof(option_value), prop_name])
		return false
	if option_value.is_none():
		write_u8(1) # Tag for None
		if has_error():
			_set_error("Failed to write None tag for Option property '%s'." % prop_name)
			return false
		return true
	else: # is_some()
		write_u8(0) # Tag for Some
		if has_error():
			_set_error("Failed to write Some tag for Option property '%s'." % prop_name)
			return false
		_write_value_from_bsatn_type(option_value.unwrap(), bsatn_type, prop_name)
		return not has_error()


## Writes an array type
func write_array(v: Array[Variant], bsatn_type: StringName, prop: Dictionary) -> void:
	var prop_name: StringName = prop.name

	# 1. Write array length (u32)
	write_u32_le(v.size())
	if has_error():
		return
	if v.is_empty():
		return

	# 2. Determine element prototype info (Variant.Type, class_name)
	var hint: int = prop.hint
	var hint_string: String = prop.hint_string
	var element_type_code: Variant.Type = TYPE_MAX
	var element_class_name: StringName = &""

	if hint == PROPERTY_HINT_TYPE_STRING and ":" in hint_string: # Godot 3: "Type:TypeName"
		var hint_parts: PackedStringArray = hint_string.split(":", true, 1)
		if hint_parts.size() == 2:
			var hint_type: PackedStringArray = hint_parts[0].split("/", true, 1) if "/" in hint_parts[0] else [hint_parts[0]]
			element_type_code = int(hint_type[0])
			if element_type_code == TYPE_OBJECT:
				element_class_name = hint_parts[1]
	elif hint == PROPERTY_HINT_ARRAY_TYPE: # Godot 4: "VariantType/ClassName:VariantType" or "VariantType:VariantType"
		var main_type_str: String = hint_string.split(":", true, 1)[0]
		if "/" in main_type_str:
			var parts: PackedStringArray = main_type_str.split("/", true, 1)
			element_type_code = int(parts[0])
			element_class_name = parts[1]
		else:
			element_type_code = int(main_type_str)

	if element_type_code == TYPE_MAX and not v.is_empty():
		var first_element: Variant = v[0]
		element_type_code = typeof(first_element)
		if element_type_code == TYPE_OBJECT:
			element_class_name = _get_value_class_name(first_element)

	if element_type_code == TYPE_MAX and bsatn_type.is_empty():
		_set_error("Array '%s' needs a typed hint or must not be empty for serialization. Hint: %d, HintString: '%s'" % [prop_name, hint, hint_string])
		return

	# 3. Create a temporary "prototype" dictionary for the element
	var element_prop_sim: Dictionary = {
		"name": prop_name + "[element]",
		"type": element_type_code,
		"class_name": element_class_name,
		"usage": PROPERTY_USAGE_STORAGE,
		"hint": 0,
		"hint_string": "",
	}

	# 4. Determine and pre-bind the element writer
	var element_writer: Callable
	if bsatn_type.begins_with(&"opt_") or bsatn_type.begins_with(&"vec_"):
		# Prefixed type — use recursive type-driven serialization for deep nesting
		element_writer = _write_value_from_bsatn_type.bind(bsatn_type, prop_name + &"[element]")
	elif element_class_name == &"Option":
		if bsatn_type.is_empty():
			_set_error("Array '%s' of Options has empty 'bsatn_type' metadata. Inner type T for Option<T> cannot be determined." % prop_name)
			return
		element_writer = write_option.bind(bsatn_type, element_prop_sim)
	else:
		var raw_writer: Callable
		if not bsatn_type.is_empty():
			raw_writer = _get_primitive_writer_from_bsatn_type(bsatn_type)
			if not raw_writer.is_valid() and debug_mode:
				push_warning("Array '%s' bsatn_type '%s' doesn't map to a primitive writer. Falling back to element type hint." % [prop_name, bsatn_type])
		if not raw_writer.is_valid():
			raw_writer = _get_writer_callable_for_property(element_prop_sim, bsatn_type)
		if not raw_writer.is_valid():
			_set_error("Cannot determine writer for elements of array '%s' (type %d, class '%s')." % [prop_name, element_type_code, element_class_name])
			return
		element_writer = raw_writer.bind(bsatn_type, element_prop_sim) if raw_writer.get_method() in CONTEXT_WRITERS else raw_writer

	var i: int = 0
	for element_value: Variant in v:
		element_writer.call(element_value)
		if has_error():
			var existing_error: String = get_last_error()
			_set_error("Failed writing element %d of array '%s'. Cause: %s" % [i, prop_name, existing_error])
			return
		i += 1


## Writes a native array-like value
func write_native_arraylike(v: Variant, bsatn_type: StringName, prop: Dictionary) -> void:
	var prop_name: StringName = prop.name

	if bsatn_type.is_empty():
		_set_error("Array-like gd type '%s' has empty 'bsatn_type' metadata. Inner component types cannot be determined." % prop_name)
		return

	var result: RegExMatch = _native_arraylike_regex.search(bsatn_type)
	if result == null:
		_set_error("Array-like gd type '%s' does not match native array-like pattern from 'bsatn_type' metadata ('%s')" % [prop_name, bsatn_type])
		return
	var bsatn_struct_type: String = result.get_string("struct")
	if bsatn_struct_type.is_empty():
		_set_error("Cannot determine struct type for array-like gd type '%s' from 'bsatn_type' metadata ('%s')" % [prop_name, bsatn_type])
		return

	if v == null:
		v = _generate_default_type(bsatn_struct_type)
	var value_type: int = typeof(v)

	# if-elif, not match: per native-vector field per send. match arm ~10 opcodes vs
	# ~2 for an if branch (see deserializer _read_native_arraylike). Float vectors first.
	var components: Array
	if value_type == TYPE_VECTOR2:
		components = [v.x, v.y]
	elif value_type == TYPE_VECTOR3:
		components = [v.x, v.y, v.z]
	elif value_type == TYPE_VECTOR4:
		components = [v.x, v.y, v.z, v.w]
	elif value_type == TYPE_COLOR:
		components = [v.r, v.g, v.b, v.a]
	elif value_type == TYPE_QUATERNION:
		components = [v.x, v.y, v.z, v.w]
	elif value_type == TYPE_VECTOR2I:
		components = [v.x, v.y]
	elif value_type == TYPE_VECTOR3I:
		components = [v.x, v.y, v.z]
	elif value_type == TYPE_VECTOR4I:
		components = [v.x, v.y, v.z, v.w]
	else:
		_set_error("Unsupported array-like gd type '%s' ('%s'). Could not assign components array." % [prop_name, type_string(value_type)])
		return

	var bsatn_types_for_components: String = result.get_string("components")
	if bsatn_types_for_components.is_empty():
		_set_error("Cannot determine inner component types for array-like gd type '%s' from 'bsatn_type' metadata ('%s')" % [prop_name, bsatn_type])
		return

	var bsatn_component_types: PackedStringArray = bsatn_types_for_components.split(",")
	if bsatn_component_types.size() != components.size():
		_set_error(
			"Array-like gd type '%s' expected 'bsatn_type' to have %d component types but has %d" % \
					[prop_name, components.size(), bsatn_component_types.size()],
		)
		return

	for i: int in components.size():
		var value: Variant = components[i]
		var bsatn_component_type: String = bsatn_component_types[i]
		_write_value_from_bsatn_type(value, bsatn_component_type, prop_name + "[%s]" % i)


func write_nested_resource(resource: Object, bsatn_type: StringName, prop: Dictionary) -> void:
	if not resource is RefCounted: # Resource extends RefCounted — SpacetimeDBMessage too
		_set_error("Cannot serialize non-RefCounted Object (got: %s)" % resource.get_class())
		return

	# Tagged-sum (enum-with-payload) fields/values are RustEnum subclasses. They serialize
	# as a u8 tag + variant payload, NOT as a product of their value/data fields. The
	# property dispatch only matches the literal "RustEnum" class_name, so subclasses reach
	# here as a generic Object — detect by instance and delegate to the sum writer.
	if resource is RustEnum:
		write_rust_enum(resource)
		return

	var prop_name: StringName = prop.name
	var nested_class_name: StringName = prop.class_name

	# Serialize resource fields directly inline (recursive)
	if not _serialize_resource_fields(resource):
		if not has_error():
			_set_error("Failed to serialize nested resource '%s' of '%s'." % [prop_name, nested_class_name])

# --- Public API ---


## Serializes a complete client message into a [PackedByteArray].[br]
## Writes the [param variant_tag] byte followed by all exported properties of
## [param payload_resource] in BSATN format. Check [method has_error] after calling.
func serialize_client_message(variant_tag: int, payload_resource: SpacetimeDBClientMessage) -> PackedByteArray:
	# Reset state
	clear_error()
	_spb.data_array = PackedByteArray()
	_spb.seek(0)

	# 1. Write the message variant tag (u8)
	write_u8(variant_tag)
	if has_error():
		return PackedByteArray()

	# 2. Serialize payload resource fields
	if payload_resource == null:
		_set_error("Cannot serialize null payload for tag %d" % variant_tag)
		return PackedByteArray()
	if not _serialize_resource_fields(payload_resource):
		if not has_error():
			_set_error("Failed to serialize payload for tag %d" % variant_tag)
		return PackedByteArray()

	return _spb.data_array if not has_error() else PackedByteArray()


func _write_fixed_bytes_le(v: PackedByteArray, expected_size: int, type_label: String) -> void:
	if v == null or v.size() != expected_size:
		# Error set → the whole serialization is abandoned (callers bail on has_error).
		# Don't emit zero-filled bytes: that left a corrupt partial packet past the
		# error point for any caller that ignored the flag.
		_set_error("Invalid %s value (null or size != %d)" % [type_label, expected_size])
		return
	var v_copy: PackedByteArray = v.duplicate()
	v_copy.reverse()
	write_bytes(v_copy)


# Sets the error message if not already set. Internal use.
func _set_error(msg: String) -> void:
	if not _has_error: # Prevent overwriting — O(1) bool check
		_last_error = "BSATNSerializer Error: %s" % msg
		_has_error = true
		printerr(_last_error)


# --- Core Serialization Logic ---
func _get_value_class_name(value: Variant) -> String:
	if value is RefCounted: # Resource extends RefCounted — one check covers both
		var script: Script = value.get_script()
		if not script:
			return value.get_class()

		var g_name: StringName = script.get_global_name()
		if g_name.is_empty():
			return value.get_class()

		return g_name

	var value_type: int = typeof(value)
	if value_type == TYPE_OBJECT:
		return value.get_class()
	return type_string(value_type)


# Helper to get the specific BSATN writer METHOD NAME based on metadata value.
func _get_primitive_writer_from_bsatn_type(bsatn_type_str: StringName) -> Callable:
	# if-elif, not match: reached per element on the recursive vec/option path
	# (_write_value_from_bsatn_type) plus at plan-build. match arm ~10 opcodes vs ~2
	# for an if branch; ordered by expected field frequency.
	if bsatn_type_str == &"u32":
		return write_u32_le
	elif bsatn_type_str == &"i32":
		return write_i32_le
	elif bsatn_type_str == &"u64":
		return write_u64_le
	elif bsatn_type_str == &"i64":
		return write_i64_le
	elif bsatn_type_str == &"f32":
		return write_f32_le
	elif bsatn_type_str == &"bool":
		return write_bool
	elif bsatn_type_str == &"string":
		return write_string_with_u32_len
	elif bsatn_type_str == &"u8":
		return write_u8
	elif bsatn_type_str == &"u16":
		return write_u16_le
	elif bsatn_type_str == &"i8":
		return write_i8
	elif bsatn_type_str == &"i16":
		return write_i16_le
	elif bsatn_type_str == &"f64":
		return write_f64_le
	elif bsatn_type_str == &"vec_u8":
		return write_vec_u8
	elif bsatn_type_str == &"identity":
		return write_identity
	elif bsatn_type_str == &"connection_id":
		return write_connection_id
	elif bsatn_type_str == &"timestamp":
		return write_timestamp
	elif bsatn_type_str == &"scheduled_at":
		return write_scheduled_at
	elif bsatn_type_str == &"u128":
		return write_u128
	elif bsatn_type_str == &"i128":
		return write_i128
	elif bsatn_type_str == &"u256":
		return write_u256
	elif bsatn_type_str == &"i256":
		return write_i256
	return Callable()


func _get_writer_callable_for_property(prop: Dictionary, bsatn_type_str: StringName) -> Callable:
	var prop_name: StringName = prop.name
	var prop_type: Variant.Type = prop.type

	var writer_callable: Callable = Callable() # Initialize with invalid Callable

	# --- Special Cases First ---
	# Add other special cases here if needed (e.g., Option<T> fields if handled generically later)
	if prop.class_name == &"Option":
		writer_callable = write_option
	elif prop.class_name == &"RustEnum":
		writer_callable = write_rust_enum

	# --- Generic Type Handling (if not a special case) ---
	elif prop_type == TYPE_ARRAY:
		writer_callable = write_array
	elif NATIVE_ARRAYLIKE_TYPES.has(prop_type):
		writer_callable = write_native_arraylike
	else:
		# Handle non-array, non-special-case properties
		# 1. Check for primitive writer with BSATN type
		if not bsatn_type_str.is_empty():
			writer_callable = _get_primitive_writer_from_bsatn_type(bsatn_type_str)

		# 2. Fallback to default based on property's Variant.Type
		if not writer_callable.is_valid():
			if prop_type == TYPE_NIL:
				_set_error("Cannot serialize null argument.")
			elif prop_type == TYPE_BOOL:
				writer_callable = write_bool
			elif prop_type == TYPE_INT:
				if bsatn_type_str == &"u8":
					writer_callable = write_u8
				elif bsatn_type_str == &"u16":
					writer_callable = write_u16_le
				elif bsatn_type_str == &"u32":
					writer_callable = write_u32_le
				elif bsatn_type_str == &"u64":
					writer_callable = write_u64_le
				elif bsatn_type_str == &"i8":
					writer_callable = write_i8
				elif bsatn_type_str == &"i16":
					writer_callable = write_i16_le
				elif bsatn_type_str == &"i32":
					writer_callable = write_i32_le
				else:
					writer_callable = write_i64_le # Default i64
			elif prop_type == TYPE_FLOAT:
				if bsatn_type_str == &"f64":
					writer_callable = write_f64_le
				else:
					writer_callable = write_f32_le # Default f32
			elif prop_type == TYPE_STRING:
				writer_callable = write_string_with_u32_len
			elif prop_type == TYPE_PACKED_BYTE_ARRAY:
				writer_callable = write_vec_u8
			elif prop_type == TYPE_OBJECT:
				writer_callable = write_nested_resource
			# TYPE_ARRAY, and native array-like types (TYPE_VECTOR2, TYPE_QUATERNION, etc.) are handled above
			else:
				# Writer remains invalid for unsupported types
				pass

		if not writer_callable.is_valid() and not bsatn_type_str.is_empty() and debug_mode:
			push_warning("Unknown 'bsatn_type' metadata value: '%s' for property '%s'. No suitable writer found." % [bsatn_type_str, prop_name])

	return writer_callable


#Helper to generate a zero struct from a bsatn type
func _generate_default_type(bsatn_type_name: StringName) -> Variant:
	if bsatn_type_name == &"i8" or bsatn_type_name == &"i16" or bsatn_type_name == &"i32" or bsatn_type_name == &"i64" or bsatn_type_name == &"u8" or bsatn_type_name == &"u16" or bsatn_type_name == &"u32" or bsatn_type_name == &"u64":
		return int(0)
	elif bsatn_type_name == &"f32" or bsatn_type_name == &"f64":
		return float(0)
	elif bsatn_type_name == &"bool":
		return false
	elif bsatn_type_name == &"string":
		return ""
	elif bsatn_type_name == &"vector2":
		return Vector2.ZERO
	elif bsatn_type_name == &"vector2i":
		return Vector2i.ZERO
	elif bsatn_type_name == &"vector3":
		return Vector3.ZERO
	elif bsatn_type_name == &"vector3i":
		return Vector3i.ZERO
	elif bsatn_type_name == &"vector4":
		return Vector4.ZERO
	elif bsatn_type_name == &"vector4i":
		return Vector4i.ZERO
	elif bsatn_type_name == &"color":
		return Color.BLACK
	elif bsatn_type_name == &"quaternion":
		return Quaternion.IDENTITY
	else:
		return null


## Helper function to serialize a value based on BSATN type string.
## Assumes bsatn_type_str is already to_lower() if it's from metadata.
func _write_value_from_bsatn_type(value: Variant, bsatn_type_str: StringName, context_prop_name_for_prototype: StringName) -> bool:
	var value_type: int = typeof(value)

	# Vec<T> — recursive array serialization via prefix
	if bsatn_type_str.begins_with(&"vec_"):
		if value is not Array:
			_set_error("Expected Array for BSATN type '%s', got %s" % [bsatn_type_str, type_string(value_type)])
			return false
		var element_type: StringName = bsatn_type_str.substr(4)
		write_u32_le((value as Array).size())
		if has_error():
			return false
		for i: int in (value as Array).size():
			if not _write_value_from_bsatn_type(value[i], element_type, &"%s[%d]" % [context_prop_name_for_prototype, i]):
				return false
		return true

	# Option<T> — recursive option serialization via prefix
	if bsatn_type_str.begins_with(&"opt_"):
		if value is not Option:
			_set_error("Expected Option for BSATN type '%s', got %s" % [bsatn_type_str, type_string(value_type)])
			return false
		if (value as Option).is_none():
			write_u8(1)
			return not has_error()
		write_u8(0)
		if has_error():
			return false
		var inner_type: StringName = bsatn_type_str.substr(4)
		return _write_value_from_bsatn_type((value as Option).unwrap(), inner_type, &"%s[inner]" % context_prop_name_for_prototype)

	# 1. Try primitive writer (expects lowercase bsatn_type_str) if not an array
	if value_type != TYPE_ARRAY:
		var primitive_writer: Callable = _get_primitive_writer_from_bsatn_type(bsatn_type_str)
		if primitive_writer.is_valid():
			primitive_writer.call(value)
			if has_error():
				return false
			return true

	# 2. Create a temporary "prototype" dictionary for the value
	var value_class_name: String = _get_value_class_name(value)
	var prop_sim: Dictionary = {
		"name": context_prop_name_for_prototype,
		"type": value_type,
		"class_name": value_class_name,
		"usage": PROPERTY_USAGE_STORAGE,
		"hint": 0,
		"hint_string": "",
	}

	# 3. Determine from value type and bsatn type string
	var writer_callable: Callable = _get_writer_callable_for_property(prop_sim, bsatn_type_str)

	if not writer_callable.is_valid() and not has_error():
		_set_error("Unsupported BSATN type '%s' or missing writer for value '%s'" % [bsatn_type_str, prop_sim["class_name"]])

	if has_error():
		return false

	if writer_callable.get_method() in CONTEXT_WRITERS:
		writer_callable.call(value, bsatn_type_str, prop_sim)
	else:
		writer_callable.call(value)

	return not has_error()


## One field of a serialization plan. A typed record (not a Dictionary) so the
## per-field hot loop reads members directly instead of a hash lookup per field
## per resource — benchmarked ~1.27x on the write loop. Mirrors the deserializer's
## [code]_PlanStep[/code].
class _SerPlanStep:
	var writer: Callable
	var prop_name: StringName


func _create_serialization_plan(script: Script) -> Array:
	var bsatn_types: Dictionary = script.get_script_constant_map().get("BSATN_TYPES", { })
	var plan: Array[_SerPlanStep] = []
	var properties: Array = script.get_script_property_list()
	for prop: Dictionary in properties:
		if not (prop["usage"] & PROPERTY_USAGE_STORAGE):
			continue

		var prop_name: StringName = prop["name"]
		var bsatn_type_str: StringName = bsatn_types.get(prop_name, &"").to_lower()

		var writer_callable: Callable = _get_writer_callable_for_property(prop, bsatn_type_str)

		if not writer_callable.is_valid():
			_set_error("Unsupported property or missing writer for '%s' in script '%s'" % [prop_name, script.resource_path])
			_serialization_plan_cache[script] = []
			return []

		# Pre-bind static context args for writers that need them, so the hot loop
		# can call every writer uniformly with just the runtime value.
		var bound_writer: Callable
		if writer_callable.get_method() in CONTEXT_WRITERS:
			bound_writer = writer_callable.bind(bsatn_type_str, prop)
		else:
			bound_writer = writer_callable

		var step: _SerPlanStep = _SerPlanStep.new()
		step.writer = bound_writer
		step.prop_name = prop_name
		plan.append(step)

	_serialization_plan_cache[script] = plan
	return plan


# Serializes the fields of a Resource instance sequentially.
func _serialize_resource_fields(resource: Object) -> bool:
	if resource == null:
		_set_error("Cannot serialize fields of null or scriptless resource")
		return false
	var script: Script = resource.get_script()
	if not script:
		_set_error("Cannot serialize fields of null or scriptless resource")
		return false

	if resource is RustEnum:
		write_rust_enum(resource)
		return true

	# Default to [] (not null): .get(script) on a cache miss returns null, which
	# crashes when assigned to a typed Array. Mirror the deserializer — distinguish
	# "missing" (build the plan) from "cached empty" (no storage fields) via has().
	var plan: Array = _serialization_plan_cache.get(script, [])
	if plan.is_empty() and not _serialization_plan_cache.has(script):
		plan = _create_serialization_plan(script)
		if has_error():
			return false

	for step: _SerPlanStep in plan:
		var instr_name: StringName = step.prop_name
		step.writer.call(resource.get(instr_name))
		if has_error():
			if not _last_error.contains(str(instr_name)):
				var existing_error: String = get_last_error()
				_set_error("Failed writing '%s' in '%s'. Cause: %s" % [instr_name, resource.get_script().get_global_name(), existing_error])
			return false

	return true # All fields serialized successfully


# --- Argument Serialization Helpers ---
## Serializes an array of arguments into a single PackedByteArray block.
func _serialize_arguments(args_array: Array, bsatn_types: Array) -> PackedByteArray:
	var args_spb: StreamPeerBuffer = StreamPeerBuffer.new()
	args_spb.big_endian = false
	var original_main_spb: StreamPeerBuffer = _spb
	_spb = args_spb # Temporarily redirect writes

	for i: int in args_array.size():
		var arg_value: Variant = args_array[i]
		var bsatn_type: StringName = bsatn_types[i] if i < bsatn_types.size() else &""

		if not _write_argument_value(arg_value, bsatn_type, "arg[%d]" % i):
			_spb = original_main_spb
			return PackedByteArray()

	_spb = original_main_spb # Restore main buffer
	return args_spb.data_array if not has_error() else PackedByteArray()


## Helper to write a single *argument* value.
func _write_argument_value(value: Variant, bsatn_type: StringName = &"", context_prop_name_for_error: StringName = &"") -> bool:
	# 1. Create a temporary "prototype" dictionary for the argument
	var value_type: int = typeof(value)
	var value_class_name: String = _get_value_class_name(value)
	var prop_sim: Dictionary = {
		"name": context_prop_name_for_error,
		"type": value_type,
		"class_name": value_class_name,
		"usage": PROPERTY_USAGE_STORAGE,
		"hint": 0,
		"hint_string": "",
	}

	var writer_callable: Callable = _get_writer_callable_for_property(prop_sim, bsatn_type)

	if not writer_callable.is_valid() and not has_error():
		_set_error("Unsupported argument type '%s' or missing writer for '%s' with 'bsatn_type' metadata ('%s')" % [prop_sim["class_name"], prop_sim["name"], bsatn_type])

	if has_error():
		return false

	if writer_callable.get_method() in CONTEXT_WRITERS:
		writer_callable.call(value, bsatn_type, prop_sim)
	else:
		writer_callable.call(value)

	return not has_error()
