## Decodes BSATN binary data from SpacetimeDB server messages into GDScript values.
##
## Used internally by [SpacetimeDBClient] to parse raw WebSocket packets into
## typed [SpacetimeDBServerMessage] subclasses. Provides low-level primitive
## readers ([method read_u8], [method read_i32], [method read_string], etc.)
## and a plan-based resource deserializer that populates a [Resource]'s exported
## properties from a byte stream.
##
## Check [method has_error] after any deserialization call; if [code]true[/code],
## retrieve the message via [method get_last_error].
class_name BSATNDeserializer
extends RefCounted

const MAX_STRING_LEN: int = 4 * 1024 * 1024 # 4 MiB
const MAX_VEC_LEN: int = 131072
const MAX_BYTE_ARRAY_LEN: int = 16 * 1024 * 1024 # 16 MiB
const IDENTITY_SIZE: int = 32
const CONNECTION_ID_SIZE: int = 16
const U128_SIZE: int = 16
const I128_SIZE: int = 16
const U256_SIZE: int = 32
const I256_SIZE: int = 32
const ROW_LIST_FIXED_SIZE: int = 0
const ROW_LIST_ROW_OFFSETS: int = 1
const NATIVE_ARRAYLIKE: Array[Variant.Type] = [
	TYPE_VECTOR2,
	TYPE_VECTOR2I,
	TYPE_VECTOR3,
	TYPE_VECTOR3I,
	TYPE_VECTOR4,
	TYPE_VECTOR4I,
	TYPE_QUATERNION,
	TYPE_COLOR,
]

var debug_mode: bool = false
## Parse outcome, single source of truth. [constant ParseStatus.OK] = clean;
## [constant ParseStatus.ERROR] = malformed/unrecoverable (framing loop drops the
## buffer); [constant ParseStatus.NEEDS_MORE] = a read ran past the buffer end and
## the rest may arrive in a later packet (framing loop keeps the tail). NEEDS_MORE
## is a recoverable subtype of error — [method has_error] is true for both non-OK
## states. Lets the framing loop distinguish "wait for more data" from a fatal
## parse error without matching on the error message text.
enum ParseStatus { OK, ERROR, NEEDS_MORE }
var _status: ParseStatus = ParseStatus.OK
var _last_error: String = ""
var _deserialization_plan_cache: Dictionary[Script, Array] = { }
var _pending_data: PackedByteArray = []
var _schema: SpacetimeDBSchema
var _native_arraylike_regex: RegEx = RegEx.new()
var _normalized_name_cache: Dictionary[StringName, StringName] = { }


func _init(p_schema: SpacetimeDBSchema, p_debug_mode: bool = false) -> void:
	debug_mode = p_debug_mode
	_schema = p_schema
	_native_arraylike_regex.compile("^(?<struct>.+)\\[(?<components>.*)\\]$")


func _normalize(name: StringName) -> StringName:
	var cached: StringName = _normalized_name_cache.get(name, &"")
	if not cached.is_empty():
		return cached
	var normalized: StringName = name.to_lower().replace("_", "")
	_normalized_name_cache[name] = normalized
	return normalized

#--- Error Handling ---


## Returns [code]true[/code] if the last deserialization operation failed.
func has_error() -> bool:
	return _status != ParseStatus.OK


## Returns and clears the last error message. Resets [method has_error] to [code]false[/code].
func get_last_error() -> String:
	var err: String = _last_error
	_last_error = ""
	_status = ParseStatus.OK
	return err


## Clears the error state without returning the message.
func clear_error() -> void:
	_last_error = ""
	_status = ParseStatus.OK


#--- Primitive Readers ---
func read_i8(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 1 > spb.get_size():
		return _read_underflow_int(spb, 1)
	return spb.get_8()


func read_i16_le(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 2 > spb.get_size():
		return _read_underflow_int(spb, 2)
	return spb.get_16()


func read_i32_le(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 4 > spb.get_size():
		return _read_underflow_int(spb, 4)
	return spb.get_32()


func read_i64_le(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 8 > spb.get_size():
		return _read_underflow_int(spb, 8)
	return spb.get_64()


func read_u8(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 1 > spb.get_size():
		return _read_underflow_int(spb, 1)
	return spb.get_u8()


func read_u16_le(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 2 > spb.get_size():
		return _read_underflow_int(spb, 2)
	return spb.get_u16()


func read_u32_le(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 4 > spb.get_size():
		return _read_underflow_int(spb, 4)
	return spb.get_u32()


func read_u64_le(spb: StreamPeerBuffer) -> int:
	if _status != ParseStatus.OK:
		return 0
	if spb.get_position() + 8 > spb.get_size():
		return _read_underflow_int(spb, 8)
	return spb.get_u64()


func read_u128(spb: StreamPeerBuffer) -> PackedByteArray:
	var num: PackedByteArray = read_bytes(spb, U128_SIZE)
	num.reverse() # We receive the bytes in reverse
	return num


func read_i128(spb: StreamPeerBuffer) -> PackedByteArray:
	var num: PackedByteArray = read_bytes(spb, I128_SIZE)
	num.reverse() # LE on the wire; reverse to canonical order (matches read_u128)
	return num


func read_u256(spb: StreamPeerBuffer) -> PackedByteArray:
	var num: PackedByteArray = read_bytes(spb, U256_SIZE)
	num.reverse() # LE on the wire; reverse to canonical order
	return num


func read_i256(spb: StreamPeerBuffer) -> PackedByteArray:
	var num: PackedByteArray = read_bytes(spb, I256_SIZE)
	num.reverse() # LE on the wire; reverse to canonical order
	return num


func read_f32_le(spb: StreamPeerBuffer) -> float:
	if _status != ParseStatus.OK:
		return 0.0
	if spb.get_position() + 4 > spb.get_size():
		_read_underflow_int(spb, 4)
		return 0.0
	return spb.get_float()


func read_f64_le(spb: StreamPeerBuffer) -> float:
	if _status != ParseStatus.OK:
		return 0.0
	if spb.get_position() + 8 > spb.get_size():
		_read_underflow_int(spb, 8)
		return 0.0
	return spb.get_double()


func read_bool(spb: StreamPeerBuffer) -> bool:
	var byte: int = read_u8(spb)
	if has_error():
		return false
	if byte != 0 and byte != 1:
		_set_error("Invalid boolean value: %d (expected 0 or 1)" % byte, spb.get_position() - 1)
		return false
	return byte == 1


func read_bytes(spb: StreamPeerBuffer, num_bytes: int) -> PackedByteArray:
	if num_bytes < 0:
		_set_error("Attempted to read negative bytes: %d" % num_bytes, spb.get_position())
		return PackedByteArray()
	if num_bytes == 0 or not _check_read(spb, num_bytes):
		return PackedByteArray()
	var result: Array = spb.get_data(num_bytes)
	if result[0] != OK:
		_set_error("StreamPeerBuffer.get_data failed: %d" % result[0], spb.get_position() - num_bytes)
		return PackedByteArray()
	return result[1]


func read_string_with_u32_len(spb: StreamPeerBuffer) -> String:
	var start_pos: int = spb.get_position()
	var length: int = read_u32_le(spb)
	if has_error() or length == 0:
		return ""
	if length > MAX_STRING_LEN:
		_set_error("String length %d exceeds limit %d" % [length, MAX_STRING_LEN], start_pos)
		return ""
	var str_bytes: PackedByteArray = read_bytes(spb, length)
	if has_error():
		return ""
	var str_result: String = str_bytes.get_string_from_utf8()
	# Empty result for non-empty bytes that also fail ASCII decode = malformed UTF-8.
	# (A legitimate embedded NUL is valid UTF-8 and is intentionally allowed.)
	if str_result.is_empty() and length > 0 and str_bytes.get_string_from_ascii().is_empty():
		_set_error("Failed to decode UTF-8 string length %d" % length, start_pos)
		return ""
	return str_result


func read_identity(spb: StreamPeerBuffer) -> PackedByteArray:
	var identity: PackedByteArray = read_bytes(spb, IDENTITY_SIZE)
	identity.reverse() # We receive the identity bytes in reverse
	return identity


func read_connection_id(spb: StreamPeerBuffer) -> PackedByteArray:
	var connection_id: PackedByteArray = read_bytes(spb, CONNECTION_ID_SIZE)
	connection_id.reverse() # LE on the wire; reverse to canonical order (matches read_identity + write_connection_id)
	return connection_id


func read_timestamp(spb: StreamPeerBuffer) -> int:
	return read_i64_le(spb)


## ScheduleAt sum: u8 tag (0=Interval, 1=Time) then the i64 microsecond payload.
func read_scheduled_at(spb: StreamPeerBuffer) -> ScheduleAt:
	var result: ScheduleAt = ScheduleAt.new()
	var tag: int = read_u8(spb)
	if has_error():
		return result
	if tag != ScheduleAt.Kind.INTERVAL and tag != ScheduleAt.Kind.TIME:
		_set_error("Invalid ScheduleAt tag %d" % tag, spb.get_position() - 1)
		return result
	result.kind = tag
	result.micros = read_i64_le(spb)
	return result


func read_query_id_data(spb: StreamPeerBuffer) -> QueryIdData:
	var query_id_data: QueryIdData = QueryIdData.new()
	query_id_data.id = read_u32_le(spb)
	return query_id_data


func read_vec_u8(spb: StreamPeerBuffer) -> PackedByteArray:
	var start_pos: int = spb.get_position()
	var length: int = read_u32_le(spb)
	if has_error() or length == 0:
		return PackedByteArray()
	if length > MAX_BYTE_ARRAY_LEN:
		_set_error("Vec<u8> length %d exceeds limit %d" % [length, MAX_BYTE_ARRAY_LEN], start_pos)
		return PackedByteArray()
	return read_bytes(spb, length)


#--- BsatnRowList Reader ---
## Reads the BsatnRowList structure (size hint, per-row offsets, data length)
## and leaves [param spb] positioned at the start of the row data block. Both row
## encodings are normalized to a header so callers can either slice rows out or
## parse them in place: returns [code]{offsets, count, data_len}[/code] where
## [code]offsets[/code] has count+1 entries with a trailing end sentinel
## (== data_len). Returns [code]{}[/code] and sets the error state on a malformed
## list (the caller checks [method has_error]).
func _read_row_block_header(spb: StreamPeerBuffer) -> Dictionary:
	var start_pos: int = spb.get_position()
	var size_hint_type: int = read_u8(spb)
	if has_error():
		return { }
	var offsets: PackedInt64Array = PackedInt64Array()

	if size_hint_type == ROW_LIST_FIXED_SIZE:
		var row_size: int = read_u16_le(spb)
		var data_len: int = read_u32_le(spb)
		if has_error():
			return { }
		if row_size == 0:
			if data_len != 0:
				_set_error("FixedSize row_size is 0 but data_len is %d" % data_len, start_pos)
				return { }
			return { "offsets": PackedInt64Array([0]), "count": 0, "data_len": 0 }
		if data_len % row_size != 0:
			_set_error("FixedSize data_len %d not divisible by row_size %d" % [data_len, row_size], start_pos)
			return { }
		var num_rows: int = data_len / row_size
		if num_rows > MAX_VEC_LEN:
			# Guard the resize below — data_len/row_size is an attacker-influenced u32
			# ratio; an unchecked huge count would allocate gigabytes before any read.
			_set_error("FixedSize row count %d exceeds limit %d" % [num_rows, MAX_VEC_LEN], start_pos)
			return { }
		offsets.resize(num_rows + 1)
		for i: int in num_rows + 1:
			offsets[i] = i * row_size
		return { "offsets": offsets, "count": num_rows, "data_len": data_len }
	elif size_hint_type == ROW_LIST_ROW_OFFSETS:
		var num_offsets: int = read_u32_le(spb)
		if has_error():
			return { }
		if num_offsets > MAX_VEC_LEN:
			# Guard the resize below — num_offsets is a raw u32 off the wire; an
			# unchecked value near u32_max would allocate ~32 GiB before reading.
			_set_error("RowOffsets count %d exceeds limit %d" % [num_offsets, MAX_VEC_LEN], start_pos)
			return { }
		offsets.resize(num_offsets + 1)
		for i: int in num_offsets:
			offsets[i] = read_u64_le(spb)
			if has_error():
				return { }
		var data_len: int = read_u32_le(spb)
		if has_error():
			return { }
		offsets[num_offsets] = data_len
		for i: int in num_offsets:
			var start_offset: int = offsets[i]
			var end_offset: int = offsets[i + 1]
			if start_offset < 0 or end_offset < start_offset or end_offset > data_len:
				_set_error(
					"Invalid row offsets: start=%d, end=%d, data_len=%d, row=%d" % [start_offset, end_offset, data_len, i],
					start_pos,
				)
				return { }
		return { "offsets": offsets, "count": num_offsets, "data_len": data_len }
	else:
		_set_error("Unknown RowSizeHint type: %d" % size_hint_type, start_pos)
		return { }

	return { }


## Reads a BsatnRowList into raw byte slices, one per row. Prefer
## [method _read_bsatn_row_list_as_resources] on the hot path — it parses rows in
## place without these per-row copies; this stays for callers that only skip past
## a row list (unknown schema).
func read_bsatn_row_list(spb: StreamPeerBuffer) -> Array[PackedByteArray]:
	var header: Dictionary = _read_row_block_header(spb)
	if has_error():
		return []
	var offsets: PackedInt64Array = header["offsets"]
	var count: int = header["count"]
	var data_len: int = header["data_len"]
	var data: PackedByteArray = read_bytes(spb, data_len)
	if has_error():
		return []
	var rows: Array[PackedByteArray] = []
	rows.resize(count)
	for i: int in count:
		rows[i] = data.slice(offsets[i], offsets[i + 1])
	return rows


## Appends [param new_data] to the internal buffer and extracts all complete
## [SpacetimeDBServerMessage] instances. Returns an array of parsed messages.
func process_bytes_and_extract_messages(new_data: PackedByteArray) -> Array[SpacetimeDBServerMessage]:
	if new_data.is_empty():
		return []
	_pending_data.append_array(new_data)
	var parsed_messages: Array[SpacetimeDBServerMessage] = []
	var spb: StreamPeerBuffer = StreamPeerBuffer.new()
	spb.big_endian = false # BSATN is little-endian; set once so per-read setters drop.
	# Parse against a single snapshot, advancing a cursor, and slice the
	# consumed prefix off _pending_data exactly once after the loop — instead of
	# rebuilding the buffer (O(n)) after every message.
	spb.data_array = _pending_data
	var buffer_size: int = _pending_data.size()
	var cursor: int = 0
	while cursor < buffer_size:
		clear_error()
		spb.seek(cursor)
		var message: SpacetimeDBServerMessage = _parse_message_from_stream(spb)

		if _status != ParseStatus.OK:
			if _status == ParseStatus.NEEDS_MORE:
				# Incomplete trailing message — keep it for the next packet.
				clear_error()
				break
			# Malformed data: drop the whole buffer to avoid an infinite loop.
			printerr("BSATNDeserializer: Unrecoverable parsing error: %s. Clearing buffer." % get_last_error())
			_pending_data.clear()
			return parsed_messages

		if message == null:
			break

		var bytes_consumed: int = spb.get_position() - cursor
		if bytes_consumed <= 0:
			printerr("BSATNDeserializer: Parser consumed 0 bytes. Clearing buffer to prevent infinite loop.")
			_pending_data.clear()
			return parsed_messages

		parsed_messages.append(message)
		cursor = spb.get_position()

	# Drop the consumed prefix once; any incomplete trailing message is retained.
	if cursor > 0:
		_pending_data = _pending_data.slice(cursor)

	return parsed_messages


func _set_error(msg: String, position: int = -1, status: ParseStatus = ParseStatus.ERROR) -> void:
	if _status != ParseStatus.OK:
		return
	var pos_str: String = " (at approx. position %d)" % position if position >= 0 else ""
	_last_error = "BSATNDeserializer Error: %s%s" % [msg, pos_str]
	_status = status
	printerr(_last_error)


# Slow path for inlined readers: sets the underflow error and returns 0. Kept out of
# the hot readers so their happy path is just the bounds compare + the native get_*.
func _read_underflow_int(spb: StreamPeerBuffer, bytes_needed: int) -> int:
	_set_error(
		"Attempted to read %d bytes past end of buffer (size: %d)." % [bytes_needed, spb.get_size()],
		spb.get_position(),
		ParseStatus.NEEDS_MORE,
	)
	return 0


func _check_read(spb: StreamPeerBuffer, bytes_needed: int) -> bool:
	if _status != ParseStatus.OK: # inlined has_error()
		return false
	if spb.get_position() + bytes_needed > spb.get_size():
		_set_error(
			"Attempted to read %d bytes past end of buffer (size: %d)." % [bytes_needed, spb.get_size()],
			spb.get_position(),
			ParseStatus.NEEDS_MORE,
		)
		return false
	return true


# --- Complex Property Readers ---
func _read_option(
		spb: StreamPeerBuffer,
		option_prop_dict: Dictionary,
		inner_type: StringName,
) -> Option:
	var option_instance: Option = Option.new()
	var prop_name: StringName = option_prop_dict.name
	var tag_pos: int = spb.get_position()
	var tag: int = read_u8(spb)
	if has_error():
		return null

	if tag == 1: # None
		option_instance.set_none()
		return option_instance

	if tag != 0:
		_set_error("Invalid Option tag %d for property '%s' (expected 0=Some, 1=None)" % [tag, prop_name], tag_pos)
		return null

	if inner_type.is_empty():
		_set_error("Missing BSATN_TYPES entry for Option property '%s'" % prop_name, tag_pos)
		return null

	var inner_value: Variant = _read_value_from_bsatn_type(spb, inner_type, prop_name)

	if has_error():
		if not _last_error.contains(str(prop_name)):
			# Preserve NEEDS_MORE through the wrap so an incomplete trailing
			# message isn't reclassified as a fatal ERROR (buffer would be dropped).
			var inner_status: ParseStatus = _status
			var cause: String = get_last_error()
			_set_error("Failed reading Some value for Option '%s' (inner type '%s'). Cause: %s" % [prop_name, inner_type, cause], tag_pos + 1, inner_status)
		return null

	option_instance.set_some(inner_value)
	return option_instance


func _read_array(spb: StreamPeerBuffer, prop: Dictionary, bsatn_type_str: StringName) -> Array:
	var prop_name: StringName = prop.name
	var start_pos: int = spb.get_position()
	var length: int = read_u32_le(spb)
	if has_error() or length == 0:
		return []
	elif length > MAX_VEC_LEN:
		_set_error("Array length %d exceeds limit %d for property '%s'" % [length, MAX_VEC_LEN, prop_name], start_pos)
		return []

	# Determine element type from hint_string
	var hint: int = prop.hint
	var hint_string: String = prop.hint_string
	var element_type_code: Variant.Type = TYPE_MAX
	var element_class_name: StringName = &""

	if hint == PROPERTY_HINT_TYPE_STRING and ":" in hint_string:
		var parts: PackedStringArray = hint_string.split(":", true, 1)
		if parts.size() == 2:
			element_type_code = int(parts[0])
			element_class_name = parts[1]
		else:
			_set_error("Array '%s': bad hint_string format '%s'" % [prop_name, hint_string], start_pos)
			return []
	elif hint == PROPERTY_HINT_ARRAY_TYPE:
		var main_type_str: String = hint_string.split(":", true, 1)[0]
		if "/" in main_type_str:
			var parts: PackedStringArray = main_type_str.split("/", true, 1)
			element_type_code = int(parts[0])
			element_class_name = parts[1]
		else:
			element_type_code = int(main_type_str)
	else:
		_set_error("Array '%s' needs a typed hint (hint=%d, hint_string='%s')" % [prop_name, hint, hint_string], start_pos)
		return []

	if element_type_code == TYPE_MAX:
		_set_error("Could not determine element type for array '%s'" % prop_name, start_pos)
		return []

	var element_prop_sim: Dictionary = {
		"name": prop_name,
		"type": element_type_code,
		"class_name": element_class_name,
		"usage": PROPERTY_USAGE_STORAGE,
		"hint": 0,
		"hint_string": "",
	}

	# Resolve element reader from pre-bound bsatn_type_str
	var element_reader: Callable
	if bsatn_type_str.begins_with(&"opt_") or bsatn_type_str.begins_with(&"vec_"):
		# Prefixed type — use recursive type-driven deserialization for deep nesting
		element_reader = _read_value_from_bsatn_type.bind(bsatn_type_str, prop_name)
	elif element_class_name == &"Option":
		if bsatn_type_str.is_empty():
			_set_error("Array '%s' of Options is missing BSATN_TYPES entry for inner type T" % prop_name, start_pos)
			return []
		element_reader = _read_option.bind(element_prop_sim, bsatn_type_str)
	else:
		if not bsatn_type_str.is_empty():
			element_reader = _get_primitive_reader_from_bsatn_type(bsatn_type_str)
			if not element_reader.is_valid() and _schema.types.has(bsatn_type_str):
				element_reader = _read_nested_resource.bind(element_prop_sim)
		if not element_reader.is_valid():
			element_reader = _get_reader_callable_for_property(element_prop_sim, &"")

	if not element_reader.is_valid():
		_set_error(
			"Cannot determine reader for elements of array '%s' (type code %d, class '%s')" % [prop_name, element_type_code, element_class_name],
			start_pos,
		)
		return []

	var result: Array[Variant] = []
	result.resize(length)
	for i: int in length:
		if has_error():
			return []
		var element_start_pos: int = spb.get_position()
		var element_value: Variant = element_reader.call(spb)
		if has_error():
			if not _last_error.contains("element %d" % i) and not _last_error.contains(str(prop_name)):
				var inner_status: ParseStatus = _status
				var cause: String = get_last_error()
				_set_error("Failed reading element %d for array '%s'. Cause: %s" % [i, prop_name, cause], element_start_pos, inner_status)
			return []
		result[i] = element_value
	return result


func _read_native_arraylike(spb: StreamPeerBuffer, prop: Dictionary, bsatn_type_str: StringName) -> Variant:
	var prop_name: StringName = prop.name
	var start_pos: int = spb.get_position()

	if bsatn_type_str.is_empty():
		_set_error("Missing BSATN_TYPES entry for '%s' (type %s)" % [prop_name, type_string(prop.type)], start_pos)
		return null

	var result: RegExMatch = _native_arraylike_regex.search(bsatn_type_str)
	var components_str: String = result.get_string("components") if result else ""
	if components_str.is_empty():
		_set_error("Missing component types in 'bsatn_type' for '%s'" % prop_name, start_pos)
		return null

	var components: Array[Variant] = []
	for component_type: StringName in components_str.split(","):
		components.append(_read_value_from_bsatn_type(spb, component_type, prop_name))

	# if-elif, not match: this runs per native-vector field per row. A GDScript match
	# arm costs ~10 opcodes (typeof check + value compare + bool materialization +
	# branch) vs ~2 for an if branch (validated == + jump). Float vectors first (most
	# common: positions/colors), int variants last.
	var t: int = prop.type
	if t == TYPE_VECTOR2:
		return Vector2.ZERO if has_error() else Vector2(components[0], components[1])
	elif t == TYPE_VECTOR3:
		return Vector3.ZERO if has_error() else Vector3(components[0], components[1], components[2])
	elif t == TYPE_VECTOR4:
		return Vector4.ZERO if has_error() else Vector4(components[0], components[1], components[2], components[3])
	elif t == TYPE_COLOR:
		return Color.BLACK if has_error() else Color(components[0], components[1], components[2], components[3])
	elif t == TYPE_QUATERNION:
		return Quaternion.IDENTITY if has_error() else Quaternion(components[0], components[1], components[2], components[3])
	elif t == TYPE_VECTOR2I:
		return Vector2i.ZERO if has_error() else Vector2i(components[0], components[1])
	elif t == TYPE_VECTOR3I:
		return Vector3i.ZERO if has_error() else Vector3i(components[0], components[1], components[2])
	elif t == TYPE_VECTOR4I:
		return Vector4i.ZERO if has_error() else Vector4i(components[0], components[1], components[2], components[3])

	_set_error("Unsupported native arraylike type for property '%s'" % prop_name, start_pos)
	return null


func _read_nested_resource(spb: StreamPeerBuffer, prop: Dictionary) -> Object:
	var prop_name: StringName = prop.name
	var nested_class_name: StringName = prop.class_name

	if nested_class_name.is_empty():
		_set_error(
			"Property '%s' is TYPE_OBJECT but has no class_name hint" % prop_name,
			spb.get_position(),
		)
		return null

	var key: StringName = _normalize(nested_class_name)
	var script: GDScript = _schema.get_type(key)
	var nested_instance: Object

	if script:
		nested_instance = script.new()
	elif ClassDB.can_instantiate(nested_class_name):
		nested_instance = ClassDB.instantiate(nested_class_name)
		if not nested_instance is RefCounted: # Resource extends RefCounted
			_set_error("ClassDB instantiated '%s' for '%s' but it is not a RefCounted" % [nested_class_name, prop_name], spb.get_position())
			return null
	else:
		_set_error("Could not find or instantiate class '%s' for property '%s'" % [nested_class_name, prop_name], spb.get_position())
		return null

	if not _populate_resource_from_bytes(nested_instance, spb):
		if not has_error():
			_set_error("Failed to populate nested resource '%s' of type '%s'" % [prop_name, nested_class_name], spb.get_position())
		return null

	return nested_instance


# Returns the schema GDScript for a property the row loop can parse via a hoisted
# plan, or null when the bound reader must run as-is.
#
# Gate 1 (authoritative): only hoist when _get_reader_callable_for_property actually
# bound _read_nested_resource. This excludes every schema type with a *custom*
# reader — ScheduleAt (sum: u8 tag + i64 via read_scheduled_at), Identity, etc. —
# whose product-of-fields plan would misread the wire. Re-deriving that exclusion
# by hand is fragile; trust the reader the plan already chose.
#
# Gate 2: a RustEnum *field* also binds _read_nested_resource, but _read_nested_resource
# routes it through _populate_resource_from_bytes' `is RustEnum` tag dispatch — the
# hoisted path calls _populate_from_plan directly and would skip that. Exclude via the
# ENUM_OPTIONS constant codegen emits on every concrete RustEnum (own constant, so a
# hand-rolled RustEnum subclass omitting it is the only escape — codegen is the sole
# source today).
func _hoistable_nested_script(prop: Dictionary, reader_callable: Callable) -> GDScript:
	if reader_callable.get_method() != &"_read_nested_resource":
		return null
	if _schema == null or prop.type != TYPE_OBJECT or prop.class_name.is_empty() or prop.class_name == &"Option":
		return null
	var script: GDScript = _schema.get_type(_normalize(prop.class_name))
	if script == null:
		return null
	if script.get_script_constant_map().has(&"ENUM_OPTIONS"):
		return null
	return script


# Hoisted nested-resource read: instantiate the pre-resolved script + run its plan
# directly, skipping the per-row schema get_type + plan-cache hashes of
# _read_nested_resource. The nested plan is built once on first use (lazy avoids any
# plan-build recursion on self-referential schemas).
func _read_nested_hoisted(spb: StreamPeerBuffer, step: _PlanStep) -> Object:
	if not step.nested_plan_ready:
		step.nested_plan = _get_or_build_plan(step.nested_script)
		if has_error():
			return null # leave nested_plan_ready false so a retry rebuilds after clear
		step.nested_plan_ready = true
	var nested_instance: Object = step.nested_script.new()
	if not _populate_from_plan(nested_instance, spb, step.nested_plan):
		if not has_error():
			_set_error(
				"Failed to populate nested resource '%s' of type '%s'" % [step.prop_name, step.nested_script.get_global_name()],
				spb.get_position(),
			)
		return null
	return nested_instance


# --- Generic Deserialization ---
func _get_primitive_reader_from_bsatn_type(bsatn_type_str: StringName) -> Callable:
	# if-elif, not match: reached per element on the recursive vec/option/nested path
	# (_read_value_from_bsatn_type) plus at plan-build. A GDScript match arm costs ~10
	# opcodes (typeof + value compare + bool materialization + branch) vs ~2 for an if
	# branch. Ordered by expected field frequency so common types short-circuit early.
	if bsatn_type_str == &"u32":
		return read_u32_le
	elif bsatn_type_str == &"i32":
		return read_i32_le
	elif bsatn_type_str == &"u64":
		return read_u64_le
	elif bsatn_type_str == &"i64":
		return read_i64_le
	elif bsatn_type_str == &"f32":
		return read_f32_le
	elif bsatn_type_str == &"bool":
		return read_bool
	elif bsatn_type_str == &"string":
		return read_string_with_u32_len
	elif bsatn_type_str == &"u8":
		return read_u8
	elif bsatn_type_str == &"u16":
		return read_u16_le
	elif bsatn_type_str == &"i8":
		return read_i8
	elif bsatn_type_str == &"i16":
		return read_i16_le
	elif bsatn_type_str == &"f64":
		return read_f64_le
	elif bsatn_type_str == &"vec_u8":
		return read_vec_u8
	elif bsatn_type_str == &"identity":
		return read_identity
	elif bsatn_type_str == &"connection_id":
		return read_connection_id
	elif bsatn_type_str == &"timestamp":
		return read_timestamp
	elif bsatn_type_str == &"scheduled_at":
		return read_scheduled_at
	elif bsatn_type_str == &"u128":
		return read_u128
	elif bsatn_type_str == &"i128":
		return read_i128
	elif bsatn_type_str == &"u256":
		return read_u256
	elif bsatn_type_str == &"i256":
		return read_i256
	elif bsatn_type_str == &"transactionupdatemessage":
		return _read_transaction_update_message
	return Callable()


func _get_reader_callable_for_property(prop: Dictionary, bsatn_type_str: StringName) -> Callable:
	var prop_type: Variant.Type = prop.type

	if prop.class_name == &"Option":
		return _read_option.bind(prop, bsatn_type_str)
	elif prop_type == TYPE_ARRAY:
		return _read_array.bind(prop, bsatn_type_str)
	elif NATIVE_ARRAYLIKE.has(prop_type):
		return _read_native_arraylike.bind(prop, bsatn_type_str)
	else:
		var reader: Callable = Callable()
		if not bsatn_type_str.is_empty():
			reader = _get_primitive_reader_from_bsatn_type(bsatn_type_str)
			if not reader.is_valid() and _schema.types.has(_normalize(bsatn_type_str)):
				reader = _read_nested_resource.bind(prop)
			elif not reader.is_valid() and debug_mode:
				push_warning("Unknown BSATN_TYPES entry '%s' for property '%s'. Falling back to Variant.Type." % [bsatn_type_str, prop.name])
		if not reader.is_valid():
			if prop_type == TYPE_BOOL:
				reader = read_bool
			elif prop_type == TYPE_INT:
				reader = read_i64_le
			elif prop_type == TYPE_FLOAT:
				reader = read_f32_le
			elif prop_type == TYPE_STRING:
				reader = read_string_with_u32_len
			elif prop_type == TYPE_PACKED_BYTE_ARRAY:
				reader = read_vec_u8
			elif prop_type == TYPE_OBJECT:
				reader = _read_nested_resource.bind(prop)
		return reader


func _read_value_from_bsatn_type(spb: StreamPeerBuffer, bsatn_type_str: StringName, context_prop_name: StringName) -> Variant:
	var start_pos: int = spb.get_position()

	# Primitive types
	var primitive_reader: Callable = _get_primitive_reader_from_bsatn_type(bsatn_type_str)
	if primitive_reader.is_valid():
		var value: Variant = primitive_reader.call(spb)
		return null if has_error() else value

	# Vec<T>
	if bsatn_type_str.begins_with("vec_"):
		var element_type: StringName = bsatn_type_str.right(-4)
		var array_length: int = read_u32_le(spb)
		if has_error():
			return null
		if array_length == 0:
			return []
		if array_length > MAX_VEC_LEN:
			_set_error(
				"Array length %d for '%s' exceeds limit %d (context: '%s')" % [array_length, bsatn_type_str, MAX_VEC_LEN, context_prop_name],
				spb.get_position() - 4,
			)
			return null
		var temp_array: Array[Variant] = []
		for i: int in array_length:
			if has_error():
				return null
			var element: Variant = _read_value_from_bsatn_type(spb, element_type, "%s[%d]" % [context_prop_name, i])
			if has_error():
				return null
			temp_array.append(element)
		return temp_array

	# Option<T>
	if bsatn_type_str.begins_with("opt_"):
		return _read_option(spb, { "name": context_prop_name }, bsatn_type_str.right(-4))

	# Custom Resource (schema type)
	var schema_key: StringName = bsatn_type_str.replace("_", "")
	if _schema.types.has(schema_key):
		var script: GDScript = _schema.get_type(schema_key)
		if script and script.can_instantiate():
			var nested_instance: Object = script.new()
			if not _populate_resource_from_bytes(nested_instance, spb):
				if not has_error():
					_set_error("Failed to populate nested resource of type '%s' (schema key '%s') for context '%s'" % [bsatn_type_str, schema_key, context_prop_name], start_pos)
				return null
			return nested_instance
		else:
			_set_error("Cannot instantiate schema for BSATN type '%s' (schema key '%s', context: '%s'). Script valid: %s, Can instantiate: %s" % [bsatn_type_str, schema_key, context_prop_name, script != null, script.can_instantiate() if script else "N/A"], start_pos)
			return null

	_set_error("Unsupported BSATN type '%s' for deserialization (context: '%s'). No primitive, vec, or custom schema found." % [bsatn_type_str, context_prop_name], start_pos)
	return null

## Per-field dispatch code. Fixed-width primitives get a code so the row loop reads
## them inline (no Callable.call, no reader fn-call); everything else is COMPLEX and
## runs via [member _PlanStep.reader] / the nested-hoist path. COMPLEX is 0 so an
## unset step defaults to the safe Callable path. Resolved once at plan-build (cold),
## so the match in [method _inline_type_code] never runs per row. The row-loop
## dispatch is an if-elif, NOT a match: in interpreted GDScript a match arm test costs
## ~10x an if-elif branch test, so a match here is slower than the Callable it replaces
## (measured), while a frequency-ordered if-elif beats it.
enum TC { COMPLEX, U32, I32, U64, I64, F32, F64, U8, U16, I8, I16 }


## One field of a deserialization plan. A typed record (not a Dictionary) so the
## per-field hot loop reads members directly instead of paying a hash lookup per
## field per row — ~10% faster across all resource deserialization.
class _PlanStep:
	var reader: Callable
	var prop_name: StringName
	var prop_type: int
	var type_code: int = 0 # TC.COMPLEX — inline dispatch code, see enum TC
	# Hoisted nested-resource path. When nested_script is set, the field is a plain
	# (non-RustEnum, non-Option) nested schema Resource: the row loop instantiates
	# it + runs nested_plan directly, skipping the per-row schema get_type + plan-cache
	# hashes _read_nested_resource pays. nested_plan is built lazily on first row (once)
	# to sidestep any plan-build recursion. nested_script == null → use reader as before.
	var nested_script: GDScript = null
	var nested_plan: Array = []
	var nested_plan_ready: bool = false


func _create_deserialization_plan(script: Script) -> Array:
	var bsatn_types: Dictionary = script.get_script_constant_map().get("BSATN_TYPES", { })
	var plan: Array[_PlanStep] = []
	var properties: Array[Dictionary] = script.get_script_property_list()
	for prop: Dictionary in properties:
		if not (prop.usage & PROPERTY_USAGE_STORAGE):
			continue

		var prop_name: StringName = prop.name
		var bsatn_type_str: StringName = bsatn_types.get(prop_name, &"")
		var reader_callable: Callable = _get_reader_callable_for_property(prop, bsatn_type_str)

		if not reader_callable.is_valid():
			_set_error("Unsupported property or missing reader for '%s' in script '%s'" % [prop_name, script.resource_path], -1)
			return []

		var step: _PlanStep = _PlanStep.new()
		step.reader = reader_callable
		step.prop_name = prop_name
		step.prop_type = prop.type
		step.nested_script = _hoistable_nested_script(prop, reader_callable)
		step.type_code = _inline_type_code(reader_callable)
		plan.append(step)

	_deserialization_plan_cache[script] = plan
	return plan


## Populates an existing Resource instance from the buffer based on its exported properties.
func _populate_resource_from_bytes(resource: Object, spb: StreamPeerBuffer) -> bool:
	if not resource:
		_set_error("Cannot populate null or scriptless resource", -1 if not spb else spb.get_position())
		return false

	var script: Variant = resource.get_script()
	if not script:
		_set_error("Cannot populate null or scriptless resource", -1 if not spb else spb.get_position())
		return false

	if resource is RustEnum:
		return _populate_enum_from_bytes(spb, resource, script)

	var plan: Array = _get_or_build_plan(script)
	if has_error():
		return false
	return _populate_from_plan(resource, spb, plan)


## Maps a resolved primitive reader to its inline dispatch code. Runs once per field
## at plan-build (cold) — never per row — so the match cost is irrelevant here. Bound
## or complex readers (arrays, options, nested, bool, strings, wide ints, native
## vectors) don't match and stay COMPLEX (the Callable / nested-hoist path).
func _inline_type_code(reader: Callable) -> int:
	var _m: StringName = reader.get_method()
	if _m == &"read_u32_le":
		return TC.U32
	elif _m == &"read_i32_le":
		return TC.I32
	elif _m == &"read_u64_le":
		return TC.U64
	elif _m == &"read_i64_le":
		return TC.I64
	elif _m == &"read_f32_le":
		return TC.F32
	elif _m == &"read_f64_le":
		return TC.F64
	elif _m == &"read_u8":
		return TC.U8
	elif _m == &"read_u16_le":
		return TC.U16
	elif _m == &"read_i8":
		return TC.I8
	elif _m == &"read_i16_le":
		return TC.I16
	else:
		return TC.COMPLEX


## Fetches the cached plan for [param script], building (and caching) it on first
## use. A plan can legitimately be empty (a schema with no storage properties), so
## the cache miss is distinguished by [method Dictionary.has], not emptiness.
func _get_or_build_plan(script: Script) -> Array:
	var plan: Array = _deserialization_plan_cache.get(script, [])
	if plan.is_empty() and not _deserialization_plan_cache.has(script):
		plan = _create_deserialization_plan(script)
	return plan


## Reads each field of [param plan] from [param spb] into [param resource]. Split
## from [method _populate_resource_from_bytes] so callers with a constant schema
## (the row-list loop) fetch the plan once instead of re-hashing the plan cache per
## row.
func _populate_from_plan(resource: Object, spb: StreamPeerBuffer, plan: Array) -> bool:
	# Dispatch is a frequency-ordered if-elif on the field's type_code, NOT a Callable
	# and NOT a match — both lose to if-elif here (see enum TC). Fixed-width primitives
	# read inline (no Callable.call, no reader fn-call, no per-field _check_read);
	# COMPLEX falls to the nested-hoist / Callable path. The loop returns on the first
	# error, so a reader is never entered with a non-OK status — the inline arms skip
	# the per-read status guard the standalone readers carry. buf_size is constant for
	# the parse (get_size = capacity, not remaining), so it is hoisted out of the loop.
	var buf_size: int = spb.get_size()
	for step: _PlanStep in plan:
		var pos: int = spb.get_position()
		var value: Variant = null
		var tc: int = step.type_code

		if tc == TC.U32:
			if pos + 4 <= buf_size:
				value = spb.get_u32()
			else:
				_read_underflow_int(spb, 4)
		elif tc == TC.I32:
			if pos + 4 <= buf_size:
				value = spb.get_32()
			else:
				_read_underflow_int(spb, 4)
		elif tc == TC.U64:
			if pos + 8 <= buf_size:
				value = spb.get_u64()
			else:
				_read_underflow_int(spb, 8)
		elif tc == TC.I64:
			if pos + 8 <= buf_size:
				value = spb.get_64()
			else:
				_read_underflow_int(spb, 8)
		elif tc == TC.F32:
			if pos + 4 <= buf_size:
				value = spb.get_float()
			else:
				_read_underflow_int(spb, 4)
		elif tc == TC.F64:
			if pos + 8 <= buf_size:
				value = spb.get_double()
			else:
				_read_underflow_int(spb, 8)
		elif tc == TC.U8:
			if pos + 1 <= buf_size:
				value = spb.get_u8()
			else:
				_read_underflow_int(spb, 1)
		elif tc == TC.U16:
			if pos + 2 <= buf_size:
				value = spb.get_u16()
			else:
				_read_underflow_int(spb, 2)
		elif tc == TC.I8:
			if pos + 1 <= buf_size:
				value = spb.get_8()
			else:
				_read_underflow_int(spb, 1)
		elif tc == TC.I16:
			if pos + 2 <= buf_size:
				value = spb.get_16()
			else:
				_read_underflow_int(spb, 2)
		elif step.nested_script != null:
			value = _read_nested_hoisted(spb, step)
		else:
			value = step.reader.call(spb)

		if _status != ParseStatus.OK:
			if not _last_error.contains(str(step.prop_name)):
				var inner_status: ParseStatus = _status
				var existing_error: String = get_last_error()
				_set_error("Failed reading property '%s'. Cause: %s" % [step.prop_name, existing_error], pos, inner_status)
			return false

		if value != null:
			if step.prop_type == TYPE_ARRAY and value is Array:
				var target_array: Variant = resource.get(step.prop_name)
				if target_array is Array:
					target_array.assign(value)
				else:
					resource[step.prop_name] = value
			else:
				resource[step.prop_name] = value
	return true


# #12: ENUM_OPTIONS cached in _deserialization_plan_cache as a single-element Array wrapper
# so get_script_constant_map() is called once per script, not once per enum deserialization
func _populate_enum_from_bytes(spb: StreamPeerBuffer, resource: Object, script: Script) -> bool:
	var cached: Array = _deserialization_plan_cache.get(script, [])
	var enum_options: Array
	if cached.is_empty():
		enum_options = script.get_script_constant_map().get(&"ENUM_OPTIONS", [])
		_deserialization_plan_cache[script] = [enum_options] # wrap so null sentinel still works
	else:
		enum_options = cached[0]
	var enum_variant: int = read_u8(spb)
	if has_error():
		return false
	if enum_variant >= enum_options.size():
		_set_error("RustEnum variant tag %d out of range (options size %d)" % [enum_variant, enum_options.size()])
		return false
	resource.value = enum_variant
	var enum_type: StringName = enum_options[enum_variant]
	if not enum_type.is_empty():
		var data: Variant = _read_value_from_bsatn_type(spb, enum_type, &"")
		if has_error():
			return false
		if data != null:
			resource.data = data
	return true


#--- Specific Message/Structure Readers ---
## wire:Reads a BsatnRowList and deserializes each row into a Resource array,
## parsing each row in place directly from [param spb] — no per-row byte copy and
## no intermediate slice array. [param spb] is left positioned at the end of the
## row data block (so the caller resumes correctly even if a row under/over-read).
func _read_bsatn_row_list_as_resources(
		spb: StreamPeerBuffer,
		row_schema_script: GDScript,
		table_name: String,
) -> Array[Resource]:
	var header: Dictionary = _read_row_block_header(spb)
	if has_error():
		return []
	var offsets: PackedInt64Array = header["offsets"]
	var count: int = header["count"]
	var data_len: int = header["data_len"]
	var block_start: int = spb.get_position()
	var block_end: int = block_start + data_len
	if block_end > spb.get_size():
		# Header claims more row data than the buffer holds. Treat as NEEDS_MORE (the
		# rest may arrive in a later packet) so the framing loop keeps the tail, rather
		# than seeking past EOF below — which clamps and silently drops every
		# subsequent message in the stream.
		_set_error(
			"Row block needs %d bytes, buffer has %d" % [block_end, spb.get_size()],
			block_start,
			ParseStatus.NEEDS_MORE,
		)
		return []

	# Plan is constant for every row in the block — fetch once instead of re-hashing
	# the plan cache per row. Table rows are product types (never a bare RustEnum
	# sum), so the plan path applies to all of them.
	var row_plan: Array = _get_or_build_plan(row_schema_script)
	if has_error():
		return []

	var result: Array[Resource] = []
	result.resize(count)
	for i: int in count:
		var row_start: int = block_start + offsets[i]
		if spb.get_position() != row_start:
			spb.seek(row_start)
		var row_resource: Variant = row_schema_script.new()
		if not _populate_from_plan(row_resource, spb, row_plan):
			push_error("Failed to parse row %d for table '%s'" % [i, table_name])
			spb.seek(block_end)
			return []
		var row_end: int = block_start + offsets[i + 1]
		var pos: int = spb.get_position()
		# Over-read means the row consumed bytes belonging to the next row — a
		# schema/wire mismatch (e.g. client/server version skew). The old
		# bounded-slice parser caught this as a hard error; preserve that rather
		# than returning a row populated with the next row's bytes. Under-read
		# (trailing bytes ignored) stays a warning — the next iteration re-anchors.
		if pos > row_end:
			_set_error(
				"Row %d for table '%s' over-read: parsed to %d, row ends at %d (schema/wire mismatch)" % [
					i,
					table_name,
					pos - block_start,
					offsets[i + 1],
				],
				block_start,
			)
			spb.seek(block_end)
			return []
		if pos < row_end:
			push_warning(
				"Row %d for table '%s': parsed to row offset %d, expected %d" % [
					i,
					table_name,
					pos - block_start,
					offsets[i + 1],
				],
			)
		result[i] = row_resource

	# Consume the whole block regardless of any per-row drift above.
	spb.seek(block_end)
	return result


## wire:TableUpdate { table_name: RawIdentifier (string), rows: Array[TableUpdateRows] }
## TableUpdateRows tag: 0=PersistentTable{inserts,deletes}, 1=EventTable{events}
func _read_table_update_instance(spb: StreamPeerBuffer, resource: TableUpdateData) -> bool:
	resource.table_name = read_string_with_u32_len(spb)
	if has_error():
		return false

	var table_name_lower: StringName = _normalize(resource.table_name)
	var row_schema_script: GDScript = _schema.get_type(table_name_lower)

	var rows_count: int = read_u32_le(spb)
	if has_error():
		return false

	var all_inserts: Array[Resource] = []
	var all_deletes: Array[Resource] = []

	for _i: int in rows_count:
		if has_error():
			return false
		var tag: int = read_u8(spb)
		if has_error():
			return false

		if tag == 0: # PersistentTable { inserts: BsatnRowList, deletes: BsatnRowList }
			if row_schema_script:
				var inserts: Array[Resource] = _read_bsatn_row_list_as_resources(spb, row_schema_script, resource.table_name)
				if has_error():
					return false
				all_inserts.append_array(inserts)
				var deletes: Array[Resource] = _read_bsatn_row_list_as_resources(spb, row_schema_script, resource.table_name)
				if has_error():
					return false
				all_deletes.append_array(deletes)
			else:
				if debug_mode:
					push_warning("No schema for '%s', skipping PersistentTable rows." % resource.table_name)
				read_bsatn_row_list(spb)
				if has_error():
					return false # inserts
				read_bsatn_row_list(spb)
				if has_error():
					return false # deletes
		elif tag == 1: # EventTable { events: BsatnRowList } — treated as inserts
			resource.is_event = true
			if row_schema_script:
				var events: Array[Resource] = _read_bsatn_row_list_as_resources(spb, row_schema_script, resource.table_name)
				if has_error():
					return false
				all_inserts.append_array(events)
			else:
				if debug_mode:
					push_warning("No schema for '%s', skipping EventTable rows." % resource.table_name)
				read_bsatn_row_list(spb)
				if has_error():
					return false
		else:
			_set_error("Unknown TableUpdateRows tag %d for table '%s'" % [tag, resource.table_name], spb.get_position() - 1)
			return false

	resource.inserts.assign(all_inserts)
	resource.deletes.assign(all_deletes)
	return true


## wire:SubscribeApplied { request_id: u32, query_set_id: QuerySetId{id:u32}, rows: QueryRows }
## QueryRows { tables: Array[SingleTableRows{table:string, rows:BsatnRowList}] }
func _read_subscripton_applied_message(spb: StreamPeerBuffer) -> SubscribeAppliedMessage:
	var resource: SubscribeAppliedMessage = SubscribeAppliedMessage.new()
	resource.request_id = read_u32_le(spb)
	if has_error():
		return null

	resource.query_set_id.id = read_u32_le(spb)
	if has_error():
		return null

	var table_count: int = read_u32_le(spb)
	if has_error():
		return null

	for _i: int in table_count:
		if has_error():
			return null
		var table_name: String = read_string_with_u32_len(spb)
		if has_error():
			return null

		var table_update: TableUpdateData = TableUpdateData.new()
		table_update.table_name = table_name

		var table_name_lower: StringName = _normalize(table_name)
		var row_schema_script: GDScript = _schema.get_type(table_name_lower)

		if row_schema_script:
			var inserts: Array[Resource] = _read_bsatn_row_list_as_resources(spb, row_schema_script, table_name)
			if has_error():
				return null
			table_update.inserts.assign(inserts)
		else:
			if debug_mode:
				push_warning("No schema for '%s' in SubscribeApplied, skipping rows." % table_name)
			read_bsatn_row_list(spb)
			if has_error():
				return null

		resource.tables.append(table_update)

	return resource


## wire:TransactionUpdate { query_sets: Array[QuerySetUpdate{query_set_id, tables}] }
func _read_transaction_update_message(spb: StreamPeerBuffer) -> TransactionUpdateMessage:
	var tx_update: TransactionUpdateMessage = TransactionUpdateMessage.new()

	var query_set_count: int = read_u32_le(spb)
	if has_error():
		return null

	for _i: int in query_set_count:
		if has_error():
			return null
		var dataset: DatabaseUpdateData = DatabaseUpdateData.new()
		tx_update.query_sets.append(dataset)

		dataset.query_id.id = read_u32_le(spb)
		if has_error():
			return null

		var table_count: int = read_u32_le(spb)
		if has_error():
			return null

		for i2: int in table_count:
			if has_error():
				return null
			var table: TableUpdateData = TableUpdateData.new()
			dataset.tables.append(table)
			if not _read_table_update_instance(spb, table):
				if not has_error():
					_set_error("Failed reading TableUpdate element %d" % i2)
				return null

	return tx_update


## wire:UnsubscribeApplied { request_id: u32, query_set_id: QuerySetId, rows: Option<QueryRows> }
## Option<QueryRows>: tag 0 = Some(QueryRows), 1 = None
## Dropped rows are placed in TableUpdateData.deletes so LocalDatabase decrements
## the refcount and removes only rows no longer held by any remaining subscription.
func _read_unsubscribe_applied_message(spb: StreamPeerBuffer) -> UnsubscribeAppliedMessage:
	var resource: UnsubscribeAppliedMessage = UnsubscribeAppliedMessage.new()
	resource.request_id = read_u32_le(spb)
	if has_error():
		return null

	resource.query_id.id = read_u32_le(spb)
	if has_error():
		return null

	# Option<QueryRows>: tag 0 = Some, 1 = None
	var option_tag: int = read_u8(spb)
	if has_error():
		return null

	if option_tag == 0: # Some(QueryRows)
		var table_count: int = read_u32_le(spb)
		if has_error():
			return null
		for _i: int in table_count:
			if has_error():
				return null
			var table_name: String = read_string_with_u32_len(spb)
			if has_error():
				return null
			var table_update: TableUpdateData = TableUpdateData.new()
			table_update.table_name = table_name
			var table_name_lower: StringName = _normalize(table_name)
			var row_schema_script: GDScript = _schema.get_type(table_name_lower)
			if row_schema_script:
				var rows: Array[Resource] = _read_bsatn_row_list_as_resources(spb, row_schema_script, table_name)
				if has_error():
					return null
				table_update.deletes.assign(rows)
			else:
				read_bsatn_row_list(spb)
				if has_error():
					return null
			resource.tables.append(table_update)
	elif option_tag != 1:
		_set_error("Invalid Option tag %d in UnsubscribeApplied" % option_tag, spb.get_position() - 1)
		return null

	return resource


## wire:SubscriptionError { request_id: Option<u32>, query_set_id: QuerySetId, error: string }
func _read_subscription_error_message(spb: StreamPeerBuffer) -> SubscriptionErrorMessage:
	var resource: SubscriptionErrorMessage = SubscriptionErrorMessage.new()

	var req_id_tag: int = read_u8(spb)
	if has_error():
		return null
	if req_id_tag == 0:
		resource.request_id = read_u32_le(spb)
	elif req_id_tag == 1:
		resource.request_id = -1
	else:
		_set_error("Invalid Option<u32> tag %d for request_id in SubscriptionError" % req_id_tag, spb.get_position() - 1)
		return null
	if has_error():
		return null

	resource.query_id = read_query_id_data(spb)
	if has_error():
		return null

	resource.error_message = read_string_with_u32_len(spb)
	if has_error():
		return null

	printerr("SubscriptionError received: ", resource.error_message)
	return resource


## wire:OneOffQueryResult { request_id: u32, result: Result<QueryRows, string> }
## Result: tag 0 = Ok(QueryRows{tables: Array[SingleTableRows]}), tag 1 = Err(string)
func _read_one_off_query_result_message(spb: StreamPeerBuffer) -> OneOffQueryResponseMessage:
	var resource: OneOffQueryResponseMessage = OneOffQueryResponseMessage.new()
	resource.request_id = read_u32_le(spb)
	if has_error():
		return null

	var result_tag: int = read_u8(spb)
	if has_error():
		return null

	if result_tag == 0: # Ok(QueryRows)
		var table_count: int = read_u32_le(spb)
		if has_error():
			return null
		for _i: int in table_count:
			if has_error():
				return null
			var table_name: String = read_string_with_u32_len(spb)
			if has_error():
				return null
			var table_update: TableUpdateData = TableUpdateData.new()
			table_update.table_name = table_name
			var table_name_lower: StringName = _normalize(table_name)
			var row_schema_script: GDScript = _schema.get_type(table_name_lower)
			if row_schema_script:
				var inserts: Array[Resource] = _read_bsatn_row_list_as_resources(spb, row_schema_script, table_name)
				if has_error():
					return null
				table_update.inserts.assign(inserts)
			else:
				if debug_mode:
					push_warning("No schema for '%s' in OneOffQueryResult, skipping rows." % table_name)
				read_bsatn_row_list(spb)
				if has_error():
					return null
			resource.tables.append(table_update)
	elif result_tag == 1: # Err(string)
		resource.is_error = true
		resource.error_message = read_string_with_u32_len(spb)
		if has_error():
			return null
	else:
		_set_error("Invalid Result tag %d in OneOffQueryResult (expected 0=Ok, 1=Err)" % result_tag, spb.get_position() - 1)
		return null

	return resource


## wire:ReducerResult { request_id: u32, timestamp: Timestamp, result: ReducerOutcome }
## ReducerOutcome: 0=Ok(ReducerOk{ret_value,transaction_update}), 1=OkEmpty, 2=Err(bytes), 3=InternalError(string)
func _read_reducer_result_message(spb: StreamPeerBuffer) -> ReducerResultMessage:
	var resource: ReducerResultMessage = ReducerResultMessage.new()

	resource.request_id = read_u32_le(spb)
	if has_error():
		return null
	resource.timestamp = read_timestamp(spb)
	if has_error():
		return null

	var outcome_tag: int = read_u8(spb)
	if has_error():
		return null

	var outcome: ReducerOutcomeEnum = ReducerOutcomeEnum.new()
	outcome.value = outcome_tag
	resource.reducer_result = outcome

	if outcome_tag == ReducerOutcomeEnum.Options.ok:
		resource.ret_value = read_vec_u8(spb)
		if has_error():
			return null
		var tx_update: TransactionUpdateMessage = _read_transaction_update_message(spb)
		if has_error():
			return null
		outcome.data = tx_update
	elif outcome_tag == ReducerOutcomeEnum.Options.okEmpty:
		outcome.data = null
	elif outcome_tag == ReducerOutcomeEnum.Options.err:
		outcome.data = read_vec_u8(spb)
		if has_error():
			return null
	elif outcome_tag == ReducerOutcomeEnum.Options.internalError:
		outcome.data = read_string_with_u32_len(spb)
		if has_error():
			return null
	else:
		_set_error("Unknown ReducerOutcome tag: %d" % outcome_tag, spb.get_position() - 1)
		return null

	return resource


## wire:ProcedureResult { status: ProcedureStatus, timestamp: Timestamp, total_host_execution_duration: TimeDuration, request_id: u32 }
## ProcedureStatus: 0=Returned(bytes), 1=InternalError(string)
func _read_procedure_result_message(spb: StreamPeerBuffer) -> ProcedureResultData:
	var resource: ProcedureResultData = ProcedureResultData.new()

	var status_tag: int = read_u8(spb)
	if has_error():
		return null
	resource.status_tag = status_tag

	if status_tag == 0: # Returned(bytes)
		resource.return_bytes = read_vec_u8(spb)
		if has_error():
			return null
	elif status_tag == 1: # InternalError(string)
		resource.error_message = read_string_with_u32_len(spb)
		if has_error():
			return null
	else:
		_set_error("Unknown ProcedureStatus tag: %d" % status_tag, spb.get_position() - 1)
		return null

	resource.timestamp = read_timestamp(spb)
	if has_error():
		return null

	resource.duration = read_timestamp(spb) # TimeDuration is also i64 micros
	if has_error():
		return null

	resource.request_id = read_u32_le(spb)
	if has_error():
		return null

	return resource


func _read_generic_server_message(msg_type: int, script_path: String, spb: StreamPeerBuffer) -> SpacetimeDBServerMessage:
	if not ResourceLoader.exists(script_path):
		_set_error("Script not found for message type 0x%02X: %s" % [msg_type, script_path], 1)
		return null
	var script: GDScript = ResourceLoader.load(script_path, "GDScript")
	if not script or not script.can_instantiate():
		_set_error("Failed to load or instantiate script for message type 0x%02X: %s" % [msg_type, script_path], 1)
		return null

	var message: SpacetimeDBServerMessage = script.new()
	if not _populate_resource_from_bytes(message, spb):
		return null
	return message


# --- Top-Level Message Parsing ---
func _parse_message_from_stream(spb: StreamPeerBuffer) -> SpacetimeDBServerMessage:
	clear_error()

	var start_pos: int = spb.get_position()
	if not _check_read(spb, 1):
		return null

	var msg_type: SpacetimeDBServerMessage.Type = read_u8(spb) as SpacetimeDBServerMessage.Type
	if has_error():
		return null

	var result: SpacetimeDBServerMessage = null
	var script_path: String = SpacetimeDBServerMessage.get_script_path(msg_type)

	if script_path.is_empty():
		_set_error("Unknown server message type: 0x%02X" % msg_type, 1)
		return null

	if msg_type == SpacetimeDBServerMessage.INITIAL_CONNECTION:
		result = _read_generic_server_message(msg_type, script_path, spb)
	elif msg_type == SpacetimeDBServerMessage.SUBSCRIBE_APPLIED:
		result = _read_subscripton_applied_message(spb)
	elif msg_type == SpacetimeDBServerMessage.UNSUBSCRIBE_APPLIED:
		result = _read_unsubscribe_applied_message(spb)
	elif msg_type == SpacetimeDBServerMessage.SUBSCRIPTION_ERROR:
		result = _read_subscription_error_message(spb)
	elif msg_type == SpacetimeDBServerMessage.TRANSACTION_UPDATE:
		result = _read_transaction_update_message(spb)
	elif msg_type == SpacetimeDBServerMessage.ONE_OFF_QUERY_RESPONSE:
		result = _read_one_off_query_result_message(spb)
	elif msg_type == SpacetimeDBServerMessage.REDUCER_RESULT:
		result = _read_reducer_result_message(spb)
	elif msg_type == SpacetimeDBServerMessage.PROCEDURE_RESULT:
		result = _read_procedure_result_message(spb)
	else:
		_set_error("Unknown server message type: 0x%02X" % msg_type, start_pos)
		return null
	if has_error():
		return null
	# No trailing-bytes warning here: this parses ONE message from a buffer that, under the
	# v3 framing, holds several concatenated messages. get_size() is the whole buffer, so
	# "remaining" is just the next message — process_bytes_and_extract_messages loops and
	# parses it. Per-message under-reads are caught by the row-list offset checks.
	return result
