use crate::util::{
    collect_case, iter_indexes, iter_procedures, iter_reducers, iter_table_names_and_types,
    print_auto_generated_file_comment, type_ref_name,
};
use crate::{CodegenOptions, OutputFile};

use super::code_indenter::CodeIndenter;
use super::util::fmt_fn;
use super::Lang;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};

use std::fmt;
use std::ops::Deref;

const INDENT: &str = "    ";

pub struct Kotlin<'opts> {
    pub package_name: &'opts str,
}

fn pkg_path(package_name: &str) -> String {
    package_name.replace('.', "/")
}

fn print_file_header(output: &mut CodeIndenter<String>, package_name: &str, subpackage: &str) {
    let full_package = if subpackage.is_empty() {
        package_name.to_string()
    } else {
        format!("{package_name}.{subpackage}")
    };
    print_auto_generated_file_comment(output);
    writeln!(output, "@file:Suppress(\"RedundantVisibilityModifier\")");
    writeln!(output);
    writeln!(output, "package {full_package}");
    writeln!(output);
    writeln!(output, "import com.clockworklabs.spacetimedb.*");
    writeln!(output, "import com.clockworklabs.spacetimedb.bsatn.*");
    writeln!(output, "import com.clockworklabs.spacetimedb.query.*");
    writeln!(output, "import kotlin.uuid.Uuid");
    if !subpackage.is_empty() {
        writeln!(output, "import {package_name}.*");
        if subpackage != "types" {
            writeln!(output, "import {package_name}.types.*");
        }
    }
    if subpackage == "reducers" || subpackage == "procedures" {
        writeln!(output, "import {package_name}.tables.*");
    }
    if subpackage.is_empty() {
        writeln!(output, "import {package_name}.types.*");
        writeln!(output, "import {package_name}.tables.*");
        // Reducers and procedures subpackages are imported later for their internal functions
    }
    writeln!(output);
}

fn ty_fmt<'a>(module: &'a ModuleDef, ty: &'a AlgebraicTypeUse) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicTypeUse::Identity => f.write_str("Identity"),
        AlgebraicTypeUse::ConnectionId => f.write_str("ConnectionId"),
        AlgebraicTypeUse::ScheduleAt => f.write_str("ScheduleAt"),
        AlgebraicTypeUse::Timestamp => f.write_str("Timestamp"),
        AlgebraicTypeUse::TimeDuration => f.write_str("TimeDuration"),
        AlgebraicTypeUse::Uuid => f.write_str("Uuid"),
        AlgebraicTypeUse::Unit => f.write_str("Unit"),
        AlgebraicTypeUse::Option(inner_ty) => write!(f, "{}?", ty_fmt(module, inner_ty)),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => write!(f, "Result<{}, {}>", ty_fmt(module, ok_ty), ty_fmt(module, err_ty)),
        AlgebraicTypeUse::Array(elem_ty) => write!(f, "List<{}>", ty_fmt(module, elem_ty)),
        AlgebraicTypeUse::String => f.write_str("String"),
        AlgebraicTypeUse::Ref(r) => f.write_str(&type_ref_name(module, *r)),
        AlgebraicTypeUse::Primitive(prim) => f.write_str(match prim {
            PrimitiveType::Bool => "Boolean",
            PrimitiveType::I8 => "Byte",
            PrimitiveType::U8 => "UByte",
            PrimitiveType::I16 => "Short",
            PrimitiveType::U16 => "UShort",
            PrimitiveType::I32 => "Int",
            PrimitiveType::U32 => "UInt",
            PrimitiveType::I64 => "Long",
            PrimitiveType::U64 => "ULong",
            PrimitiveType::I128 => "spacetimedb_lib.i256",
            PrimitiveType::U128 => "spacetimedb_lib.u256",
            PrimitiveType::I256 => "spacetimedb_lib.i256",
            PrimitiveType::U256 => "spacetimedb_lib.u256",
            PrimitiveType::F32 => "Float",
            PrimitiveType::F64 => "Double",
        }),
        AlgebraicTypeUse::Never => unreachable!(),
    })
}

fn write_bsatn_serialize_field(
    module: &ModuleDef,
    output: &mut CodeIndenter<String>,
    prefix: &str,
    field_name: &Identifier,
    field_type: &AlgebraicTypeUse,
) {
    let field_expr = format!("{}.{}", prefix, field_name.deref().to_case(Case::Camel));
    write_bsatn_serialize_expr(module, output, &field_expr, field_type);
}

fn write_bsatn_serialize_expr(
    module: &ModuleDef,
    output: &mut CodeIndenter<String>,
    expr: &str,
    ty: &AlgebraicTypeUse,
) {
    write_bsatn_serialize_expr_with_writer(module, output, "writer", expr, ty);
}

fn write_bsatn_serialize_expr_with_writer(
    module: &ModuleDef,
    output: &mut CodeIndenter<String>,
    writer_var: &str,
    expr: &str,
    ty: &AlgebraicTypeUse,
) {
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
                PrimitiveType::I128 | PrimitiveType::U128 | PrimitiveType::I256 | PrimitiveType::U256 => "writeByteArray",
            };
            writeln!(output, "{writer_var}.{method}({expr})");
        }
        AlgebraicTypeUse::String => {
            writeln!(output, "{writer_var}.writeString({expr})");
        }
        AlgebraicTypeUse::Identity => {
            writeln!(output, "Identity.write({writer_var}, {expr})");
        }
        AlgebraicTypeUse::ConnectionId => {
            writeln!(output, "ConnectionId.write({writer_var}, {expr})");
        }
        AlgebraicTypeUse::Timestamp => {
            writeln!(output, "Timestamp.write({writer_var}, {expr})");
        }
        AlgebraicTypeUse::TimeDuration => {
            writeln!(output, "// TODO: serialize TimeDuration {expr}");
        }
        AlgebraicTypeUse::ScheduleAt => {
            writeln!(output, "// TODO: serialize ScheduleAt {expr}");
        }
        AlgebraicTypeUse::Uuid => {
            writeln!(output, "Uuid.write({writer_var}, {expr})");
        }
        AlgebraicTypeUse::Option(inner) => {
            writeln!(output, "{writer_var}.writeOption({expr}) {{ v, inner ->");
            output.indent(1);
            write_bsatn_serialize_expr_with_writer(module, output, "v", "inner", inner);
            output.dedent(1);
            writeln!(output, "}}");
        }
        AlgebraicTypeUse::Array(elem) => {
            writeln!(output, "{writer_var}.writeArray({expr}) {{ w, elem ->");
            output.indent(1);
            write_bsatn_serialize_expr_with_writer(module, output, "w", "elem", elem);
            output.dedent(1);
            writeln!(output, "}}");
        }
        AlgebraicTypeUse::Result { .. } => {
            writeln!(output, "// TODO: serialize Result {expr}");
        }
        AlgebraicTypeUse::Unit => {}
        AlgebraicTypeUse::Ref(r) => {
            let type_name = type_ref_name(module, *r);
            writeln!(output, "{type_name}.write({writer_var}, {expr})");
        }
        AlgebraicTypeUse::Never => unreachable!(),
    }
}

fn write_bsatn_deserialize_field(
    module: &ModuleDef,
    output: &mut CodeIndenter<String>,
    field_name: &Identifier,
    field_type: &AlgebraicTypeUse,
) {
    let camel_name = field_name.deref().to_case(Case::Camel);
    write!(output, "{camel_name} = ");
    write_bsatn_deserialize_expr(module, output, "reader", field_type);
    writeln!(output, ",");
}

fn write_bsatn_deserialize_expr(
    module: &ModuleDef,
    output: &mut CodeIndenter<String>,
    reader_var: &str,
    ty: &AlgebraicTypeUse,
) {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => {
            let method = match prim {
                PrimitiveType::Bool => "readBool",
                PrimitiveType::I8 => "readI8",
                PrimitiveType::U8 => "readU8",
                PrimitiveType::I16 => "readI16",
                PrimitiveType::U16 => "readU16",
                PrimitiveType::I32 => "readI32",
                PrimitiveType::U32 => "readU32",
                PrimitiveType::I64 => "readI64",
                PrimitiveType::U64 => "readU64",
                PrimitiveType::F32 => "readF32",
                PrimitiveType::F64 => "readF64",
                PrimitiveType::I128 | PrimitiveType::U128 | PrimitiveType::I256 | PrimitiveType::U256 => "readByteArray",
            };
            write!(output, "{reader_var}.{method}()");
        }
        AlgebraicTypeUse::String => {
            write!(output, "{reader_var}.readString()");
        }
        AlgebraicTypeUse::Identity => {
            write!(output, "Identity.read({reader_var})");
        }
        AlgebraicTypeUse::ConnectionId => {
            write!(output, "ConnectionId.read({reader_var})");
        }
        AlgebraicTypeUse::Timestamp => {
            write!(output, "Timestamp.read({reader_var})");
        }
        AlgebraicTypeUse::TimeDuration => {
            write!(output, "TODO(\"read TimeDuration\")");
        }
        AlgebraicTypeUse::ScheduleAt => {
            write!(output, "TODO(\"read ScheduleAt\")");
        }
        AlgebraicTypeUse::Uuid => {
            write!(output, "Uuid.read({reader_var})");
        }
        AlgebraicTypeUse::Option(inner) => {
            write!(output, "{reader_var}.readOption {{ r -> ");
            write_bsatn_deserialize_expr(module, output, "r", inner);
            write!(output, " }}");
        }
        AlgebraicTypeUse::Array(elem) => {
            write!(output, "{reader_var}.readArray {{ r -> ");
            write_bsatn_deserialize_expr(module, output, "r", elem);
            write!(output, " }}");
        }
        AlgebraicTypeUse::Result { .. } => {
            write!(output, "TODO(\"read Result\")");
        }
        AlgebraicTypeUse::Unit => {
            write!(output, "Unit");
        }
        AlgebraicTypeUse::Ref(r) => {
            let type_name = type_ref_name(module, *r);
            write!(output, "{type_name}.read({reader_var})");
        }
        AlgebraicTypeUse::Never => unreachable!(),
    }
}

impl Lang for Kotlin<'_> {
    fn generate_table_file_from_schema(
        &self,
        module: &ModuleDef,
        table: &TableDef,
        _schema: TableSchema,
    ) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, &self.package_name, "tables");

        let row_type = type_ref_name(module, table.product_type_ref);
        let table_class_name = format!("{}Handle", table.accessor_name.deref().to_case(Case::Pascal));
        let accessor_pascal = table.accessor_name.deref().to_case(Case::Pascal);

        let base = if table.is_event { "EventTable" } else { "TableWithPrimaryKey" };

        writeln!(out, "class {table_class_name}(private val conn: DbConnection) : {base}<{row_type}> {{");
        writeln!(out, "    override val tableName: String get() = \"{}\"", table.name);
        writeln!(out);
        writeln!(out, "    override val count: Int get() = conn.clientCache.getTable(tableName)?.count ?: 0");
        writeln!(out);
        writeln!(out, "    override fun iter(): Sequence<{row_type}> {{");
        writeln!(out, "        val cache = conn.clientCache.getTable(tableName) ?: return emptySequence()");
        writeln!(out, "        return cache.allRows().map {{ bytes ->");
        writeln!(out, "            {row_type}.read(BsatnReader(bytes))");
        writeln!(out, "        }}.asSequence()");
        writeln!(out, "    }}");
        writeln!(out);
        writeln!(out, "    override fun onInsert(callback: (EventContext<*>, {row_type}) -> Unit): CallbackId {{");
        writeln!(out, "        return conn.table(tableName).onInsert {{ bytes ->");
        writeln!(out, "            val row = {row_type}.read(BsatnReader(bytes))");
        writeln!(out, "            callback(conn.makeEventContext(), row)");
        writeln!(out, "        }}");
        writeln!(out, "    }}");
        writeln!(out);
        writeln!(out, "    override fun removeOnInsert(id: CallbackId) = conn.table(tableName).removeOnInsert(id)");
        writeln!(out);
        writeln!(out, "    override fun onDelete(callback: (EventContext<*>, {row_type}) -> Unit): CallbackId {{");
        writeln!(out, "        return conn.table(tableName).onDelete {{ bytes ->");
        writeln!(out, "            val row = {row_type}.read(BsatnReader(bytes))");
        writeln!(out, "            callback(conn.makeEventContext(), row)");
        writeln!(out, "        }}");
        writeln!(out, "    }}");
        writeln!(out);
        writeln!(out, "    override fun removeOnDelete(id: CallbackId) = conn.table(tableName).removeOnDelete(id)");
        writeln!(out);

        if !table.is_event {
            writeln!(out, "    override fun onUpdate(callback: (EventContext<*>, {row_type}, {row_type}) -> Unit): CallbackId {{");
            writeln!(out, "        return conn.table(tableName).onUpdate {{ oldBytes, newBytes ->");
            writeln!(out, "            val oldRow = {row_type}.read(BsatnReader(oldBytes))");
            writeln!(out, "            val newRow = {row_type}.read(BsatnReader(newBytes))");
            writeln!(out, "            callback(conn.makeEventContext(), oldRow, newRow)");
            writeln!(out, "        }}");
            writeln!(out, "    }}");
            writeln!(out);
            writeln!(out, "    override fun removeOnUpdate(id: CallbackId) = conn.table(tableName).removeOnUpdate(id)");
            writeln!(out);
        }

        // Index-based lookups (e.g. findByEmail) can be added here
        // by generating methods that query the cache by unique constraint.


        writeln!(out, "}}");
        writeln!(out);

        // Generate typed column accessors
        let cols_name = format!("{}Cols", accessor_pascal);
        let product_def = module.typespace_for_generate()[table.product_type_ref].as_product().unwrap();
        writeln!(out, "class {cols_name}(tableName: String) : Cols<{row_type}>(tableName) {{");
        for (field_name, field_type) in &product_def.elements {
            let camel = field_name.deref().to_case(Case::Camel);
            let ty_str = ty_fmt(module, field_type).to_string();
            writeln!(out, "    val {camel}: Col<{ty_str}> = Col(\"{camel}\")");
        }
        writeln!(out, "}}");

        // Generate IxCols for indexed columns
        let mut ix_col_positions: Vec<usize> = Vec::new();
        for idx in iter_indexes(table) {
            for col_pos in idx.algorithm.columns().iter() {
                if !ix_col_positions.contains(&col_pos.idx()) {
                    ix_col_positions.push(col_pos.idx());
                }
            }
        }
        if !ix_col_positions.is_empty() {
            writeln!(out);
            let ixcols_name = format!("{}IxCols", accessor_pascal);
            writeln!(out, "class {ixcols_name}(tableName: String) : Cols<{row_type}>(tableName) {{");
            for &pos in &ix_col_positions {
                if let Some((field_name, field_type)) = product_def.elements.get(pos) {
                    let camel = field_name.deref().to_case(Case::Camel);
                    let ty_str = ty_fmt(module, field_type).to_string();
                    writeln!(out, "    val {camel}: Col<{ty_str}> = Col(\"{camel}\")");
                }
            }
            writeln!(out, "}}");
        }

        OutputFile {
            filename: format!("{}/tables/{accessor_pascal}.kt", pkg_path(&self.package_name)),
            code: output.into_inner(),
        }
    }

    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile> {
        let name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
        let pkg_prefix = pkg_path(&self.package_name);
        let filename = format!("{pkg_prefix}/types/{name}.kt");
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, &self.package_name, "types");

        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                writeln!(out, "data class {name}(");
                out.indent(1);
                for (field_name, field_type) in &product.elements {
                    let camel_name = field_name.deref().to_case(Case::Camel);
                    let ty_str = ty_fmt(module, field_type).to_string();
                    writeln!(out, "val {camel_name}: {ty_str},");
                }
                out.dedent(1);
                writeln!(out, ") {{");
                writeln!(out, "    companion object {{");
                writeln!(out, "        fun read(reader: BsatnReader): {name} =");
                writeln!(out, "            {name}(");
                out.indent(4);
                for (field_name, field_type) in &product.elements {
                    write_bsatn_deserialize_field(module, out, field_name, field_type);
                }
                out.dedent(4);
                writeln!(out, "            )");
                writeln!(out);
                writeln!(out, "        fun write(writer: BsatnWriter, value: {name}) {{");
                out.indent(3);
                let elements_copy = product.elements.clone();
                for (field_name, field_type) in &elements_copy {
                    write_bsatn_serialize_field(module, out, "value", field_name, field_type);
                }
                out.dedent(3);
                writeln!(out, "        }}");
                writeln!(out, "    }}");
                writeln!(out, "}}");
            }
            AlgebraicTypeDef::Sum(sum) => {
                writeln!(out, "sealed class {name} {{");
                for (variant_name, variant_type) in &sum.variants {
                    let pascal_variant = variant_name.deref().to_case(Case::Pascal);
                    match variant_type {
                        AlgebraicTypeUse::Unit => {
                            writeln!(out, "    data object {pascal_variant} : {name}()");
                        }
                        _ => {
                            let ty_str = ty_fmt(module, variant_type).to_string();
                            writeln!(out, "    data class {pascal_variant}(val value: {ty_str}) : {name}()");
                        }
                    }
                }
                writeln!(out);
                writeln!(out, "    companion object {{");
                writeln!(out, "        fun read(reader: BsatnReader): {name} {{");
                writeln!(out, "            val tag = reader.readTag().toInt()");
                writeln!(out, "            return when (tag) {{");
                for (i, (variant_name, variant_type)) in sum.variants.iter().enumerate() {
                    let pascal_variant = variant_name.deref().to_case(Case::Pascal);
                    match variant_type {
                        AlgebraicTypeUse::Unit => {
                            writeln!(out, "                {i} -> {name}.{pascal_variant}");
                        }
                        _ => {
                            writeln!(out, "                {i} -> {name}.{pascal_variant}(TODO(\"read variant payload\"))");
                        }
                    }
                }
                writeln!(out, "                else -> throw IllegalStateException(\"Unknown {name} tag\")");
                writeln!(out, "            }}");
                writeln!(out, "        }}");
                writeln!(out);
                writeln!(out, "        fun write(writer: BsatnWriter, value: {name}) {{");
                writeln!(out, "            when (value) {{");
                for (i, (variant_name, variant_type)) in sum.variants.iter().enumerate() {
                    let pascal_variant = variant_name.deref().to_case(Case::Pascal);
                    match variant_type {
                        AlgebraicTypeUse::Unit => {
                            writeln!(out, "                is {name}.{pascal_variant} -> writer.writeTag({i}u)");
                        }
                        _ => {
                            writeln!(out, "                is {name}.{pascal_variant} -> writer.writeTag({i}u) // TODO: write variant payload");
                        }
                    }
                }
                writeln!(out, "            }}");
                writeln!(out, "        }}");
                writeln!(out, "    }}");
                writeln!(out, "}}");
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                writeln!(out, "enum class {name} {{");
                for (_i, variant_name) in plain_enum.variants.iter().enumerate() {
                    let pascal_variant = variant_name.deref().to_case(Case::Pascal);
                    writeln!(out, "    {pascal_variant},");
                }
                writeln!(out, ";");
                writeln!(out);
                writeln!(out, "    companion object {{");
                writeln!(out, "        fun read(reader: BsatnReader): {name} =");
                writeln!(out, "            entries[reader.readU8().toInt()]");
                writeln!(out);
                writeln!(out, "        fun write(writer: BsatnWriter, value: {name}) {{");
                writeln!(out, "            writer.writeU8(value.ordinal.toUByte())");
                writeln!(out, "        }}");
                writeln!(out, "    }}");
                writeln!(out, "}}");
            }
        }

        vec![OutputFile { filename, code: output.into_inner() }]
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, &self.package_name, "reducers");

        let pascal_name = reducer.accessor_name.deref().to_case(Case::Pascal);
        let camel_name = reducer.accessor_name.deref().to_case(Case::Camel);

        if !reducer.params_for_generate.elements.is_empty() {
            writeln!(out, "data class {pascal_name}Args(");
            out.indent(1);
            for (param_name, param_type) in &reducer.params_for_generate {
                let camel_param = param_name.deref().to_case(Case::Camel);
                let ty_str = ty_fmt(module, param_type).to_string();
                writeln!(out, "val {camel_param}: {ty_str},");
            }
            out.dedent(1);
            writeln!(out, ")");
        } else {
            writeln!(out, "data object {pascal_name}Args");
        }
        writeln!(out);
        writeln!(out, "internal fun {camel_name}Reducer(conn: DbConnection, args: {pascal_name}Args, callback: ((ReducerResult) -> Unit)? = null) {{");
        out.indent(1);
        if reducer.params_for_generate.elements.is_empty() {
            writeln!(out, "val bytes = ByteArray(0)");
        } else {
            writeln!(out, "val writer = BsatnWriter()");
            for (param_name, param_type) in &reducer.params_for_generate {
                write_bsatn_serialize_field(module, out, "args", param_name, param_type);
            }
            writeln!(out, "val bytes = writer.toByteArray()");
        }
        writeln!(out, "conn.callReducer(\"{name}\", bytes, callback)", name = reducer.name);
        out.dedent(1);
        writeln!(out, "}}");

        OutputFile {
            filename: format!("{}/reducers/{pascal_name}.kt", pkg_path(&self.package_name)),
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

        print_file_header(out, &self.package_name, "procedures");

        let pascal_name = procedure.accessor_name.deref().to_case(Case::Pascal);
        let camel_name = procedure.accessor_name.deref().to_case(Case::Camel);

        if !procedure.params_for_generate.elements.is_empty() {
            writeln!(out, "data class {pascal_name}Args(");
            out.indent(1);
            for (param_name, param_type) in &procedure.params_for_generate {
                let camel_param = param_name.deref().to_case(Case::Camel);
                let ty_str = ty_fmt(module, param_type).to_string();
                writeln!(out, "val {camel_param}: {ty_str},");
            }
            out.dedent(1);
            writeln!(out, ")");
        } else {
            writeln!(out, "data object {pascal_name}Args");
        }
        writeln!(out);
        writeln!(out, "internal fun {camel_name}Procedure(conn: DbConnection, args: {pascal_name}Args, callback: ((ProcedureResult) -> Unit)? = null) {{");
        out.indent(1);
        writeln!(out, "val writer = BsatnWriter()");
        for (param_name, param_type) in &procedure.params_for_generate {
            write_bsatn_serialize_field(module, out, "args", param_name, param_type);
        }
        writeln!(out, "conn.callProcedure(\"{name}\", writer.toByteArray(), callback)", name = procedure.name);
        out.dedent(1);
        writeln!(out, "}}");

        OutputFile {
            filename: format!("{}/procedures/{pascal_name}.kt", pkg_path(&self.package_name)),
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, &self.package_name, "");

        let has_reducers = iter_reducers(module, options.visibility).count() > 0;
        let has_procedures = iter_procedures(module, options.visibility).count() > 0;
        if has_reducers {
            writeln!(out, "import {pkg}.reducers.*", pkg = self.package_name);
        }
        if has_procedures {
            writeln!(out, "import {pkg}.procedures.*", pkg = self.package_name);
        }
        if has_reducers || has_procedures {
            writeln!(out);
        }

        // RemoteTables
        writeln!(out, "class RemoteTables(private val conn: DbConnection) {{");
        for (_, accessor_name, _product_type_ref) in iter_table_names_and_types(module, options.visibility) {
            let camel_table = accessor_name.deref().to_case(Case::Camel);
            let pascal_table = accessor_name.deref().to_case(Case::Pascal);
            let table_class = format!("{pascal_table}Handle");
            writeln!(out, "    val {camel_table}: {table_class} = {table_class}(conn)");
        }
        writeln!(out, "}}");
        writeln!(out);

        // RemoteReducers
        writeln!(out, "class RemoteReducers(val conn: DbConnection) {{");
        writeln!(out, "    internal var onUnhandledReducerError: ((ReducerEventContext, Exception) -> Unit)? = null");
        for reducer in iter_reducers(module, options.visibility) {
            let camel = reducer.accessor_name.deref().to_case(Case::Camel);
            let pascal = reducer.accessor_name.deref().to_case(Case::Pascal);
            writeln!(out, "    fun {camel}(args: {pascal}Args, callback: ((ReducerResult) -> Unit)? = null) = {camel}Reducer(conn, args, callback)");
        }
        writeln!(out, "}}");
        writeln!(out);
        // RemoteProcedures
        writeln!(out, "class RemoteProcedures(val conn: DbConnection) {{");
        for procedure in iter_procedures(module, options.visibility) {
            let camel = procedure.accessor_name.deref().to_case(Case::Camel);
            let pascal = procedure.accessor_name.deref().to_case(Case::Pascal);
            writeln!(out, "    fun {camel}(args: {pascal}Args, callback: ((ProcedureResult) -> Unit)? = null) = {camel}Procedure(conn, args, callback)");
        }
        writeln!(out, "}}");
        writeln!(out);

        // From class — typed table accessors for query building
        writeln!(out, "class From {{");
        for (name, accessor_name, product_type_ref) in iter_table_names_and_types(module, options.visibility) {
            let row_type = type_ref_name(module, product_type_ref);
            let camel = accessor_name.deref().to_case(Case::Camel);
            let cols_name = format!("{}Cols", accessor_name.deref().to_case(Case::Pascal));
            let sql_name = name.deref();
            writeln!(out, "    val {camel}: QueryTable<{row_type}> = QueryTable(\"{sql_name}\") {{ {cols_name}(it) }}");
        }
        writeln!(out, "}}");
        writeln!(out);
        writeln!(out, "fun SubscriptionBuilder.addQuery(provider: From.() -> QueryProvider): SubscriptionBuilder {{");
        writeln!(out, "    val from = From()");
        writeln!(out, "    return addQueryFrom(from.run(provider))");
        writeln!(out, "}}");
        writeln!(out);
        writeln!(out, "val DbConnection.db: RemoteTables get() = RemoteTables(this)");
        writeln!(out, "val DbConnection.reducers: RemoteReducers get() = RemoteReducers(this)");
        writeln!(out, "val DbConnection.procedures: RemoteProcedures get() = RemoteProcedures(this)");
        writeln!(out);
        writeln!(out, "// EventContext factory");
        writeln!(out, "fun DbConnection.makeEventContext(): EventContext<Nothing> {{");
        writeln!(out, "    return EventContext(identity, connectionId, savedToken, isActive, connectionState, Event.Transaction, this)");
        writeln!(out, "}}");

        vec![OutputFile {
            filename: format!("{}/RemoteModule.kt", pkg_path(&self.package_name)),
            code: output.into_inner(),
        }]
    }
}
