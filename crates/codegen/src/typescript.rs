use crate::util::{
    is_reducer_invokable, iter_constraints, iter_indexes, iter_procedures, iter_reducers, iter_table_names_and_types,
    iter_tables, iter_types,
};
use crate::{CodegenOptions, OutputFile};

use super::util::{collect_case, print_auto_generated_file_comment, type_ref_name};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{ColumnDef, IndexAlgorithm, ModuleDef, ReducerDef, TableDef, TypeDef};
use spacetimedb_schema::identifier::{Identifier, NamespacePath};
use spacetimedb_schema::reducer_name::ReducerName;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};

use super::code_indenter::{CodeIndenter, Indenter};
use super::Lang;
use spacetimedb_lib::version::spacetimedb_lib_version;

const INDENT: &str = "  ";

fn ts_string_literal(s: &str) -> String {
    serde_json::to_string(s).expect("serializing a string literal cannot fail")
}

pub struct TypeScript;

impl TypeScript {
    /// Generate registered types for internal SDK schemas, without client wiring.
    pub fn generate_type_declarations(&self, module: &ModuleDef) -> OutputFile {
        types_only(module)
    }
}

impl Lang for TypeScript {
    fn generate_type_files(&self, _module: &ModuleDef, _typ: &TypeDef) -> Vec<OutputFile> {
        vec![]
    }

    /// e.g.
    /// ```ts
    /// table({
    ///   name: 'player',
    ///   indexes: [
    ///     {
    ///       accessor: 'this_is_an_index',
    ///       name: 'this_is_an_index',
    ///       algorithm: "btree",
    ///       columns: [ "ownerId" ],
    ///     }
    ///   ],
    /// }, t.row({
    ///   id: t.u32().primaryKey(),
    ///   ownerId: t.string(),
    ///   name: t.string().unique(),
    ///   location: pointType,
    /// }))
    /// ```
    fn generate_table_file_from_schema(
        &self,
        module: &ModuleDef,
        table: &TableDef,
        _schema: TableSchema,
    ) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out);

        let type_ref = table.product_type_ref;
        let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();

        // Import the types of all fields.
        // We only need to import fields which have indices or unique constraints,
        // but it's easier to just import all of 'em, since we have `// @ts-nocheck` anyway.
        gen_and_print_imports(
            module,
            out,
            product_def.element_types(),
            &[], // No need to skip any imports; we're not defining a type, so there's no chance of circular imports.
        );

        writeln!(out);

        writeln!(out, "export default __t.row({{");
        out.indent(1);
        write_object_type_builder_fields(
            module,
            out,
            &product_def.elements,
            table.primary_key,
            true,
            Some(&table.columns),
        )
        .unwrap();
        out.dedent(1);
        writeln!(out, "}});");
        OutputFile {
            filename: table_module_name(&table.accessor_name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out);

        out.newline();

        gen_and_print_imports(
            module,
            out,
            reducer.params_for_generate.element_types(),
            // No need to skip any imports; we're not emitting a type that other modules can import.
            &[],
        );

        define_body_for_reducer(module, out, &reducer.params_for_generate.elements);

        OutputFile {
            filename: reducer_module_name(&reducer.accessor_name) + ".ts",
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

        out.newline();

        gen_and_print_imports(
            module,
            out,
            procedure
                .params_for_generate
                .element_types()
                .chain([&procedure.return_type_for_generate]),
            // No need to skip any imports; we're not emitting a type that other modules can import.
            &[],
        );

        writeln!(out, "export const params = {{");
        out.with_indent(|out| {
            write_object_type_builder_fields(module, out, &procedure.params_for_generate.elements, None, true, None)
                .unwrap()
        });
        writeln!(out, "}};");

        write!(out, "export const returnType = ");
        write_type_builder(module, out, &procedure.return_type_for_generate).unwrap();

        OutputFile {
            filename: procedure_module_name(&procedure.accessor_name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
        let reducers_file = generate_reducers_file(module, options);
        let procedures_file = generate_procedures_file(module, options);
        let types_file = generate_types_file(module);

        let mut files = vec![reducers_file, procedures_file, types_file];

        // Generate types.ts for each submodule namespace so that the
        // namespace-scoped reducer/procedure/table files can resolve their
        // `import { … } from "./types"` imports.
        let mut submodule_namespaces: BTreeMap<String, (NamespacePath, &ModuleDef)> = BTreeMap::new();
        collect_submodule_namespaces(module, &NamespacePath::root(), &mut submodule_namespaces);
        for (_, owning_def) in submodule_namespaces.values() {
            let ns_path = submodule_ns_path(owning_def.accessor_path());
            let filename = format!("{ns_path}/types.ts");
            files.push(generate_types_file_with_path(owning_def, filename));
        }

        generate_declarations(module, options, files)
    }
}

fn generate_reducers_file(module: &ModuleDef, options: &CodegenOptions) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_auto_generated_file_comment(out);
    print_lint_suppression(out);
    writeln!(out, "import {{ type Infer as __Infer }} from \"spacetimedb\";");

    writeln!(out);
    writeln!(out, "// Import all reducer arg schemas");
    for reducer in iter_reducers(module, options.visibility) {
        let reducer_module_name = reducer_module_name(&reducer.accessor_name);
        let args_type = reducer_args_type_name(&reducer.accessor_name);
        writeln!(out, "import {args_type} from \"../{reducer_module_name}\";");
    }

    writeln!(out);
    for reducer in iter_reducers(module, options.visibility) {
        let reducer_name_pascalcase = reducer.accessor_name.deref().to_case(Case::Pascal);
        let args_type = reducer_args_type_name(&reducer.accessor_name);
        writeln!(
            out,
            "export type {reducer_name_pascalcase}Params = __Infer<typeof {args_type}>;"
        );
    }
    out.newline();

    OutputFile {
        filename: "types/reducers.ts".to_string(),
        code: output.into_inner(),
    }
}

fn generate_procedures_file(module: &ModuleDef, options: &CodegenOptions) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_auto_generated_file_comment(out);
    print_lint_suppression(out);
    writeln!(out, "import {{ type Infer as __Infer }} from \"spacetimedb\";");

    writeln!(out);
    writeln!(out, "// Import all procedure arg schemas");
    for procedure in iter_procedures(module, options.visibility) {
        let procedure_module_name = procedure_module_name(&procedure.accessor_name);
        let args_type = procedure_args_type_name(&procedure.accessor_name);
        writeln!(out, "import * as {args_type} from \"../{procedure_module_name}\";");
    }

    writeln!(out);
    for procedure in iter_procedures(module, options.visibility) {
        let procedure_name_pascalcase = procedure.accessor_name.deref().to_case(Case::Pascal);
        let args_type = procedure_args_type_name(&procedure.accessor_name);
        writeln!(
            out,
            "export type {procedure_name_pascalcase}Args = __Infer<typeof {args_type}.params>;"
        );
        writeln!(
            out,
            "export type {procedure_name_pascalcase}Result = __Infer<typeof {args_type}.returnType>;"
        );
    }
    out.newline();

    OutputFile {
        filename: "types/procedures.ts".to_string(),
        code: output.into_inner(),
    }
}

fn generate_types_file(module: &ModuleDef) -> OutputFile {
    generate_types_file_with_path(module, "types.ts".to_string())
}

fn generate_types_file_with_path(module: &ModuleDef, filename: String) -> OutputFile {
    generate_types_file_with_reexports(module, filename, "", &BTreeSet::new())
}

fn generate_types_file_with_reexports(
    module: &ModuleDef,
    filename: String,
    declarations: &str,
    reexports: &BTreeSet<AlgebraicTypeRef>,
) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_file_header(out);
    out.newline();

    let reducer_type_names = module
        .reducers()
        .map(|reducer| reducer.accessor_name.deref().to_case(Case::Pascal))
        .collect::<BTreeSet<_>>();

    for ty in iter_types(module) {
        let type_name = collect_case(Case::Pascal, ty.accessor_name.name_segments());
        if reducer_type_names.contains(&type_name) {
            continue;
        }

        if reexports.contains(&ty.ty) {
            writeln!(
                out,
                "import {{ {type_name} }} from {};",
                ts_string_literal(declarations)
            );
            writeln!(out, "export {{ {type_name} }};");
            writeln!(out, "export type {type_name} = __Infer<typeof {type_name}>;");
            out.newline();
            continue;
        }

        match &module.typespace_for_generate()[ty.ty] {
            AlgebraicTypeDef::Product(product) => define_body_for_product(module, out, &type_name, &product.elements),
            AlgebraicTypeDef::Sum(sum) => define_body_for_sum(module, out, &type_name, &sum.variants),
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                let variants = plain_enum
                    .variants
                    .iter()
                    .cloned()
                    .map(|var| (var, AlgebraicTypeUse::Unit))
                    .collect::<Vec<_>>();
                define_body_for_sum(module, out, &type_name, &variants)
            }
        }
    }

    OutputFile {
        filename,
        code: output.into_inner(),
    }
}

/// Recursively collect all submodule namespaces in depth-first order.
/// Keys are dot-terminated prefix strings (e.g. `"lib."`, `"lib.sublib."`).
/// Values are references to the `ModuleDef` that owns that namespace.
fn collect_submodule_namespaces<'a>(
    module: &'a ModuleDef,
    prefix: &NamespacePath,
    out: &mut BTreeMap<String, (NamespacePath, &'a ModuleDef)>,
) {
    for (ns, submodule_def) in module.submodules() {
        let full_prefix = prefix.child(ns.clone());
        out.insert(full_prefix.to_string(), (full_prefix.clone(), submodule_def));
        collect_submodule_namespaces(submodule_def, &full_prefix, out);
    }
}

fn print_type_builder_imports(out: &mut Indenter) {
    // All library imports are prefixed with `__` to avoid
    // clashing with the names of user generated types.
    let mut types = [
        "TypeBuilder as __TypeBuilder",
        "type AlgebraicTypeType as __AlgebraicTypeType",
        "type Infer as __Infer",
        "t as __t",
    ];
    types.sort();
    writeln!(out, "import {{");
    out.indent(1);
    for ty in types {
        writeln!(out, "{ty},");
    }
    out.dedent(1);
    writeln!(out, "}} from \"spacetimedb\";");
}

fn print_file_header(output: &mut Indenter) {
    print_auto_generated_file_comment(output);
    print_lint_suppression(output);
    print_type_builder_imports(output);
}

fn print_lint_suppression(output: &mut Indenter) {
    writeln!(output, "/* eslint-disable */");
    writeln!(output, "/* tslint:disable */");
}

/// e.g.
/// ```ts
/// export default {
///   x: __t.f32(),
///   y: __t.f32(),
///   fooBar: __t.string(),
/// };
/// ```
fn define_body_for_reducer(module: &ModuleDef, out: &mut Indenter, params: &[(Identifier, AlgebraicTypeUse)]) {
    write!(out, "export default {{");
    if params.is_empty() {
        writeln!(out, "}};");
    } else {
        writeln!(out);
        out.with_indent(|out| write_object_type_builder_fields(module, out, params, None, true, None).unwrap());
        writeln!(out, "}};");
    }
}

/// e.g.
/// ```ts
/// export const Point = __t.object('Point', {
///   x: __t.f32(),
///   y: __t.f32(),
///   fooBar: __t.string(),
/// });
/// export type Point = __Infer<typeof Point>;
/// ```
fn define_body_for_product(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {
    write!(out, "export const {name} = __t.object(\"{name}\", {{");
    if elements.is_empty() {
        writeln!(out, "}});");
    } else {
        writeln!(out);
        out.with_indent(|out| write_object_type_builder_fields(module, out, elements, None, true, None).unwrap());
        writeln!(out, "}});");
    }
    writeln!(out, "export type {name} = __Infer<typeof {name}>;");
    out.newline();
}

fn write_object_type_builder_fields(
    module: &ModuleDef,
    out: &mut Indenter,
    elements: &[(Identifier, AlgebraicTypeUse)],
    primary_key: Option<ColId>,
    convert_case: bool,
    columns: Option<&[ColumnDef]>,
) -> anyhow::Result<()> {
    for (i, (ident, ty)) in elements.iter().enumerate() {
        let name = if convert_case {
            ident.deref().to_case(Case::Camel)
        } else {
            ident.deref().into()
        };

        let is_primary_key = match primary_key {
            Some(pk) => pk.idx() == i,
            None => false,
        };
        // The `.name(..)` value is the in-database (canonical) column name, which may
        // differ from the generated camelCase accessor key. Emit it only when the
        // canonical name differs, so the client maps to the correct wire/column name
        // regardless of the source identifier's casing.
        let original_name = columns
            .and_then(|columns| columns.get(i))
            .map(|column| column.name.deref())
            .filter(|canonical| convert_case && *canonical != name.as_str());
        write_type_builder_field(module, out, &name, original_name, ty, is_primary_key)?;
    }

    Ok(())
}

/// Returns whether `ty` recursively contains an `AlgebraicTypeUse::Ref`
fn type_contains_ref(ty: &AlgebraicTypeUse) -> bool {
    match ty {
        AlgebraicTypeUse::Ref(_) => true,
        AlgebraicTypeUse::Option(inner) | AlgebraicTypeUse::Array(inner) => type_contains_ref(inner),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => type_contains_ref(ok_ty) || type_contains_ref(err_ty),
        _ => false,
    }
}

fn write_type_builder_field(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    original_name: Option<&str>,
    ty: &AlgebraicTypeUse,
    is_primary_key: bool,
) -> fmt::Result {
    // If the type contains a ref, we need to use a getter to prevent access-before-initialization.
    let needs_getter = type_contains_ref(ty);

    if needs_getter {
        writeln!(out, "get {name}() {{");
        out.indent(1);
        write!(out, "return ");
    } else {
        write!(out, "{name}: ");
    }
    write_type_builder(module, out, ty)?;
    if is_primary_key {
        write!(out, ".primaryKey()");
    }
    if let Some(original_name) = original_name {
        write!(out, ".name(\"{original_name}\")");
    }
    if needs_getter {
        writeln!(out, ";");
        out.dedent(1);
        writeln!(out, "}},");
    } else {
        writeln!(out, ",");
    }

    Ok(())
}

/// e.g. `__t.option(__t.i32())`, `__t.string()`
fn write_type_builder<W: Write>(module: &ModuleDef, out: &mut W, ty: &AlgebraicTypeUse) -> fmt::Result {
    write_type_builder_with_names(module, out, ty, "__t", "")
}

fn write_type_builder_with_names<W: Write>(
    module: &ModuleDef,
    out: &mut W,
    ty: &AlgebraicTypeUse,
    builder: &str,
    types: &str,
) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "{builder}.unit()")?,
        AlgebraicTypeUse::Never => write!(out, "{builder}.never()")?,
        AlgebraicTypeUse::Identity => write!(out, "{builder}.identity()")?,
        AlgebraicTypeUse::ConnectionId => write!(out, "{builder}.connectionId()")?,
        AlgebraicTypeUse::Timestamp => write!(out, "{builder}.timestamp()")?,
        AlgebraicTypeUse::TimeDuration => write!(out, "{builder}.timeDuration()")?,
        AlgebraicTypeUse::ScheduleAt => write!(out, "{builder}.scheduleAt()")?,
        AlgebraicTypeUse::Uuid => write!(out, "{builder}.uuid()")?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write!(out, "{builder}.option(")?;
            write_type_builder_with_names(module, out, inner_ty, builder, types)?;
            write!(out, ")")?;
        }
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            write!(out, "{builder}.result(")?;
            write_type_builder_with_names(module, out, ok_ty, builder, types)?;
            write!(out, ", ")?;
            write_type_builder_with_names(module, out, err_ty, builder, types)?;
            write!(out, ")")?;
        }
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::Bool => write!(out, "{builder}.bool()")?,
            PrimitiveType::I8 => write!(out, "{builder}.i8()")?,
            PrimitiveType::U8 => write!(out, "{builder}.u8()")?,
            PrimitiveType::I16 => write!(out, "{builder}.i16()")?,
            PrimitiveType::U16 => write!(out, "{builder}.u16()")?,
            PrimitiveType::I32 => write!(out, "{builder}.i32()")?,
            PrimitiveType::U32 => write!(out, "{builder}.u32()")?,
            PrimitiveType::I64 => write!(out, "{builder}.i64()")?,
            PrimitiveType::U64 => write!(out, "{builder}.u64()")?,
            PrimitiveType::I128 => write!(out, "{builder}.i128()")?,
            PrimitiveType::U128 => write!(out, "{builder}.u128()")?,
            PrimitiveType::I256 => write!(out, "{builder}.i256()")?,
            PrimitiveType::U256 => write!(out, "{builder}.u256()")?,
            PrimitiveType::F32 => write!(out, "{builder}.f32()")?,
            PrimitiveType::F64 => write!(out, "{builder}.f64()")?,
        },
        AlgebraicTypeUse::String => write!(out, "{builder}.string()")?,
        AlgebraicTypeUse::Array(elem_ty) => {
            if matches!(&**elem_ty, AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                return write!(out, "{builder}.byteArray()");
            }
            write!(out, "{builder}.array(")?;
            write_type_builder_with_names(module, out, elem_ty, builder, types)?;
            write!(out, ")")?;
        }
        AlgebraicTypeUse::Ref(r) => {
            write!(out, "{types}{}", type_ref_name(module, *r))?;
        }
    }
    Ok(())
}

/// e.g.
/// ```ts
/// // The tagged union or sum type for the algebraic type `Option`.
/// export const Option = __t.enum("Option", {
///   none: __t.unit(),
///   some: { value: __t.i32() },
/// });
/// export type Option = __Infer<typeof Option>;
/// ```
fn define_body_for_sum(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(out, "// The tagged union or sum type for the algebraic type `{name}`.");
    write!(out, "export const {name}");
    if name == "AlgebraicType" {
        write!(out, ": __TypeBuilder<__AlgebraicTypeType, __AlgebraicTypeType>");
    }
    writeln!(out, " = __t.enum(\"{name}\", {{");
    // Convert variant names to PascalCase
    let pascal_variants: Vec<(Identifier, AlgebraicTypeUse)> = variants
        .iter()
        .map(|(ident, ty)| {
            let pascal = ident.deref().to_case(Case::Pascal);
            (Identifier::for_test(pascal), ty.clone())
        })
        .collect();
    out.with_indent(|out| write_object_type_builder_fields(module, out, &pascal_variants, None, false, None).unwrap());
    writeln!(out, "}});");
    writeln!(out, "export type {name} = __Infer<typeof {name}>;");
    out.newline();
}

fn table_module_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake) + "_table"
}

/// Source name (wire name) for a submodule namespace table/view.
///
/// This is the *canonical* name, not the accessor name: it is what the host stores and
/// what appears on the wire. E.g. namespace="lib.", name="fruit_basket" → "lib.fruit_basket".
fn reducer_args_type_name(reducer_name: &ReducerName) -> String {
    reducer_name.deref().to_case(Case::Pascal) + "Reducer"
}

fn procedure_args_type_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal) + "Procedure"
}

fn reducer_module_name(reducer_name: &ReducerName) -> String {
    reducer_name.deref().to_case(Case::Snake) + "_reducer"
}

fn procedure_module_name(procedure_name: &Identifier) -> String {
    procedure_name.deref().to_case(Case::Snake) + "_procedure"
}

/// Converts a namespace path like `"lib."` or `"lib.sublib."` to a directory path like `"lib"` or `"lib/sublib"`.
/// Callers pass the *accessor* path, so generated directories follow the names used in module code.
fn submodule_ns_path(namespace: &NamespacePath) -> String {
    namespace.join_segments("/")
}

/// The key under which a submodule reducer or procedure is registered in the SDK's
/// accessor map. This corresponds to the owning module's accessor path and the camelCase
/// accessor name, joined by `.`, e.g. `myLib.libInsert`.
pub fn type_name(module: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
    let mut s = String::new();
    write_type(module, &mut s, ty, None, None).unwrap();
    s
}

// This should return true if we should wrap the type in parentheses when it is the element type of
// an array. This is needed if the type has a `|` in it, e.g. `Option<T>` or `Foo | Bar`, since
// without parens, `Foo | Bar[]` would be parsed as `Foo | (Bar[])`.
fn needs_parens_within_array(ty: &AlgebraicTypeUse) -> bool {
    match ty {
        AlgebraicTypeUse::Unit
        | AlgebraicTypeUse::Never
        | AlgebraicTypeUse::Identity
        | AlgebraicTypeUse::ConnectionId
        | AlgebraicTypeUse::Timestamp
        | AlgebraicTypeUse::TimeDuration
        | AlgebraicTypeUse::Uuid
        | AlgebraicTypeUse::Primitive(_)
        | AlgebraicTypeUse::Array(_)
        | AlgebraicTypeUse::Ref(_) // We use the type name for these.
        | AlgebraicTypeUse::String => {
            false
        }
        AlgebraicTypeUse::ScheduleAt | AlgebraicTypeUse::Option(_) | AlgebraicTypeUse::Result { .. } => {
            true
        }
    }
}

pub fn write_type<W: Write>(
    module: &ModuleDef,
    out: &mut W,
    ty: &AlgebraicTypeUse,
    ref_prefix: Option<&str>,
    ref_suffix: Option<&str>,
) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "void")?,
        AlgebraicTypeUse::Never => write!(out, "never")?,
        AlgebraicTypeUse::Identity => write!(out, "__Infer<typeof __t.identity()>")?,
        AlgebraicTypeUse::ConnectionId => write!(out, "__Infer<typeof __t.connectionId()>")?,
        AlgebraicTypeUse::Timestamp => write!(out, "__Infer<typeof __t.timestamp()>")?,
        AlgebraicTypeUse::TimeDuration => write!(out, "__Infer<typeof __t.timeDuration()>")?,
        AlgebraicTypeUse::Uuid => write!(out, "__Uuid")?,
        AlgebraicTypeUse::ScheduleAt => write!(
            out,
            "{{ tag: \"Interval\", value: __Infer<typeof __t.timeDuration()> }} | {{ tag: \"Time\", value: __Infer<typeof __t.timestamp()> }}"
        )?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write_type(module, out, inner_ty, ref_prefix, ref_suffix)?;
            write!(out, " | undefined")?;
        }
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            write_type(module, out, ok_ty, ref_prefix, ref_suffix)?;
            write!(out, " | ")?;
            write_type(module, out, err_ty, ref_prefix, ref_suffix)?;
        }
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::Bool => write!(out, "boolean")?,
            PrimitiveType::I8 => write!(out, "number")?,
            PrimitiveType::U8 => write!(out, "number")?,
            PrimitiveType::I16 => write!(out, "number")?,
            PrimitiveType::U16 => write!(out, "number")?,
            PrimitiveType::I32 => write!(out, "number")?,
            PrimitiveType::U32 => write!(out, "number")?,
            PrimitiveType::I64 => write!(out, "bigint")?,
            PrimitiveType::U64 => write!(out, "bigint")?,
            PrimitiveType::I128 => write!(out, "bigint")?,
            PrimitiveType::U128 => write!(out, "bigint")?,
            PrimitiveType::I256 => write!(out, "bigint")?,
            PrimitiveType::U256 => write!(out, "bigint")?,
            PrimitiveType::F32 => write!(out, "number")?,
            PrimitiveType::F64 => write!(out, "number")?,
        },
        AlgebraicTypeUse::String => write!(out, "string")?,
        AlgebraicTypeUse::Array(elem_ty) => {
            if matches!(&**elem_ty, AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                return write!(out, "Uint8Array");
            }
            let needs_parens = needs_parens_within_array(elem_ty);
            // We wrap the inner type in parentheses to avoid ambiguity with the [] binding.
            if needs_parens {
                write!(out, "(")?;
            }
            write_type(module, out, elem_ty, ref_prefix, ref_suffix)?;
            if needs_parens {
                write!(out, ")")?;
            }
            write!(out, "[]")?;
        }
        AlgebraicTypeUse::Ref(r) => {
            write!(out, "__Infer<typeof ")?;
            if let Some(prefix) = ref_prefix {
                write!(out, "{prefix}")?;
            }
            write!(out, "{}", type_ref_name(module, *r))?;
            if let Some(suffix) = ref_suffix {
                write!(out, "{suffix}")?;
            }
            write!(out, ">")?;
        }
    }
    Ok(())
}

/// Use `search_function` on `roots` to detect required imports, then print them with `print_imports`.
///
/// `this_file` is passed and excluded for the case of recursive types:
/// without it, the definition for a type like `struct Foo { foos: Vec<Foo> }`
/// would attempt to include `import { Foo } from "./foo"`.
fn gen_and_print_imports<'a>(
    module: &ModuleDef,
    out: &mut Indenter,
    roots: impl Iterator<Item = &'a AlgebraicTypeUse>,
    dont_import: &[AlgebraicTypeRef],
) {
    let mut imports = BTreeSet::new();

    for ty in roots {
        ty.for_each_ref(|r| {
            imports.insert(r);
        });
    }
    for skip in dont_import {
        imports.remove(skip);
    }

    if !imports.is_empty() {
        writeln!(out, "import {{");
        out.indent(1);
        for typeref in imports {
            let type_name = type_ref_name(module, typeref);
            writeln!(out, "{type_name},");
        }
        out.dedent(1);
        writeln!(out, "}} from \"./types\";");
        out.newline()
    }
}

// const RESERVED_KEYWORDS: [&str; 36] = [
//     "break",
//     "case",
//     "catch",
//     "class",
//     "const",
//     "continue",
//     "debugger",
//     "default",
//     "delete",
//     "do",
//     "else",
//     "enum",
//     "export",
//     "extends",
//     "false",
//     "finally",
//     "for",
//     "function",
//     "if",
//     "import",
//     "in",
//     "instanceof",
//     "new",
//     "null",
//     "return",
//     "super",
//     "switch",
//     "this",
//     "throw",
//     "true",
//     "try",
//     "typeof",
//     "var",
//     "void",
//     "while",
//     "with",
// ];

// fn typescript_field_name(field_name: String) -> String {
//     if RESERVED_KEYWORDS
//         .into_iter()
//         .map(String::from)
//         .collect::<Vec<String>>()
//         .contains(&field_name)
//     {
//         return format!("_{field_name}");
//     }

//     field_name
// }

fn generate_declarations(module: &ModuleDef, options: &CodegenOptions, mut files: Vec<OutputFile>) -> Vec<OutputFile> {
    generate_module(module, options, "", &mut files);
    files.push(entry_point(module, options, ""));
    files
}

// Local names are readable by default; aliases are only needed for actual collisions.
fn binding_name(preferred: &str, used: &mut BTreeSet<String>) -> String {
    let mut name = preferred.to_string();
    while used.contains(&name) {
        name.push('_');
    }
    used.insert(name.clone());
    name
}

fn generate_module(module: &ModuleDef, options: &CodegenOptions, dir: &str, files: &mut Vec<OutputFile>) {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;
    print_auto_generated_file_comment(out);

    let mut used: BTreeSet<String> = [
        "schema",
        "table",
        "t",
        "spacetimedb",
        "__BinaryReader",
        "default",
        "return",
        "class",
        "function",
        "var",
        "let",
        "const",
        "new",
        "delete",
        "switch",
        "case",
        "break",
        "continue",
        "for",
        "while",
        "do",
        "if",
        "else",
        "try",
        "catch",
        "finally",
        "throw",
        "extends",
        "import",
        "export",
        "in",
        "instanceof",
        "typeof",
        "void",
        "with",
        "yield",
        "await",
        "enum",
        "implements",
        "interface",
        "package",
        "private",
        "protected",
        "public",
        "static",
        "super",
        "this",
        "null",
        "true",
        "false",
        "debugger",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    // Reserve function exports before allocating table and child-module locals.
    let mut functions = BTreeMap::new();
    for accessor in iter_reducers(module, options.visibility)
        .filter(|r| is_reducer_invokable(r))
        .map(|r| r.accessor_name.deref())
        .chain(iter_procedures(module, options.visibility).map(|p| p.accessor_name.deref()))
        .chain(module.views().map(|v| v.accessor_name.deref()))
    {
        let accessor = accessor.to_case(Case::Camel);
        let binding = binding_name(&accessor, &mut used);
        functions.insert(accessor, binding);
    }
    let tables: Vec<_> = iter_tables(module, options.visibility).collect();
    let mut rows = tables.clone();
    for table in module.tables() {
        if table.schedule.as_ref().is_some_and(|s| {
            iter_reducers(module, options.visibility)
                .any(|r| is_reducer_invokable(r) && local_name(&r.name) == local_name(&s.function_name))
                || iter_procedures(module, options.visibility)
                    .any(|p| local_name(&p.name) == local_name(&s.function_name))
        }) && !rows.iter().any(|t| t.name == table.name)
        {
            rows.push(table);
        }
    }
    let mut used_types: Vec<_> = rows
        .iter()
        .flat_map(|table| {
            module.typespace_for_generate()[table.product_type_ref]
                .as_product()
                .unwrap()
                .element_types()
        })
        .collect();
    let scheduled = |name: &str| {
        rows.iter().any(|t| {
            t.schedule
                .as_ref()
                .is_some_and(|s| local_name(&s.function_name) == local_name(name))
        })
    };
    used_types.extend(
        iter_reducers(module, options.visibility)
            .filter(|r| is_reducer_invokable(r) && !scheduled(&r.name))
            .flat_map(|r| r.params_for_generate.element_types()),
    );
    for procedure in iter_procedures(module, options.visibility) {
        if !scheduled(&procedure.name) {
            used_types.extend(procedure.params_for_generate.element_types());
        }
        used_types.push(&procedure.return_type_for_generate);
    }
    used_types.extend(module.views().flat_map(|v| v.params_for_generate.element_types()));
    for view in module.views() {
        used_types.extend(
            module.typespace_for_generate()[TableDef::from(view.clone()).product_type_ref]
                .as_product()
                .unwrap()
                .element_types(),
        );
    }
    // Row types already represented by a table or view need a separate builder
    // only when another declaration refers to them.
    let row_models: Vec<_> = iter_tables(module, crate::CodegenVisibility::IncludePrivate)
        .cloned()
        .chain(module.views().cloned().map(TableDef::from))
        .collect();
    let named = selected_types(module, &used_types, &row_models);
    // Reuse named object/enum builders, but retain object builders for row types:
    // a RowBuilder in declarations does not have the old ObjectBuilder API.
    let reexports = named
        .iter()
        .filter(|ty| !row_models.iter().any(|row| row.product_type_ref == ty.ty))
        .map(|ty| ty.ty)
        .collect();
    let accessor_dir = module.accessor_path().join_segments("/");
    let filename = if accessor_dir.is_empty() {
        "types.ts".to_string()
    } else {
        format!("{accessor_dir}/types.ts")
    };
    let prefix = if accessor_dir.is_empty() {
        "./".to_string()
    } else {
        "../".repeat(module.accessor_path().segments().len())
    };
    let compatibility_types = generate_types_file_with_reexports(
        module,
        filename.clone(),
        &format!("{prefix}{dir}declarations"),
        &reexports,
    );
    files.retain(|file| file.filename != filename);
    files.push(compatibility_types);

    used.extend(
        named
            .iter()
            .map(|ty| collect_case(Case::Pascal, ty.accessor_name.name_segments())),
    );
    let mut imports = vec!["schema"];
    if !rows.is_empty() {
        imports.push("table");
    }
    if !named.is_empty()
        || module.views().next().is_some()
        || used_types.iter().any(|t| !matches!(t, AlgebraicTypeUse::Ref(_)))
    {
        imports.push("t");
    }
    writeln!(out, "import {{ {} }} from 'spacetimedb/schema';", imports.join(", "));
    if rows
        .iter()
        .copied()
        .chain(
            row_models
                .iter()
                .filter(|row| named.iter().any(|ty| ty.ty == row.product_type_ref)),
        )
        .any(|t| {
            t.columns
                .iter()
                .any(|c| c.default_value.as_ref().is_some_and(|v| default_literal(v).is_none()))
        })
    {
        writeln!(out, "import {{ BinaryReader as __BinaryReader }} from 'spacetimedb';");
    }
    let table_names: Vec<_> = rows
        .iter()
        .map(|t| binding_name(&t.accessor_name.deref().to_case(Case::Camel), &mut used))
        .collect();
    let children: Vec<_> = module
        .submodules()
        .iter()
        .map(|(canonical, child)| {
            let name = binding_name(&canonical.deref().to_case(Case::Camel), &mut used);
            writeln!(out, "import * as {name} from './{canonical}/declarations';");
            generate_module(child, options, &format!("{dir}{canonical}/"), files);
            (canonical, child, name)
        })
        .collect();
    out.newline();
    emit_named_types(module, out, &named, &row_models, false);
    for (i, table) in rows.iter().enumerate() {
        writeln!(out, "const {} = table(", table_names[i]);
        out.indent(1);
        table_options(out, table);
        if let Some(name) = shared_row_name(table, &named, &row_models) {
            writeln!(out, ", {name}");
        } else {
            writeln!(out, ", {{");
            write_row_fields(module, out, table);
            writeln!(out, "}}");
        }
        out.dedent(1);
        writeln!(out, ");");
        out.newline();
    }
    let view_rows: Vec<_> = module
        .views()
        .map(|view| {
            let row = TableDef::from(view.clone());
            if let Some(name) = shared_row_name(&row, &named, &row_models) {
                return name;
            }
            let name = binding_name(
                &format!("{}Row", view.accessor_name.deref().to_case(Case::Pascal)),
                &mut used,
            );
            writeln!(out, "export const {name} = t.row({{");
            write_row_fields(module, out, &row);
            writeln!(out, "}});");
            out.newline();
            name
        })
        .collect();
    writeln!(out, "const spacetimedb = schema({{");
    out.indent(1);
    for table in &tables {
        let i = rows.iter().position(|t| t.name == table.name).unwrap();
        let accessor = table.accessor_name.deref().to_case(Case::Camel);
        if accessor == table_names[i] {
            writeln!(out, "{accessor},");
        } else {
            writeln!(out, "{}: {},", ts_string_literal(&accessor), table_names[i]);
        }
    }
    for (canonical, child, name) in children {
        writeln!(
            out,
            "{}: {{ name: {}, module: {name} }},",
            ts_string_literal(child.mount_accessor_name().unwrap_or(canonical)),
            ts_string_literal(canonical)
        );
    }
    out.dedent(1);
    writeln!(out, "}});\nexport default spacetimedb;");
    for reducer in iter_reducers(module, options.visibility).filter(|r| is_reducer_invokable(r)) {
        let accessor = reducer.accessor_name.deref().to_case(Case::Camel);
        let binding = &functions[&accessor];
        out.newline();
        function_start(out, &accessor, binding, "reducer");
        function_args(
            module,
            out,
            &reducer.name,
            &accessor,
            &reducer.params_for_generate.elements,
            &rows,
            &table_names,
        );
        writeln!(out, ");");
        function_export(out, &accessor, binding);
    }
    for procedure in iter_procedures(module, options.visibility) {
        let accessor = procedure.accessor_name.deref().to_case(Case::Camel);
        let binding = &functions[&accessor];
        out.newline();
        function_start(out, &accessor, binding, "procedure");
        function_args(
            module,
            out,
            &procedure.name,
            &accessor,
            &procedure.params_for_generate.elements,
            &rows,
            &table_names,
        );
        write!(out, ", ");
        write_type_builder_with_names(module, out, &procedure.return_type_for_generate, "t", "").unwrap();
        writeln!(out, ");");
        function_export(out, &accessor, binding);
    }
    for (i, view) in module.views().enumerate() {
        let accessor = view.accessor_name.deref().to_case(Case::Camel);
        let binding = &functions[&accessor];
        let kind = if view.is_anonymous { "anonymousView" } else { "view" };
        let cardinality = if matches!(view.return_type_for_generate, AlgebraicTypeUse::Option(_)) {
            "option"
        } else {
            "array"
        };
        out.newline();
        function_start(out, &accessor, binding, kind);
        write!(
            out,
            "{{ name: {}, public: true }}, ",
            ts_string_literal(local_name(&view.name))
        );
        if !view.params_for_generate.elements.is_empty() {
            params(module, out, &view.params_for_generate.elements);
            write!(out, ", ");
        }
        writeln!(out, "t.{cardinality}({}));", view_rows[i]);
        function_export(out, &accessor, binding);
    }
    files.push(OutputFile {
        filename: format!("{dir}declarations.ts"),
        code: output.into_inner(),
    });
}

fn selected_types<'a>(module: &'a ModuleDef, uses: &[&AlgebraicTypeUse], rows: &[TableDef]) -> Vec<&'a TypeDef> {
    fn refs(ty: &AlgebraicTypeUse, needed: &mut BTreeSet<AlgebraicTypeRef>) {
        match ty {
            AlgebraicTypeUse::Ref(r) => {
                needed.insert(*r);
            }
            AlgebraicTypeUse::Array(t) | AlgebraicTypeUse::Option(t) => refs(t, needed),
            AlgebraicTypeUse::Result { ok_ty, err_ty } => {
                refs(ok_ty, needed);
                refs(err_ty, needed);
            }
            _ => {}
        }
    }
    let reducer_args = |ty: &TypeDef| {
        module.reducers().any(|reducer| {
            collect_case(Case::Pascal, ty.accessor_name.name_segments())
                == reducer.accessor_name.deref().to_case(Case::Pascal)
                && module.typespace_for_generate()[ty.ty]
                    .as_product()
                    .is_some_and(|p| p.elements == reducer.params_for_generate.elements)
        })
    };
    let mut needed: BTreeSet<_> = module
        .types()
        .filter(|ty| !rows.iter().any(|r| r.product_type_ref == ty.ty) && !reducer_args(ty))
        .map(|ty| ty.ty)
        .collect();
    for ty in uses {
        refs(ty, &mut needed);
    }
    loop {
        let previous = needed.clone();
        for r in &previous {
            match &module.typespace_for_generate()[*r] {
                AlgebraicTypeDef::Product(p) => {
                    for ty in p.element_types() {
                        refs(ty, &mut needed);
                    }
                }
                AlgebraicTypeDef::Sum(s) => {
                    for (_, ty) in &s.variants {
                        refs(ty, &mut needed);
                    }
                }
                AlgebraicTypeDef::PlainEnum(_) => {}
            }
        }
        if previous == needed {
            break;
        }
    }
    iter_types(module).filter(|ty| needed.contains(&ty.ty)).collect()
}

fn shared_row_name(table: &TableDef, types: &[&TypeDef], rows: &[TableDef]) -> Option<String> {
    let ty = types.iter().find(|ty| ty.ty == table.product_type_ref)?;
    let row = rows.iter().find(|row| row.product_type_ref == ty.ty)?;
    let sequences = |t: &TableDef| t.sequences.values().map(|s| s.column).collect::<BTreeSet<_>>();
    let same =
        row.primary_key == table.primary_key
            && sequences(row) == sequences(table)
            && row.columns.iter().zip(&table.columns).all(|(a, b)| {
                a.name == b.name && a.accessor_name == b.accessor_name && a.default_value == b.default_value
            });
    same.then(|| collect_case(Case::Pascal, ty.accessor_name.name_segments()))
}

// Internal SDK wire/schema generation needs the same named type declarations,
// without constructing a module or depending on the client runtime.
fn types_only(module: &ModuleDef) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    print_auto_generated_file_comment(&mut output);
    writeln!(output, "import {{ t }} from 'spacetimedb/schema';");
    emit_named_types(module, &mut output, &iter_types(module).collect::<Vec<_>>(), &[], true);
    OutputFile {
        filename: "types.ts".into(),
        code: output.into_inner(),
    }
}

// RowBuilder evaluates its fields at construction, unlike the lazy object and
// enum builders. Define named dependencies first so shared rows can reuse them.
fn ordered_types<'a>(module: &ModuleDef, types: &[&'a TypeDef]) -> Vec<&'a TypeDef> {
    fn visit<'a>(
        module: &ModuleDef,
        ty: &'a TypeDef,
        types: &[&'a TypeDef],
        seen: &mut BTreeSet<AlgebraicTypeRef>,
        out: &mut Vec<&'a TypeDef>,
    ) {
        if !seen.insert(ty.ty) {
            return;
        }
        fn visit_use<'a>(
            module: &ModuleDef,
            usage: &AlgebraicTypeUse,
            types: &[&'a TypeDef],
            seen: &mut BTreeSet<AlgebraicTypeRef>,
            out: &mut Vec<&'a TypeDef>,
        ) {
            match usage {
                AlgebraicTypeUse::Ref(r) => {
                    if let Some(ty) = types.iter().find(|ty| ty.ty == *r) {
                        visit(module, ty, types, seen, out);
                    }
                }
                AlgebraicTypeUse::Array(t) | AlgebraicTypeUse::Option(t) => visit_use(module, t, types, seen, out),
                AlgebraicTypeUse::Result { ok_ty, err_ty } => {
                    visit_use(module, ok_ty, types, seen, out);
                    visit_use(module, err_ty, types, seen, out);
                }
                _ => {}
            }
        }
        match &module.typespace_for_generate()[ty.ty] {
            AlgebraicTypeDef::Product(p) => {
                for usage in p.element_types() {
                    visit_use(module, usage, types, seen, out);
                }
            }
            AlgebraicTypeDef::Sum(s) => {
                for (_, usage) in &s.variants {
                    visit_use(module, usage, types, seen, out);
                }
            }
            AlgebraicTypeDef::PlainEnum(_) => {}
        }
        out.push(ty);
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for ty in types {
        visit(module, ty, types, &mut seen, &mut out);
    }
    out
}

fn emit_named_types(module: &ModuleDef, out: &mut Indenter, types: &[&TypeDef], rows: &[TableDef], data_aliases: bool) {
    if types.is_empty() {
        return;
    }
    let mut names: BTreeSet<_> = types
        .iter()
        .map(|ty| collect_case(Case::Pascal, ty.accessor_name.name_segments()))
        .collect();
    let algebraic_type = names.contains("AlgebraicType");
    let infer = binding_name("Infer", &mut names);
    let type_builder = binding_name("TypeBuilder", &mut names);
    let algebraic_type_type = binding_name("AlgebraicTypeType", &mut names);
    let import = if infer == "Infer" {
        "Infer".to_string()
    } else {
        format!("Infer as {infer}")
    };
    if data_aliases {
        writeln!(out, "import type {{ {import} }} from 'spacetimedb/schema';");
    }
    if algebraic_type {
        writeln!(out, "import type {{ TypeBuilder as {type_builder}, AlgebraicTypeType as {algebraic_type_type} }} from 'spacetimedb';");
    }
    for ty in ordered_types(module, types) {
        let name = collect_case(Case::Pascal, ty.accessor_name.name_segments());
        if let Some(row) = rows.iter().find(|row| row.product_type_ref == ty.ty) {
            writeln!(out, "export const {name} = t.row({}, {{", ts_string_literal(&name));
            write_row_fields(module, out, row);
            writeln!(out, "}});");
            if data_aliases {
                writeln!(out, "export type {name} = {infer}<typeof {name}>;");
            }
            out.newline();
            continue;
        }
        let (kind, fields, case) = match &module.typespace_for_generate()[ty.ty] {
            AlgebraicTypeDef::Product(p) => ("object", p.elements.clone(), Case::Camel),
            AlgebraicTypeDef::Sum(s) => ("enum", s.variants.clone(), Case::Pascal),
            AlgebraicTypeDef::PlainEnum(e) => (
                "enum",
                e.variants
                    .iter()
                    .cloned()
                    .map(|v| (v, AlgebraicTypeUse::Unit))
                    .collect(),
                Case::Pascal,
            ),
        };
        out.newline();
        if name == "AlgebraicType" {
            write!(
                out,
                "export const {name}: {type_builder}<{algebraic_type_type}, {algebraic_type_type}>"
            );
        } else {
            write!(out, "export const {name}");
        }
        writeln!(out, " = t.{kind}({}, {{", ts_string_literal(&name));
        out.indent(1);
        for (field, ty) in fields {
            let field = field.deref().to_case(case);
            let getter = type_contains_ref(&ty);
            if getter {
                write!(out, "get {field}() {{ return ");
            } else {
                write!(out, "{field}: ");
            }
            write_type_builder_with_names(module, out, &ty, "t", "").unwrap();
            if getter {
                writeln!(out, "; }},");
            } else {
                writeln!(out, ",");
            }
        }
        out.dedent(1);
        writeln!(out, "}});");
        if data_aliases {
            writeln!(out, "export type {name} = {infer}<typeof {name}>;");
        }
    }
}

fn params(module: &ModuleDef, out: &mut Indenter, elements: &[(Identifier, AlgebraicTypeUse)]) {
    if elements.is_empty() {
        write!(out, "{{}}");
        return;
    }
    writeln!(out, "{{");
    out.indent(1);
    for (name, ty) in elements {
        write!(out, "{}: ", name.deref().to_case(Case::Camel));
        write_type_builder_with_names(module, out, ty, "t", "").unwrap();
        writeln!(out, ",");
    }
    out.dedent(1);
    write!(out, "}}");
}

fn function_args(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    accessor: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
    rows: &[&TableDef],
    names: &[String],
) {
    let scheduled = rows.iter().position(|t| {
        t.schedule
            .as_ref()
            .is_some_and(|s| local_name(&s.function_name) == local_name(name))
    });
    if local_name(name) != accessor || scheduled.is_some() {
        write!(out, "{{ name: {}", ts_string_literal(local_name(name)));
        if let Some(i) = scheduled {
            write!(out, ", onSchedule: {}", names[i]);
        }
        write!(out, " }}, ");
    }
    if let Some(i) = scheduled {
        write!(
            out,
            "{{ {}: {}.rowType }}",
            elements[0].0.deref().to_case(Case::Camel),
            names[i]
        );
    } else {
        params(module, out, elements);
    }
}

fn function_start(out: &mut Indenter, accessor: &str, binding: &str, kind: &str) {
    let export = if accessor == binding { "export " } else { "" };
    write!(out, "{export}const {binding} = spacetimedb.{kind}(");
}

fn function_export(out: &mut Indenter, accessor: &str, binding: &str) {
    if accessor == "default" {
        writeln!(
            out,
            "export const __defaultFunction = spacetimedb.exportGroup({{ default: {binding} }});"
        );
    } else if accessor != binding {
        writeln!(out, "export {{ {binding} as {accessor} }};");
    }
}

fn local_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap()
}

fn write_row_fields(module: &ModuleDef, out: &mut Indenter, table: &TableDef) {
    let product = module.typespace_for_generate()[table.product_type_ref]
        .as_product()
        .unwrap();
    out.indent(1);
    for (i, (_, ty)) in product.elements.iter().enumerate() {
        let column = &table.columns[i];
        let name = column.accessor_name.deref().to_case(Case::Camel);
        let getter = type_contains_ref(ty);
        if getter {
            write!(out, "get {name}() {{ return ");
        } else {
            write!(out, "{name}: ");
        }
        write_type_builder_with_names(module, out, ty, "t", "").unwrap();
        if table.primary_key.is_some_and(|pk| pk.idx() == i) {
            write!(out, ".primaryKey()");
        }
        if table.sequences.values().any(|s| s.column.idx() == i) {
            write!(out, ".autoInc()");
        }
        if column.name.deref() != name {
            write!(out, ".name({})", ts_string_literal(&column.name));
        }
        if let Some(value) = &column.default_value {
            write!(out, ".default(");
            if let Some(literal) = default_literal(value) {
                write!(out, "{literal}");
            } else {
                let bytes = spacetimedb_lib::sats::bsatn::to_vec(value).unwrap();
                write_type_builder_with_names(module, out, ty, "t", "").unwrap();
                write!(out, ".deserialize(new __BinaryReader(new Uint8Array({bytes:?})))");
            }
            write!(out, ")");
        }
        if getter {
            writeln!(out, "; }},");
        } else {
            writeln!(out, ",");
        }
    }
    out.dedent(1);
}

// Keep common defaults readable; arbitrary compound values retain their exact BSATN representation.
fn default_literal(value: &spacetimedb_lib::AlgebraicValue) -> Option<String> {
    use spacetimedb_lib::AlgebraicValue::*;
    Some(match value {
        Bool(v) => v.to_string(),
        I8(v) => v.to_string(),
        U8(v) => v.to_string(),
        I16(v) => v.to_string(),
        U16(v) => v.to_string(),
        I32(v) => v.to_string(),
        U32(v) => v.to_string(),
        I64(v) => format!("{v}n"),
        U64(v) => format!("{v}n"),
        I128(v) => format!("{}n", { v.0 }),
        U128(v) => format!("{}n", { v.0 }),
        I256(v) => format!("{v}n"),
        U256(v) => format!("{v}n"),
        String(v) => ts_string_literal(v),
        _ => return None,
    })
}

fn table_options(out: &mut Indenter, table: &TableDef) {
    write!(
        out,
        "{{ name: {}, public: {}",
        ts_string_literal(&table.name),
        table.table_access == TableAccess::Public
    );
    if table.is_event {
        write!(out, ", event: true");
    }
    let indexes: Vec<_> = iter_indexes(table).filter(|i| !i.generated()).collect();
    if !indexes.is_empty() {
        writeln!(out, ", indexes: [");
        out.indent(1);
        for index in indexes {
            let algorithm = match index.algorithm {
                IndexAlgorithm::BTree(_) => "btree",
                IndexAlgorithm::Hash(_) => "hash",
                IndexAlgorithm::Direct(_) => "direct",
                _ => panic!("unsupported index algorithm in TypeScript declarations"),
            };
            let columns: Vec<_> = index
                .algorithm
                .columns()
                .iter()
                .map(|id| ts_string_literal(&table.columns[id.idx()].accessor_name.deref().to_case(Case::Camel)))
                .collect();
            write!(
                out,
                "{{ accessor: {}, name: {}, algorithm: '{algorithm}', ",
                ts_string_literal(index.accessor_name.as_deref().unwrap_or(&index.name)),
                ts_string_literal(&index.name)
            );
            if algorithm == "direct" {
                writeln!(out, "column: {} }},", columns[0]);
            } else {
                writeln!(out, "columns: [{}] }},", columns.join(", "));
            }
        }
        out.dedent(1);
        write!(out, "]");
    }
    let constraints: Vec<_> = iter_constraints(table).collect();
    if !constraints.is_empty() {
        writeln!(out, ", constraints: [");
        out.indent(1);
        for constraint in constraints {
            let columns: Vec<_> = constraint
                .data
                .unique_columns()
                .into_iter()
                .flat_map(|c| c.iter())
                .map(|id| ts_string_literal(&table.columns[id.idx()].accessor_name.deref().to_case(Case::Camel)))
                .collect();
            writeln!(
                out,
                "{{ name: {}, constraint: 'unique', columns: [{}] }},",
                ts_string_literal(&constraint.name),
                columns.join(", ")
            );
        }
        out.dedent(1);
        write!(out, "]");
    }
    write!(out, " }}");
}

fn entry_point(module: &ModuleDef, options: &CodegenOptions, dir: &str) -> OutputFile {
    let mut header = CodeIndenter::new(String::new(), INDENT);
    print_auto_generated_file_comment(&mut header);
    crate::util::print_auto_generated_version_comment(&mut header);
    let mut code = header.into_inner();
    writeln!(code, "import * as declarations from './declarations';").unwrap();
    writeln!(code, "import {{ createClient, type EventContextInterface, type ReducerEventContextInterface, type SubscriptionEventContextInterface, type ErrorContextInterface, type SubscriptionHandleImpl, type RemoteModuleOf }} from 'spacetimedb';").unwrap();
    write!(
        code,
        "const client = createClient(declarations, {{ cliVersion: {}, tableAliases: {{",
        ts_string_literal(spacetimedb_lib_version())
    )
    .unwrap();
    for (_, accessor, _) in iter_table_names_and_types(module, options.visibility) {
        let camel = accessor.deref().to_case(Case::Camel);
        if accessor.deref() != camel {
            write!(code, "{}: {},", ts_string_literal(accessor), ts_string_literal(&camel)).unwrap();
        }
    }
    writeln!(code, "}} }});").unwrap();
    writeln!(code, "export const {{ DbConnection, DbConnectionBuilder, SubscriptionBuilder, tables, reducers, procedures }} = client;").unwrap();
    writeln!(code, "export type DbConnection = InstanceType<typeof DbConnection>;\nexport type DbConnectionBuilder = InstanceType<typeof DbConnectionBuilder>;\nexport type SubscriptionBuilder = InstanceType<typeof SubscriptionBuilder>;").unwrap();
    writeln!(code, "type RemoteModule = RemoteModuleOf<DbConnection>;\nexport type DbView = DbConnection['db'];\nexport type Tables = typeof tables;").unwrap();
    for (name, interface) in [
        ("EventContext", "EventContextInterface"),
        ("ReducerEventContext", "ReducerEventContextInterface"),
        ("SubscriptionEventContext", "SubscriptionEventContextInterface"),
        ("ErrorContext", "ErrorContextInterface"),
    ] {
        writeln!(code, "export type {name} = {interface}<RemoteModule>;").unwrap();
    }
    writeln!(
        code,
        "export type SubscriptionHandle = SubscriptionHandleImpl<RemoteModule>;"
    )
    .unwrap();
    // Preserve the existing reducer/procedure type namespaces.
    writeln!(
        code,
        "export type * as Reducers from './types/reducers';\nexport type * as Procedures from './types/procedures';"
    )
    .unwrap();
    OutputFile {
        filename: format!("{dir}index.ts"),
        code,
    }
}
