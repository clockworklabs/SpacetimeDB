use crate::util::{
    collect_case, is_reducer_invokable, iter_indexes, iter_procedures, iter_reducers, iter_table_names_and_types,
    iter_types, print_auto_generated_file_comment, print_auto_generated_version_comment, type_ref_name,
};
use crate::{CodegenOptions, OutputFile};

use super::code_indenter::{CodeIndenter, Indenter};
use super::Lang;

use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::version::spacetimedb_lib_version;
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{IndexAlgorithm, ModuleDef, ReducerDef, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};

use std::collections::BTreeSet;

const INDENT: &str = "    ";
const SDK_PKG: &str = "com.clockworklabs.spacetimedb_kotlin_sdk.shared_client";

/// Kotlin hard keywords that must be escaped with backticks when used as identifiers.
/// See: https://kotlinlang.org/docs/keyword-reference.html#hard-keywords
const KOTLIN_HARD_KEYWORDS: &[&str] = &[
    "as",
    "break",
    "class",
    "continue",
    "do",
    "else",
    "false",
    "for",
    "fun",
    "if",
    "in",
    "interface",
    "is",
    "null",
    "object",
    "package",
    "return",
    "super",
    "this",
    "throw",
    "true",
    "try",
    "typealias",
    "typeof",
    "val",
    "var",
    "when",
    "while",
];

/// Escapes a Kotlin identifier with backticks if it collides with a hard keyword.
fn kotlin_ident(name: String) -> String {
    if KOTLIN_HARD_KEYWORDS.contains(&name.as_str()) {
        format!("`{name}`")
    } else {
        name
    }
}

pub struct Kotlin;

impl Lang for Kotlin {
    fn generate_type_files(&self, _module: &ModuleDef, _typ: &TypeDef) -> Vec<OutputFile> {
        // All types are emitted in a single Types.kt file via generate_global_files.
        vec![]
    }

    fn generate_table_file_from_schema(&self, module: &ModuleDef, table: &TableDef, schema: TableSchema) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out);
        writeln!(out);

        let type_ref = table.product_type_ref;
        let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();
        let type_name = type_ref_name(module, type_ref);
        let table_name_pascal = table.accessor_name.deref().to_case(Case::Pascal);

        let is_event = table.is_event;

        // Check if this table has user-defined indexes (event tables never have indexes)
        let has_unique_index = !is_event
            && iter_indexes(table).any(|idx| idx.accessor_name.is_some() && schema.is_unique(&idx.algorithm.columns()));
        let has_btree_index = !is_event
            && iter_indexes(table)
                .any(|idx| idx.accessor_name.is_some() && !schema.is_unique(&idx.algorithm.columns()));

        // Collect indexed column positions for IxCols generation
        let mut ix_col_positions: BTreeSet<usize> = BTreeSet::new();
        if !is_event {
            for idx in iter_indexes(table) {
                if let IndexAlgorithm::BTree(btree) = &idx.algorithm {
                    for col_pos in btree.columns.iter() {
                        ix_col_positions.insert(col_pos.idx());
                    }
                }
            }
        }
        let has_ix_cols = !ix_col_positions.is_empty();

        // Imports
        if has_btree_index {
            writeln!(out, "import {SDK_PKG}.BTreeIndex");
        }
        writeln!(out, "import {SDK_PKG}.Col");
        writeln!(out, "import {SDK_PKG}.DbConnection");
        writeln!(out, "import {SDK_PKG}.EventContext");
        writeln!(out, "import {SDK_PKG}.InternalSpacetimeApi");
        if has_ix_cols {
            writeln!(out, "import {SDK_PKG}.IxCol");
        }
        if is_event {
            writeln!(out, "import {SDK_PKG}.RemoteEventTable");
        } else if table.primary_key.is_some() {
            writeln!(out, "import {SDK_PKG}.RemotePersistentTableWithPrimaryKey");
        } else {
            writeln!(out, "import {SDK_PKG}.RemotePersistentTable");
        }
        writeln!(out, "import {SDK_PKG}.TableCache");
        if has_unique_index {
            writeln!(out, "import {SDK_PKG}.UniqueIndex");
        }
        writeln!(out, "import {SDK_PKG}.protocol.QueryResult");
        gen_and_print_imports(module, out, product_def.element_types());

        writeln!(out);

        // Table handle class
        let table_marker = if is_event {
            "RemoteEventTable"
        } else if table.primary_key.is_some() {
            "RemotePersistentTableWithPrimaryKey"
        } else {
            "RemotePersistentTable"
        };
        writeln!(out, "/** Client-side handle for the `{}` table. */", table.name.deref());
        writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
        writeln!(out, "class {table_name_pascal}TableHandle internal constructor(");
        out.indent(1);
        writeln!(out, "private val conn: DbConnection,");
        writeln!(out, "private val tableCache: TableCache<{type_name}, *>,");
        out.dedent(1);
        writeln!(out, ") : {table_marker}<{type_name}> {{");
        out.indent(1);

        // Constants
        writeln!(out, "companion object {{");
        out.indent(1);
        writeln!(out, "const val TABLE_NAME = \"{}\"", table.name.deref());
        writeln!(out);
        // Field name constants
        for (ident, _) in product_def.elements.iter() {
            let const_name = ident.deref().to_case(Case::ScreamingSnake);
            writeln!(out, "const val FIELD_{const_name} = \"{}\"", ident.deref());
        }
        writeln!(out);
        writeln!(out, "fun createTableCache(): TableCache<{type_name}, *> {{");
        out.indent(1);
        // Primary key extractor
        if let Some(pk_col) = table.primary_key {
            let pk_field = table.get_column(pk_col).unwrap();
            let pk_field_camel = kotlin_ident(pk_field.accessor_name.deref().to_case(Case::Camel));
            writeln!(
                out,
                "return TableCache.withPrimaryKey({{ reader -> {type_name}.decode(reader) }}) {{ row -> row.{pk_field_camel} }}"
            );
        } else {
            writeln!(
                out,
                "return TableCache.withContentKey {{ reader -> {type_name}.decode(reader) }}"
            );
        }
        out.dedent(1);
        writeln!(out, "}}");
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);

        // Accessors (event tables don't store rows)
        if !is_event {
            writeln!(out, "override fun count(): Int = tableCache.count()");
            writeln!(out, "override fun all(): List<{type_name}> = tableCache.all()");
            writeln!(out, "override fun iter(): Sequence<{type_name}> = tableCache.iter()");
            writeln!(out);
        }

        // Callbacks
        writeln!(
            out,
            "override fun onInsert(cb: (EventContext, {type_name}) -> Unit) {{ tableCache.onInsert(cb) }}"
        );
        writeln!(
            out,
            "override fun removeOnInsert(cb: (EventContext, {type_name}) -> Unit) {{ tableCache.removeOnInsert(cb) }}"
        );
        if !is_event {
            writeln!(
                out,
                "override fun onDelete(cb: (EventContext, {type_name}) -> Unit) {{ tableCache.onDelete(cb) }}"
            );
            if table.primary_key.is_some() {
                writeln!(out, "override fun onUpdate(cb: (EventContext, {type_name}, {type_name}) -> Unit) {{ tableCache.onUpdate(cb) }}");
            }
            writeln!(out, "override fun onBeforeDelete(cb: (EventContext, {type_name}) -> Unit) {{ tableCache.onBeforeDelete(cb) }}");
            writeln!(out);
            writeln!(out, "override fun removeOnDelete(cb: (EventContext, {type_name}) -> Unit) {{ tableCache.removeOnDelete(cb) }}");
            if table.primary_key.is_some() {
                writeln!(out, "override fun removeOnUpdate(cb: (EventContext, {type_name}, {type_name}) -> Unit) {{ tableCache.removeOnUpdate(cb) }}");
            }
            writeln!(out, "override fun removeOnBeforeDelete(cb: (EventContext, {type_name}) -> Unit) {{ tableCache.removeOnBeforeDelete(cb) }}");
        }
        writeln!(out);

        // Index properties
        let get_field_name_and_type = |col_pos: ColId| -> (String, String) {
            let (field_name, field_type) = &product_def.elements[col_pos.idx()];
            let name_camel = kotlin_ident(field_name.deref().to_case(Case::Camel));
            let kt_type = kotlin_type(module, field_type);
            (name_camel, kt_type)
        };

        for idx in iter_indexes(table) {
            let Some(accessor_name) = idx.accessor_name.as_ref() else {
                // System-generated indexes don't get client-side accessors
                continue;
            };

            let columns = idx.algorithm.columns();
            let is_unique = schema.is_unique(&columns);
            let index_name_camel = kotlin_ident(accessor_name.deref().to_case(Case::Camel));
            let index_class = if is_unique { "UniqueIndex" } else { "BTreeIndex" };

            match columns.as_singleton() {
                Some(col_pos) => {
                    // Single-column index
                    let (field_camel, kt_ty) = get_field_name_and_type(col_pos);
                    writeln!(
                        out,
                        "val {index_name_camel} = {index_class}<{type_name}, {kt_ty}>(tableCache) {{ it.{field_camel} }}"
                    );
                }
                None => {
                    // Multi-column index
                    let col_fields: Vec<(String, String)> = columns.iter().map(get_field_name_and_type).collect();

                    match col_fields.len() {
                        2 => {
                            let col_types = format!("{}, {}", col_fields[0].1, col_fields[1].1);
                            let key_expr = format!("Pair(it.{}, it.{})", col_fields[0].0, col_fields[1].0);
                            writeln!(
                                out,
                                "val {index_name_camel} = {index_class}<{type_name}, Pair<{col_types}>>(tableCache) {{ {key_expr} }}"
                            );
                        }
                        3 => {
                            let col_types = format!("{}, {}, {}", col_fields[0].1, col_fields[1].1, col_fields[2].1);
                            let key_expr = format!(
                                "Triple(it.{}, it.{}, it.{})",
                                col_fields[0].0, col_fields[1].0, col_fields[2].0
                            );
                            writeln!(
                                out,
                                "val {index_name_camel} = {index_class}<{type_name}, Triple<{col_types}>>(tableCache) {{ {key_expr} }}"
                            );
                        }
                        _ => {
                            let key_expr_fields = col_fields
                                .iter()
                                .map(|(name, _)| format!("it.{name}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            writeln!(
                                out,
                                "val {index_name_camel} = {index_class}<{type_name}, List<Any?>>(tableCache) {{ listOf({key_expr_fields}) }}"
                            );
                        }
                    }
                }
            }
            writeln!(out);
        }

        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);

        // --- {Table}Cols class: typed column references for all fields ---
        writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
        writeln!(out, "class {table_name_pascal}Cols(tableName: String) {{");
        out.indent(1);
        for (ident, field_type) in product_def.elements.iter() {
            let field_camel = kotlin_ident(ident.deref().to_case(Case::Camel));
            let col_name = ident.deref();
            let value_type = match field_type {
                AlgebraicTypeUse::Option(inner) => kotlin_type(module, inner),
                _ => kotlin_type(module, field_type),
            };
            writeln!(
                out,
                "val {field_camel} = Col<{type_name}, {value_type}>(tableName, \"{col_name}\")"
            );
        }
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);

        // --- {Table}IxCols class: typed column references for indexed fields only ---
        if has_ix_cols {
            writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
            writeln!(out, "class {table_name_pascal}IxCols(tableName: String) {{");
            out.indent(1);
            for (i, (ident, field_type)) in product_def.elements.iter().enumerate() {
                if !ix_col_positions.contains(&i) {
                    continue;
                }
                let field_camel = kotlin_ident(ident.deref().to_case(Case::Camel));
                let col_name = ident.deref();
                let value_type = match field_type {
                    AlgebraicTypeUse::Option(inner) => kotlin_type(module, inner),
                    _ => kotlin_type(module, field_type),
                };
                writeln!(
                    out,
                    "val {field_camel} = IxCol<{type_name}, {value_type}>(tableName, \"{col_name}\")"
                );
            }
            out.dedent(1);
            writeln!(out, "}}");
        } else {
            // No indexed columns — emit a simple empty class
            writeln!(out, "class {table_name_pascal}IxCols");
        }

        OutputFile {
            filename: format!("{table_name_pascal}TableHandle.kt"),
            code: output.into_inner(),
        }
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out);
        writeln!(out);

        let reducer_name_pascal = reducer.accessor_name.deref().to_case(Case::Pascal);

        // Imports
        writeln!(out, "import {SDK_PKG}.bsatn.BsatnReader");
        writeln!(out, "import {SDK_PKG}.bsatn.BsatnWriter");
        gen_and_print_imports(module, out, reducer.params_for_generate.element_types());

        writeln!(out);

        // Emit args data class with encode/decode (if there are params)
        if !reducer.params_for_generate.elements.is_empty() {
            writeln!(out, "/** Arguments for the `{}` reducer. */", reducer.name.deref());
            writeln!(out, "data class {reducer_name_pascal}Args(");
            out.indent(1);
            for (i, (ident, ty)) in reducer.params_for_generate.elements.iter().enumerate() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                let kotlin_ty = kotlin_type(module, ty);
                let comma = if i + 1 < reducer.params_for_generate.elements.len() {
                    ","
                } else {
                    ""
                };
                writeln!(out, "val {field_name}: {kotlin_ty}{comma}");
            }
            out.dedent(1);
            writeln!(out, ") {{");
            out.indent(1);

            // encode method
            writeln!(out, "/** Encodes these arguments to BSATN. */");
            writeln!(out, "fun encode(): ByteArray {{");
            out.indent(1);
            writeln!(out, "val writer = BsatnWriter()");
            for (ident, ty) in reducer.params_for_generate.elements.iter() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                write_encode_field(module, out, &field_name, ty);
            }
            writeln!(out, "return writer.toByteArray()");
            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out);

            // companion object with decode
            writeln!(out, "companion object {{");
            out.indent(1);
            writeln!(out, "/** Decodes [{reducer_name_pascal}Args] from BSATN. */");
            writeln!(out, "fun decode(reader: BsatnReader): {reducer_name_pascal}Args {{");
            out.indent(1);
            for (ident, ty) in reducer.params_for_generate.elements.iter() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                write_decode_field(module, out, &field_name, ty);
            }
            let field_names: Vec<String> = reducer
                .params_for_generate
                .elements
                .iter()
                .map(|(ident, _)| kotlin_ident(ident.deref().to_case(Case::Camel)))
                .collect();
            let args = field_names.join(", ");
            writeln!(out, "return {reducer_name_pascal}Args({args})");
            out.dedent(1);
            writeln!(out, "}}");
            out.dedent(1);
            writeln!(out, "}}");

            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out);
        }

        // Reducer companion object
        writeln!(out, "/** Constants for the `{}` reducer. */", reducer.name.deref());
        writeln!(out, "object {reducer_name_pascal}Reducer {{");
        out.indent(1);
        writeln!(out, "const val REDUCER_NAME = \"{}\"", reducer.name.deref());
        out.dedent(1);
        writeln!(out, "}}");

        OutputFile {
            filename: format!("{reducer_name_pascal}Reducer.kt"),
            code: output.into_inner(),
        }
    }

    fn generate_procedure_file(
        &self,
        module: &ModuleDef,
        procedure: &spacetimedb_schema::def::ProcedureDef,
    ) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out);
        writeln!(out);

        // Imports
        writeln!(out, "import {SDK_PKG}.bsatn.BsatnReader");
        writeln!(out, "import {SDK_PKG}.bsatn.BsatnWriter");
        gen_and_print_imports(
            module,
            out,
            procedure
                .params_for_generate
                .element_types()
                .chain([&procedure.return_type_for_generate]),
        );

        let procedure_name_pascal = procedure.accessor_name.deref().to_case(Case::Pascal);

        if procedure.params_for_generate.elements.is_empty() {
            writeln!(out, "object {procedure_name_pascal}Procedure {{");
            out.indent(1);
            writeln!(out, "const val PROCEDURE_NAME = \"{}\"", procedure.name.deref());
            let return_ty = kotlin_type(module, &procedure.return_type_for_generate);
            writeln!(out, "// Returns: {return_ty}");
            out.dedent(1);
            writeln!(out, "}}");
        } else {
            writeln!(out, "/** Arguments for the `{}` procedure. */", procedure.name.deref());
            writeln!(out, "data class {procedure_name_pascal}Args(");
            out.indent(1);
            for (i, (ident, ty)) in procedure.params_for_generate.elements.iter().enumerate() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                let kotlin_ty = kotlin_type(module, ty);
                let comma = if i + 1 < procedure.params_for_generate.elements.len() {
                    ","
                } else {
                    ""
                };
                writeln!(out, "val {field_name}: {kotlin_ty}{comma}");
            }
            out.dedent(1);
            writeln!(out, ") {{");
            out.indent(1);

            // encode method
            writeln!(out, "/** Encodes these arguments to BSATN. */");
            writeln!(out, "fun encode(): ByteArray {{");
            out.indent(1);
            writeln!(out, "val writer = BsatnWriter()");
            for (ident, ty) in procedure.params_for_generate.elements.iter() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                write_encode_field(module, out, &field_name, ty);
            }
            writeln!(out, "return writer.toByteArray()");
            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out);

            // companion object with decode
            writeln!(out, "companion object {{");
            out.indent(1);
            writeln!(out, "/** Decodes [{procedure_name_pascal}Args] from BSATN. */");
            writeln!(out, "fun decode(reader: BsatnReader): {procedure_name_pascal}Args {{");
            out.indent(1);
            for (ident, ty) in procedure.params_for_generate.elements.iter() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                write_decode_field(module, out, &field_name, ty);
            }
            let field_names: Vec<String> = procedure
                .params_for_generate
                .elements
                .iter()
                .map(|(ident, _)| kotlin_ident(ident.deref().to_case(Case::Camel)))
                .collect();
            let args = field_names.join(", ");
            writeln!(out, "return {procedure_name_pascal}Args({args})");
            out.dedent(1);
            writeln!(out, "}}");
            out.dedent(1);
            writeln!(out, "}}");
            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out);
            writeln!(out, "object {procedure_name_pascal}Procedure {{");
            out.indent(1);
            writeln!(out, "const val PROCEDURE_NAME = \"{}\"", procedure.name.deref());
            let return_ty = kotlin_type(module, &procedure.return_type_for_generate);
            writeln!(out, "// Returns: {return_ty}");
            out.dedent(1);
            writeln!(out, "}}");
        }

        OutputFile {
            filename: format!("{procedure_name_pascal}Procedure.kt"),
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
        let files = vec![
            generate_types_file(module),
            generate_remote_tables_file(module, options),
            generate_remote_reducers_file(module, options),
            generate_remote_procedures_file(module, options),
            generate_module_file(module, options),
        ];

        files
    }
}

// --- Type mapping ---

fn kotlin_type(module: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
    match ty {
        AlgebraicTypeUse::Unit => "Unit".to_string(),
        AlgebraicTypeUse::Never => "Nothing".to_string(),
        AlgebraicTypeUse::Identity => "Identity".to_string(),
        AlgebraicTypeUse::ConnectionId => "ConnectionId".to_string(),
        AlgebraicTypeUse::Timestamp => "Timestamp".to_string(),
        AlgebraicTypeUse::TimeDuration => "TimeDuration".to_string(),
        AlgebraicTypeUse::ScheduleAt => "ScheduleAt".to_string(),
        AlgebraicTypeUse::Uuid => "SpacetimeUuid".to_string(),
        AlgebraicTypeUse::Option(inner_ty) => format!("{}?", kotlin_type(module, inner_ty)),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => format!(
            "SpacetimeResult<{}, {}>",
            kotlin_type(module, ok_ty),
            kotlin_type(module, err_ty)
        ),
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::Bool => "Boolean",
            PrimitiveType::I8 => "Byte",
            PrimitiveType::U8 => "UByte",
            PrimitiveType::I16 => "Short",
            PrimitiveType::U16 => "UShort",
            PrimitiveType::I32 => "Int",
            PrimitiveType::U32 => "UInt",
            PrimitiveType::I64 => "Long",
            PrimitiveType::U64 => "ULong",
            PrimitiveType::I128 => "Int128",
            PrimitiveType::U128 => "UInt128",
            PrimitiveType::I256 => "Int256",
            PrimitiveType::U256 => "UInt256",
            PrimitiveType::F32 => "Float",
            PrimitiveType::F64 => "Double",
        }
        .to_string(),
        AlgebraicTypeUse::String => "String".to_string(),
        AlgebraicTypeUse::Array(elem_ty) => {
            if matches!(&**elem_ty, AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                return "ByteArray".to_string();
            }
            format!("List<{}>", kotlin_type(module, elem_ty))
        }
        AlgebraicTypeUse::Ref(r) => type_ref_name(module, *r),
    }
}

/// Returns the FQN import path for a type. Used for import statements.
fn kotlin_type_fqn(_module: &ModuleDef, ty: &AlgebraicTypeUse) -> Option<String> {
    match ty {
        AlgebraicTypeUse::Identity => Some(format!("{SDK_PKG}.type.Identity")),
        AlgebraicTypeUse::ConnectionId => Some(format!("{SDK_PKG}.type.ConnectionId")),
        AlgebraicTypeUse::Timestamp => Some(format!("{SDK_PKG}.type.Timestamp")),
        AlgebraicTypeUse::TimeDuration => Some(format!("{SDK_PKG}.type.TimeDuration")),
        AlgebraicTypeUse::ScheduleAt => Some(format!("{SDK_PKG}.type.ScheduleAt")),
        AlgebraicTypeUse::Uuid => Some(format!("{SDK_PKG}.type.SpacetimeUuid")),
        AlgebraicTypeUse::Result { .. } => Some(format!("{SDK_PKG}.SpacetimeResult")),
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::I128 => Some(format!("{SDK_PKG}.Int128")),
            PrimitiveType::U128 => Some(format!("{SDK_PKG}.UInt128")),
            PrimitiveType::I256 => Some(format!("{SDK_PKG}.Int256")),
            PrimitiveType::U256 => Some(format!("{SDK_PKG}.UInt256")),
            _ => None,
        },
        _ => None,
    }
}

// --- BSATN encode/decode generation helpers ---

/// Write the BSATN encode call for a single field.
fn write_encode_field(module: &ModuleDef, out: &mut Indenter, field_name: &str, ty: &AlgebraicTypeUse) {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => {
            let method = match prim {
                PrimitiveType::Bool => "writeBool",
                PrimitiveType::I8 => "writeI8",
                PrimitiveType::U8 => "writeU8",
                PrimitiveType::I16 => "writeI16",
                PrimitiveType::U16 => "writeU16",
                PrimitiveType::I32 => "writeI32",
                PrimitiveType::U32 => "writeU32",
                PrimitiveType::I64 => "writeI64",
                PrimitiveType::U64 => "writeU64",
                PrimitiveType::F32 => "writeF32",
                PrimitiveType::F64 => "writeF64",
                PrimitiveType::I128 | PrimitiveType::U128 | PrimitiveType::I256 | PrimitiveType::U256 => {
                    // These SDK wrapper types have their own encode method
                    writeln!(out, "{field_name}.encode(writer)");
                    return;
                }
            };
            writeln!(out, "writer.{method}({field_name})");
        }
        AlgebraicTypeUse::String => {
            writeln!(out, "writer.writeString({field_name})");
        }
        AlgebraicTypeUse::Identity
        | AlgebraicTypeUse::ConnectionId
        | AlgebraicTypeUse::Timestamp
        | AlgebraicTypeUse::TimeDuration
        | AlgebraicTypeUse::Uuid => {
            writeln!(out, "{field_name}.encode(writer)");
        }
        AlgebraicTypeUse::ScheduleAt => {
            writeln!(out, "{field_name}.encode(writer)");
        }
        AlgebraicTypeUse::Ref(_) => {
            writeln!(out, "{field_name}.encode(writer)");
        }
        AlgebraicTypeUse::Option(inner) => {
            writeln!(out, "if ({field_name} != null) {{");
            out.indent(1);
            writeln!(out, "writer.writeSumTag(0u)");
            write_encode_value(module, out, field_name, inner);
            out.dedent(1);
            writeln!(out, "}} else {{");
            out.indent(1);
            writeln!(out, "writer.writeSumTag(1u)");
            out.dedent(1);
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Array(elem) => {
            if matches!(&**elem, AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                writeln!(out, "writer.writeByteArray({field_name})");
            } else {
                writeln!(out, "writer.writeArrayLen({field_name}.size)");
                writeln!(out, "for (elem in {field_name}) {{");
                out.indent(1);
                write_encode_value(module, out, "elem", elem);
                out.dedent(1);
                writeln!(out, "}}");
            }
        }
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            writeln!(out, "when ({field_name}) {{");
            out.indent(1);
            writeln!(out, "is SpacetimeResult.Ok -> {{");
            out.indent(1);
            writeln!(out, "writer.writeSumTag(0u)");
            write_encode_value(module, out, &format!("{field_name}.value"), ok_ty);
            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out, "is SpacetimeResult.Err -> {{");
            out.indent(1);
            writeln!(out, "writer.writeSumTag(1u)");
            write_encode_value(module, out, &format!("{field_name}.error"), err_ty);
            out.dedent(1);
            writeln!(out, "}}");
            out.dedent(1);
            writeln!(out, "}}");
        }
        AlgebraicTypeUse::Unit => {
            // Unit is encoded as empty product — nothing to write
        }
        AlgebraicTypeUse::Never => {
            writeln!(out, "// Never type — unreachable");
        }
    }
}

/// Write encode for a value expression (not a field reference).
fn write_encode_value(module: &ModuleDef, out: &mut Indenter, expr: &str, ty: &AlgebraicTypeUse) {
    // For simple primitives, delegate to the same logic
    write_encode_field(module, out, expr, ty);
}

/// Write the BSATN decode expression for a type, returning a string expression.
fn write_decode_expr(module: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => {
            let method = match prim {
                PrimitiveType::Bool => "reader.readBool()",
                PrimitiveType::I8 => "reader.readI8()",
                PrimitiveType::U8 => "reader.readU8()",
                PrimitiveType::I16 => "reader.readI16()",
                PrimitiveType::U16 => "reader.readU16()",
                PrimitiveType::I32 => "reader.readI32()",
                PrimitiveType::U32 => "reader.readU32()",
                PrimitiveType::I64 => "reader.readI64()",
                PrimitiveType::U64 => "reader.readU64()",
                PrimitiveType::F32 => "reader.readF32()",
                PrimitiveType::F64 => "reader.readF64()",
                PrimitiveType::I128 => "Int128.decode(reader)",
                PrimitiveType::U128 => "UInt128.decode(reader)",
                PrimitiveType::I256 => "Int256.decode(reader)",
                PrimitiveType::U256 => "UInt256.decode(reader)",
            };
            method.to_string()
        }
        AlgebraicTypeUse::String => "reader.readString()".to_string(),
        AlgebraicTypeUse::Identity => "Identity.decode(reader)".to_string(),
        AlgebraicTypeUse::ConnectionId => "ConnectionId.decode(reader)".to_string(),
        AlgebraicTypeUse::Timestamp => "Timestamp.decode(reader)".to_string(),
        AlgebraicTypeUse::TimeDuration => "TimeDuration.decode(reader)".to_string(),
        AlgebraicTypeUse::ScheduleAt => "ScheduleAt.decode(reader)".to_string(),
        AlgebraicTypeUse::Uuid => "SpacetimeUuid.decode(reader)".to_string(),
        AlgebraicTypeUse::Ref(r) => {
            let name = type_ref_name(module, *r);
            format!("{name}.decode(reader)")
        }
        AlgebraicTypeUse::Unit => "Unit".to_string(),
        AlgebraicTypeUse::Never => "error(\"Never type\")".to_string(),
        // Option, Array, Result are handled inline in write_decode_field
        AlgebraicTypeUse::Option(_) | AlgebraicTypeUse::Array(_) | AlgebraicTypeUse::Result { .. } => {
            // These need multi-line decode; handled by write_decode_field
            String::new()
        }
    }
}

/// Returns true if the type can be decoded as a single expression.
fn is_simple_decode(ty: &AlgebraicTypeUse) -> bool {
    !matches!(
        ty,
        AlgebraicTypeUse::Option(_) | AlgebraicTypeUse::Array(_) | AlgebraicTypeUse::Result { .. }
    )
}

/// Write the decode for a field, assigning to a val.
fn write_decode_field(module: &ModuleDef, out: &mut Indenter, var_name: &str, ty: &AlgebraicTypeUse) {
    match ty {
        AlgebraicTypeUse::Option(inner) => {
            if is_simple_decode(inner) {
                let inner_expr = write_decode_expr(module, inner);
                writeln!(
                    out,
                    "val {var_name} = if (reader.readSumTag().toInt() == 0) {inner_expr} else null"
                );
            } else {
                writeln!(out, "val {var_name} = if (reader.readSumTag().toInt() == 0) {{");
                out.indent(1);
                write_decode_field(module, out, "__inner", inner);
                writeln!(out, "__inner");
                out.dedent(1);
                writeln!(out, "}} else null");
            }
        }
        AlgebraicTypeUse::Array(elem) => {
            if matches!(&**elem, AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                writeln!(out, "val {var_name} = reader.readByteArray()");
            } else if is_simple_decode(elem) {
                let elem_expr = write_decode_expr(module, elem);
                writeln!(out, "val {var_name} = List(reader.readArrayLen()) {{ {elem_expr} }}");
            } else {
                writeln!(out, "val __{var_name}Len = reader.readArrayLen()");
                writeln!(out, "val {var_name} = buildList(__{var_name}Len) {{");
                out.indent(1);
                writeln!(out, "repeat(__{var_name}Len) {{");
                out.indent(1);
                write_decode_field(module, out, "__elem", elem);
                writeln!(out, "add(__elem)");
                out.dedent(1);
                writeln!(out, "}}");
                out.dedent(1);
                writeln!(out, "}}");
            }
        }
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            writeln!(out, "val {var_name} = when (reader.readSumTag().toInt()) {{");
            out.indent(1);
            if is_simple_decode(ok_ty) {
                let ok_expr = write_decode_expr(module, ok_ty);
                writeln!(out, "0 -> SpacetimeResult.Ok({ok_expr})");
            } else {
                writeln!(out, "0 -> {{");
                out.indent(1);
                write_decode_field(module, out, "__ok", ok_ty);
                writeln!(out, "SpacetimeResult.Ok(__ok)");
                out.dedent(1);
                writeln!(out, "}}");
            }
            if is_simple_decode(err_ty) {
                let err_expr = write_decode_expr(module, err_ty);
                writeln!(out, "1 -> SpacetimeResult.Err({err_expr})");
            } else {
                writeln!(out, "1 -> {{");
                out.indent(1);
                write_decode_field(module, out, "__err", err_ty);
                writeln!(out, "SpacetimeResult.Err(__err)");
                out.dedent(1);
                writeln!(out, "}}");
            }
            writeln!(out, "else -> error(\"Unknown Result tag\")");
            out.dedent(1);
            writeln!(out, "}}");
        }
        _ => {
            let expr = write_decode_expr(module, ty);
            writeln!(out, "val {var_name} = {expr}");
        }
    }
}

// --- File generation helpers ---

fn print_file_header(output: &mut Indenter) {
    print_auto_generated_file_comment(output);
    writeln!(output, "@file:Suppress(\"UNUSED\", \"SpellCheckingInspection\")");
    writeln!(output);
    writeln!(output, "package module_bindings");
}

fn gen_and_print_imports<'a>(
    module: &ModuleDef,
    out: &mut Indenter,
    roots: impl Iterator<Item = &'a AlgebraicTypeUse>,
) {
    let mut imports = BTreeSet::new();

    for ty in roots {
        collect_type_imports(module, ty, &mut imports);
    }

    if !imports.is_empty() {
        for import in imports {
            writeln!(out, "import {import}");
        }
    }
}

fn collect_type_imports(module: &ModuleDef, ty: &AlgebraicTypeUse, imports: &mut BTreeSet<String>) {
    if let Some(fqn) = kotlin_type_fqn(module, ty) {
        imports.insert(fqn);
    }
    match ty {
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            collect_type_imports(module, ok_ty, imports);
            collect_type_imports(module, err_ty, imports);
        }
        AlgebraicTypeUse::Option(inner) => {
            collect_type_imports(module, inner, imports);
        }
        AlgebraicTypeUse::Array(inner) => {
            collect_type_imports(module, inner, imports);
        }
        _ => {}
    }
}

// --- Types.kt ---

fn generate_types_file(module: &ModuleDef) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_file_header(out);
    writeln!(out);

    // Collect imports from all types
    let mut imports = BTreeSet::new();
    // Always import BSATN reader/writer for encode/decode
    imports.insert(format!("{SDK_PKG}.bsatn.BsatnReader"));
    imports.insert(format!("{SDK_PKG}.bsatn.BsatnWriter"));

    for ty in iter_types(module) {
        match &module.typespace_for_generate()[ty.ty] {
            AlgebraicTypeDef::Product(product) => {
                for (_, field_ty) in product.elements.iter() {
                    collect_type_imports(module, field_ty, &mut imports);
                }
            }
            AlgebraicTypeDef::Sum(sum) => {
                for (_, variant_ty) in sum.variants.iter() {
                    collect_type_imports(module, variant_ty, &mut imports);
                }
            }
            AlgebraicTypeDef::PlainEnum(_) => {}
        }
    }
    if !imports.is_empty() {
        for import in &imports {
            writeln!(out, "import {import}");
        }
        writeln!(out);
    }

    let reducer_type_names: BTreeSet<String> = module
        .reducers()
        .map(|reducer| reducer.accessor_name.deref().to_case(Case::Pascal))
        .collect();

    for ty in iter_types(module) {
        let type_name = collect_case(Case::Pascal, ty.accessor_name.name_segments());
        if reducer_type_names.contains(&type_name) {
            continue;
        }

        match &module.typespace_for_generate()[ty.ty] {
            AlgebraicTypeDef::Product(product) => {
                define_product_type(module, out, &type_name, &product.elements);
            }
            AlgebraicTypeDef::Sum(sum) => {
                define_sum_type(module, out, &type_name, &sum.variants);
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                define_plain_enum(out, &type_name, &plain_enum.variants);
            }
        }
    }

    OutputFile {
        filename: "Types.kt".to_string(),
        code: output.into_inner(),
    }
}

fn define_product_type(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {
    if elements.is_empty() {
        writeln!(out, "/** Data type `{name}` from the module schema. */");
        writeln!(out, "data object {name} {{");
        out.indent(1);
        writeln!(out, "/** Encodes this value to BSATN. */");
        writeln!(out, "fun encode(writer: BsatnWriter) {{ }}");
        writeln!(out);
        writeln!(out, "/** Decodes a [{name}] from BSATN. */");
        writeln!(out, "fun decode(reader: BsatnReader): {name} = {name}");
        out.dedent(1);
        writeln!(out, "}}");
    } else {
        writeln!(out, "/** Data type `{name}` from the module schema. */");
        writeln!(out, "data class {name}(");
        out.indent(1);
        for (i, (ident, ty)) in elements.iter().enumerate() {
            let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
            let kotlin_ty = kotlin_type(module, ty);
            let comma = if i + 1 < elements.len() { "," } else { "" };
            writeln!(out, "val {field_name}: {kotlin_ty}{comma}");
        }
        out.dedent(1);
        writeln!(out, ") {{");
        out.indent(1);

        // encode method
        writeln!(out, "/** Encodes this value to BSATN. */");
        writeln!(out, "fun encode(writer: BsatnWriter) {{");
        out.indent(1);
        for (ident, ty) in elements.iter() {
            let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
            write_encode_field(module, out, &field_name, ty);
        }
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);

        // companion object with decode
        writeln!(out, "companion object {{");
        out.indent(1);
        writeln!(out, "/** Decodes a [{name}] from BSATN. */");
        writeln!(out, "fun decode(reader: BsatnReader): {name} {{");
        out.indent(1);
        for (ident, ty) in elements.iter() {
            let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
            write_decode_field(module, out, &field_name, ty);
        }
        // Constructor call
        let field_names: Vec<String> = elements
            .iter()
            .map(|(ident, _)| kotlin_ident(ident.deref().to_case(Case::Camel)))
            .collect();
        let args = field_names.join(", ");
        writeln!(out, "return {name}({args})");
        out.dedent(1);
        writeln!(out, "}}");
        out.dedent(1);
        writeln!(out, "}}");

        // ByteArray fields need custom equals/hashCode
        let has_byte_array = elements.iter().any(|(_, ty)| {
            matches!(ty, AlgebraicTypeUse::Array(inner) if matches!(&**inner, AlgebraicTypeUse::Primitive(PrimitiveType::U8)))
        });
        if has_byte_array {
            writeln!(out);
            // equals
            writeln!(out, "override fun equals(other: Any?): Boolean {{");
            out.indent(1);
            writeln!(out, "if (this === other) return true");
            writeln!(out, "if (other !is {name}) return false");
            for (ident, ty) in elements.iter() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                if matches!(ty, AlgebraicTypeUse::Array(inner) if matches!(&**inner, AlgebraicTypeUse::Primitive(PrimitiveType::U8)))
                {
                    writeln!(out, "if (!{field_name}.contentEquals(other.{field_name})) return false");
                } else {
                    writeln!(out, "if ({field_name} != other.{field_name}) return false");
                }
            }
            writeln!(out, "return true");
            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out);
            // hashCode
            writeln!(out, "override fun hashCode(): Int {{");
            out.indent(1);
            writeln!(out, "var result = 0");
            for (ident, ty) in elements.iter() {
                let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                if matches!(ty, AlgebraicTypeUse::Array(inner) if matches!(&**inner, AlgebraicTypeUse::Primitive(PrimitiveType::U8)))
                {
                    writeln!(out, "result = 31 * result + {field_name}.contentHashCode()");
                } else {
                    writeln!(out, "result = 31 * result + {field_name}.hashCode()");
                }
            }
            writeln!(out, "return result");
            out.dedent(1);
            writeln!(out, "}}");
        }

        out.dedent(1);
        writeln!(out, "}}");
    }
    writeln!(out);
}

/// Returns the Kotlin type name for `ty`, qualifying with `module_bindings.` when
/// a variant name in `variant_names` would shadow the type inside a sealed interface scope.
fn kotlin_type_avoiding_variants(module: &ModuleDef, ty: &AlgebraicTypeUse, variant_names: &[String]) -> String {
    let base = kotlin_type(module, ty);
    if variant_names.contains(&base) {
        format!("module_bindings.{base}")
    } else {
        base
    }
}

/// Like [write_decode_expr] but qualifies `Ref` types that collide with variant names.
fn write_decode_expr_avoiding_variants(module: &ModuleDef, ty: &AlgebraicTypeUse, variant_names: &[String]) -> String {
    if let AlgebraicTypeUse::Ref(r) = ty {
        let name = type_ref_name(module, *r);
        if variant_names.contains(&name) {
            return format!("module_bindings.{name}.decode(reader)");
        }
    }
    write_decode_expr(module, ty)
}

fn define_sum_type(module: &ModuleDef, out: &mut Indenter, name: &str, variants: &[(Identifier, AlgebraicTypeUse)]) {
    assert!(
        variants.len() <= 256,
        "Sum type `{name}` has {} variants, but BSATN sum tags are limited to 256",
        variants.len()
    );
    // Collect all variant names so we can detect when a payload type name collides
    // with a variant name (which would resolve to the sealed interface member instead
    // of the top-level type).
    let variant_names: Vec<String> = variants
        .iter()
        .map(|(ident, _)| ident.deref().to_case(Case::Pascal))
        .collect();

    writeln!(out, "/** Sum type `{name}` from the module schema. */");
    writeln!(out, "sealed interface {name} {{");
    out.indent(1);

    // Variants
    for (ident, ty) in variants.iter() {
        let variant_name = ident.deref().to_case(Case::Pascal);
        match ty {
            AlgebraicTypeUse::Unit => {
                writeln!(out, "data object {variant_name} : {name}");
            }
            _ => {
                let kotlin_ty = kotlin_type_avoiding_variants(module, ty, &variant_names);
                writeln!(out, "data class {variant_name}(val value: {kotlin_ty}) : {name}");
            }
        }
    }
    writeln!(out);

    // encode method
    writeln!(out, "fun encode(writer: BsatnWriter) {{");
    out.indent(1);
    writeln!(out, "when (this) {{");
    out.indent(1);
    for (i, (ident, ty)) in variants.iter().enumerate() {
        let variant_name = ident.deref().to_case(Case::Pascal);
        let tag = i;
        match ty {
            AlgebraicTypeUse::Unit => {
                writeln!(out, "is {variant_name} -> writer.writeSumTag({tag}u)");
            }
            _ => {
                writeln!(out, "is {variant_name} -> {{");
                out.indent(1);
                writeln!(out, "writer.writeSumTag({tag}u)");
                write_encode_field(module, out, "value", ty);
                out.dedent(1);
                writeln!(out, "}}");
            }
        }
    }
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // companion decode
    writeln!(out, "companion object {{");
    out.indent(1);
    writeln!(out, "fun decode(reader: BsatnReader): {name} {{");
    out.indent(1);
    writeln!(out, "return when (val tag = reader.readSumTag().toInt()) {{");
    out.indent(1);
    for (i, (ident, ty)) in variants.iter().enumerate() {
        let variant_name = ident.deref().to_case(Case::Pascal);
        match ty {
            AlgebraicTypeUse::Unit => {
                writeln!(out, "{i} -> {variant_name}");
            }
            _ => {
                if is_simple_decode(ty) {
                    let expr = write_decode_expr_avoiding_variants(module, ty, &variant_names);
                    writeln!(out, "{i} -> {variant_name}({expr})");
                } else {
                    writeln!(out, "{i} -> {{");
                    out.indent(1);
                    write_decode_field(module, out, "__value", ty);
                    writeln!(out, "{variant_name}(__value)");
                    out.dedent(1);
                    writeln!(out, "}}");
                }
            }
        }
    }
    writeln!(out, "else -> error(\"Unknown {name} tag: $tag\")");
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");

    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);
}

fn define_plain_enum(out: &mut Indenter, name: &str, variants: &[Identifier]) {
    assert!(
        variants.len() <= 256,
        "Enum `{name}` has {} variants, but BSATN sum tags are limited to 256",
        variants.len()
    );
    writeln!(out, "/** Enum type `{name}` from the module schema. */");
    writeln!(out, "enum class {name} {{");
    out.indent(1);
    for (i, variant) in variants.iter().enumerate() {
        let variant_name = variant.deref().to_case(Case::Pascal);
        let comma = if i + 1 < variants.len() { "," } else { ";" };
        writeln!(out, "{variant_name}{comma}");
    }
    writeln!(out);
    writeln!(out, "fun encode(writer: BsatnWriter) {{");
    out.indent(1);
    writeln!(out, "writer.writeSumTag(ordinal.toUByte())");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);
    writeln!(out, "companion object {{");
    out.indent(1);
    writeln!(out, "fun decode(reader: BsatnReader): {name} {{");
    out.indent(1);
    writeln!(out, "val tag = reader.readSumTag().toInt()");
    writeln!(
        out,
        "return entries.getOrElse(tag) {{ error(\"Unknown {name} tag: $tag\") }}"
    );
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);
}

// --- RemoteTables.kt ---

fn generate_remote_tables_file(module: &ModuleDef, options: &CodegenOptions) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_file_header(out);
    writeln!(out);

    writeln!(out, "import {SDK_PKG}.ClientCache");
    writeln!(out, "import {SDK_PKG}.DbConnection");
    writeln!(out, "import {SDK_PKG}.InternalSpacetimeApi");
    writeln!(out, "import {SDK_PKG}.ModuleTables");
    writeln!(out);

    writeln!(out, "/** Generated table accessors for all tables in this module. */");
    writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
    writeln!(out, "class RemoteTables internal constructor(");
    out.indent(1);
    writeln!(out, "private val conn: DbConnection,");
    writeln!(out, "private val clientCache: ClientCache,");
    out.dedent(1);
    writeln!(out, ") : ModuleTables {{");
    out.indent(1);

    for (_, accessor_name, product_type_ref) in iter_table_names_and_types(module, options.visibility) {
        let table_name_pascal = accessor_name.deref().to_case(Case::Pascal);
        let table_name_camel = kotlin_ident(accessor_name.deref().to_case(Case::Camel));
        let type_name = type_ref_name(module, product_type_ref);

        writeln!(out, "val {table_name_camel}: {table_name_pascal}TableHandle by lazy {{");
        out.indent(1);
        writeln!(out, "@Suppress(\"UNCHECKED_CAST\")");
        writeln!(
            out,
            "val cache = clientCache.getOrCreateTable<{type_name}>({table_name_pascal}TableHandle.TABLE_NAME) {{"
        );
        out.indent(1);
        writeln!(out, "{table_name_pascal}TableHandle.createTableCache()");
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out, "{table_name_pascal}TableHandle(conn, cache)");
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);
    }

    out.dedent(1);
    writeln!(out, "}}");

    OutputFile {
        filename: "RemoteTables.kt".to_string(),
        code: output.into_inner(),
    }
}

// --- RemoteReducers.kt ---

fn generate_remote_reducers_file(module: &ModuleDef, options: &CodegenOptions) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_file_header(out);
    writeln!(out);

    // Collect all imports needed by reducer params
    let mut imports = BTreeSet::new();
    imports.insert(format!("{SDK_PKG}.CallbackList"));
    imports.insert(format!("{SDK_PKG}.DbConnection"));
    imports.insert(format!("{SDK_PKG}.EventContext"));
    imports.insert(format!("{SDK_PKG}.InternalSpacetimeApi"));
    imports.insert(format!("{SDK_PKG}.ModuleReducers"));
    imports.insert(format!("{SDK_PKG}.Status"));

    for reducer in iter_reducers(module, options.visibility) {
        for (_, ty) in reducer.params_for_generate.elements.iter() {
            collect_type_imports(module, ty, &mut imports);
        }
    }

    for import in &imports {
        writeln!(out, "import {import}");
    }
    writeln!(out);

    writeln!(out, "/** Generated reducer call methods and callback registration. */");
    writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
    writeln!(out, "class RemoteReducers internal constructor(");
    out.indent(1);
    writeln!(out, "private val conn: DbConnection,");
    out.dedent(1);
    writeln!(out, ") : ModuleReducers {{");
    out.indent(1);

    // --- Invocation methods ---
    for reducer in iter_reducers(module, options.visibility) {
        if !is_reducer_invokable(reducer) {
            continue;
        }

        let reducer_name_camel = kotlin_ident(reducer.accessor_name.deref().to_case(Case::Camel));
        let reducer_name_pascal = reducer.accessor_name.deref().to_case(Case::Pascal);

        if reducer.params_for_generate.elements.is_empty() {
            writeln!(
                out,
                "fun {reducer_name_camel}(callback: ((EventContext.Reducer<Unit>) -> Unit)? = null) {{"
            );
            out.indent(1);
            writeln!(
                out,
                "conn.callReducer({reducer_name_pascal}Reducer.REDUCER_NAME, ByteArray(0), Unit, callback)"
            );
            out.dedent(1);
            writeln!(out, "}}");
        } else {
            let params: Vec<String> = reducer
                .params_for_generate
                .elements
                .iter()
                .map(|(ident, ty)| {
                    let name = kotlin_ident(ident.deref().to_case(Case::Camel));
                    let kotlin_ty = kotlin_type(module, ty);
                    format!("{name}: {kotlin_ty}")
                })
                .collect();
            let params_str = params.join(", ");
            writeln!(out, "fun {reducer_name_camel}({params_str}, callback: ((EventContext.Reducer<{reducer_name_pascal}Args>) -> Unit)? = null) {{");
            out.indent(1);
            // Build the args object
            let arg_names: Vec<String> = reducer
                .params_for_generate
                .elements
                .iter()
                .map(|(ident, _)| kotlin_ident(ident.deref().to_case(Case::Camel)))
                .collect();
            let arg_names_str = arg_names.join(", ");
            writeln!(out, "val args = {reducer_name_pascal}Args({arg_names_str})");
            writeln!(
                out,
                "conn.callReducer({reducer_name_pascal}Reducer.REDUCER_NAME, args.encode(), args, callback)"
            );
            out.dedent(1);
            writeln!(out, "}}");
        }
        writeln!(out);
    }

    // --- Per-reducer persistent callbacks ---
    for reducer in iter_reducers(module, options.visibility) {
        let reducer_name_pascal = reducer.accessor_name.deref().to_case(Case::Pascal);

        // Build the typed callback signature: (EventContext.Reducer<ArgsType>, arg1Type, arg2Type, ...) -> Unit
        let args_type = if reducer.params_for_generate.elements.is_empty() {
            "Unit".to_string()
        } else {
            format!("{reducer_name_pascal}Args")
        };
        let cb_params: Vec<String> = std::iter::once(format!("EventContext.Reducer<{args_type}>"))
            .chain(
                reducer
                    .params_for_generate
                    .elements
                    .iter()
                    .map(|(_, ty)| kotlin_type(module, ty)),
            )
            .collect();
        let cb_type = format!("({}) -> Unit", cb_params.join(", "));

        // Callback list
        writeln!(
            out,
            "private val on{reducer_name_pascal}Callbacks = CallbackList<{cb_type}>()"
        );
        writeln!(out);

        // on{Reducer}
        writeln!(out, "fun on{reducer_name_pascal}(cb: {cb_type}) {{");
        out.indent(1);
        writeln!(out, "on{reducer_name_pascal}Callbacks.add(cb)");
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);

        // removeOn{Reducer}
        writeln!(out, "fun removeOn{reducer_name_pascal}(cb: {cb_type}) {{");
        out.indent(1);
        writeln!(out, "on{reducer_name_pascal}Callbacks.remove(cb)");
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);
    }

    // --- Unhandled reducer error fallback ---
    writeln!(
        out,
        "private val onUnhandledReducerErrorCallbacks = CallbackList<(EventContext.Reducer<*>) -> Unit>()"
    );
    writeln!(out);
    writeln!(
        out,
        "/** Register a callback for reducer errors with no specific handler. */"
    );
    writeln!(
        out,
        "fun onUnhandledReducerError(cb: (EventContext.Reducer<*>) -> Unit) {{"
    );
    out.indent(1);
    writeln!(out, "onUnhandledReducerErrorCallbacks.add(cb)");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);
    writeln!(
        out,
        "fun removeOnUnhandledReducerError(cb: (EventContext.Reducer<*>) -> Unit) {{"
    );
    out.indent(1);
    writeln!(out, "onUnhandledReducerErrorCallbacks.remove(cb)");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // --- handleReducerEvent dispatch ---
    writeln!(out, "internal fun handleReducerEvent(ctx: EventContext.Reducer<*>) {{");
    out.indent(1);
    writeln!(out, "when (ctx.reducerName) {{");
    out.indent(1);

    for reducer in iter_reducers(module, options.visibility) {
        let reducer_name_pascal = reducer.accessor_name.deref().to_case(Case::Pascal);

        writeln!(out, "{reducer_name_pascal}Reducer.REDUCER_NAME -> {{");
        out.indent(1);
        writeln!(out, "if (on{reducer_name_pascal}Callbacks.isNotEmpty()) {{");
        out.indent(1);

        if reducer.params_for_generate.elements.is_empty() {
            writeln!(out, "@Suppress(\"UNCHECKED_CAST\")");
            writeln!(out, "val typedCtx = ctx as EventContext.Reducer<Unit>");
            writeln!(out, "on{reducer_name_pascal}Callbacks.forEach {{ it(typedCtx) }}");
        } else {
            writeln!(out, "@Suppress(\"UNCHECKED_CAST\")");
            writeln!(
                out,
                "val typedCtx = ctx as EventContext.Reducer<{reducer_name_pascal}Args>"
            );
            // Build the call args from typed args fields
            let call_args: Vec<String> = std::iter::once("typedCtx".to_string())
                .chain(reducer.params_for_generate.elements.iter().map(|(ident, _)| {
                    let field_name = kotlin_ident(ident.deref().to_case(Case::Camel));
                    format!("typedCtx.args.{field_name}")
                }))
                .collect();
            let call_args_str = call_args.join(", ");
            writeln!(
                out,
                "on{reducer_name_pascal}Callbacks.forEach {{ it({call_args_str}) }}"
            );
        }

        out.dedent(1);
        writeln!(out, "}} else if (ctx.status is Status.Failed) {{");
        out.indent(1);
        writeln!(out, "onUnhandledReducerErrorCallbacks.forEach {{ it(ctx) }}");
        out.dedent(1);
        writeln!(out, "}}");
        out.dedent(1);
        writeln!(out, "}}");
    }

    // Fallback for unknown reducer names
    writeln!(out, "else -> {{");
    out.indent(1);
    writeln!(out, "if (ctx.status is Status.Failed) {{");
    out.indent(1);
    writeln!(out, "onUnhandledReducerErrorCallbacks.forEach {{ it(ctx) }}");
    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");

    out.dedent(1);
    writeln!(out, "}}");
    out.dedent(1);
    writeln!(out, "}}");

    out.dedent(1);
    writeln!(out, "}}");

    OutputFile {
        filename: "RemoteReducers.kt".to_string(),
        code: output.into_inner(),
    }
}

// --- RemoteProcedures.kt ---

fn generate_remote_procedures_file(module: &ModuleDef, options: &CodegenOptions) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_file_header(out);
    writeln!(out);

    // Collect all imports needed by procedure params and return types
    let mut imports = BTreeSet::new();
    imports.insert(format!("{SDK_PKG}.DbConnection"));
    imports.insert(format!("{SDK_PKG}.InternalSpacetimeApi"));
    imports.insert(format!("{SDK_PKG}.ModuleProcedures"));

    let has_procedures = iter_procedures(module, options.visibility).next().is_some();
    if has_procedures {
        imports.insert(format!("{SDK_PKG}.EventContext"));
        imports.insert(format!("{SDK_PKG}.ProcedureError"));
        imports.insert(format!("{SDK_PKG}.SdkResult"));
        imports.insert(format!("{SDK_PKG}.bsatn.BsatnWriter"));
        imports.insert(format!("{SDK_PKG}.bsatn.BsatnReader"));
        imports.insert(format!("{SDK_PKG}.protocol.ServerMessage"));
        imports.insert(format!("{SDK_PKG}.protocol.ProcedureStatus"));
    }

    for procedure in iter_procedures(module, options.visibility) {
        for (_, ty) in procedure.params_for_generate.elements.iter() {
            collect_type_imports(module, ty, &mut imports);
        }
        collect_type_imports(module, &procedure.return_type_for_generate, &mut imports);
    }

    for import in &imports {
        writeln!(out, "import {import}");
    }
    writeln!(out);

    writeln!(
        out,
        "/** Generated procedure call methods and callback registration. */"
    );
    writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
    writeln!(out, "class RemoteProcedures internal constructor(");
    out.indent(1);
    writeln!(out, "private val conn: DbConnection,");
    out.dedent(1);
    writeln!(out, ") : ModuleProcedures {{");
    out.indent(1);

    for procedure in iter_procedures(module, options.visibility) {
        let procedure_name_camel = kotlin_ident(procedure.accessor_name.deref().to_case(Case::Camel));
        let procedure_name_pascal = procedure.accessor_name.deref().to_case(Case::Pascal);
        let return_ty = &procedure.return_type_for_generate;
        let return_ty_str = kotlin_type(module, return_ty);
        let is_unit_return = matches!(return_ty, AlgebraicTypeUse::Unit);

        // Build parameter list
        let params: Vec<String> = procedure
            .params_for_generate
            .elements
            .iter()
            .map(|(ident, ty)| {
                let name = kotlin_ident(ident.deref().to_case(Case::Camel));
                let kotlin_ty = kotlin_type(module, ty);
                format!("{name}: {kotlin_ty}")
            })
            .collect();

        // Callback type uses SdkResult to surface both success and ProcedureError
        let callback_type = if is_unit_return {
            "((EventContext.Procedure, SdkResult<Unit, ProcedureError>) -> Unit)?".to_string()
        } else {
            format!("((EventContext.Procedure, SdkResult<{return_ty_str}, ProcedureError>) -> Unit)?")
        };

        if params.is_empty() {
            writeln!(out, "fun {procedure_name_camel}(callback: {callback_type} = null) {{");
        } else {
            let params_str = params.join(", ");
            writeln!(
                out,
                "fun {procedure_name_camel}({params_str}, callback: {callback_type} = null) {{"
            );
        }
        out.indent(1);

        let args_expr = if procedure.params_for_generate.elements.is_empty() {
            "ByteArray(0)".to_string()
        } else {
            let arg_names: Vec<String> = procedure
                .params_for_generate
                .elements
                .iter()
                .map(|(ident, _)| kotlin_ident(ident.deref().to_case(Case::Camel)))
                .collect();
            let arg_names_str = arg_names.join(", ");
            writeln!(out, "val args = {procedure_name_pascal}Args({arg_names_str})");
            "args.encode()".to_string()
        };

        // Generate wrapper callback that decodes the return value into a Result
        writeln!(out, "val wrappedCallback = callback?.let {{ userCb ->");
        out.indent(1);
        writeln!(
            out,
            "{{ ctx: EventContext.Procedure, msg: ServerMessage.ProcedureResultMsg ->"
        );
        out.indent(1);
        writeln!(out, "when (val status = msg.status) {{");
        out.indent(1);
        writeln!(out, "is ProcedureStatus.Returned -> {{");
        out.indent(1);
        if is_unit_return {
            writeln!(out, "userCb(ctx, SdkResult.Success(Unit))");
        } else if is_simple_decode(return_ty) {
            writeln!(out, "val reader = BsatnReader(status.value)");
            let decode_expr = write_decode_expr(module, return_ty);
            writeln!(out, "userCb(ctx, SdkResult.Success({decode_expr}))");
        } else {
            writeln!(out, "val reader = BsatnReader(status.value)");
            write_decode_field(module, out, "__retVal", return_ty);
            writeln!(out, "userCb(ctx, SdkResult.Success(__retVal))");
        }
        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out, "is ProcedureStatus.InternalError -> {{");
        out.indent(1);
        writeln!(
            out,
            "userCb(ctx, SdkResult.Failure(ProcedureError.InternalError(status.message)))"
        );
        out.dedent(1);
        writeln!(out, "}}");
        out.dedent(1);
        writeln!(out, "}}");
        out.dedent(1);
        writeln!(out, "}}");
        out.dedent(1);
        writeln!(out, "}}");

        writeln!(
            out,
            "conn.callProcedure({procedure_name_pascal}Procedure.PROCEDURE_NAME, {args_expr}, wrappedCallback)"
        );

        out.dedent(1);
        writeln!(out, "}}");
        writeln!(out);
    }

    out.dedent(1);
    writeln!(out, "}}");

    OutputFile {
        filename: "RemoteProcedures.kt".to_string(),
        code: output.into_inner(),
    }
}

// --- Module.kt ---

fn generate_module_file(module: &ModuleDef, options: &CodegenOptions) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_file_header(out);
    print_auto_generated_version_comment(out);
    writeln!(out);

    writeln!(out, "import {SDK_PKG}.ClientCache");
    writeln!(out, "import {SDK_PKG}.DbConnection");
    writeln!(out, "import {SDK_PKG}.DbConnectionView");
    writeln!(out, "import {SDK_PKG}.EventContext");
    writeln!(out, "import {SDK_PKG}.InternalSpacetimeApi");
    writeln!(out, "import {SDK_PKG}.ModuleAccessors");
    writeln!(out, "import {SDK_PKG}.ModuleDescriptor");
    writeln!(out, "import {SDK_PKG}.Query");
    writeln!(out, "import {SDK_PKG}.SubscriptionBuilder");
    writeln!(out, "import {SDK_PKG}.Table");
    writeln!(out);

    // RemoteModule object with version info and table/reducer/procedure names
    writeln!(out, "/**");
    writeln!(out, " * Module metadata generated by the SpacetimeDB CLI.");
    writeln!(
        out,
        " * Contains version info and the names of all tables, reducers, and procedures."
    );
    writeln!(out, " */");
    writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
    writeln!(out, "object RemoteModule : ModuleDescriptor {{");
    out.indent(1);

    writeln!(
        out,
        "override val cliVersion: String = \"{}\"",
        spacetimedb_lib_version()
    );
    writeln!(out);

    // Table and view names list
    writeln!(out, "val tableNames: List<String> = listOf(");
    out.indent(1);
    for (name, _, _) in iter_table_names_and_types(module, options.visibility) {
        writeln!(out, "\"{}\",", name.deref());
    }
    out.dedent(1);
    writeln!(out, ")");
    writeln!(out);

    // Subscribable (persistent) table/view names — excludes event tables
    writeln!(out, "override val subscribableTableNames: List<String> = listOf(");
    out.indent(1);
    for (name, _, _) in iter_table_names_and_types(module, options.visibility) {
        // Event tables are not subscribable; views are never event tables.
        let is_event = module.tables().any(|t| t.name == *name && t.is_event);
        if !is_event {
            writeln!(out, "\"{}\",", name.deref());
        }
    }
    out.dedent(1);
    writeln!(out, ")");
    writeln!(out);

    // Reducer names list
    writeln!(out, "val reducerNames: List<String> = listOf(");
    out.indent(1);
    for reducer in iter_reducers(module, options.visibility) {
        if !is_reducer_invokable(reducer) {
            continue;
        }
        writeln!(out, "\"{}\",", reducer.name.deref());
    }
    out.dedent(1);
    writeln!(out, ")");
    writeln!(out);

    // Procedure names list
    writeln!(out, "val procedureNames: List<String> = listOf(");
    out.indent(1);
    for procedure in iter_procedures(module, options.visibility) {
        writeln!(out, "\"{}\",", procedure.name.deref());
    }
    out.dedent(1);
    writeln!(out, ")");

    writeln!(out);

    // registerTables() — ModuleDescriptor implementation
    writeln!(out, "override fun registerTables(cache: ClientCache) {{");
    out.indent(1);
    for (_, accessor_name, _) in iter_table_names_and_types(module, options.visibility) {
        let table_name_pascal = accessor_name.deref().to_case(Case::Pascal);
        writeln!(
            out,
            "cache.register({table_name_pascal}TableHandle.TABLE_NAME, {table_name_pascal}TableHandle.createTableCache())"
        );
    }
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // createAccessors() — ModuleDescriptor implementation
    writeln!(
        out,
        "override fun createAccessors(conn: DbConnection): ModuleAccessors {{"
    );
    out.indent(1);
    writeln!(out, "return ModuleAccessors(");
    out.indent(1);
    writeln!(out, "tables = RemoteTables(conn, conn.clientCache),");
    writeln!(out, "reducers = RemoteReducers(conn),");
    writeln!(out, "procedures = RemoteProcedures(conn),");
    out.dedent(1);
    writeln!(out, ")");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // handleReducerEvent() — ModuleDescriptor implementation
    writeln!(
        out,
        "override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {{"
    );
    out.indent(1);
    writeln!(out, "conn.reducers.handleReducerEvent(ctx)");
    out.dedent(1);
    writeln!(out, "}}");

    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // Extension properties on DbConnection
    writeln!(out, "/**");
    writeln!(out, " * Typed table accessors for this module's tables.");
    writeln!(out, " */");
    writeln!(out, "val DbConnection.db: RemoteTables");
    out.indent(1);
    writeln!(out, "get() = moduleTables as RemoteTables");
    out.dedent(1);
    writeln!(out);

    writeln!(out, "/**");
    writeln!(out, " * Typed reducer call functions for this module's reducers.");
    writeln!(out, " */");
    writeln!(out, "val DbConnection.reducers: RemoteReducers");
    out.indent(1);
    writeln!(out, "get() = moduleReducers as RemoteReducers");
    out.dedent(1);
    writeln!(out);

    writeln!(out, "/**");
    writeln!(out, " * Typed procedure call functions for this module's procedures.");
    writeln!(out, " */");
    writeln!(out, "val DbConnection.procedures: RemoteProcedures");
    out.indent(1);
    writeln!(out, "get() = moduleProcedures as RemoteProcedures");
    out.dedent(1);
    writeln!(out);

    // Extension properties on DbConnectionView (exposed via EventContext.connection)
    writeln!(out, "/**");
    writeln!(out, " * Typed table accessors for this module's tables.");
    writeln!(out, " */");
    writeln!(out, "val DbConnectionView.db: RemoteTables");
    out.indent(1);
    writeln!(out, "get() = moduleTables as RemoteTables");
    out.dedent(1);
    writeln!(out);

    writeln!(out, "/**");
    writeln!(out, " * Typed reducer call functions for this module's reducers.");
    writeln!(out, " */");
    writeln!(out, "val DbConnectionView.reducers: RemoteReducers");
    out.indent(1);
    writeln!(out, "get() = moduleReducers as RemoteReducers");
    out.dedent(1);
    writeln!(out);

    writeln!(out, "/**");
    writeln!(out, " * Typed procedure call functions for this module's procedures.");
    writeln!(out, " */");
    writeln!(out, "val DbConnectionView.procedures: RemoteProcedures");
    out.indent(1);
    writeln!(out, "get() = moduleProcedures as RemoteProcedures");
    out.dedent(1);
    writeln!(out);

    // Extension properties on EventContext for typed access in callbacks
    writeln!(out, "/**");
    writeln!(out, " * Typed table accessors available directly on event context.");
    writeln!(out, " */");
    writeln!(out, "val EventContext.db: RemoteTables");
    out.indent(1);
    writeln!(out, "get() = connection.db");
    out.dedent(1);
    writeln!(out);

    writeln!(out, "/**");
    writeln!(
        out,
        " * Typed reducer call functions available directly on event context."
    );
    writeln!(out, " */");
    writeln!(out, "val EventContext.reducers: RemoteReducers");
    out.indent(1);
    writeln!(out, "get() = connection.reducers");
    out.dedent(1);
    writeln!(out);

    writeln!(out, "/**");
    writeln!(
        out,
        " * Typed procedure call functions available directly on event context."
    );
    writeln!(out, " */");
    writeln!(out, "val EventContext.procedures: RemoteProcedures");
    out.indent(1);
    writeln!(out, "get() = connection.procedures");
    out.dedent(1);
    writeln!(out);

    // Builder extension for zero-config setup
    writeln!(out, "/**");
    writeln!(out, " * Registers this module's tables with the connection builder.");
    writeln!(
        out,
        " * Call this on the builder to enable typed [db], [reducers], and [procedures] accessors."
    );
    writeln!(out, " *");
    writeln!(out, " * Example:");
    writeln!(out, " * ```kotlin");
    writeln!(out, " * val conn = DbConnection.Builder()");
    writeln!(out, " *     .withUri(\"ws://localhost:3000\")");
    writeln!(out, " *     .withDatabaseName(\"my_module\")");
    writeln!(out, " *     .withModuleBindings()");
    writeln!(out, " *     .build()");
    writeln!(out, " * ```");
    writeln!(out, " */");
    writeln!(out, "@OptIn(InternalSpacetimeApi::class)");
    writeln!(
        out,
        "fun DbConnection.Builder.withModuleBindings(): DbConnection.Builder {{"
    );
    out.indent(1);
    writeln!(out, "return withModule(RemoteModule)");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // QueryBuilder — typed per-table query builder
    writeln!(out, "/**");
    writeln!(out, " * Type-safe query builder for this module's tables.");
    writeln!(out, " * Supports WHERE predicates and semi-joins.");
    writeln!(out, " */");
    writeln!(out, "class QueryBuilder {{");
    out.indent(1);
    for (name, accessor_name, product_type_ref) in iter_table_names_and_types(module, options.visibility) {
        let table_name = name.deref();
        let type_name = type_ref_name(module, product_type_ref);
        let table_name_pascal = accessor_name.deref().to_case(Case::Pascal);
        let method_name = kotlin_ident(accessor_name.deref().to_case(Case::Camel));

        // Check if this table has indexed columns (views have none)
        let has_ix = module
            .tables()
            .find(|t| t.name == *name)
            .is_some_and(|t| iter_indexes(t).any(|idx| matches!(&idx.algorithm, IndexAlgorithm::BTree(_))));

        if has_ix {
            writeln!(
                out,
                "fun {method_name}(): Table<{type_name}, {table_name_pascal}Cols, {table_name_pascal}IxCols> = Table(\"{table_name}\", {table_name_pascal}Cols(\"{table_name}\"), {table_name_pascal}IxCols(\"{table_name}\"))"
            );
        } else {
            writeln!(
                out,
                "fun {method_name}(): Table<{type_name}, {table_name_pascal}Cols, {table_name_pascal}IxCols> = Table(\"{table_name}\", {table_name_pascal}Cols(\"{table_name}\"), {table_name_pascal}IxCols())"
            );
        }
    }
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // Typed addQuery extension on SubscriptionBuilder
    writeln!(out, "/**");
    writeln!(out, " * Add a type-safe table query to this subscription.");
    writeln!(out, " *");
    writeln!(out, " * Example:");
    writeln!(out, " * ```kotlin");
    writeln!(out, " * conn.subscriptionBuilder()");
    writeln!(out, " *     .addQuery {{ qb -> qb.player() }}");
    writeln!(
        out,
        " *     .addQuery {{ qb -> qb.player().where {{ c -> c.health.gt(50) }} }}"
    );
    writeln!(out, " *     .subscribe()");
    writeln!(out, " * ```");
    writeln!(out, " */");
    writeln!(
        out,
        "fun SubscriptionBuilder.addQuery(build: (QueryBuilder) -> Query<*>): SubscriptionBuilder {{"
    );
    out.indent(1);
    writeln!(out, "return addQuery(build(QueryBuilder()).toSql())");
    out.dedent(1);
    writeln!(out, "}}");
    writeln!(out);

    // Generated subscribeToAllTables with baked-in queries via QueryBuilder
    writeln!(out, "/**");
    writeln!(out, " * Subscribe to all persistent tables in this module.");
    writeln!(
        out,
        " * Event tables are excluded because the server does not support subscribing to them."
    );
    writeln!(out, " */");
    writeln!(
        out,
        "fun SubscriptionBuilder.subscribeToAllTables(): {SDK_PKG}.SubscriptionHandle {{"
    );
    out.indent(1);
    writeln!(out, "val qb = QueryBuilder()");
    for (name, accessor_name, _) in iter_table_names_and_types(module, options.visibility) {
        // Event tables are not subscribable; views are never event tables.
        let is_event = module.tables().any(|t| t.name == *name && t.is_event);
        if !is_event {
            let method_name = kotlin_ident(accessor_name.deref().to_case(Case::Camel));
            writeln!(out, "addQuery(qb.{method_name}().toSql())");
        }
    }
    writeln!(out, "return subscribe()");
    out.dedent(1);
    writeln!(out, "}}");

    OutputFile {
        filename: "Module.kt".to_string(),
        code: output.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kotlin_ident_escapes_hard_keywords() {
        for &kw in KOTLIN_HARD_KEYWORDS {
            assert_eq!(
                kotlin_ident(kw.to_string()),
                format!("`{kw}`"),
                "Expected keyword '{kw}' to be backtick-escaped"
            );
        }
    }

    #[test]
    fn kotlin_ident_passes_through_non_keywords() {
        let non_keywords = ["name", "age", "id", "foo", "bar", "myField", "data", "value"];
        for &name in &non_keywords {
            assert_eq!(
                kotlin_ident(name.to_string()),
                name,
                "Non-keyword '{name}' should not be escaped"
            );
        }
    }

    #[test]
    fn kotlin_ident_is_case_sensitive() {
        // PascalCase versions of keywords are NOT keywords
        assert_eq!(kotlin_ident("Object".to_string()), "Object");
        assert_eq!(kotlin_ident("Class".to_string()), "Class");
        assert_eq!(kotlin_ident("When".to_string()), "When");
        assert_eq!(kotlin_ident("Val".to_string()), "Val");
        // But lowercase versions are
        assert_eq!(kotlin_ident("object".to_string()), "`object`");
        assert_eq!(kotlin_ident("class".to_string()), "`class`");
        assert_eq!(kotlin_ident("when".to_string()), "`when`");
        assert_eq!(kotlin_ident("val".to_string()), "`val`");
    }
}
