class_name SpacetimeSchemaParser

const GDNATIVE_PRIMITIVE_TYPES: Dictionary[String, String] = {
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
	"Bool": "bool",
	"Nil": "null", # For Option<()>
}
const GDNATIVE_ARRAYLIKE_TYPES: Dictionary[String, String] = {
	"Vector4": "Vector4",
	"Vector4I": "Vector4i",
	"Vector3": "Vector3",
	"Vector3I": "Vector3i",
	"Vector2": "Vector2",
	"Vector2I": "Vector2i",
	"Quaternion": "Quaternion",
	"Color": "Color",
}
const GDNATIVE_DICTLIKE_TYPES: Dictionary[String, String] = {
	"Plane": "Plane",
}
const DEFAULT_TYPE_MAP: Dictionary[String, String] = {
	"__identity__": "PackedByteArray",
	"__connection_id__": "PackedByteArray",
	"__uuid__": "PackedByteArray",
	"__timestamp_micros_since_unix_epoch__": "int",
	"__time_duration_micros__": "int",
	"U128": "PackedByteArray",
	"I128": "PackedByteArray",
	"U256": "PackedByteArray",
	"I256": "PackedByteArray",
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
	"U128": "u128",
	"I128": "i128",
	"U256": "u256",
	"I256": "i256",
	"F32": "f32",
	"F64": "f64",
	"String": "string", # For BSATN, e.g. option_string or vec_String (if Option<Array<String>>)
	"Bool": "bool", # For BSATN, e.g. option_bool
	"Nil": "nil", # For BSATN Option<()>
	"Vector4": "vector4", # For BSATN, e.g. vector4[f32,f32,f32,f32]
	"Vector4I": "vector4i", # For BSATN, e.g. vector4i[i32,i32,i32,i32]
	"Vector3": "vector3", # For BSATN, e.g. vector3[f32,f32,f32]
	"Vector3I": "vector3i", # For BSATN, e.g. vector3i[i32,i32,i32]
	"Vector2": "vector2", # For BSATN, e.g. vector2[f32,f32]
	"Vector2I": "vector2i", # For BSATN, e.g. vector2i[i32,i32]
	"Quaternion": "quaternion", # For BSATN, e.g. quaternion[f32,f32,f32,f32]
	"Color": "color", # For BSATN, e.g. color[f32,f32,f32,f32]
	"__identity__": "identity",
	"__connection_id__": "connection_id",
	# Uuid is Product { __uuid__: u128 } — wire-identical to u128 (16 bytes, reversed
	# on read yields canonical UUID byte order). Reuse the u128 reader/writer.
	"__uuid__": "u128",
	"__timestamp_micros_since_unix_epoch__": "i64",
	"__time_duration_micros__": "i64",
}


static func _sort_by_ty(a: Dictionary, b: Dictionary) -> bool:
	return a.get("ty", -1) < b.get("ty", -1)


static func _find_type_index(type_name: String, parsed_types_list: Array[Dictionary]) -> int:
	for i: int in parsed_types_list.size():
		if parsed_types_list[i].name == type_name:
			return i
	return -1


## Returns the index of the struct field with [param field_name], or -1 if absent.
static func _find_struct_field_index(struct_fields: Array, field_name: String) -> int:
	for i: int in struct_fields.size():
		if struct_fields[i].get("name", "") == field_name:
			return i
	return -1


# First key of [param d] without allocating its keys() Array — dicts iterate in
# insertion order, so the first yielded key is keys()[0]. For the String-keyed
# schema dicts parsed here (tagged-union tags, lifecycle specs).
static func _first_key(d: Dictionary) -> String:
	for k: String in d:
		return k
	return ""

# Synthesized sum types for anonymous inline `Result<T, E>` columns, accumulated by
# _parse_field_type during a parse and flushed into the type list afterward. Anonymous
# inline sums (the only ones are Option — handled separately — and Result) have no named
# Typespace entry, so we synthesize a named RustEnum-style type per distinct Result<T, E>
# and let the regular enum-with-payload codegen + BSATN path handle it. Keyed by the
# synthesized bare type name (e.g. "ResultI32String"); reset at the start of each parse.
static var _synth_result_types: Dictionary = { }


static func parse_schema(schema: Dictionary, module_name: String, project_enums: Dictionary = { }) -> SpacetimeParsedSchema:
	_synth_result_types.clear()
	var type_map: Dictionary[String, String] = DEFAULT_TYPE_MAP.duplicate() as Dictionary[String, String]
	type_map.merge(GDNATIVE_PRIMITIVE_TYPES)
	type_map.merge(GDNATIVE_ARRAYLIKE_TYPES)
	type_map.merge(GDNATIVE_DICTLIKE_TYPES)
	var meta_type_map: Dictionary = DEFAULT_META_TYPE_MAP.duplicate()

	var schema_tables: Array = []
	var schema_types_raw: Array = []
	var schema_reducers: Array = []
	var typespace: Array = []
	var misc_exports: Array = []

	if not schema.has("sections"):
		SpacetimePlugin.print_err("Schema v10 required (missing 'sections'). Please update SpacetimeDB to 2.1.0+.")
		return SpacetimeParsedSchema.new()

	var lifecycle_map: Dictionary = { } # function_name -> lifecycle spec key
	var schedules_by_table: Dictionary = { } # table source_name -> schedule dict
	var canonical_names: Dictionary = { } # source_name -> canonical_name
	var view_pk_by_view: Dictionary = { } # view source_name -> primary key column name

	# First pass: extract lifecycle, schedules, and explicit names
	for section: Dictionary in schema["sections"]:
		if section.has("LifeCycleReducers"):
			for lc: Dictionary in section["LifeCycleReducers"]:
				var fn_name: String = lc.get("function_name", "")
				var spec: Dictionary = lc.get("lifecycle_spec", { })
				if not fn_name.is_empty() and not spec.is_empty():
					lifecycle_map[fn_name] = _first_key(spec)
		elif section.has("Schedules"):
			for sched: Dictionary in section["Schedules"]:
				var tbl: String = sched.get("table_name", "")
				if not tbl.is_empty():
					schedules_by_table[tbl] = {
						"reducer_name": sched.get("function_name", ""),
						"schedule_at_col": sched.get("schedule_at_col", 0),
					}
		elif section.has("ExplicitNames"):
			for entry: Dictionary in section["ExplicitNames"].get("entries", []):
				var mapping: Dictionary = entry.get("Table", entry.get("Function", entry.get("Index", { })))
				if not mapping.is_empty():
					canonical_names[mapping.get("source_name", "")] = mapping.get("canonical_name", "")
		elif section.has("ViewPrimaryKeys"):
			# Schema V10 (SpacetimeDB 2.2.0+): primary keys for procedural views.
			# Single-column only for now; columns is a Vec to allow future composites.
			for vpk: Dictionary in section["ViewPrimaryKeys"]:
				var view_src: String = vpk.get("view_source_name", "")
				var pk_cols: Array = vpk.get("columns", [])
				if not view_src.is_empty() and not pk_cols.is_empty():
					view_pk_by_view[view_src] = String(pk_cols[0])

	# Second pass: extract content sections
	for section: Dictionary in schema["sections"]:
		if section.has("Typespace"):
			typespace = section["Typespace"].get("types", [])
		elif section.has("Types"):
			for td: Dictionary in section["Types"]:
				var src: String = td.get("source_name", { }).get("source_name", "")
				schema_types_raw.append({ "name": { "name": src }, "ty": td.get("ty", -1) })
		elif section.has("Tables"):
			for td: Dictionary in section["Tables"]:
				var src: String = td.get("source_name", "")
				# canonical_name is the name the server registers the table/reducer under and
				# uses on the wire (TableUpdate identifiers, reducer-call lookup). source_name is
				# only the original Rust spelling. Verified live: reducers resolve ONLY by
				# canonical (e.g. insert_one_u_128, not insert_one_u128).
				var name: String = canonical_names.get(src, src)
				var indexes: Array = []
				for idx: Dictionary in td.get("indexes", []):
					indexes.append({ "name": idx.get("source_name", { "some": null }), "accessor_name": idx.get("accessor_name", { "some": null }), "algorithm": idx.get("algorithm", { }) })
				var constraints: Array = []
				for con: Dictionary in td.get("constraints", []):
					constraints.append({ "name": con.get("source_name", { "some": null }), "data": con.get("data", { }) })
				var tbl: Dictionary = {
					"name": name,
					"product_type_ref": td.get("product_type_ref", -1),
					"primary_key": td.get("primary_key", []),
					"indexes": indexes,
					"constraints": constraints,
					"sequences": td.get("sequences", []),
					"table_type": td.get("table_type", { "User": [] }),
					"table_access": td.get("table_access", { "Public": [] }),
					"is_event": td.get("is_event", false),
				}
				if schedules_by_table.has(src):
					tbl["schedule"] = { "some": schedules_by_table[src] }
				elif schedules_by_table.has(name):
					tbl["schedule"] = { "some": schedules_by_table[name] }
				schema_tables.append(tbl)
		elif section.has("Reducers"):
			for rd: Dictionary in section["Reducers"]:
				var src: String = rd.get("source_name", "")
				var name: String = canonical_names.get(src, src)
				var r: Dictionary = { "name": name, "params": rd.get("params", { }), "ok_return_type": rd.get("ok_return_type", { }) }
				if lifecycle_map.has(src) or lifecycle_map.has(name):
					r["lifecycle"] = { "some": lifecycle_map.get(src, lifecycle_map.get(name, "")) }
				else:
					r["lifecycle"] = { "some": null }
				schema_reducers.append(r)
		elif section.has("Procedures"):
			for pd: Dictionary in section["Procedures"]:
				var src: String = pd.get("source_name", "")
				var name: String = canonical_names.get(src, src)
				misc_exports.append({ "Procedure": { "name": name, "params": pd.get("params", { }), "return_type": pd.get("return_type", { }) } })
		elif section.has("Views"):
			for vd: Dictionary in section["Views"]:
				var src: String = vd.get("source_name", "")
				var name: String = canonical_names.get(src, src)
				# ViewPrimaryKeys keys by view source name, so look up by src (not canonical name).
				var view_pk_name: String = view_pk_by_view.get(src, "")
				misc_exports.append({ "View": { "name": name, "return_type": vd.get("return_type", { }), "primary_key_name": view_pk_name } })

	schema_types_raw.sort_custom(_sort_by_ty)
	var parsed_schema: SpacetimeParsedSchema = SpacetimeParsedSchema.new()
	parsed_schema.module = module_name.to_pascal_case()

	var parsed_types_list: Array[Dictionary] = []
	for type_info: Dictionary in schema_types_raw:
		var type_name: String = type_info.get("name", { }).get("name", null)
		if not type_name:
			SpacetimePlugin.print_err("Invalid schema: Type name not found for type: %s" % type_info)
			return parsed_schema
		var type_data: Dictionary = { "name": type_name }
		if _is_gd_native(type_name):
			_set_gd_native(type_name, type_data)

		var ty_idx: int = int(type_info.get("ty", -1))
		if ty_idx == -1:
			SpacetimePlugin.print_err("Invalid schema: Type 'ty' not found for type: %s" % type_info)
			return parsed_schema
		if ty_idx >= typespace.size():
			SpacetimePlugin.print_err("Invalid schema: Type index %d out of bounds for typespace (size %d) for type %s" % [ty_idx, typespace.size(), type_name])
			return parsed_schema

		var current_type_definition: Dictionary = typespace[ty_idx]
		var struct_def: Dictionary = current_type_definition.get("Product", { })
		var sum_type_def: Dictionary = current_type_definition.get("Sum", { })
		if struct_def:
			var struct_elements: Array[Dictionary] = []
			for el: Dictionary in struct_def.get("elements", []):
				var data: Dictionary = {
					"name": el.get("name", { }).get("some", null),
				}
				var type: String = _parse_field_type(el.get("algebraic_type", { }), data, schema_types_raw)
				if not type.is_empty():
					data["type"] = type
				struct_elements.append(data)
			type_data["struct"] = struct_elements

			if not type_data.has("gd_native"):
				type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
				meta_type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
			elif not _validate_gd_native(type_name, type_data):
				# Error should be printed in _validate_gd_native
				return parsed_schema
			parsed_types_list.append(type_data)
		elif sum_type_def:
			var parsed_variants: Array[Dictionary] = []
			type_data["is_sum_type"] = _is_sum_type(sum_type_def)
			for v: Dictionary in sum_type_def.get("variants", []):
				var variant_data: Dictionary = { "name": v.get("name", { }).get("some", null) }
				var type: String = _parse_field_type(v.get("algebraic_type", { }), variant_data, schema_types_raw)
				if not type.is_empty():
					variant_data["type"] = type
				parsed_variants.append(variant_data)
			type_data["enum"] = parsed_variants
			parsed_types_list.append(type_data)

			if not type_data.get("is_sum_type"):
				meta_type_map[type_name] = "u8"
				var pascal_name: String = type_name if project_enums.has(type_name) else type_name.to_pascal_case()
				if project_enums.has(pascal_name):
					var project_enum: Dictionary = project_enums[pascal_name]
					var schema_variants: Array[String] = []
					for v: Dictionary in parsed_variants:
						schema_variants.append(v.get("name", "").to_snake_case())
					var project_variants: Array[String] = []
					for pv: String in project_enum["variants"]:
						project_variants.append(pv.to_snake_case())
					if schema_variants == project_variants:
						type_map[type_name] = project_enum["path"]
						type_data["project_enum"] = project_enum["path"]
						SpacetimePlugin.print_log("Enum '%s' matched project enum '%s'" % [pascal_name, project_enum["path"]])
					else:
						type_map[type_name] = "%sTypes.%s" % [module_name.to_pascal_case(), pascal_name]
						SpacetimePlugin.print_log("Enum '%s' found in project as '%s' but variants differ, generating standalone" % [pascal_name, project_enum["path"]])
				else:
					type_map[type_name] = "%sTypes.%s" % [module_name.to_pascal_case(), pascal_name]
			else:
				type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
				meta_type_map[type_name] = module_name.to_pascal_case() + type_name.to_pascal_case()
		else:
			if not type_data.has("gd_native"):
				if type_map.has(type_name) and not _is_gd_native(type_name):
					type_data["struct"] = []
					parsed_types_list.append(type_data)
				else:
					SpacetimePlugin.print_log("Type '%s' has no Product/Sum definition in typespace and is not GDNative. Skipping." % type_name)

	# Flush synthesized Result<T, E> types so codegen emits them (as RustEnum subclasses)
	# and fields referencing them resolve a type_idx below. Done after the main type loop
	# so all inline Results encountered while parsing fields/variants are included.
	for synth_name: String in _synth_result_types:
		parsed_types_list.append(_synth_result_types[synth_name])
		var synth_class: String = module_name.to_pascal_case() + synth_name.to_pascal_case()
		type_map[synth_name] = synth_class
		meta_type_map[synth_name] = synth_class

	for parsed_type: Dictionary in parsed_types_list:
		if not parsed_type.has("struct"):
			continue

		for field_type: Dictionary in parsed_type.get("struct", []):
			var type_name = field_type.get("type", null)
			if not type_name or GDNATIVE_PRIMITIVE_TYPES.has(type_name) or DEFAULT_TYPE_MAP.has(type_name):
				continue

			var type_idx: int = _find_type_index(type_name, parsed_types_list)
			if type_idx >= 0:
				field_type["type_idx"] = type_idx

	var parsed_tables_list: Array[Dictionary] = []
	var scheduled_reducers: Array[String] = []
	for table_info: Dictionary in schema_tables:
		var table_name_str: String = table_info.get("name", null)
		var ref_idx_raw = table_info.get("product_type_ref", null)
		if ref_idx_raw == null or table_name_str == null:
			continue
		var ref_idx: int = int(ref_idx_raw)

		var original_type_name_for_table: String = "UNKNOWN_TYPE_FOR_TABLE"
		if ref_idx < schema_types_raw.size():
			original_type_name_for_table = schema_types_raw[ref_idx].get("name", { }).get("name")
		var target_type_idx: int = _find_type_index(original_type_name_for_table, parsed_types_list)
		var target_type_def: Dictionary = parsed_types_list[target_type_idx] if target_type_idx >= 0 else { }

		if target_type_def.is_empty() or not target_type_def.has("struct"):
			SpacetimePlugin.print_err("Table '%s' refers to an invalid or non-struct type (index %s in original schema, name %s)." % [table_name_str, str(ref_idx), original_type_name_for_table if original_type_name_for_table else "N/A"])
			continue

		var table_data: Dictionary = {
			"name": table_name_str,
			"type_idx": target_type_idx,
			"is_event": table_info.get("is_event", false),
		}

		if not target_type_def.has("table_names"):
			target_type_def.table_names = []
		target_type_def.table_names.append(table_name_str)
		target_type_def.table_name = table_name_str

		var pk_col_idx: int = -1
		var primary_key_indices: Array = table_info.get("primary_key", [])
		if primary_key_indices.size() == 1:
			var pk_field_idx: int = int(primary_key_indices[0])
			if pk_field_idx < target_type_def.struct.size():
				var pk_field_name: String = target_type_def.struct[pk_field_idx].name
				pk_col_idx = pk_field_idx
				table_data.primary_key = pk_field_idx
				table_data.primary_key_name = pk_field_name
				target_type_def.primary_key = pk_field_idx
				target_type_def.primary_key_name = pk_field_name
			else:
				SpacetimePlugin.print_err("Primary key index %d out of bounds for table %s (struct size %d)" % [pk_field_idx, table_name_str, target_type_def.struct.size()])

		var parsed_unique_indexes: Array[Dictionary] = []
		var unique_col_set: Dictionary[int, bool] = { }
		var constraints_def = table_info.get("constraints", [])
		for constraint_def: Dictionary in constraints_def:
			var constraint_name_str: String = constraint_def.get("name", { }).get("some", null)
			var column_indices: Array = constraint_def.get("data", { }).get("Unique", { }).get("columns", [])
			if column_indices.size() != 1 or constraint_name_str == null:
				continue

			var unique_field_idx: int = int(column_indices[0])
			if unique_field_idx < target_type_def.struct.size():
				var unique_index: Dictionary = target_type_def.struct[unique_field_idx].duplicate()
				unique_index.constraint_name = constraint_name_str
				parsed_unique_indexes.append(unique_index)
				unique_col_set[unique_field_idx] = true
			else:
				SpacetimePlugin.print_err("Unique field index %d out of bounds for table %s (struct size %d)" % [unique_field_idx, table_name_str, target_type_def.struct.size()])

		table_data.unique_indexes = parsed_unique_indexes

		# Non-unique btree indexes get a filter() accessor. Single-column only;
		# skip columns already covered by the primary key or a unique index (those
		# expose find() and the auto-created btree mirror would only duplicate them).
		var parsed_btree_indexes: Array[Dictionary] = []
		for index_def: Dictionary in table_info.get("indexes", []):
			var btree_cols: Array = index_def.get("algorithm", { }).get("BTree", [])
			if btree_cols.size() != 1:
				continue
			var btree_col_idx: int = int(btree_cols[0])
			if btree_col_idx == pk_col_idx or unique_col_set.has(btree_col_idx):
				continue
			if btree_col_idx >= target_type_def.struct.size():
				SpacetimePlugin.print_err("BTree index column %d out of bounds for table %s (struct size %d)" % [btree_col_idx, table_name_str, target_type_def.struct.size()])
				continue
			parsed_btree_indexes.append(target_type_def.struct[btree_col_idx].duplicate())

		table_data.btree_indexes = parsed_btree_indexes

		var is_public: bool = true
		if not target_type_def.has("is_public"):
			target_type_def.is_public = []
		if table_info.get("table_access", { }).has("Private"):
			is_public = false

		table_data.is_public = is_public
		target_type_def.is_public.append(is_public)

		if table_info.get("schedule", { }).has("some"):
			var schedule = table_info.get("schedule", { }).some
			table_data.schedule = schedule
			target_type_def.schedule = schedule
			scheduled_reducers.append(schedule.reducer_name)
		parsed_tables_list.append(table_data)

	var parsed_reducers_list: Array[Dictionary] = []
	for reducer_info: Dictionary in schema_reducers:
		var lifecycle = reducer_info.get("lifecycle", { }).get("some", null)
		if lifecycle:
			continue
		var r_name: String = reducer_info.get("name", "")
		if r_name.is_empty():
			SpacetimePlugin.print_err("Reducer found with no name: %s" % [reducer_info])
			continue
		var reducer_data: Dictionary = { "name": r_name }

		var reducer_raw_params: Array = reducer_info.get("params", { }).get("elements", [])
		var reducer_params: Array[Dictionary] = []
		for raw_param: Dictionary in reducer_raw_params:
			var data: Dictionary = { "name": raw_param.get("name", { }).get("some", null) }
			var type: String = _parse_field_type(raw_param.get("algebraic_type", { }), data, schema_types_raw)
			data["type"] = type

			if type and not (GDNATIVE_PRIMITIVE_TYPES.has(type) or DEFAULT_TYPE_MAP.has(type)):
				var type_idx: int = _find_type_index(type, parsed_types_list)
				if type_idx >= 0:
					data["type_idx"] = type_idx
			reducer_params.append(data)
		reducer_data["params"] = reducer_params

		# Parse the reducer's ok return type (every v10 reducer carries one; a unit
		# return is an empty Product → empty type → no-op decode at the call site).
		var ret_data: Dictionary = { }
		var ret_type: String = _parse_field_type(reducer_info.get("ok_return_type", { }), ret_data, schema_types_raw)
		reducer_data["return_type"] = ret_type
		reducer_data["return_data"] = ret_data
		if ret_type and not (GDNATIVE_PRIMITIVE_TYPES.has(ret_type) or DEFAULT_TYPE_MAP.has(ret_type)):
			var ret_type_idx: int = _find_type_index(ret_type, parsed_types_list)
			if ret_type_idx >= 0:
				reducer_data["return_type_idx"] = ret_type_idx

		if r_name in scheduled_reducers:
			reducer_data["is_scheduled"] = true
		parsed_reducers_list.append(reducer_data)

	var parsed_procedures_list: Array[Dictionary] = []
	for export_dict: Dictionary in misc_exports:
		# --- Procedure exports ---
		var proc: Dictionary = export_dict.get("Procedure", { })
		if not proc.is_empty():
			var proc_name: String = proc.get("name", "")
			if proc_name.is_empty():
				SpacetimePlugin.print_err("Procedure found with no name: %s" % [proc])
				continue
			SpacetimePlugin.print_log("Parsing procedure: %s" % proc_name)
			var proc_data: Dictionary = { "name": proc_name }

			# Parse params (same as reducer params)
			var raw_params: Array = proc.get("params", { }).get("elements", [])
			var proc_params: Array[Dictionary] = []
			for raw_param: Dictionary in raw_params:
				var data: Dictionary = { "name": raw_param.get("name", { }).get("some", null) }
				var type: String = _parse_field_type(raw_param.get("algebraic_type", { }), data, schema_types_raw)
				data["type"] = type

				if type and not (GDNATIVE_PRIMITIVE_TYPES.has(type) or DEFAULT_TYPE_MAP.has(type)):
					var type_idx: int = _find_type_index(type, parsed_types_list)
					if type_idx >= 0:
						data["type_idx"] = type_idx
				proc_params.append(data)
			proc_data["params"] = proc_params

			# Parse return type
			var ret_data: Dictionary = { }
			var ret_type: String = _parse_field_type(proc.get("return_type", { }), ret_data, schema_types_raw)
			proc_data["return_type"] = ret_type
			proc_data["return_data"] = ret_data

			# Resolve return type_idx for BSATN type lookup
			if ret_type and not (GDNATIVE_PRIMITIVE_TYPES.has(ret_type) or DEFAULT_TYPE_MAP.has(ret_type)):
				var ret_type_idx: int = _find_type_index(ret_type, parsed_types_list)
				if ret_type_idx >= 0:
					proc_data["return_type_idx"] = ret_type_idx

			parsed_procedures_list.append(proc_data)
			continue

		# --- View exports ---
		var view: Dictionary = export_dict.get("View", { })
		if view.is_empty():
			continue
		var name: String = view.get("name", "")
		if name.is_empty():
			SpacetimePlugin.print_err("View found with no name: %s" % [view])
			continue
		var return_type_dict: Dictionary = view.get("return_type", { })
		if return_type_dict.is_empty():
			SpacetimePlugin.print_err("View '%s' has no return_type" % name)
			continue
		var type_index: int = -1
		var return_type: Dictionary
		SpacetimePlugin.print_log("parsing return type for view: %s" % name)
		if return_type_dict.get("Array", { }).is_empty():
			if not return_type_dict.get("Sum", { }).is_empty():
				var variants: Array = return_type_dict.get("Sum", { }).get("variants", [])
				if variants.size() == 2:
					if variants[0].get("name", { }).get("some", "") == "some":
						var ref_val = variants[0].get("algebraic_type", { }).get("Ref", null)
						if ref_val != null:
							type_index = int(ref_val)
							if type_index >= 0 and type_index < parsed_types_list.size():
								return_type = parsed_types_list[type_index]
							else:
								SpacetimePlugin.print_err("View '%s': Ref index %d out of bounds (types size %d)" % [name, type_index, parsed_types_list.size()])
								continue
			elif not return_type_dict.get("Product", { }).is_empty():
				# Query<T> views encode their return as a single-element product
				# { __query__: Ref(T) } (QUERY_VIEW_RETURN_TAG). Unwrap to the row type.
				var elements: Array = return_type_dict.get("Product", { }).get("elements", [])
				if elements.size() == 1 and elements[0].get("name", { }).get("some", "") == "__query__":
					var ref_val = elements[0].get("algebraic_type", { }).get("Ref", null)
					if ref_val != null:
						type_index = int(ref_val)
						if type_index >= 0 and type_index < parsed_types_list.size():
							return_type = parsed_types_list[type_index]
						else:
							SpacetimePlugin.print_err("View '%s': Ref index %d out of bounds (types size %d)" % [name, type_index, parsed_types_list.size()])
							continue
				else:
					SpacetimePlugin.print_err("View '%s': unsupported product return type: %s" % [name, return_type_dict])
					continue
			else:
				SpacetimePlugin.print_err("view return type not yet supported in the parser: %s" % [return_type_dict])
				continue
		else:
			var ref_val = return_type_dict.get("Array", { }).get("Ref", null)
			if ref_val == null:
				SpacetimePlugin.print_err("View '%s': Array return type has no Ref" % name)
				continue
			type_index = int(ref_val)
			if type_index < 0 or type_index >= parsed_types_list.size():
				SpacetimePlugin.print_err("View '%s': Ref index %d out of bounds (types size %d)" % [name, type_index, parsed_types_list.size()])
				continue
			return_type = parsed_types_list[type_index]
		if return_type.is_empty():
			SpacetimePlugin.print_err("view return type not found: %s" % [return_type_dict])
			continue

		# Resolve the view's primary key (Schema V10, SpacetimeDB 2.2.0+).
		# ViewPrimaryKeys gives a column name; map it to a field index in the struct.
		var view_pk_name: String = view.get("primary_key_name", "")
		var view_pk_idx: int = 0
		if not view_pk_name.is_empty():
			view_pk_idx = _find_struct_field_index(return_type.get("struct", []), view_pk_name)
			if view_pk_idx < 0:
				SpacetimePlugin.print_err("View '%s': primary key column '%s' not found in struct" % [name, view_pk_name])
				view_pk_idx = 0
				view_pk_name = ""

		if return_type.get("table_names", []).is_empty():
			return_type = {
				"name": return_type["name"],
				"struct": return_type["struct"],
				&"table_names": [
					"%s" % name,
				],
				&"table_name": "%s" % name,
				&"primary_key": view_pk_idx,
				&"primary_key_name": view_pk_name,
				&"is_public": [
					true,
				],
			}
		else:
			var type_table_list = return_type["table_names"]
			type_table_list.append(name)
			return_type["table_names"] = type_table_list
			var is_public_list = return_type["is_public"]
			is_public_list.append(true)
			return_type["is_public"] = is_public_list
			# Query-builder view reusing an existing table's row type: only override
			# the PK when this view declares its own ViewPrimaryKeys entry. An empty
			# view_pk_name must NOT clobber the underlying table's PK — the type_def is
			# shared by every table of this row type, and dropping it kills row_updated
			# for the table and the view alike. Leaving it inherits the table's PK,
			# matching SpacetimeDB's assign_query_view_primary_keys.
			if not view_pk_name.is_empty():
				return_type["primary_key"] = view_pk_idx
				return_type["primary_key_name"] = view_pk_name
		parsed_types_list[type_index] = return_type

		var tables_of_same_type: Array = []
		for table: Dictionary in parsed_tables_list:
			if table.get("type_idx", -1) == type_index:
				tables_of_same_type.append(table)
		var new_table_dict: Dictionary
		if tables_of_same_type.is_empty():
			new_table_dict = {
				"name": name,
				"type_idx": type_index,
				"primary_key": view_pk_idx,
				"primary_key_name": view_pk_name,
				"unique_indexes": [],
				"is_public": true,
			}
		else:
			new_table_dict = tables_of_same_type[0].duplicate()
			new_table_dict["name"] = name
			new_table_dict["is_public"] = true
		parsed_tables_list.append(new_table_dict)

	SpacetimePlugin.print_log("Schema parser finished")
	parsed_schema.types = parsed_types_list
	parsed_schema.reducers = parsed_reducers_list
	parsed_schema.procedures = parsed_procedures_list
	parsed_schema.tables = parsed_tables_list
	parsed_schema.type_map = type_map
	parsed_schema.meta_type_map = meta_type_map
	parsed_schema.typespace = typespace
	return parsed_schema


static func _is_gd_native(type_name: String) -> bool:
	return GDNATIVE_PRIMITIVE_TYPES.has(type_name) or GDNATIVE_ARRAYLIKE_TYPES.has(type_name) or GDNATIVE_DICTLIKE_TYPES.has(type_name)


static func _set_gd_native(type_name: String, type_data: Dictionary) -> void:
	type_data["gd_native"] = true

	if GDNATIVE_PRIMITIVE_TYPES.has(type_name):
		type_data["gd_primitive"] = true
	elif GDNATIVE_ARRAYLIKE_TYPES.has(type_name):
		type_data["gd_arraylike"] = true
	elif GDNATIVE_DICTLIKE_TYPES.has(type_name):
		type_data["gd_dictlike"] = true


static func _validate_gd_native(type_name: String, type_data: Dictionary) -> bool:
	if type_data.has("gd_primitive"):
		return true

	if type_data.has("gd_arraylike"):
		var expected_struct_size = 0
		var expected_primitive_type = "float"
		if type_name == "Vector4":
			expected_struct_size = 4
		elif type_name == "Vector4I":
			expected_struct_size = 4
			expected_primitive_type = "int"
		elif type_name == "Vector3":
			expected_struct_size = 3
		elif type_name == "Vector3I":
			expected_struct_size = 3
			expected_primitive_type = "int"
		elif type_name == "Vector2":
			expected_struct_size = 2
		elif type_name == "Vector2I":
			expected_struct_size = 2
			expected_primitive_type = "int"
		elif type_name == "Quaternion":
			expected_struct_size = 4
		elif type_name == "Color":
			expected_struct_size = 4
		else:
			SpacetimePlugin.print_err("Unsupported array-like GD native type: %s" % [type_name])
			return false

		if type_data.struct.size() != expected_struct_size:
			SpacetimePlugin.print_err("Array-like GD native type '%s' expected length of %d but is %d" % [type_name, expected_struct_size, type_data.struct.size()])
			return false

		for element: Dictionary in type_data.struct:
			var primitive_type = GDNATIVE_PRIMITIVE_TYPES.get(element.type, null)
			if not primitive_type:
				SpacetimePlugin.print_err("Property '%s' in array-like GD native type '%s' must be a primitive type" % [element.name, type_name])
				return false

			if primitive_type != expected_primitive_type:
				SpacetimePlugin.print_err("Property '%s' in array-like GD native type '%s' should map to a '%s' primitive type" % [element.name, type_name, expected_primitive_type])
				return false

	if type_data.has("gd_dictlike"):
		if type_name == "Plane":
			if not type_data.has("struct") or type_data.struct.size() != 4:
				SpacetimePlugin.print_err("Plane type expects 4 struct elements (normal.x, normal.y, normal.z, d), got %d" % (type_data.get("struct", []).size()))
				return false
			for element: Dictionary in type_data.struct:
				var primitive_type = GDNATIVE_PRIMITIVE_TYPES.get(element.type, null)
				if primitive_type != "float":
					SpacetimePlugin.print_err("Plane element '%s' must be a float type, got '%s'" % [element.name, element.type])
					return false

	return true


static func _is_sum_type(sum_def: Dictionary) -> bool:
	var variants = sum_def.get("variants", [])
	for variant: Dictionary in variants:
		var type = variant.get("algebraic_type", { })
		if not type.has("Product"):
			return true
		var elements = type.Product.get("elements", [])
		if not elements.is_empty():
			return true
	return false


static func _is_sum_option(sum_def: Dictionary) -> bool:
	var variants = sum_def.get("variants", [])
	if variants.size() != 2:
		return false

	var found_some: bool = false
	var found_none: bool = false
	var none_is_unit: bool = false

	for v_idx: int in variants.size():
		var v_name = variants[v_idx].get("name", { }).get("some", "")
		if v_name == "some":
			found_some = true
		elif v_name == "none":
			found_none = true
			var none_variant_type = variants[v_idx].get("algebraic_type", { })
			if none_variant_type.has("Product") and none_variant_type.Product.get("elements", []).is_empty():
				none_is_unit = true
			elif none_variant_type.is_empty():
				none_is_unit = true

	return found_some and found_none and none_is_unit


# Structural Result: exactly two variants named "ok" then "err" (lowercase, ok first).
# Matches SpacetimeDB's `SumType::is_result`. Option is checked separately and wins.
static func _is_sum_result(sum_def: Dictionary) -> bool:
	var variants: Array = sum_def.get("variants", [])
	if variants.size() != 2:
		return false
	var n0: String = variants[0].get("name", { }).get("some", "")
	var n1: String = variants[1].get("name", { }).get("some", "")
	return n0 == "ok" and n1 == "err"


# Synthesizes a named RustEnum-style sum type for an anonymous inline Result<T, E>,
# returning its bare type name (e.g. "ResultI32String"). The variant payload types are
# parsed exactly like normal enum variants so the standard enum codegen + BSATN path
# (u8 tag + payload) handles it. Deduped by name; flushed into the type list by
# parse_schema via [member _synth_result_types].
static func _synthesize_result_type(sum_def: Dictionary, schema_types: Array, depth: int) -> String:
	var variants: Array = sum_def.get("variants", [])
	var ok_data: Dictionary = { "name": "ok" }
	var ok_type: String = _parse_field_type(variants[0].get("algebraic_type", { }), ok_data, schema_types, depth + 1)
	if not ok_type.is_empty():
		ok_data["type"] = ok_type
	var err_data: Dictionary = { "name": "err" }
	var err_type: String = _parse_field_type(variants[1].get("algebraic_type", { }), err_data, schema_types, depth + 1)
	if not err_type.is_empty():
		err_data["type"] = err_type

	var synth_name: String = "Result%s%s" % [_result_name_part(ok_data), _result_name_part(err_data)]
	if not _synth_result_types.has(synth_name):
		_synth_result_types[synth_name] = {
			"name": synth_name,
			"is_sum_type": true,
			"enum": [ok_data, err_data],
		}
	return synth_name


# Builds a stable identifier fragment for a Result variant, folding in nesting markers
# (Vec/Option) so Result<Vec<i32>, _> and Result<i32, _> get distinct synthesized names.
static func _result_name_part(variant_data: Dictionary) -> String:
	var part: String = ""
	for marker: StringName in variant_data.get("nested_type", []):
		part += String(marker)
	part += variant_data.get("type", "Unit")
	var sanitized: String = ""
	for c: String in part:
		sanitized += c if c.is_valid_identifier() or c.is_valid_int() else "_"
	return sanitized


const _PARSE_FIELD_TYPE_MAX_DEPTH: int = 32


# Recursively parse a field type
static func _parse_field_type(field_type: Dictionary, data: Dictionary, schema_types: Array, depth: int = 0) -> String:
	if depth > _PARSE_FIELD_TYPE_MAX_DEPTH:
		SpacetimePlugin.print_err("_parse_field_type recursion exceeded %d levels; aborting" % _PARSE_FIELD_TYPE_MAX_DEPTH)
		return ""
	if field_type.has("Array"):
		var nested_type = data.get("nested_type", [])
		nested_type.append(&"Array")
		data["nested_type"] = nested_type
		if data.has("is_option"):
			data["is_array_inside_option"] = true
		else:
			data["is_array"] = true
		field_type = field_type.Array
		return _parse_field_type(field_type, data, schema_types, depth + 1)
	elif field_type.has("Product"):
		var elements: Array = field_type.Product.get("elements", [])
		if elements.is_empty():
			return ""
		return elements[0].get('name', { }).get('some', null)
	elif field_type.has("Sum"):
		# Anonymous inline Result<T, E> — synthesize a named RustEnum-style type and
		# return its name so it rides the enum-with-payload path (must precede the
		# generic collapse below, which would otherwise drop the err variant).
		if _is_sum_result(field_type.Sum):
			return _synthesize_result_type(field_type.Sum, schema_types, depth)
		if _is_sum_option(field_type.Sum):
			var nested_type = data.get("nested_type", [])
			nested_type.append(&"Option")
			data["nested_type"] = nested_type
			if data.has("is_array"):
				data["is_option_inside_array"] = true
			else:
				data["is_option"] = true
		field_type = field_type.Sum.variants[0].get('algebraic_type', { })
		return _parse_field_type(field_type, data, schema_types, depth + 1)
	elif field_type.has("Ref"):
		var ref_idx: int = int(field_type.Ref)
		if ref_idx < 0 or ref_idx >= schema_types.size():
			SpacetimePlugin.print_err("Invalid schema: Ref index %d out of bounds (typespace size %d)" % [ref_idx, schema_types.size()])
			return ""
		return schema_types[ref_idx].get("name", { }).get("name", null)
	else:
		if field_type.is_empty():
			SpacetimePlugin.print_err("Invalid schema: Empty algebraic_type encountered")
			return ""
		return _first_key(field_type)
