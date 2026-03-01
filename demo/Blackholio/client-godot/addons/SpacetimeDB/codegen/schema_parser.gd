class_name SpacetimeSchemaParser

const GDNATIVE_TYPES: Dictionary[String, String] = {
    "I8": "int",
    "I16": "int",
    "I32": "int",
    "I64": "int",
    "U8": "int",
    "U16": "int",
    "U32": "int",
    "U64": "int",
    "F32": "float",
    "F64": "float",
    "String": "String",
    "Vector4": "Vector4",
    "Vector4I": "Vector4i",
    "Vector3": "Vector3",
    "Vector3I": "Vector3i",
    "Vector2": "Vector2",
    "Vector2I": "Vector2i",
    "Plane": "Plane",
    "Color": "Color",
    "Quaternion": "Quaternion",
    "Bool": "bool",
    "Nil": "null", # For Option<()>
}

const DEFAULT_TYPE_MAP: Dictionary[String, String] = {
    "__identity__": "PackedByteArray",
    "__connection_id__": "PackedByteArray",
    "__timestamp_micros_since_unix_epoch__": "int",
    "__time_duration_micros__": "int",
}

const DEFAULT_META_TYPE_MAP: Dictionary[String, String] = {
    "I8": "i8",
    "I16": "i16",
    "I32": "i32",
    "I64": "i64",
    "U8": "u8",
    "U16": "u16",
    "U32": "u32",
    "U64": "u64",
    "F32": "f32",
    "F64": "f64",
    "String": "string", # For BSATN, e.g. option_string or vec_String (if Option<Array<String>>)
    "Bool": "bool",   # For BSATN, e.g. option_bool
    "Nil": "nil",     # For BSATN Option<()>
    "__identity__": "identity",
    "__connection_id__": "connection_id",
    "__timestamp_micros_since_unix_epoch__": "i64",
    "__time_duration_micros__": "i64",
}

static func parse_schema(schema: Dictionary, module_name: String) -> SpacetimeParsedSchema:
    var type_map: Dictionary[String, String] = DEFAULT_TYPE_MAP.duplicate() as Dictionary[String, String]
    type_map.merge(GDNATIVE_TYPES)
    var meta_type_map = DEFAULT_META_TYPE_MAP.duplicate()
    
    var schema_tables: Array = schema.get("tables", [])
    var schema_types_raw: Array = schema.get("types", [])
    schema_types_raw.sort_custom(func(a, b): return a.get("ty", -1) < b.get("ty", -1))
    var schema_reducers: Array = schema.get("reducers", [])
    var typespace: Array = schema.get("typespace", {}).get("types", [])
    
    var parsed_schema := SpacetimeParsedSchema.new()
    parsed_schema.module = module_name.to_pascal_case()
    
    var parsed_types_list: Array[Dictionary] = []
    for type_info in schema_types_raw:
        var type_name: String = type_info.get("name", {}).get("name", null)
        if not type_name:
            SpacetimePlugin.print_err("Invalid schema: Type name not found for type: %s" % type_info)
            return parsed_schema
        var type_data := {"name": type_name}
        if GDNATIVE_TYPES.has(type_name):
            type_data["gd_native"] = true
        
        var ty_idx := int(type_info.get("ty", -1)) 
        if ty_idx == -1:
            SpacetimePlugin.print_err("Invalid schema: Type 'ty' not found for type: %s" % type_info)
            return parsed_schema
        if ty_idx >= typespace.size():
            SpacetimePlugin.print_err("Invalid schema: Type index %d out of bounds for typespace (size %d) for type %s" % [ty_idx, typespace.size(), type_name])
            return parsed_schema

        var current_type_definition = typespace[ty_idx]
        var struct_def: Dictionary = current_type_definition.get("Product", {}) 
        var sum_type_def: Dictionary = current_type_definition.get("Sum", {}) 

        if struct_def:
            var struct_elements: Array[Dictionary] = []
            for el in struct_def.get("elements", []):
                var data = {
                    "name": el.get("name", {}).get("some", null),
                }
                var type = _parse_field_type(el.get("algebraic_type", {}), data, schema_types_raw)
                if not type.is_empty():
                    data["type"] = type
                struct_elements.append(data)
            
            if not type_data.has("gd_native"):
                type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
                meta_type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
            type_data["struct"] = struct_elements
            parsed_types_list.append(type_data)
        elif sum_type_def: 
            var parsed_variants := []
            type_data["is_sum_type"] = _is_sum_type(sum_type_def)
            for v in sum_type_def.get("variants", []):
                var variant_data := { "name": v.get("name",{}).get("some", null) }
                var type = _parse_sum_type(v.get("algebraic_type", {}), variant_data, schema_types_raw)
                if not type.is_empty():
                    variant_data["type"] = type
                parsed_variants.append(variant_data)			
            type_data["enum"] = parsed_variants
            parsed_types_list.append(type_data)

            if not type_data.get("is_sum_type"): 
                meta_type_map[type_name] = "u8"
                type_map[type_name] = "%sModuleClient.Types.%s" % [module_name.to_pascal_case(), type_name.to_pascal_case()]
            else: 
                type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
                meta_type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
        else:
            if not type_data.has("gd_native"):
                if type_map.has(type_name) and not GDNATIVE_TYPES.has(type_name):
                    type_data["struct"] = [] 
                    parsed_types_list.append(type_data)
                else:
                    SpacetimePlugin.print_log("Type '%s' has no Product/Sum definition in typespace and is not GDNative. Skipping." % type_name)
    
    var parsed_tables_list: Array[Dictionary] = []
    var scheduled_reducers: Array[String] = []
    for table_info in schema_tables:
        var table_name_str: String = table_info.get("name", null)
        var ref_idx_raw = table_info.get("product_type_ref", null)
        if ref_idx_raw == null or table_name_str == null: continue
        var ref_idx = int(ref_idx_raw)
        
        var target_type_def = null
        var target_type_idx = 0
        var original_type_name_for_table = "UNKNOWN_TYPE_FOR_TABLE"
        if ref_idx < schema_types_raw.size():
            original_type_name_for_table = schema_types_raw[ref_idx].get("name", {}).get("name")
            for pt in parsed_types_list:
                if pt.name == original_type_name_for_table:
                    target_type_def = pt
                    break
                target_type_idx += 1
        
        if target_type_def == null or not target_type_def.has("struct"):
            SpacetimePlugin.print_err("Table '%s' refers to an invalid or non-struct type (index %s in original schema, name %s)." % [table_name_str, str(ref_idx), original_type_name_for_table if original_type_name_for_table else "N/A"])
            continue
            
        var table_data := {
            "name": table_name_str,
            "type_idx": target_type_idx
        }

        if not target_type_def.has("table_names"):
            target_type_def.table_names = []
        target_type_def.table_names.append(table_name_str)
        target_type_def.table_name = table_name_str
        
        var primary_key_indices: Array = table_info.get("primary_key", [])
        if primary_key_indices.size() == 1:
            var pk_field_idx = int(primary_key_indices[0])
            if pk_field_idx < target_type_def.struct.size():
                var pk_field_name: String = target_type_def.struct[pk_field_idx].name
                table_data.primary_key = pk_field_idx
                table_data.primary_key_name = pk_field_name
                target_type_def.primary_key = pk_field_idx
                target_type_def.primary_key_name = pk_field_name
            else:
                SpacetimePlugin.print_err("Primary key index %d out of bounds for table %s (struct size %d)" % [pk_field_idx, table_name_str, target_type_def.struct.size()])
        
        var parsed_unique_indexes: Array[Dictionary] = []
        var constraints_def = table_info.get("constraints", [])
        for constraint_def in constraints_def:
            var constraint_name_str: String = constraint_def.get("name", {}).get("some", null)
            var column_indices: Array = constraint_def.get("data", {}).get("Unique", {}).get("columns", [])
            if column_indices.size() != 1 or constraint_name_str == null: continue
            
            var unique_field_idx = int(column_indices[0])
            if unique_field_idx < target_type_def.struct.size():
                var unique_index: Dictionary = target_type_def.struct[unique_field_idx].duplicate()
                unique_index.constraint_name = constraint_name_str
                parsed_unique_indexes.append(unique_index)
            else:
                SpacetimePlugin.print_err("Unique field index %d out of bounds for table %s (struct size %d)" % [unique_field_idx, table_name_str, target_type_def.struct.size()])
    
        table_data.unique_indexes = parsed_unique_indexes
        
        var is_public = true
        if not target_type_def.has("is_public"):
            target_type_def.is_public = []
        if table_info.get("table_access", {}).has("Private"):
            is_public = false
        
        table_data.is_public = is_public
        target_type_def.is_public.append(is_public)
        
        if table_info.get("schedule", {}).has("some"):
            var schedule = table_info.get("schedule", {}).some
            table_data.schedule = schedule
            target_type_def.schedule = schedule
            scheduled_reducers.append(schedule.reducer_name)
        parsed_tables_list.append(table_data)
    
    var parsed_reducers_list: Array[Dictionary] = [] 
    for reducer_info in schema_reducers:
        var lifecycle = reducer_info.get("lifecycle", {}).get("some", null)
        if lifecycle: continue 
        var r_name = reducer_info.get("name", null) 
        if r_name == null:
            SpacetimePlugin.print_err("Reducer found with no name: %s" % [reducer_info])
            continue
        var reducer_data: Dictionary = {"name": r_name}
        
        var reducer_raw_params = reducer_info.get("params", {}).get("elements", [])
        var reducer_params = []
        for raw_param in reducer_raw_params:
            var data = {"name": raw_param.get("name", {}).get("some", null)}
            var type = _parse_field_type(raw_param.get("algebraic_type", {}), data, schema_types_raw)
            data["type"] = type
            reducer_params.append(data)	
        reducer_data["params"] = reducer_params
        
        if r_name in scheduled_reducers:
            reducer_data["is_scheduled"] = true
        parsed_reducers_list.append(reducer_data)

    parsed_schema.types = parsed_types_list
    parsed_schema.reducers = parsed_reducers_list
    parsed_schema.tables = parsed_tables_list
    parsed_schema.type_map = type_map
    parsed_schema.meta_type_map = meta_type_map
    parsed_schema.typespace = typespace
    return parsed_schema

static func _is_sum_type(sum_def) -> bool:
    var variants = sum_def.get("variants", [])
    for variant in variants:
        var type = variant.get("algebraic_type", {})
        if not type.has("Product"):
            return true
        var elements = type.Product.get("elements", [])
        if elements.size() > 0:
            return true
    return false

static func _is_sum_option(sum_def) -> bool: 
    var variants = sum_def.get("variants", [])
    if variants.size() != 2:
        return false
    
    var name1 = variants[0].get("name", {}).get("some", "")
    var name2 = variants[1].get("name", {}).get("some", "")

    var found_some = false
    var found_none = false
    var none_is_unit = false

    for v_idx in range(variants.size()):
        var v_name = variants[v_idx].get("name", {}).get("some", "")
        if v_name == "some":
            found_some = true
        elif v_name == "none":
            found_none = true
            var none_variant_type = variants[v_idx].get("algebraic_type", {})
            if none_variant_type.has("Product") and none_variant_type.Product.get("elements", []).is_empty():
                none_is_unit = true
            elif none_variant_type.is_empty():
                none_is_unit = true


    return found_some and found_none and none_is_unit

# Recursively parse a field type
static func _parse_field_type(field_type: Dictionary, data: Dictionary, schema_types: Array) -> String:
    if field_type.has("Array"):
        var nested_type = data.get("nested_type", [])
        nested_type.append(&"Array")
        data["nested_type"] = nested_type
        if data.has("is_option"):
            data["is_array_inside_option"] = true
        else:
            data["is_array"] = true		
        field_type = field_type.Array
        return _parse_field_type(field_type, data, schema_types)
    elif field_type.has("Product"):
        return field_type.Product.get("elements", [])[0].get('name', {}).get('some', null)
    elif field_type.has("Sum"):
        if _is_sum_option(field_type.Sum):
            var nested_type = data.get("nested_type", [])
            nested_type.append(&"Option")
            data["nested_type"] = nested_type
            if data.has("is_array"):
                data["is_option_inside_array"] = true
            else:
                data["is_option"] = true			
        field_type = field_type.Sum.variants[0].get('algebraic_type', {})
        return _parse_field_type(field_type, data, schema_types)
    elif field_type.has("Ref"):
        return schema_types[field_type.Ref].get("name", {}).get("name", null)
    else:
        return field_type.keys()[0]

# Recursively parse a sum type
static func _parse_sum_type(variant_type: Dictionary, data: Dictionary, schema_types: Array) -> String:
    if variant_type.has("Array"):
        var nested_type = data.get("nested_type", [])
        nested_type.append(&"Array")
        data["nested_type"] = nested_type
        if data.has("is_option"):
            data["is_array_inside_option"] = true
        else:
            data["is_array"] = true
        variant_type = variant_type.Array
        return _parse_sum_type(variant_type, data, schema_types)
    elif variant_type.has("Product"):
        var variant_type_array = variant_type.Product.get("elements", [])
        if variant_type_array.size() >= 1:
            return variant_type_array[0].get('name', {}).get('some', null)
        else:
            return ""
    elif variant_type.has("Sum"):
        if _is_sum_option(variant_type.Sum):
            var nested_type = data.get("nested_type", [])
            nested_type.append(&"Option")
            data["nested_type"] = nested_type
            if data.has("is_array"):
                data["is_option_inside_array"] = true
            else:
                data["is_option"] = true
        variant_type = variant_type.Sum.variants[0].get('algebraic_type', {})
        return _parse_sum_type(variant_type, data, schema_types)
    elif variant_type.has("Ref"):
        return schema_types[variant_type.Ref].get("name", {}).get("name", null)
    else:
        return variant_type.keys()[0]
