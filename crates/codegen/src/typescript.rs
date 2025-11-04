use crate::util::{
    is_reducer_invokable, iter_indexes, iter_reducers, iter_tables, iter_types, iter_unique_cols, print_auto_generated_version_comment
};
use crate::{OutputFile};

use super::util::{collect_case, print_auto_generated_file_comment, type_ref_name};

use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{BTreeAlgorithm, IndexAlgorithm, ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse, ProductTypeDef};

use super::code_indenter::{CodeIndenter, Indenter};
use super::Lang;
use spacetimedb_lib::version::spacetimedb_lib_version;

type Imports = BTreeSet<AlgebraicTypeRef>;

const INDENT: &str = "  ";

pub struct TypeScript;

impl Lang for TypeScript {
    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile> {
        let type_name = collect_case(Case::Pascal, typ.name.name_segments());

        let define_type_for_product = |product: &ProductTypeDef| {
            let mut output = CodeIndenter::new(String::new(), INDENT);
            let out = &mut output;

            print_file_header(out, false);
            gen_and_print_imports(module, out, &product.elements, &[typ.ty], None);
            writeln!(out);
            define_body_for_product(module, out, &type_name, &product.elements);
            out.newline();
            OutputFile {
                filename: type_module_name(&typ.name) + ".ts",
                code: output.into_inner(),
            }
        };

        let define_variants_for_sum = |variants: &[(Identifier, AlgebraicTypeUse)]| {
            let mut output = CodeIndenter::new(String::new(), INDENT);
            let out = &mut output;

            print_file_header(out, false);
            // Note that the current type is not included in dont_import below.
            gen_and_print_imports(module, out, variants, &[], Some("Type"));
            writeln!(out);
            write_variant_types(module, out, variants);
            out.newline();
            OutputFile {
                filename: variants_module_name(&typ.name) + ".ts",
                code: output.into_inner(),
            }
        };

        let define_type_for_sum = |variants: &[(Identifier, AlgebraicTypeUse)]| {
            let mut output = CodeIndenter::new(String::new(), INDENT);
            let out = &mut output;

            print_file_header(out, false);
            gen_and_print_imports(module, out, variants, &[typ.ty], None);
            writeln!(
                out,
                "import * as {}Variants from './{}'",
                type_name,
                variants_module_name(&typ.name)
            );
            writeln!(out);
            // For the purpose of bootstrapping AlgebraicType, if the name of the type
            // is `AlgebraicType`, we need to use an alias.
            define_body_for_sum(module, out, &type_name, variants);
            out.newline();
            OutputFile {
                filename: type_module_name(&typ.name) + ".ts",
                code: output.into_inner(),
            }
        };

        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                vec![define_type_for_product(product)]
            }
            AlgebraicTypeDef::Sum(sum) => {
                vec![
                    define_variants_for_sum(&sum.variants),
                    define_type_for_sum(&sum.variants),
                ]
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                let variants = plain_enum
                    .variants
                    .iter()
                    .cloned()
                    .map(|var| (var, AlgebraicTypeUse::Unit))
                    .collect::<Vec<_>>();
                vec![define_variants_for_sum(&variants), define_type_for_sum(&variants)]
            }
        }
    }


    /// e.g.
    /// ```ts
    /// table({
    ///   name: 'player',
    ///   indexes: [
    ///     { name: 'this_is_an_index', algorithm: "btree", columns: [ "ownerId" ] } 
    ///   ],
    /// }, t.row({
    ///   id: t.u32().primaryKey(),
    ///   ownerId: t.string(),
    ///   name: t.string().unique(),
    ///   location: pointType,
    /// }))
    /// ```
    fn generate_table_file(&self, module: &ModuleDef, table: &TableDef) -> OutputFile {
        let schema = TableSchema::from_module_def(module, table, (), 0.into())
            .validated()
            .expect("Failed to generate table due to validation errors");

        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, false);

        let type_ref = table.product_type_ref;
        let row_type = type_ref_name(module, type_ref);
        let row_type_module = type_ref_module_name(module, type_ref);

        writeln!(out, "import {{ {row_type} }} from \"./{row_type_module}\";");

        let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();

        // Import the types of all fields.
        // We only need to import fields which have indices or unique constraints,
        // but it's easier to just import all of 'em, since we have `// @ts-nocheck` anyway.
        gen_and_print_imports(
            module,
            out,
            &product_def.elements,
            &[], // No need to skip any imports; we're not defining a type, so there's no chance of circular imports.
            None,
        );

        writeln!(out);

        writeln!(out, "export default table({{");
        out.indent(1);
        writeln!(
            out,
            "name: '{}',",
            table.name.deref()
        );
        writeln!(out, "indexes: [");
        out.indent(1);
        for index_def in iter_indexes(table) {
            if !index_def.generated() {
                // Skip system-defined indexes
                continue;
            }
            match &index_def.algorithm {
                IndexAlgorithm::BTree(BTreeAlgorithm { columns }) => {
                    let get_name_and_type = |col_pos: ColId| {
                        let (field_name, field_type) = &product_def.elements[col_pos.idx()];
                        let name_camel = field_name.deref().to_case(Case::Camel);
                        (name_camel, field_type)
                    };
                    writeln!(out, "{{ name: '{}', algorithm: 'btree', columns: [", index_def.name);
                    out.indent(1);
                    for col_id in columns.iter() {
                        writeln!(out, "'{}',", get_name_and_type(col_id).0);
                    }
                    out.dedent(1);
                    writeln!(out, "] }},");
                }
                IndexAlgorithm::Direct(_) => {
                    // Direct indexes are not implemented yet.
                    continue;
                }
                _ => todo!(),
            };
        }
        out.dedent(1);
        writeln!(out, "}}, {{");
        out.indent(1);
        for (field_ident, field_ty) in &product_def.elements {
            let field_name = field_ident.deref().to_case(Case::Camel);
            write!(out, "{field_name}: ");
            write_type(module, out, field_ty, None, None).unwrap();

            let mut annotations = Vec::new();
            if schema.pk().map(|pk| *field_ident == Identifier::new(pk.col_name.clone()).unwrap()).unwrap_or(false) {
                annotations.push("primaryKey()");
            } else {
                for (unique_field_ident, _) in
                    iter_unique_cols(module.typespace_for_generate(), &schema, product_def)
                {
                    if field_ident == unique_field_ident {
                        annotations.push("unique()");
                    }
                }
            }
        }
        writeln!(out, "}});");
        out.dedent(1);
        OutputFile {
            filename: table_module_name(&table.name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, false);

        out.newline();

        gen_and_print_imports(
            module,
            out,
            &reducer.params_for_generate.elements,
            // No need to skip any imports; we're not emitting a type that other modules can import.
            &[],
            None,
        );

        let args_type = reducer_args_type_name(&reducer.name);

        define_body_for_product(module, out, &args_type, &reducer.params_for_generate.elements);

        OutputFile {
            filename: reducer_module_name(&reducer.name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef) -> Vec<OutputFile> {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, true);

        out.newline();

        // const tablesSchema = schema(
//   table({ name: 'player', }, t.row({
//     ownerId: t.string(),
//     name: t.string(),
//     location: pointType,
//   })),
//   table({ name: 'unindexed_player', }, t.row({
//     ownerId: t.string(),
//     name: t.string(),
//     location: pointType,
//   })),
//   table({ name: 'user', primaryKey: 'identity', }, t.row({
//     identity: t.string(),
//     name: t.string(),
//   })),
// );

// const reducersSchema = reducers(
//   reducerSchema('create_player', {
//     name: t.string(),
//     location: pointType,
//   }),
//   reducerSchema('foo_bar', {
//     name: t.string(),
//     location: pointType,
//   }),
// );

// const REMOTE_MODULE = {
//   versionInfo: {
//     cliVersion: '1.6.0' as const,
//   },
//   tables: tablesSchema.schemaType.tables,
//   reducers: reducersSchema.reducersType.reducers,
// } satisfies RemoteModule<
//   typeof tablesSchema.schemaType,
//   typeof reducersSchema.reducersType
// >;

// export type EventContext = __EventContextInterface<
//   typeof REMOTE_MODULE
// >;

// export type ReducerEventContext = __ReducerEventContextInterface<
//   typeof REMOTE_MODULE
// >;

// export type SubscriptionEventContext = __SubscriptionEventContextInterface<
//   typeof REMOTE_MODULE
// >;

// export type ErrorContext = __ErrorContextInterface<
//   typeof REMOTE_MODULE
// >;

// export class SubscriptionBuilder extends __SubscriptionBuilderImpl<
//   typeof REMOTE_MODULE
// > {}

// export class DbConnectionBuilder extends __DbConnectionBuilder<
//   typeof REMOTE_MODULE,
//   DbConnection
// > {};

// export class DbConnection extends __DbConnectionImpl<typeof REMOTE_MODULE> {
//   static builder = (): DbConnectionBuilder => {
//     return new DbConnectionBuilder(REMOTE_MODULE, (config: DbConnectionConfig<typeof REMOTE_MODULE>) => new DbConnection(config));
//   };
//   subscriptionBuilder = (): SubscriptionBuilder => {
//     return new SubscriptionBuilder(this);
//   };

        writeln!(out, "// Import and reexport all reducer arg types");
        for reducer in iter_reducers(module) {
            let reducer_name = &reducer.name;
            let reducer_module_name = reducer_module_name(reducer_name) + ".ts";
            let args_type = reducer_args_type_name(&reducer.name);
            writeln!(out, "import {args_type} from \"./{reducer_module_name}\";");
            writeln!(out, "export {{ {args_type} }};");
        }

        writeln!(out);
        writeln!(out, "// Import and reexport all table handle types");
        for table in iter_tables(module) {
            let table_name = &table.name;
            let table_module_name = table_module_name(table_name) + ".ts";
            let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
            writeln!(out, "import {table_name_pascalcase} from \"./{table_module_name}\";");
            writeln!(out, "export {{ {table_name_pascalcase} }};");
        }

        writeln!(out);
        writeln!(out, "// Import and reexport all types");
        for ty in iter_types(module) {
            let type_name = collect_case(Case::Pascal, ty.name.name_segments());
            let type_module_name = type_module_name(&ty.name) + ".ts";
            writeln!(out, "import {type_name} from \"./{type_module_name}\";");
            writeln!(out, "export {{ {type_name} }};");
        }

        out.newline();
        
        writeln!(out);
        writeln!(out, "const tablesSchema = schema(");
        out.indent(1);
        for table in iter_tables(module) {
            let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
            writeln!(out, "{},", table_name_pascalcase);
        }
        out.dedent(1);
        writeln!(out, ");");

        writeln!(out);

        writeln!(out, "const reducersSchema = reducers(");
        out.indent(1);
        for reducer in iter_reducers(module) {
            if !is_reducer_invokable(reducer) {
                // Skip system-defined reducers
                continue;
            }
            let reducer_name = &reducer.name;
            let args_type = reducer_args_type_name(&reducer.name);
            writeln!(out, "reducerSchema(\"{}\", {}),", reducer_name, args_type);
        }
        out.dedent(1);
        writeln!(out, ");");

        writeln!(out);

        writeln!(out, "const REMOTE_MODULE = {{");
        out.indent(1);
        writeln!(out, "versionInfo: {{");
        out.indent(1);
        writeln!(out, "cliVersion: \"{}\" as const,", spacetimedb_lib_version());
        out.dedent(1);
        writeln!(out, "}},");
        writeln!(out, "tables: tablesSchema.schemaType.tables,");
        writeln!(out, "reducers: reducersSchema.reducersType.reducers,");
        out.dedent(1);
        writeln!(out, "}} satisfies __RemoteModule<");
        out.indent(1);
        writeln!(out, "typeof tablesSchema.schemaType,");
        writeln!(out, "typeof reducersSchema.reducersType");
        out.dedent(1);
        writeln!(out, ">;");
        out.dedent(1);

        out.newline();

        // Write type aliases for EventContext, ReducerEventContext, SubscriptionEventContext, ErrorContext
        writeln!(
            out,
            "export type EventContext = __EventContextInterface<typeof REMOTE_MODULE>;"
        );
        writeln!(
            out,
            "export type ReducerEventContext = __ReducerEventContextInterface<typeof REMOTE_MODULE>;"
        );
        writeln!(
            out,
            "export type SubscriptionEventContext = __SubscriptionEventContextInterface<typeof REMOTE_MODULE>;"
        );
        writeln!(
            out,
            "export type ErrorContext = __ErrorContextInterface<typeof REMOTE_MODULE>;"
        );

        writeln!(out);

        writeln!(out, "export class SubscriptionBuilder extends __SubscriptionBuilderImpl<");
        out.indent(1);
        writeln!(out, "typeof REMOTE_MODULE");
        out.dedent(1);
        writeln!(out, "> {{}}");

        writeln!(out);
        writeln!(out, "export class DbConnectionBuilder extends __DbConnectionBuilder<");
        out.indent(1);
        writeln!(out, "typeof REMOTE_MODULE,");
        writeln!(out, "DbConnection");
        out.dedent(1);
        writeln!(out, "> {{}}");

        writeln!(out);
        writeln!(
            out,
            "export class DbConnection extends __DbConnectionImpl<typeof REMOTE_MODULE> {{"
        );
        out.indent(1);
        writeln!(
            out,
            "static builder = (): DbConnectionBuilder => {{"
        );
        out.indent(1);
        writeln!(
            out,
            "return new DbConnectionBuilder(REMOTE_MODULE, (config: DbConnectionConfig<typeof REMOTE_MODULE>) => new DbConnection(config));"
        );
        out.dedent(1);
        writeln!(out, "}};");
        writeln!(
            out,
            "subscriptionBuilder = (): SubscriptionBuilder => {{"
        );
        out.indent(1);
        writeln!(out, "return new SubscriptionBuilder(this);");
        out.dedent(1);
        writeln!(out, "}};");
        out.dedent(1);
        writeln!(out, "}}");
        out.newline();

        vec![OutputFile {
            filename: "index.ts".to_string(),
            code: output.into_inner(),
        }]
    }
}

fn print_spacetimedb_imports(out: &mut Indenter) {
    // All library imports are prefixed with `__` to avoid
    // clashing with the names of user generated types.
    let mut types = [
        "type AlgebraicType as __AlgebraicTypeType",
        "AlgebraicType as __AlgebraicTypeValue",
        "type AlgebraicTypeVariants as __AlgebraicTypeVariants",
        "Identity as __Identity",
        "ClientCache as __ClientCache",
        "ClientTable as __ClientTable",
        "ConnectionId as __ConnectionId",
        "Timestamp as __Timestamp",
        "TimeDuration as __TimeDuration",
        "DbConnectionBuilder as __DbConnectionBuilder",
        "BinaryWriter as __BinaryWriter",
        "type CallReducerFlags as __CallReducerFlags",
        "type EventContextInterface as __EventContextInterface",
        "type ReducerEventContextInterface as __ReducerEventContextInterface",
        "type RemoteModule as __RemoteModule",
        "type SubscriptionEventContextInterface as __SubscriptionEventContextInterface",
        "type ErrorContextInterface as __ErrorContextInterface",
        "SubscriptionBuilderImpl as __SubscriptionBuilderImpl",
        "BinaryReader as __BinaryReader",
        "DbConnectionImpl as __DbConnectionImpl",
        "type Event as __Event",
        "deepEqual as __deepEqual",
        "schema as __schema",
        "table as __table",
        "reducers as __reducers",
        "reducerSchema as __reducerSchema",
        "DbConnectionConfig as __DbConnectionConfig",
        "RemoteModule as __RemoteModule",
        "t as __t",
    ];
    types.sort();
    writeln!(out, "import {{");
    out.indent(1);
    for ty in &types {
        writeln!(out, "{ty},");
    }
    out.dedent(1);
    writeln!(out, "}} from \"spacetimedb\";");
}

fn print_file_header(output: &mut Indenter, include_version: bool) {
    print_auto_generated_file_comment(output);
    if include_version {
        print_auto_generated_version_comment(output);
    }
    print_lint_suppression(output);
    print_spacetimedb_imports(output);
}

fn print_lint_suppression(output: &mut Indenter) {
    writeln!(output, "/* eslint-disable */");
    writeln!(output, "/* tslint:disable */");
}

/// e.g.
/// ```ts
/// export default __t.object('Point', {
///   x: __t.f32(),
///   y: __t.f32(),
///   fooBar: __t.string(),
/// });
/// ```
fn define_body_for_product(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    elements: &[(Identifier, AlgebraicTypeUse)],
) {

    write!(out, "export default __t.object(\"{name}\", {{");
    if elements.is_empty() {
        writeln!(out, "}};");
    } else {
        writeln!(out);
        out.with_indent(|out| write_object_type_builder_fields(module, out, elements, true).unwrap());
        writeln!(out, "}};");
    }
    out.newline();
}

/// e.g.
/// ```ts
///   x: __t.f32(),
///   y: __t.f32(),
///   fooBar: __t.string(),
/// ```
fn write_object_type_builder_fields(
    module: &ModuleDef,
    out: &mut impl Write,
    elements: &[(Identifier, AlgebraicTypeUse)],
    convert_case: bool,
) -> anyhow::Result<()> {
    for (ident, ty) in elements {
        let name = if convert_case {
            ident.deref().to_case(Case::Camel)
        } else {
            ident.deref().into()
        };

        write!(out, "{name}: ")?;
        write_type_builder(module, out, ty)?;
        writeln!(out, ",")?;
    }

    Ok(())
}

/// e.g. `__t.option(__t.i32())`, `__t.string()`
fn write_type_builder<W: Write>(module: &ModuleDef, out: &mut W, ty: &AlgebraicTypeUse) -> fmt::Result {
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "__t.unit()")?,
        AlgebraicTypeUse::Never => write!(out, "__t.never()")?,
        AlgebraicTypeUse::Identity => write!(out, "__t.identity()")?,
        AlgebraicTypeUse::ConnectionId => write!(out, "__t.connectionId()")?,
        AlgebraicTypeUse::Timestamp => write!(out, "__t.timestamp()")?,
        AlgebraicTypeUse::TimeDuration => write!(out, "__t.timeDuration()")?,
        AlgebraicTypeUse::ScheduleAt => write!(out, "__t.scheduleAt()")?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write!(out, "__t.option(")?;
            write_type_builder(module, out, inner_ty)?;
            write!(out, ")")?;
        },
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::Bool => write!(out, "__t.bool()")?,
            PrimitiveType::I8 => write!(out, "__t.i8()")?,
            PrimitiveType::U8 => write!(out, "__t.u8()")?,
            PrimitiveType::I16 => write!(out, "__t.i16()")?,
            PrimitiveType::U16 => write!(out, "__t.u16()")?,
            PrimitiveType::I32 => write!(out, "__t.i32()")?,
            PrimitiveType::U32 => write!(out, "__t.u32()")?,
            PrimitiveType::I64 => write!(out, "__t.i64()")?,
            PrimitiveType::U64 => write!(out, "__t.u64()")?,
            PrimitiveType::I128 => write!(out, "__t.i128()")?,
            PrimitiveType::U128 => write!(out, "__t.u128()")?,
            PrimitiveType::I256 => write!(out, "__t.i256()")?,
            PrimitiveType::U256 => write!(out, "__t.u256()")?,
            PrimitiveType::F32 => write!(out, "__t.f32()")?,
            PrimitiveType::F64 => write!(out, "__t.f64()")?,
        },
        AlgebraicTypeUse::String => write!(out, "__t.string()")?,
        AlgebraicTypeUse::Array(elem_ty) => {
            write!(out, "__t.array(")?;
            write_type_builder(module, out, elem_ty)?;
            write!(out, ")")?;
        },
        AlgebraicTypeUse::Ref(r) => {
            write!(out, "{}", type_ref_name(module, *r))?;
        },
    }
    Ok(())
}

fn write_sum_variant_type(module: &ModuleDef, out: &mut Indenter, ident: &Identifier, ty: &AlgebraicTypeUse) {
    let name = ident.deref().to_case(Case::Pascal);
    write!(out, "export type {name} = ");

    // If the contained type is the unit type, i.e. this variant has no members,
    // write only the tag.
    // ```
    // { tag: "Foo" }
    // ```
    write!(out, "{{ ");
    write!(out, "tag: \"{name}\"");

    // If the contained type is not the unit type, write the tag and the value.
    // ```
    // { tag: "Bar", value: BarType }
    // { tag: "Bar", value: number }
    // { tag: "Bar", value: string }
    // ```
    // Note you could alternatively do:
    // ```
    // { tag: "Bar" } & BarType
    // ```
    // for non-primitive types but that doesn't extend to primitives.
    // Another alternative would be to name the value field the same as the tag field, but lowercased
    // ```
    // { tag: "Bar", bar: BarType }
    // { tag: "Bar", bar: number }
    // { tag: "Bar", bar: string }
    // ```
    // but this is a departure from our previous convention and is not much different.
    if !matches!(ty, AlgebraicTypeUse::Unit) {
        write!(out, ", value: ");
        write_type(module, out, ty, None, Some("Type")).unwrap();
    }

    writeln!(out, " }};");
}

fn write_variant_types(module: &ModuleDef, out: &mut Indenter, variants: &[(Identifier, AlgebraicTypeUse)]) {
    // Write all the variant types.
    for (ident, ty) in variants {
        write_sum_variant_type(module, out, ident, ty);
    }
}

/// e.g.
/// ```ts
/// // The tagged union or sum type for the algebraic type `Option`.
/// export default __t.enum("Option", {
///   none: __t.unit(),
///   some: { value: __t.i32() },
/// });
/// ```
fn define_body_for_sum(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    variants: &[(Identifier, AlgebraicTypeUse)],
) {
    writeln!(out, "// The tagged union or sum type for the algebraic type `{name}`.");
    write!(out, "export default __t.enum(\"{name}\", {{");
    out.with_indent(|out| write_object_type_builder_fields(module, out, variants, true).unwrap());
    writeln!(out, "}});");
    out.newline();
}

fn type_ref_module_name(module: &ModuleDef, type_ref: AlgebraicTypeRef) -> String {
    let (name, _) = module.type_def_from_ref(type_ref).unwrap();
    type_module_name(name)
}

fn type_module_name(type_name: &ScopedTypeName) -> String {
    collect_case(Case::Snake, type_name.name_segments()) + "_type"
}

fn variants_module_name(type_name: &ScopedTypeName) -> String {
    collect_case(Case::Snake, type_name.name_segments()) + "_variants"
}

fn table_module_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake) + "_table"
}

fn table_method_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Camel)
}

fn reducer_args_type_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal)
}

fn reducer_variant_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal)
}

fn reducer_module_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Snake) + "_reducer"
}

fn reducer_function_name(reducer: &ReducerDef) -> String {
    reducer.name.deref().to_case(Case::Camel)
}

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
        | AlgebraicTypeUse::Primitive(_)
        | AlgebraicTypeUse::Array(_)
        | AlgebraicTypeUse::Ref(_) // We use the type name for these.
        | AlgebraicTypeUse::String => {
            false
        }
        AlgebraicTypeUse::ScheduleAt | AlgebraicTypeUse::Option(_) => {
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
        AlgebraicTypeUse::Identity => write!(out, "__Identity")?,
        AlgebraicTypeUse::ConnectionId => write!(out, "__ConnectionId")?,
        AlgebraicTypeUse::Timestamp => write!(out, "__Timestamp")?,
        AlgebraicTypeUse::TimeDuration => write!(out, "__TimeDuration")?,
        AlgebraicTypeUse::ScheduleAt => write!(
            out,
            "{{ tag: \"Interval\", value: __TimeDuration }} | {{ tag: \"Time\", value: __Timestamp }}"
        )?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write_type(module, out, inner_ty, ref_prefix, ref_suffix)?;
            write!(out, " | undefined")?;
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
            if let Some(prefix) = ref_prefix {
                write!(out, "{prefix}")?;
            }
            write!(out, "{}", type_ref_name(module, *r))?;
            if let Some(suffix) = ref_suffix {
                write!(out, "{suffix}")?;
            }
        }
    }
    Ok(())
}

/// Print imports for each of the `imports`.
fn print_imports(module: &ModuleDef, out: &mut Indenter, imports: Imports, suffix: Option<&str>) {
    for typeref in imports {
        let module_name = type_ref_module_name(module, typeref);
        let type_name = type_ref_name(module, typeref);
        if let Some(suffix) = suffix {
            writeln!(
                out,
                "import {{ {type_name} as {type_name}{suffix} }} from \"./{module_name}\";"
            );
            writeln!(out, "// Mark import as potentially unused");
            writeln!(out, "declare type __keep_{type_name}{suffix} = {type_name}{suffix};");
        } else {
            writeln!(out, "import {{ {type_name} }} from \"./{module_name}\";");
            writeln!(out, "// Mark import as potentially unused");
            writeln!(out, "declare type __keep_{type_name} = {type_name};");
        }
    }
}

/// Use `search_function` on `roots` to detect required imports, then print them with `print_imports`.
///
/// `this_file` is passed and excluded for the case of recursive types:
/// without it, the definition for a type like `struct Foo { foos: Vec<Foo> }`
/// would attempt to include `import { Foo } from "./foo"`.
fn gen_and_print_imports(
    module: &ModuleDef,
    out: &mut Indenter,
    roots: &[(Identifier, AlgebraicTypeUse)],
    dont_import: &[AlgebraicTypeRef],
    suffix: Option<&str>,
) {
    let mut imports = BTreeSet::new();

    for (_, ty) in roots {
        ty.for_each_ref(|r| {
            imports.insert(r);
        });
    }
    for skip in dont_import {
        imports.remove(skip);
    }
    let len = imports.len();

    print_imports(module, out, imports, suffix);

    if len > 0 {
        out.newline();
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
