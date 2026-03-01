class_name BSATNSerializer extends RefCounted

# --- Constants ---
const IDENTITY_SIZE := 32
const CONNECTION_ID_SIZE := 16

# --- Properties ---
var _last_error: String = ""
var _spb: StreamPeerBuffer # Internal buffer used by writing functions

# --- Initialization ---
func _init() -> void:
    _spb = StreamPeerBuffer.new()
    _spb.big_endian = false # Use Little-Endian

# --- Error Handling ---
func has_error() -> bool: return _last_error != ""
func get_last_error() -> String: var e = _last_error; _last_error = ""; return e
func clear_error() -> void: _last_error = ""
# Sets the error message if not already set. Internal use.
func _set_error(msg: String) -> void:
    if _last_error == "": # Prevent overwriting
        _last_error = "BSATNSerializer Error: %s" % msg
        printerr(_last_error) # Always print errors

# --- Primitive Value Writers ---
# These directly write basic types to the internal StreamPeerBuffer.

func write_i8(v: int) -> void:
    if v < -128 or v > 127: _set_error("Value %d out of range for i8" % v); v = 0
    _spb.put_u8(v if v >= 0 else v + 256)

func write_i16_le(v: int) -> void:
    if v < -32768 or v > 32767: _set_error("Value %d out of range for i16" % v); v = 0
    _spb.put_u16(v if v >= 0 else v + 65536)

func write_i32_le(v: int) -> void:
    if v < -2147483648 or v > 2147483647: _set_error("Value %d out of range for i32" % v); v = 0
    _spb.put_u32(v) # put_u32 handles negative i32 correctly via two's complement

func write_i64_le(v: int) -> void:
    _spb.put_u64(v) # put_u64 handles negative i64 correctly via two's complement

func write_u8(v: int) -> void:
    if v < 0 or v > 255: _set_error("Value %d out of range for u8" % v); v = 0
    _spb.put_u8(v)

func write_u16_le(v: int) -> void:
    if v < 0 or v > 65535: _set_error("Value %d out of range for u16" % v); v = 0
    _spb.put_u16(v)

func write_u32_le(v: int) -> void:
    if v < 0 or v > 4294967295: _set_error("Value %d out of range for u32" % v); v = 0
    _spb.put_u32(v)

func write_u64_le(v: int) -> void:
    if v < 0: _set_error("Value %d out of range for u64" % v); v = 0
    _spb.put_u64(v)

func write_f32_le(v: float) -> void:
    _spb.put_float(v)

func write_f64_le(v: float) -> void:
    _spb.put_double(v)

func write_bool(v: bool) -> void:
    _spb.put_u8(1 if v else 0)

func write_bytes(v: PackedByteArray) -> void:
    if v == null: v = PackedByteArray() # Avoid error on null
    var result = _spb.put_data(v)
    if result != OK: _set_error("StreamPeerBuffer.put_data failed with code %d" % result)

func write_string_with_u32_len(v: String) -> void:
    if v == null: v = ""
    var str_bytes := v.to_utf8_buffer()
    write_u32_le(str_bytes.size())
    if str_bytes.size() > 0: write_bytes(str_bytes)

func write_identity(v: PackedByteArray) -> void:
    if v == null or v.size() != IDENTITY_SIZE:
        _set_error("Invalid Identity value (null or size != %d)" % IDENTITY_SIZE)
        var default_bytes = PackedByteArray(); default_bytes.resize(IDENTITY_SIZE)
        write_bytes(default_bytes) # Write default value to avoid stopping serialization
        return
    write_bytes(v)

func write_connection_id(v: PackedByteArray) -> void:
    if v == null or v.size() != CONNECTION_ID_SIZE:
        _set_error("Invalid ConnectionId value (null or size != %d)" % CONNECTION_ID_SIZE)
        var default_bytes = PackedByteArray(); default_bytes.resize(CONNECTION_ID_SIZE)
        write_bytes(default_bytes) # Write default value
        return
    write_bytes(v)

func write_timestamp(v: int) -> void:
    write_i64_le(v) # Timestamps are typically i64

func write_vector3(v: Vector3) -> void:
    if v == null: v = Vector3.ZERO # Handle potential null value
    write_f32_le(v.x); write_f32_le(v.y); write_f32_le(v.z)

func write_vector2(v: Vector2) -> void:
    if v == null: v = Vector2.ZERO # Handle potential null value
    write_f32_le(v.x); write_f32_le(v.y)

func write_vector2i(v: Vector2i) -> void:
    if v == null: v = Vector2i.ZERO # Handle potential null value
    write_i32_le(v.x); write_i32_le(v.y)
    
func write_color(v: Color) -> void:
    if v == null: v = Color.BLACK # Handle potential null value
    write_f32_le(v.r); write_f32_le(v.g); write_f32_le(v.b); write_f32_le(v.a)

func write_quaternion(v: Quaternion) -> void:
    if v == null: v = Quaternion.IDENTITY # Handle potential null value
    write_f32_le(v.x); write_f32_le(v.y); write_f32_le(v.z); write_f32_le(v.w)

# Writes a PackedByteArray prefixed with its u32 length (Vec<u8> format)
func write_vec_u8(v: PackedByteArray) -> void:
    if v == null: v = PackedByteArray()
    write_u32_le(v.size())
    if v.size() > 0: write_bytes(v) # Avoid calling put_data with empty array if possible

#Writes a Rust sum type enum
func write_rust_enum(rust_enum: RustEnum) -> void:
    write_u8(rust_enum.value)
    var sub_class: String = rust_enum.get_meta("enum_options")[rust_enum.value]
    var data = rust_enum.data
    if sub_class.begins_with("vec"):
        if data is not Array:
            _set_error("Sum type of rust enum is Vec<T> but the godot type is not an array.")
            return
        var vec_type = sub_class.right(-4)
        # If it's an Option type, we need to remove the opt prefix for the serializer
        # This is a special case, the enum needs more info for the deserializer
        if vec_type.begins_with("opt"):
            vec_type = vec_type.right(-4)
        _write_argument_value(data, vec_type)
        return
    if sub_class.begins_with("opt"):
        if data is not Option:
            _set_error("Sum type of rust enum is Option<T> but the godot type is not an Option.")
            return
        var opt_type = sub_class.right(-4)
        # If it's a Vec type, we need to remove the vec prefix for the serializer
        # This is a special case, the enum needs more info for the deserializer
        if opt_type.begins_with("vec"):
            opt_type = opt_type.right(-4)
        _write_argument_value(data, opt_type)
        return
    if not sub_class.is_empty():
        if not data:
            data = _generate_default_type(sub_class)
        _write_argument_value(data, sub_class)

func _write_option(option_value: Option, option_property_name: StringName, rust_type: String) -> bool:
    if not option_value is Option:
        _set_error("Value provided to _write_option is not an Option instance (type: %s) for property '%s'." % [typeof(option_value), option_property_name])
        return false
    if option_value.is_none():
        write_u8(1) # Tag for None
        if has_error():
            _set_error("Failed to write None tag for Option property '%s'." % option_property_name)
            return false
        return true
    else: # is_some()
        write_u8(0) # Tag for Some
        if has_error():
            _set_error("Failed to write Some tag for Option property '%s'." % option_property_name)
            return false
        if rust_type.begins_with("vec"):
            if option_value.unwrap() is not Array:
                _set_error("Option type is Vec<T> but the godot type is not an array.")
                return false
            var vec_type = rust_type.right(-4)
            _write_argument_value(option_value.unwrap(), vec_type)
        else:
            _write_argument_value(option_value.unwrap(), rust_type)				
        return true

func _write_array_of_option(array_of_option_value: Array, rust_type: String) -> bool:
    write_u32_le(array_of_option_value.size())
    for option_value in array_of_option_value:
        if not _write_option(option_value, "", rust_type):
            return false
    return true

# --- Core Serialization Logic ---

# Helper to get the specific BSATN writer METHOD NAME based on metadata value.
func _get_specific_writer_method_name(bsatn_type_value) -> StringName:
    if bsatn_type_value == null: return &""
    var bsatn_type_str := str(bsatn_type_value).to_lower()
    match bsatn_type_str:
        &"u64": return &"write_u64_le"
        &"i64": return &"write_i64_le"
        &"u32": return &"write_u32_le"
        &"i32": return &"write_i32_le"
        &"u16": return &"write_u16_le"
        &"i16": return &"write_i16_le"
        &"u8": return &"write_u8"
        &"i8": return &"write_i8"
        &"identity": return &"write_identity"
        &"connection_id": return &"write_connection_id"
        &"timestamp": return &"write_timestamp"
        &"f64": return &"write_f64_le"
        &"f32": return &"write_f32_le"
        &"vec_u8": return &"write_vec_u8"
        &'string': return &'write_string_with_u32_len'
        &'bool': return &'write_bool'
        # Add other specific types mapped to writer methods if needed
        _: return &"" # Unknown or non-primitive type

# The central recursive function to write any value.
# Uses metadata for specific types, otherwise defaults based on Variant.Type.
func _write_value(value, value_variant_type: Variant.Type, specific_writer_override: StringName = &"", \
                  element_variant_type: Variant.Type = TYPE_MAX, \
                  element_class_name: StringName = &"" \
                 ) -> bool:
    # 1. Use specific writer method if provided (highest priority, except for arrays)
    if specific_writer_override != &"" and value_variant_type != TYPE_ARRAY:
        if has_method(specific_writer_override):
            call(specific_writer_override, value)
        else:
            _set_error("Internal error: Specific writer method '%s' not found." % specific_writer_override)
            return false # Critical error
    else:
        # 2. If no specific writer, use default based on Variant.Type
        match value_variant_type:
            TYPE_NIL: _set_error("Cannot serialize null value."); return false # Or handle as Option<T>?
            TYPE_BOOL: write_bool(value)
            TYPE_INT: write_i64_le(value)  # Default int serialization is i64
            TYPE_FLOAT: write_f32_le(value) # Default float serialization is f32
            TYPE_STRING: write_string_with_u32_len(value)
            TYPE_VECTOR2: write_vector2(value)
            TYPE_VECTOR2I: write_vector2i(value)
            TYPE_VECTOR3: write_vector3(value)
            TYPE_COLOR: write_color(value)
            TYPE_QUATERNION: write_quaternion(value)
            TYPE_PACKED_BYTE_ARRAY: write_vec_u8(value) # Default PackedByteArray serialization is Vec<u8>
            TYPE_ARRAY:
                if value == null: value = [] # Treat null array as empty for serialization
                if not value is Array: _set_error("Value is not an Array but type is TYPE_ARRAY"); return false
                # Element type info is required for recursive calls
                if element_variant_type == TYPE_MAX: _set_error("Cannot serialize array without element type info: " + str(value)); return false

                write_u32_le(value.size()) # Write array length (u32)
                
                for element in value:
                    if has_error(): return false # Stop early if an error occurred writing previous elements
                    # Recursively call _write_value for the element.
                    # Pass the element's type info.
                    # Crucially, pass the specific_writer_override determined from the *array's* metadata,
                    # as this override applies to the *elements*.
                    if not _write_value(element, element_variant_type, specific_writer_override, TYPE_MAX, element_class_name):
                        # Error should be set by the recursive call
                        if not has_error(): _set_error("Failed to write array element.") # Ensure error is set
                        return false
            TYPE_OBJECT:
                if value is Option:	
                    _set_error("Internal error: _write_value called directly for an Option instance. This should be handled by _serialize_resource_fields.")
                    return false
                if value is RustEnum:
                    write_rust_enum(value)
                elif value is Resource:
                    # Serialize nested resource fields *inline* without length prefix
                    if not _serialize_resource_fields(value): # Recursive call
                        # Error should be set by _serialize_resource_fields
                        return false
                else:
                    # Cannot serialize non-Resource objects by default
                    _set_error("Cannot serialize non-Resource Object value of type '%s'." % value.get_class()); return false
            _:
                # Type not handled by specific writers or default cases
                _set_error("Unsupported default value type '%s' for serialization." % type_string(value_variant_type)); return false

    # Check for errors one last time after attempting the write operation
    return not has_error()

# Serializes the fields of a Resource instance sequentially.
func _serialize_resource_fields(resource: Resource) -> bool:
    if not resource or not resource.get_script():
        _set_error("Cannot serialize fields of null or scriptless resource"); return false
    
    if resource is RustEnum:
        write_rust_enum(resource)
        return true
        
    
    var properties: Array = resource.get_script().get_script_property_list()
    for prop in properties:
        # Only serialize properties marked for storage
        if not (prop.usage & PROPERTY_USAGE_STORAGE): continue
        var prop_name: StringName = prop.name
        var prop_type: Variant.Type = prop.type
        var value = resource.get(prop_name) # Get the actual value from the resource instance
        var specific_writer_method: StringName = &""
        var element_type: Variant.Type = TYPE_MAX
        var element_class: StringName = &""

        # Check for 'bsatn_type' metadata to override default serialization for this field
        var meta_key := "bsatn_type_" + prop_name
        if resource.has_meta(meta_key):
            # This metadata applies to the field itself, or to the *elements* if it's an array.
            specific_writer_method = _get_specific_writer_method_name(resource.get_meta(meta_key))
            
        if value is Option:
            _write_option(value, prop_name, resource.get_meta(meta_key))
            continue
        
        # If the property is an array, we need element type info for the recursive call
        if prop_type == TYPE_ARRAY:
            # Extract element type info from the hint string (Godot 3 or 4 format)
            var hint_ok = false
            if prop.hint == PROPERTY_HINT_TYPE_STRING and ":" in prop.hint_string: # Godot 3: "Type:TypeName"
                var hint_parts = prop.hint_string.split(":", true, 1)
                if hint_parts.size() == 2:
                    if hint_parts[1] == "Option":						
                        _write_array_of_option(value, resource.get_meta(meta_key))
                        continue
                    # Need to check if this is a split type like 24/17
                    # Take the first part as the element type
                    var hint_type = hint_parts[0].split("/", true, 1) if "/" in hint_parts[0] else [hint_parts[0]]
                    element_type = int(hint_type[0])
                    if element_type == TYPE_OBJECT: element_class = hint_parts[1]					
                    hint_ok = true
            elif prop.hint == PROPERTY_HINT_ARRAY_TYPE: # Godot 4: "VariantType/ClassName:VariantType" or "VariantType:VariantType"
                var main_type_str = prop.hint_string.split(":", true, 1)[0]
                if "/" in main_type_str: var parts = main_type_str.split("/", true, 1); element_type = int(parts[0]); element_class = parts[1]
                else: element_type = int(main_type_str)
                hint_ok = true # Assume format is correct if hint matches

            if not hint_ok:
                _set_error("Array property '%s' needs a typed hint for serialization. Hint: %d, HintString: '%s'" % [prop_name, prop.hint, prop.hint_string]); return false

            # Call _write_value for the array. Pass the specific_writer_method (from array's metadata)
            # as the override for the ELEMENTS.
            if not _write_value(value, TYPE_ARRAY, specific_writer_method, element_type, element_class):
                if not has_error(): _set_error("Failed writing array property '%s'" % prop_name)
                return false
        else:
            # For non-arrays, call _write_value, passing the specific_writer_method for the value itself.
            if not _write_value(value, prop_type, specific_writer_method):
                if not has_error(): _set_error("Failed writing property '%s'" % prop_name)
                return false

    return true # All fields serialized successfully

# --- Argument Serialization Helpers (Optional - Keep if needed for specific use cases) ---

# Serializes an array of arguments into a single PackedByteArray block.
# Note: This uses default serialization rules and ignores metadata.
func _serialize_arguments(args_array: Array, rust_types: Array) -> PackedByteArray:
    var args_spb := StreamPeerBuffer.new(); args_spb.big_endian = false
    var original_main_spb := _spb; _spb = args_spb # Temporarily redirect writes

    for i in range(args_array.size()):
        var arg_value = args_array[i]
        var rust_type = ""
        if i < rust_types.size():
            rust_type = rust_types[i]
        if not _write_argument_value(arg_value, rust_type): # Use dedicated argument writer
            # Error should be set by _write_argument_value
            push_error("Failed to serialize argument %d." % i) # Add context
            _spb = original_main_spb # Restore main buffer
            return PackedByteArray() # Return empty on error

    _spb = original_main_spb # Restore main buffer
    return args_spb.data_array if not has_error() else PackedByteArray()

# Helper to write a single *argument* value (no metadata used).
func _write_argument_value(value, rust_type: String = "") -> bool:
    var value_type := typeof(value)
    match value_type:
        TYPE_NIL: _set_error("Cannot serialize null argument."); return false
        TYPE_BOOL: write_bool(value)
        TYPE_INT: 
            match rust_type:
                &"u8": write_u8(value)
                &"u16": write_u16_le(value)
                &"u32": write_u32_le(value)
                &"u64": write_u64_le(value)
                &"i8": write_i8(value)
                &"i16": write_i16_le(value)
                &"i32": write_i32_le(value)
                _: write_i64_le(value) #Default i64
        TYPE_FLOAT: 
            match rust_type:
                &"f64": write_f64_le(value)
                _: write_f32_le(value) # Default f32
        TYPE_STRING: write_string_with_u32_len(value)
        TYPE_VECTOR2: write_vector2(value)
        TYPE_VECTOR2I: write_vector2i(value)
        TYPE_VECTOR3: write_vector3(value)
        TYPE_COLOR: write_color(value)
        TYPE_QUATERNION: write_quaternion(value)
        TYPE_PACKED_BYTE_ARRAY: write_vec_u8(value) # Default Vec<u8> for arguments
        TYPE_ARRAY:
            write_u32_le(value.size()) # Write array length (u32)
            for v in value:
                _write_argument_value(v, rust_type)
        TYPE_OBJECT:
            if value is RustEnum:
                write_rust_enum(value)				
            elif value is Option:
                _write_option(value, "", rust_type)				
            elif value is Resource:
                # Serialize resource fields directly inline (recursive)
                if not _serialize_resource_fields(value):
                    if not has_error(): _set_error("Failed to serialize nested Resource argument.")
                    return false
            else:
                _set_error("Cannot serialize non-Resource Object argument."); return false
        _:
            _set_error("Unsupported argument type: %s" % type_string(value_type)); return false
    return not has_error()

#Helper to generate a zero struct from a rust type
func _generate_default_type(rust_type: String) -> Variant:
    match rust_type:
        &"i8", &"i16", &"i32", &"i64", &"u8", &"u16", &"u32", &"u64":
            return int(0)
        &"f32", &"f64":
            return float(0)
        &"bool": return false
        &"String": return ""
        &"Vector3": return Vector3.ZERO
        &"Vector2": return Vector2.ZERO
        &"Vector2i": return Vector2i.ZERO
        &"Color": return Color.BLACK
        &"Quaternion": return Quaternion.IDENTITY
        _: return null

# Helper to serialize a single Resource argument into raw bytes (e.g., for reducer calls)
func _serialize_resource_argument(resource_arg: Resource) -> PackedByteArray:
    if not resource_arg: _set_error("Cannot serialize null resource argument."); return PackedByteArray()
    var arg_spb := StreamPeerBuffer.new(); arg_spb.big_endian = false
    var original_main_spb := _spb; _spb = arg_spb # Redirect writes

    if not _serialize_resource_fields(resource_arg):
        # Error should be set by _serialize_resource_fields
        push_error("Failed to serialize resource argument fields.") # Add context
        _spb = original_main_spb # Restore
        return PackedByteArray()

    _spb = original_main_spb # Restore
    return arg_spb.data_array if not has_error() else PackedByteArray()

# --- Public API ---

# Serializes a complete ClientMessage (variant tag + payload resource fields).
func serialize_client_message(variant_tag: int, payload_resource: Resource) -> PackedByteArray:
    clear_error(); _spb.data_array = PackedByteArray(); _spb.seek(0) # Reset state

    # 1. Write the message variant tag (u8)
    write_u8(variant_tag)
    if has_error(): return PackedByteArray()

    # 2. Serialize payload resource fields inline after the tag
    if payload_resource != null: # Allow null payload for messages without one
        if not _serialize_resource_fields(payload_resource):
            if not has_error(): _set_error("Failed to serialize payload resource for tag %d" % variant_tag)
            return PackedByteArray()
    # else: No payload to serialize

    return _spb.data_array if not has_error() else PackedByteArray()
