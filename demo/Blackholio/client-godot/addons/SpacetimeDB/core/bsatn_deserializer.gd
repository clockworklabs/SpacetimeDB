class_name BSATNDeserializer extends RefCounted

# --- Constants ---
const MAX_STRING_LEN := 4 * 1024 * 1024 # 4 MiB limit for strings
const MAX_VEC_LEN := 131072            # Limit for vector elements (used by read_vec_u8 and _read_array)
const MAX_BYTE_ARRAY_LEN := 16 * 1024 * 1024 # Limit for Vec<u8> style byte arrays
const IDENTITY_SIZE := 32
const CONNECTION_ID_SIZE := 16

const COMPRESSION_NONE := 0x00
const COMPRESSION_BROTLI := 0x01
const COMPRESSION_GZIP := 0x02

# Row List Format Tags
const ROW_LIST_FIXED_SIZE := 0
const ROW_LIST_ROW_OFFSETS := 1

# --- Properties ---
var _last_error: String = ""
var _deserialization_plan_cache: Dictionary = {}
var _pending_data := PackedByteArray()
var _schema: SpacetimeDBSchema
var debug_mode := false # Controls verbose debug printing

# --- Initialization ---
func _init(p_schema: SpacetimeDBSchema, p_debug_mode: bool = false) -> void:
    debug_mode = p_debug_mode
    _schema = p_schema

# --- Error Handling ---
func has_error() -> bool: return _last_error != ""
func get_last_error() -> String: var err := _last_error; _last_error = ""; return err
func clear_error() -> void: _last_error = ""
func _set_error(msg: String, position: int = -1) -> void:
    if _last_error == "": # Prevent overwriting the first error
        var pos_str := " (at approx. position %d)" % position if position >= 0 else ""
        _last_error = "BSATNDeserializer Error: %s%s" % [msg, pos_str]
        printerr(_last_error) # Always print errors
func _check_read(spb: StreamPeerBuffer, bytes_needed: int) -> bool:
    if has_error(): return false
    if spb.get_position() + bytes_needed > spb.get_size():
        _set_error("Attempted to read %d bytes past end of buffer (size: %d)." % [bytes_needed, spb.get_size()], spb.get_position())
        return false
    return true

# --- Primitive Value Readers ---
func read_i8(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 1): return 0
    return spb.get_8();
func read_i16_le(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 2): return 0
    spb.big_endian = false; return spb.get_16();
func read_i32_le(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 4): return 0
    spb.big_endian = false; return spb.get_32();
func read_i64_le(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 8): return 0
    spb.big_endian = false; return spb.get_64();
func read_u8(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 1): return 0
    return spb.get_u8()
func read_u16_le(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 2): return 0
    spb.big_endian = false; return spb.get_u16()
func read_u32_le(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 4): return 0
    spb.big_endian = false; return spb.get_u32()
func read_u64_le(spb: StreamPeerBuffer) -> int:
    if not _check_read(spb, 8): return 0
    spb.big_endian = false; return spb.get_u64()
func read_f32_le(spb: StreamPeerBuffer) -> float:
    if not _check_read(spb, 4): return 0.0
    spb.big_endian = false; return spb.get_float()
func read_f64_le(spb: StreamPeerBuffer) -> float:
    if not _check_read(spb, 8): return 0.0
    spb.big_endian = false; return spb.get_double()
func read_bool(spb: StreamPeerBuffer) -> bool:
    var byte := read_u8(spb)
    if has_error(): return false
    if byte != 0 and byte != 1: _set_error("Invalid boolean value: %d (expected 0 or 1)" % byte, spb.get_position() - 1); return false
    return byte == 1
func read_bytes(spb: StreamPeerBuffer, num_bytes: int) -> PackedByteArray:
    if num_bytes < 0: _set_error("Attempted to read negative bytes: %d" % num_bytes, spb.get_position()); return PackedByteArray()
    if num_bytes == 0: return PackedByteArray()
    if not _check_read(spb, num_bytes): return PackedByteArray()
    var result: Array = spb.get_data(num_bytes)
    if result[0] != OK: _set_error("StreamPeerBuffer.get_data failed: %d" % result[0], spb.get_position() - num_bytes); return PackedByteArray()
    return result[1]
func read_string_with_u32_len(spb: StreamPeerBuffer) -> String:
    var start_pos := spb.get_position()
    var length := read_u32_le(spb)
    if has_error() or length == 0: return ""
    if length > MAX_STRING_LEN: _set_error("String length %d exceeds limit %d" % [length, MAX_STRING_LEN], start_pos); return ""
    var str_bytes := read_bytes(spb, length)
    if has_error(): return ""
    var str_result := str_bytes.get_string_from_utf8()
    # More robust check for UTF-8 decoding errors
    if str_result == "" and length > 0 and (str_bytes.get_string_from_ascii() == "" or str_bytes.find(0) != -1):
        _set_error("Failed to decode UTF-8 string length %d" % length, start_pos); return ""
    return str_result
func read_identity(spb: StreamPeerBuffer) -> PackedByteArray:
    var identity := read_bytes(spb, IDENTITY_SIZE)
    identity.reverse() # We receive the identity bytes in reverse
    return identity
func read_connection_id(spb: StreamPeerBuffer) -> PackedByteArray:
    return read_bytes(spb, CONNECTION_ID_SIZE)
func read_timestamp(spb: StreamPeerBuffer) -> int:
    return read_i64_le(spb) # Timestamps are i64
func read_vector3(spb: StreamPeerBuffer) -> Vector3:
    var x := read_f32_le(spb); var y := read_f32_le(spb); var z := read_f32_le(spb)
    return Vector3.ZERO if has_error() else Vector3(x, y, z)
func read_vector2(spb: StreamPeerBuffer) -> Vector2:
    var x := read_f32_le(spb); var y := read_f32_le(spb)
    return Vector2.ZERO if has_error() else Vector2(x, y)
func read_vector2i(spb: StreamPeerBuffer) -> Vector2i:
    var x := read_i32_le(spb); var y := read_i32_le(spb)
    return Vector2i.ZERO if has_error() else Vector2i(x, y)
func read_color(spb: StreamPeerBuffer) -> Color:
    var r := read_f32_le(spb); var g := read_f32_le(spb); var b := read_f32_le(spb); var a := read_f32_le(spb)
    return Color.BLACK if has_error() else Color(r, g, b, a)
func read_quaternion(spb: StreamPeerBuffer) -> Quaternion:
    var x := read_f32_le(spb); var y := read_f32_le(spb); var z := read_f32_le(spb); var w := read_f32_le(spb)
    return Quaternion.IDENTITY if has_error() else Quaternion(x, y, z, w)
func read_vec_u8(spb: StreamPeerBuffer) -> PackedByteArray:
    var start_pos := spb.get_position()
    var length := read_u32_le(spb)
    if has_error(): return PackedByteArray()
    if length > MAX_BYTE_ARRAY_LEN: _set_error("Vec<u8> length %d exceeds limit %d" % [length, MAX_BYTE_ARRAY_LEN], start_pos); return PackedByteArray()
    if length == 0: return PackedByteArray()
    return read_bytes(spb, length)

# --- BsatnRowList Reader ---
func read_bsatn_row_list(spb: StreamPeerBuffer) -> Array[PackedByteArray]:
    var start_pos := spb.get_position()
    var size_hint_type := read_u8(spb)
    if has_error(): return []
    var rows: Array[PackedByteArray] = []
    match size_hint_type:
        ROW_LIST_FIXED_SIZE:
            var row_size := read_u16_le(spb); var data_len := read_u32_le(spb)
            if has_error(): return []
            if row_size == 0:
                if data_len != 0: _set_error("FixedSize row_size 0 but data_len %d" % data_len, start_pos); read_bytes(spb, data_len); return []
                return []
            var data := read_bytes(spb, data_len)
            if has_error(): return []
            if data_len % row_size != 0: _set_error("FixedSize data_len %d not divisible by row_size %d" % [data_len, row_size], start_pos); return []
            var num_rows := data_len / row_size
            rows.resize(num_rows)
            for i in range(num_rows): rows[i] = data.slice(i * row_size, (i + 1) * row_size)
        ROW_LIST_ROW_OFFSETS:
            var num_offsets := read_u32_le(spb)
            if has_error(): return []
            var offsets: Array[int] = []; offsets.resize(num_offsets)
            for i in range(num_offsets): offsets[i] = read_u64_le(spb); if has_error(): return []
            var data_len := read_u32_le(spb)
            if has_error(): return []
            var data := read_bytes(spb, data_len)
            if has_error(): return []
            rows.resize(num_offsets)
            for i in range(num_offsets):
                var start_offset : int = offsets[i]
                var end_offset : int = data_len if (i + 1 == num_offsets) else offsets[i+1]
                if start_offset < 0 or end_offset < start_offset or end_offset > data_len: _set_error("Invalid row offsets: start=%d, end=%d, data_len=%d row %d" % [start_offset, end_offset, data_len, i], start_pos); return []
                rows[i] = data.slice(start_offset, end_offset)
        _: _set_error("Unknown RowSizeHint type: %d" % size_hint_type, start_pos); return []
    return rows

# --- Core Deserialization Logic ---

# Helper to get a primitive reader Callable based on a BSATN type string.
func _get_primitive_reader_from_bsatn_type(bsatn_type_str: String) -> Callable:
    match bsatn_type_str:
        &"u64": return Callable(self, "read_u64_le")
        &"i64": return Callable(self, "read_i64_le")
        &"u32": return Callable(self, "read_u32_le")
        &"i32": return Callable(self, "read_i32_le")
        &"u16": return Callable(self, "read_u16_le")
        &"i16": return Callable(self, "read_i16_le")
        &"u8": return Callable(self, "read_u8")
        &"i8": return Callable(self, "read_i8")
        &"identity": return Callable(self, "read_identity")
        &"connection_id": return Callable(self, "read_connection_id")
        &"timestamp": return Callable(self, "read_timestamp")
        &"f64": return Callable(self, "read_f64_le")
        &"f32": return Callable(self, "read_f32_le")
        &"vec_u8": return Callable(self, "read_vec_u8")
        &"bool": return Callable(self, "read_bool")
        &"string": return Callable(self, "read_string_with_u32_len")
        _: return Callable() # Return invalid Callable if type is not primitive/known

# Determines the correct reader function (Callable) for a given property.
func _get_reader_callable_for_property(resource: Resource, prop: Dictionary) -> Callable:
    var prop_name: StringName = prop.name
    var prop_type: Variant.Type = prop.type
    var meta_key := "bsatn_type_" + prop_name

    var reader_callable := Callable() # Initialize with invalid Callable

    # --- Special Cases First ---
    # Handle specific properties requiring custom logic before generic checks
    if resource is TransactionUpdateMessage and prop_name == "status":
        reader_callable = Callable(self, "_read_update_status")
    # Add other special cases here if needed (e.g., Option<T> fields if handled generically later)
    if prop.class_name == &'Option':
        reader_callable = Callable(self, "_read_option")

    # --- Generic Type Handling (if not a special case) ---
    elif prop_type == TYPE_ARRAY:
        # Handle arrays: Distinguish between standard arrays and the special TableUpdate array
        if resource is DatabaseUpdateData and prop_name == "tables":
            reader_callable = Callable(self, "_read_array_of_table_updates")
        else:
            reader_callable = Callable(self, "_read_array") # Standard array reader
    else:
        # Handle non-array, non-special-case properties
        # 1. Check for specific BSATN type override via metadata
        if resource.has_meta(meta_key):
            var bsatn_type_str: String = str(resource.get_meta(meta_key)).to_lower()
            reader_callable = _get_primitive_reader_from_bsatn_type(bsatn_type_str)
            if not reader_callable.is_valid() and debug_mode:
                # Metadata exists but doesn't map to a primitive reader
                push_warning("Unknown 'bsatn_type' metadata value: '%s' for property '%s'. Falling back to default type." % [bsatn_type_str, prop_name])

        # 2. Fallback to default reader based on property's Variant.Type if metadata didn't provide a valid reader
        if not reader_callable.is_valid():
            match prop_type:
                TYPE_BOOL: reader_callable = Callable(self, "read_bool")
                TYPE_INT: reader_callable = Callable(self, "read_i64_le") # Default int is i64
                TYPE_FLOAT: reader_callable = Callable(self, "read_f32_le") # Default float is f32
                TYPE_STRING: reader_callable = Callable(self, "read_string_with_u32_len")
                TYPE_VECTOR2: reader_callable = Callable(self, "read_vector2")
                TYPE_VECTOR2I: reader_callable = Callable(self, "read_vector2i")
                TYPE_VECTOR3: reader_callable = Callable(self, "read_vector3")
                TYPE_COLOR: reader_callable = Callable(self, "read_color")
                TYPE_QUATERNION: reader_callable = Callable(self, "read_quaternion")
                TYPE_PACKED_BYTE_ARRAY: reader_callable = Callable(self, "read_vec_u8") # Default PBA is Vec<u8>
                # TYPE_ARRAY is handled above
                TYPE_OBJECT: 
                    reader_callable = Callable(self, "_read_nested_resource") # Handles nested Resources
                _:
                    # reader_callable remains invalid for unsupported types
                    pass

    # --- Debug Print (Optional) ---
    if debug_mode:
        var resource_id = resource.resource_path if resource and resource.resource_path else (resource.get_class() if resource else "NullResource")
        print("DEBUG: _get_reader_callable: For '%s' in '%s', returning: %s" % [prop.name, resource_id, reader_callable.get_method() if reader_callable.is_valid() else "INVALID"])
    # --- End Debug ---

    return reader_callable

# Reads a value for a specific property using the determined reader.
func _read_value_for_property(spb: StreamPeerBuffer, resource: Resource, prop: Dictionary):	
    var meta: String = ""
    if resource.has_meta("bsatn_type_" + prop.name):
        meta = resource.get_meta("bsatn_type_" + prop.name).to_lower()
    if prop.class_name == &'Option':
        return _read_option(spb, resource, prop, meta)

    var reader_callable := _get_reader_callable_for_property(resource, prop)

    if not reader_callable.is_valid():
        _set_error("Unsupported property type '%s' or missing reader for property '%s' in resource '%s'" % [type_string(prop.type), prop.name, resource.resource_path if resource else "Unknown"], spb.get_position())
        return null # Return null on error/unsupported

    # Call the determined reader function.
    if reader_callable.get_object() == self:
        var method_name = reader_callable.get_method()
        # Check if the method requires the full context (spb, resource, prop)
        # Typically needed for recursive or context-aware readers.
        match method_name:
            "_read_array", "_read_nested_resource", "_read_array_of_table_updates", "_read_option":
                return reader_callable.call(spb, resource, prop) # Pass full context
            _: 
                # Standard primitive/complex readers usually only need the buffer.
                # This includes _read_update_status.
                return reader_callable.call(spb) # Pass only spb		
    else:
        # Should not happen with Callables created above, but handle defensively
        _set_error("Internal error: Invalid reader callable.", spb.get_position())
        return null

# Populates an existing Resource instance from the buffer based on its exported properties.
func _populate_resource_from_bytes(resource: Resource, spb: StreamPeerBuffer) -> bool:
    var script := resource.get_script()
    if not resource or not script:
        _set_error("Cannot populate null or scriptless resource", -1 if not spb else spb.get_position())
        return false

    if resource is RustEnum:
        return _populate_enum_from_bytes(spb, resource)

    var plan = _deserialization_plan_cache.get(script)

    if plan == null:
        if debug_mode: print("DEBUG: Creating deserialization plan for script: %s" % script.resource_path)
        
        plan = []
        var properties: Array = script.get_script_property_list()
        for prop in properties:
            if not (prop.usage & PROPERTY_USAGE_STORAGE):
                continue

            var prop_name: StringName = prop.name
            var reader_callable: Callable = _get_reader_callable_for_property(resource, prop)
            
            if not reader_callable.is_valid():
                _set_error("Unsupported property or missing reader for '%s' in script '%s'" % [prop_name, script.resource_path], -1)
                _deserialization_plan_cache[script] = []
                return false
                
            var method_name := reader_callable.get_method()
            var needs_full_context := method_name in ["_read_array", "_read_nested_resource", "_read_array_of_table_updates", "_read_option"]

            plan.append({
                "name": prop_name,
                "type": prop.type,
                "reader": reader_callable,
                "full_context": needs_full_context,
                "prop_dict": prop
            })
        
        _deserialization_plan_cache[script] = plan
        
    for instruction in plan:
        var value_start_pos = spb.get_position()
        var value
        if instruction.full_context:
            value = instruction.reader.call(spb, resource, instruction.prop_dict)
        else:
            value = instruction.reader.call(spb)

        if has_error():
            if not _last_error.contains(str(instruction.name)):
                var existing_error = get_last_error()
                _set_error("Failed reading value for property '%s' in '%s'. Cause: %s" % [instruction.name, resource.get_script().get_global_name() if resource else "Unknown", existing_error], value_start_pos)
            return false

        if value != null:
            if instruction.type == TYPE_ARRAY and value is Array:
                var target_array = resource.get(instruction.name)
                if target_array is Array:
                    target_array.assign(value)
                else:
                    resource[instruction.name] = value
            else:
                resource[instruction.name] = value
    return true

# Populates the value property of a sumtype enum
func _populate_enum_from_bytes(spb: StreamPeerBuffer, resource: Resource) -> bool:
    var enum_type = resource.get_meta("bsatn_enum_type")
    var enum_variant: int = spb.get_u8()
    var instance: Resource = null
    var script := _schema.get_type(enum_type.to_lower())
    if script and script.can_instantiate():
        instance = script.new()
        resource.value = enum_variant
        _populate_enum_data_from_bytes(resource, spb)
    return true

# Populates the data property of a sumtype enum
func _populate_enum_data_from_bytes(resource: Resource, spb: StreamPeerBuffer) -> bool:	
    var enum_type: StringName = resource.get_meta("enum_options")[resource.value]
    if enum_type == &"": return true
    var data = _read_value_from_bsatn_type(spb, enum_type.to_lower(), &"")
    if data:
        resource.data = data
        return true
    return false
    
# --- Special Readers ---
# Add this new function to the BSATNDeserializer class

# Helper function to deserialize a value based on BSATN type string.
# Assumes bsatn_type_str is already to_lower() if it's from metadata.
func _read_value_from_bsatn_type(spb: StreamPeerBuffer, bsatn_type_str: String, context_prop_name_for_error: StringName) -> Variant:
    var value = null
    var start_pos_val_read = spb.get_position() # For error reporting

    # 1. Try primitive reader (expects lowercase bsatn_type_str)
    var primitive_reader := _get_primitive_reader_from_bsatn_type(bsatn_type_str)
    if primitive_reader.is_valid():
        value = primitive_reader.call(spb)
        if has_error(): return null
        return value

    # 2. Handle Vec<T> (e.g., "vec_u8", "vec_mycustomresource")
    # Assumes bsatn_type_str is already lowercase
    if bsatn_type_str.begins_with("vec_"):
        var element_bsatn_type_str = bsatn_type_str.right(-4) # This will also be lowercase
        
        var array_length := read_u32_le(spb)
        if has_error(): return null
        if array_length == 0: return []
        if array_length > MAX_VEC_LEN:
            _set_error("Array length %d (for BSATN type '%s') exceeds limit %d for context '%s'" % [array_length, bsatn_type_str, MAX_VEC_LEN, context_prop_name_for_error], spb.get_position() - 4) # -4 for u32 length
            return null
            
        var temp_array := []
        # temp_array.resize(array_length) # Not strictly necessary if appending
        for i in range(array_length):
            if has_error(): return null # Stop if previous element failed
            var element_value = _read_value_from_bsatn_type(spb, element_bsatn_type_str, "%s[element %d]" % [context_prop_name_for_error, i])
            if has_error(): return null # Stop if current element failed
            temp_array.append(element_value)
        return temp_array

    # 3. Handle Option<T> (e.g., "opt_u8", "opt_mycustomresource")
    # Assumes bsatn_type_str is already lowercase
    if bsatn_type_str.begins_with("opt_"):
        var element_bsatn_type_str = bsatn_type_str.right(-4) # This will also be lowercase
        var option = _read_option(spb, null, {"name": context_prop_name_for_error}, element_bsatn_type_str)
        return option

    # 4. Handle Custom Resource (non-array)
    # schema type names are table_name.to_lower().replace("_", "")
    # bsatn_type_str from metadata should be .to_lower()'d before calling this.
    var schema_key = bsatn_type_str.replace("_", "") # e.g., "maindamage" -> "maindamage", "my_type" -> "mytype"
    if _schema.types.has(schema_key):
        var script := _schema.get_type(schema_key)
        if script and script.can_instantiate():
            var nested_instance = script.new()
            if not _populate_resource_from_bytes(nested_instance, spb):
                # Error should be set by _populate_resource_from_bytes
                if not has_error(): _set_error("Failed to populate nested resource of type '%s' (schema key '%s') for context '%s'" % [bsatn_type_str, schema_key, context_prop_name_for_error], start_pos_val_read)
                return null
            return nested_instance
        else:
            _set_error("Cannot instantiate schema for BSATN type '%s' (schema key '%s', context: '%s'). Script valid: %s, Can instantiate: %s" % [bsatn_type_str, schema_key, context_prop_name_for_error, script != null, script.can_instantiate() if script else "N/A"], start_pos_val_read)
            return null
            
    _set_error("Unsupported BSATN type '%s' for deserialization (context: '%s'). No primitive, vec, or custom schema found." % [bsatn_type_str, context_prop_name_for_error], start_pos_val_read)
    return null

func _read_option(spb: StreamPeerBuffer, parent_resource_containing_option: Resource, option_property_dict: Dictionary, explicit_inner_bsatn_type_str: String = "") -> Option:
    var option_instance := Option.new()
    var option_prop_name: StringName = option_property_dict.name # For error messages and metadata key

    # Wire format: u8 tag (0 for Some, 1 for None)
    # If Some (0): followed by T value
    var tag_pos := spb.get_position()
    var is_present_tag := read_u8(spb) 
    if has_error(): return null # Error reading tag
    if is_present_tag == 1: # It's None
        option_instance.set_none()
        if debug_mode: print("DEBUG: _read_option: Read None for Option property '%s'" % option_prop_name)
        return option_instance
    elif is_present_tag == 0: # It's Some
        var inner_bsatn_type_str_to_use: String

        if not explicit_inner_bsatn_type_str.is_empty():
            inner_bsatn_type_str_to_use = explicit_inner_bsatn_type_str # Assumed to be already .to_lower() by caller (_read_array)
        else:
            var bsatn_meta_key_for_inner_type := "bsatn_type_" + option_prop_name
            if not parent_resource_containing_option.has_meta(bsatn_meta_key_for_inner_type):
                _set_error("Missing 'bsatn_type' metadata for Option property '%s' in resource '%s'. Cannot determine inner type T." % [option_prop_name, parent_resource_containing_option.resource_path if parent_resource_containing_option else "UnknownResource"], tag_pos)
                return null
            inner_bsatn_type_str_to_use = str(parent_resource_containing_option.get_meta(bsatn_meta_key_for_inner_type)).to_lower()
            if inner_bsatn_type_str_to_use.is_empty():
                _set_error("'bsatn_type' metadata for Option property '%s' is empty. Cannot determine inner type T." % option_prop_name, tag_pos)
                return null

        if debug_mode: print("DEBUG: _read_option: Read Some for Option property '%s', deserializing inner type: '%s'" % [option_prop_name, inner_bsatn_type_str_to_use])
        var inner_value = _read_value_from_bsatn_type(spb, inner_bsatn_type_str_to_use, option_prop_name)

        if has_error():
            # Error should have been set by _read_value_from_bsatn_type or its callees.
            # Add context if the error message doesn't already mention the property.
            if not _last_error.contains(str(option_prop_name)):
                var existing_error = get_last_error() # Consume the error
                _set_error("Failed reading 'Some' value for Option property '%s' (inner BSATN type '%s'). Cause: %s" % [option_prop_name, inner_bsatn_type_str_to_use, existing_error], tag_pos + 1) # Position after tag
            return null

        option_instance.set_some(inner_value)
        return option_instance
    else:
        _set_error("Invalid tag %d for Option property '%s' (expected 0 for Some, 1 for None)." % [is_present_tag, option_prop_name], tag_pos)
        return null
    
# Reads an array property.
func _read_array(spb: StreamPeerBuffer, resource: Resource, prop: Dictionary) -> Array:
    var prop_name: StringName = prop.name
    var start_pos := spb.get_position()
    var meta_key := "bsatn_type_" + prop_name

    # 1. Read array length (u32)
    var length := read_u32_le(spb)
    if has_error(): return []
    if length == 0: return []
    if length > MAX_VEC_LEN: _set_error("Array length %d exceeds limit %d for property '%s'" % [length, MAX_VEC_LEN, prop_name], start_pos); return []

    # 2. Determine element prototype info (Variant.Type, class_name) from hint_string
    var hint: int = prop.hint
    var hint_string: String = prop.hint_string
    var element_type_code: Variant.Type = TYPE_MAX
    var element_class_name: StringName = &""

    

    if hint == PROPERTY_HINT_TYPE_STRING and ":" in hint_string: # Godot 3 style: "Type:TypeName"
        var hint_parts = hint_string.split(":", true, 1)
        if hint_parts.size() == 2: 
            element_type_code = int(hint_parts[0]); 
            element_class_name = hint_parts[1]
        else: _set_error("Array property '%s': Bad hint_string format '%s'." % [prop_name, hint_string], start_pos); return []
    elif hint == PROPERTY_HINT_ARRAY_TYPE: # Godot 4 style: "VariantType/ClassName:VariantType" or "VariantType:VariantType"
        var main_type_str = hint_string.split(":", true, 1)[0]
        if "/" in main_type_str: var parts = main_type_str.split("/", true, 1); element_type_code = int(parts[0]); element_class_name = parts[1]
        else: element_type_code = int(main_type_str)
    else: _set_error("Array property '%s' needs a typed hint. Hint: %d, HintString: '%s'" % [prop_name, hint, hint_string], start_pos); return []
    if element_type_code == TYPE_MAX: _set_error("Could not determine element type for array '%s'." % prop_name, start_pos); return []
    
    # 3. Create a temporary "prototype" dictionary for the element
    var element_prop_sim = { "name": prop_name + "[element]", "type": element_type_code, "class_name": element_class_name, "usage": PROPERTY_USAGE_STORAGE, "hint": 0, "hint_string": "" }

    # 4. Determine the reader function for the ELEMENTS
    var element_reader_callable : Callable
    var array_bsatn_meta_key := "bsatn_type_" + prop_name # Metadata for the array property itself
    var inner_type_for_option_elements: String = "" # To store T's BSATN type for Array[Option<T>]
    if element_class_name == &"Option":
        element_reader_callable = Callable(self, "_read_option")
        if resource.has_meta(array_bsatn_meta_key):
            inner_type_for_option_elements = str(resource.get_meta(array_bsatn_meta_key)).to_lower()
            if inner_type_for_option_elements.is_empty():
                _set_error("Array '%s' of Options has empty 'bsatn_type' metadata. Inner type T for Option<T> cannot be determined." % prop_name, start_pos)
                return []
        else:
            # This metadata is essential for Array[Option<T>]
            _set_error("Array '%s' of Options is missing 'bsatn_type' metadata. This metadata should specify the BSATN type of T in Option<T> (e.g., 'u8' for Array[Option<u8>])." % prop_name, start_pos)
            return []
    else: # Not an array of Options, proceed with existing logic
        if resource.has_meta(array_bsatn_meta_key): # Check array's metadata first (defines element BSATN type)
            var bsatn_element_type_str = str(resource.get_meta(array_bsatn_meta_key)).to_lower()
            element_reader_callable = _get_primitive_reader_from_bsatn_type(bsatn_element_type_str)
            # Check if resource is a nested resource in possible row schemas
            if not element_reader_callable.is_valid() and _schema.types.has(bsatn_element_type_str):
                element_reader_callable = Callable(self, "_read_nested_resource")
            if not element_reader_callable.is_valid() and debug_mode:
                push_warning("Array '%s' has 'bsatn_type' metadata ('%s'), but it doesn't map to a primitive reader. Falling back to element type hint." % [prop_name, bsatn_element_type_str])
        
        if not element_reader_callable.is_valid(): # Fallback to element's Variant.Type if no valid primitive reader from metadata
            element_reader_callable = _get_reader_callable_for_property(resource, element_prop_sim) # Use element prototype here
    
    if not element_reader_callable.is_valid():
        _set_error("Cannot determine reader for elements of array '%s' (element type code %d, class '%s')." % [prop_name, element_type_code, element_class_name], start_pos)
        return []

    # 5. Read elements recursively
    var result_array := []; result_array.resize(length) # Pre-allocate for typed arrays if needed, or just append
    var element_reader_method_name = element_reader_callable.get_method() if element_reader_callable.is_valid() else ""

    for i in range(length):
        if has_error(): return [] # Stop on error
        var element_start_pos = spb.get_position()
        var element_value = null
        
        if element_reader_callable.get_object() == self:
            match element_reader_method_name:
                # Special handling for _read_option when it's an array element
                "_read_option":
                    element_value = element_reader_callable.call(spb, resource, element_prop_sim, inner_type_for_option_elements)
                # Existing logic for other recursive/contextual readers
                "_read_array", "_read_nested_resource", "_read_array_of_table_updates":
                    element_value = element_reader_callable.call(spb, resource, element_prop_sim)
                # Primitive reader or other simple reader
                _:
                    element_value = element_reader_callable.call(spb)
        else: 
            _set_error("Internal error: Invalid element reader callable for array '%s'." % prop_name, element_start_pos); return []
        
        if has_error():
            if not _last_error.contains("element %d" % i) and not _last_error.contains(str(prop_name)): # Avoid redundant context
                var existing_error = get_last_error(); 
                _set_error("Failed reading element %d for array '%s'. Cause: %s" % [i, prop_name, existing_error], element_start_pos)
            return []
        result_array[i] = element_value # Or result_array.append(element_value) if not resizing
    return result_array

# Reads a nested Resource property.
func _read_nested_resource(spb: StreamPeerBuffer, resource: Resource, prop: Dictionary) -> Resource:
    var prop_name: StringName = prop.name
    var nested_class_name: StringName = prop.class_name

    if nested_class_name == &"":
        _set_error("Property '%s' is TYPE_OBJECT but has no class_name hint in script '%s'." % [prop_name, resource.get_script().resource_path if resource and resource.get_script() else "Unknown"], spb.get_position())
        return null

    # Try to find script in preloaded schemas first (common for table rows)
    var key := nested_class_name.to_lower()
    var script := _schema.get_type(key)
    var nested_instance: Resource = null

    if script:
        nested_instance = script.new()
    else:
        # If not preloaded, try ClassDB (for built-ins or globally registered scripts)
        if ClassDB.can_instantiate(nested_class_name):
            nested_instance = ClassDB.instantiate(nested_class_name)
            if not nested_instance is Resource:
                _set_error("ClassDB instantiated '%s' for property '%s', but it's not a Resource. (instance: %s)" % [nested_class_name, prop_name, nested_instance], spb.get_position())
                return null
            # If it's a Resource without an explicit script (e.g., built-in), population might still work
            if debug_mode and nested_instance.get_script() == null:
                push_warning("Instantiated nested object '%s' via ClassDB without a script. Population relies on ClassDB properties." % nested_class_name)
        else:
            # Cannot find script or instantiate via ClassDB
            _set_error("Could not find preloaded schema or instantiate class '%s' (required by property '%s')." % [nested_class_name, prop_name], spb.get_position())
            return null

    if nested_instance == null:
        _set_error("Failed to create instance of nested resource '%s' for property '%s'." % [nested_class_name, prop_name], spb.get_position())
        return null

    # Recursively populate the nested instance
    if not _populate_resource_from_bytes(nested_instance, spb):
        # Error should be set by the recursive call
        if not has_error(): _set_error("Failed during recursive population for nested resource '%s' of type '%s'." % [prop_name, nested_class_name], spb.get_position())
        return null

    return nested_instance

# --- Specific Message/Structure Readers ---

# Reads UpdateStatus structure (handles enum tag)
func _read_update_status(spb: StreamPeerBuffer) -> UpdateStatusData:
    var resource := UpdateStatusData.new()
    var tag := read_u8(spb) # Enum tag
    if has_error(): return null

    match tag:
        UpdateStatusData.StatusType.COMMITTED: # 0
            resource.status_type = UpdateStatusData.StatusType.COMMITTED
            var db_update_res = DatabaseUpdateData.new()
            if not _populate_resource_from_bytes(db_update_res, spb): return null
            resource.committed_update = db_update_res
        UpdateStatusData.StatusType.FAILED: # 1
            resource.status_type = UpdateStatusData.StatusType.FAILED
            resource.failure_message = read_string_with_u32_len(spb)
        UpdateStatusData.StatusType.OUT_OF_ENERGY: # 2
            resource.status_type = UpdateStatusData.StatusType.OUT_OF_ENERGY
        _:
            _set_error("Unknown UpdateStatus tag: %d" % tag, spb.get_position() - 1)
            return null

    return null if has_error() else resource

# Reads the Vec<TableUpdate> structure specifically
func _read_array_of_table_updates(spb: StreamPeerBuffer, resource: Resource, prop: Dictionary) -> Array:
    var start_pos := spb.get_position()
    var length := read_u32_le(spb)
    if debug_mode: print("DEBUG: _read_array_of_table_updates: Called for '%s' at pos %d. Read length: %d. New pos: %d" % [prop.name, start_pos, length, spb.get_position()])
    if has_error(): return []
    if length == 0: return []
    if length > MAX_VEC_LEN: _set_error("DatabaseUpdate tables length %d exceeds limit %d" % [length, MAX_VEC_LEN], start_pos); return []

    var result_array := []; result_array.resize(length)

    for i in range(length):
        if has_error(): return []
        var element_start_pos = spb.get_position()
        var table_update_instance = TableUpdateData.new()
        # Use the specialized instance reader for TableUpdateData's complex structure
        if not _read_table_update_instance(spb, table_update_instance):
            if not has_error(): _set_error("Failed reading TableUpdate element %d" % i, element_start_pos)
            return []
        result_array[i] = table_update_instance

    return result_array

# Reads the content of a SINGLE TableUpdate structure into an existing instance.
# Handles the custom CompressableQueryUpdate format for deletes/inserts.
func _read_table_update_instance(spb: StreamPeerBuffer, resource: TableUpdateData) -> bool:
    # Read standard fields first using direct readers
    resource.table_id = read_u32_le(spb)
    resource.table_name = read_string_with_u32_len(spb)
    resource.num_rows = read_u64_le(spb)
    if has_error(): return false

    # Now handle the custom CompressableQueryUpdate structure
    var updates_count := read_u32_le(spb) # Number of CompressableQueryUpdate blocks
    if has_error(): return false

    var all_parsed_deletes: Array[Resource] = []
    var all_parsed_inserts: Array[Resource] = []

    var table_name_lower := resource.table_name.to_lower().replace("_","")
    var row_schema_script := _schema.get_type(table_name_lower)
    
    var row_spb := StreamPeerBuffer.new()
    
    if not row_schema_script and updates_count > 0:
        if debug_mode: push_warning("No row schema found for table '%s', cannot deserialize rows. Skipping row data." % resource.table_name)
        # Consume the data even if we can't parse it
        
        for i in range(updates_count):
            if has_error(): break
            var update_start_pos := spb.get_position()
            var query_update_spb: StreamPeerBuffer = _get_query_update_stream(spb, resource.table_name)
            if has_error() or query_update_spb == null:
                if not has_error(): _set_error("Failed to get query update stream for table '%s'." % resource.table_name, update_start_pos)
                break
            read_bsatn_row_list(query_update_spb); if has_error(): break # Consume deletes
            read_bsatn_row_list(query_update_spb); if has_error(): break # Consume inserts
            if query_update_spb != spb and query_update_spb.get_position() < query_update_spb.get_size():
                push_error("Extra %d bytes remaining in skipped QueryUpdate block for table '%s'" % [query_update_spb.get_size() - query_update_spb.get_position(), resource.table_name])
        resource.deletes.assign([]); resource.inserts.assign([])
        return not has_error()

    # Schema found, parse rows
    for i in range(updates_count):
        if has_error(): break
        var update_start_pos := spb.get_position()
        var query_update_spb: StreamPeerBuffer = _get_query_update_stream(spb, resource.table_name)
        if has_error() or query_update_spb == null:
            if not has_error(): _set_error("Failed to get query update stream for table '%s'." % resource.table_name, update_start_pos)
            break

        var raw_deletes := read_bsatn_row_list(query_update_spb); if has_error(): break
        var raw_inserts := read_bsatn_row_list(query_update_spb); if has_error(): break

        if query_update_spb != spb and query_update_spb.get_position() < query_update_spb.get_size():
            push_error("Extra %d bytes remaining in decompressed QueryUpdate block for table '%s'" % [query_update_spb.get_size() - query_update_spb.get_position(), resource.table_name])

        # Process deletes
        for raw_row_bytes in raw_deletes:
            var row_resource = row_schema_script.new()
            row_spb.data_array = raw_row_bytes
            row_spb.seek(0) # Важно! Сбрасываем позицию на начало
            #var row_spb := StreamPeerBuffer.new(); row_spb.data_array = raw_row_bytes
            if _populate_resource_from_bytes(row_resource, row_spb):
                if row_spb.get_position() < row_spb.get_size(): push_error("Extra %d bytes after parsing delete row for table '%s'" % [row_spb.get_size() - row_spb.get_position(), resource.table_name])
                all_parsed_deletes.append(row_resource)
            else: push_error("Stopping update processing for table '%s' due to delete row parsing failure." % resource.table_name); break
        if has_error(): break

        # Process inserts
        for raw_row_bytes in raw_inserts:
            var row_resource = row_schema_script.new()
            row_spb.data_array = raw_row_bytes
            row_spb.seek(0) # Важно! Сбрасываем позицию на начало
            if _populate_resource_from_bytes(row_resource, row_spb):
                if row_spb.get_position() < row_spb.get_size(): push_error("Extra %d bytes after parsing insert row for table '%s'" % [row_spb.get_size() - row_spb.get_position(), resource.table_name])
                all_parsed_inserts.append(row_resource)
            else: push_error("Stopping update processing for table '%s' due to insert row parsing failure." % resource.table_name); break
        if has_error(): break

    if has_error(): return false

    resource.deletes.assign(all_parsed_deletes)
    resource.inserts.assign(all_parsed_inserts)
    return true

# Helper to handle potential compression of a QueryUpdate block.
func _get_query_update_stream(spb: StreamPeerBuffer, table_name_for_error: String) -> StreamPeerBuffer:
    var compression_tag_raw := read_u8(spb)
    if has_error(): return null

    match compression_tag_raw:
        COMPRESSION_NONE:
            return spb

        COMPRESSION_GZIP:
            var compressed_len := read_u32_le(spb)
            if has_error(): return null
            if compressed_len == 0:return StreamPeerBuffer.new()
            
            var compressed_data := read_bytes(spb, compressed_len)
            var decompressed_data := DataDecompressor.decompress_packet(compressed_data)
            var temp_spb := StreamPeerBuffer.new()
            temp_spb.data_array = decompressed_data
            return temp_spb
        _:
            _set_error("Unknown QueryUpdate compression tag %d for table '%s'" % [compression_tag_raw, table_name_for_error], spb.get_position() - 1)
            return null

# Manual reader specifically for SubscriptionErrorMessage due to Option<T> fields
# Keep this manual until Option<T> is handled generically (if ever needed)
func _read_subscription_error_manual(spb: StreamPeerBuffer) -> SubscriptionErrorMessage:
    var resource := SubscriptionErrorMessage.new()

    resource.total_host_execution_duration_micros = read_u64_le(spb); if has_error(): return null

    # Read Option<u32> request_id (0 = Some, 1 = None)
    var req_id_tag = read_u8(spb); if has_error(): return null
    if req_id_tag == 0: resource.request_id = read_u32_le(spb)
    elif req_id_tag == 1: resource.request_id = -1 # Using -1 to represent None
    else: _set_error("Invalid tag %d for Option<u32> request_id" % req_id_tag, spb.get_position() - 1); return null
    if has_error(): return null

    # Read Option<u32> query_id
    var query_id_tag = read_u8(spb); if has_error(): return null
    if query_id_tag == 0: resource.query_id = read_u32_le(spb)
    elif query_id_tag == 1: resource.query_id = -1 # Using -1 to represent None
    else: _set_error("Invalid tag %d for Option<u32> query_id" % query_id_tag, spb.get_position() - 1); return null
    if has_error(): return null

    # Read Option<TableId> table_id_resource
    var table_id_tag = read_u8(spb); if has_error(): return null
    if table_id_tag == 0: # Some(TableId)
        var table_id_res = TableIdData.new()
        if not _populate_resource_from_bytes(table_id_res, spb): return null
        resource.table_id_resource = table_id_res
    elif table_id_tag == 1: # None
        resource.table_id_resource = null
    else: _set_error("Invalid tag %d for Option<TableId>" % table_id_tag, spb.get_position() - 1); return null

    resource.error_message = read_string_with_u32_len(spb)
    return null if has_error() else resource

func process_bytes_and_extract_messages(new_data: PackedByteArray) -> Array[Resource]:
    if new_data.is_empty():
        return []
    _pending_data.append_array(new_data)
    var parsed_messages: Array[Resource] = []
    var spb := StreamPeerBuffer.new()
    while not _pending_data.is_empty():
        clear_error()
        spb.data_array = _pending_data
        spb.seek(0)
        var initial_buffer_size = _pending_data.size()
        var message_resource = _parse_message_from_stream(spb)

        if has_error():
            if _last_error.contains("past end of buffer"):
                clear_error()
                break
            else:
                printerr("BSATNDeserializer: Unrecoverable parsing error: %s. Clearing buffer to prevent infinite loop." % get_last_error())
                _pending_data.clear()
                break
                
        if message_resource:
            parsed_messages.append(message_resource)
            var bytes_consumed = spb.get_position()
            
            if bytes_consumed == 0:
                printerr("BSATNDeserializer: Parser consumed 0 bytes. Clearing buffer to prevent infinite loop.")
                _pending_data.clear()
                break
            _pending_data = _pending_data.slice(bytes_consumed)
        else:
            break
            
    return parsed_messages
    
# --- Top-Level Message Parsing ---
# Entry point: Parses the entire byte buffer into a top-level message Resource.
func parse_packet(buffer: PackedByteArray) -> Resource:
    push_warning("BSATNDeserializer.parse_packet is deprecated. Use process_bytes_and_extract_messages instead.")
    var results = process_bytes_and_extract_messages(buffer)
    return results[0] if not results.is_empty() else null
    

func _parse_message_from_stream(spb: StreamPeerBuffer) -> Resource:
    clear_error()
    #if spb.get_available_bytes().is_empty(): _set_error("Input buffer is empty", 0); return null
    
    var start_pos = spb.get_position()
    if not _check_read(spb, 1):
        return null
        
    var msg_type := read_u8(spb)
    if has_error(): return null

    var result_resource: Resource = null
    # Path to the GDScript file for the message type
    var resource_script_path := SpacetimeDBServerMessage.get_resource_path(msg_type)

    if resource_script_path == "":
        _set_error("Unknown server message type: 0x%02X" % msg_type, 1)
        return null
    
    # --- Special handling for types requiring manual parsing ---
    if msg_type == SpacetimeDBServerMessage.SUBSCRIPTION_ERROR:
        # Use the manual reader due to Option<T> complexity
        result_resource = _read_subscription_error_manual(spb)
        if has_error(): return null
        # Error message is printed by _set_error, but we can add context
        if result_resource.error_message: printerr("Subscription Error Received: ", result_resource.error_message)

    # --- TODO: Implement reader for OneOffQueryResponseData ---
    elif msg_type == SpacetimeDBServerMessage.ONE_OFF_QUERY_RESPONSE:
        _set_error("Reader for OneOffQueryResponse (0x04) not implemented.", spb.get_position() -1)
        return null # Or return an empty resource shell if preferred

    # --- Generic handling for types parsed via _populate_resource_from_bytes ---
    else:
        if not ResourceLoader.exists(resource_script_path):
            _set_error("Script not found for message type 0x%02X: %s" % [msg_type, resource_script_path], 1)
            return null
        var script: GDScript = ResourceLoader.load(resource_script_path, "GDScript")
        if not script or not script.can_instantiate():
            _set_error("Failed to load or instantiate script for message type 0x%02X: %s" % [msg_type, resource_script_path], 1)
            return null
        
        result_resource = script.new()
        if not _populate_resource_from_bytes(result_resource, spb):
            # Error already set by _populate_resource_from_bytes or its callees
            return null # Return null on population failure

    # Optional: Check if all bytes were consumed after parsing the message body
    var remaining_bytes := spb.get_size() - spb.get_position()
    if remaining_bytes > 0:
        # This might indicate a parsing error or extra data. Warning is appropriate.
        push_warning("Bytes remaining after parsing message type 0x%02X: %d" % [msg_type, remaining_bytes])

    return result_resource
