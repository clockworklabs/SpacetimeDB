use crate::util::{
    is_reducer_invokable, iter_constraints, iter_indexes, iter_procedures, iter_reducers, iter_table_names_and_types,
    iter_tables, iter_types, iter_views, print_auto_generated_version_comment,
};
use crate::OutputFile;

use super::util::{collect_case, print_auto_generated_file_comment, type_ref_name};

use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::iter;
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{ConstraintDef, IndexDef, ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::TableSchema;
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

            print_file_header(out, false, true);
            gen_and_print_imports(module, out, product.element_types(), &[typ.ty], None);
            writeln!(out);
            define_body_for_product(module, out, &type_name, &product.elements);
            out.newline();
            OutputFile {
                filename: type_module_name(&typ.name) + ".ts",
                code: output.into_inner(),
            }
        };

        let define_type_for_sum = |variants: &[(Identifier, AlgebraicTypeUse)]| {
            let mut output = CodeIndenter::new(String::new(), INDENT);
            let out = &mut output;

            print_file_header(out, false, true);
            gen_and_print_imports(module, out, variants.iter().map(|(_, ty)| ty), &[typ.ty], None);
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
                vec![define_type_for_sum(&sum.variants)]
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                let variants = plain_enum
                    .variants
                    .iter()
                    .cloned()
                    .map(|var| (var, AlgebraicTypeUse::Unit))
                    .collect::<Vec<_>>();
                vec![define_type_for_sum(&variants)]
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
    fn generate_table_file_from_schema(
        &self,
        module: &ModuleDef,
        table: &TableDef,
        _schema: TableSchema,
    ) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, false, true);

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
            None,
        );

        writeln!(out);

        writeln!(out, "export default __t.row({{");
        out.indent(1);
        write_object_type_builder_fields(module, out, &product_def.elements, table.primary_key, true, true).unwrap();
        out.dedent(1);
        writeln!(out, "}});");
        OutputFile {
            filename: table_module_name(&table.name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, false, true);

        out.newline();

        gen_and_print_imports(
            module,
            out,
            reducer.params_for_generate.element_types(),
            // No need to skip any imports; we're not emitting a type that other modules can import.
            &[],
            None,
        );

        define_body_for_reducer(module, out, &reducer.params_for_generate.elements);

        OutputFile {
            filename: reducer_module_name(&reducer.name) + ".ts",
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

        print_file_header(out, false, true);

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
            None,
        );

        writeln!(out, "export const params = {{");
        out.with_indent(|out| {
            write_object_type_builder_fields(module, out, &procedure.params_for_generate.elements, None, true, false)
                .unwrap()
        });
        writeln!(out, "}};");

        write!(out, "export const returnType = ");
        write_type_builder(module, out, &procedure.return_type_for_generate).unwrap();

        OutputFile {
            filename: procedure_module_name(&procedure.name) + ".ts",
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef) -> Vec<OutputFile> {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, true, false);

        writeln!(out);
        writeln!(out, "// Import and reexport all reducer arg types");
        for reducer in iter_reducers(module) {
            let reducer_name = &reducer.name;
            let reducer_module_name = reducer_module_name(reducer_name);
            let args_type = reducer_args_type_name(&reducer.name);
            writeln!(out, "import {args_type} from \"./{reducer_module_name}\";");
            writeln!(out, "export {{ {args_type} }};");
        }

        writeln!(out);
        writeln!(out, "// Import and reexport all procedure arg types");
        for procedure in iter_procedures(module) {
            let procedure_name = &procedure.name;
            let procedure_module_name = procedure_module_name(procedure_name);
            let args_type = procedure_args_type_name(&procedure.name);
            writeln!(out, "import * as {args_type} from \"./{procedure_module_name}\";");
            writeln!(out, "export {{ {args_type} }};");
        }

        writeln!(out);
        writeln!(out, "// Import and reexport all table handle types");
        for (table_name, _) in iter_table_names_and_types(module) {
            let table_module_name = table_module_name(table_name);
            let table_name_pascalcase = table_name.deref().to_case(Case::Pascal);
            // TODO: This really shouldn't be necessary. We could also have `table()` accept
            // `__t.object(...)`s.
            writeln!(out, "import {table_name_pascalcase}Row from \"./{table_module_name}\";");
            writeln!(out, "export {{ {table_name_pascalcase}Row }};");
        }

        writeln!(out);
        writeln!(out, "// Import and reexport all types");
        for ty in iter_types(module) {
            let type_name = collect_case(Case::Pascal, ty.name.name_segments());
            let type_module_name = type_module_name(&ty.name);
            writeln!(out, "import {type_name} from \"./{type_module_name}\";");
            writeln!(out, "export {{ {type_name} }};");
        }

        writeln!(out);
        writeln!(out, "/** The schema information for all tables in this module. This is defined the same was as the tables would have been defined in the server. */");
        writeln!(out, "const tablesSchema = __schema(");
        out.indent(1);
        for table in iter_tables(module) {
            let type_ref = table.product_type_ref;
            let table_name_pascalcase = table.name.deref().to_case(Case::Pascal);
            writeln!(out, "__table({{");
            out.indent(1);
            write_table_opts(
                module,
                out,
                type_ref,
                &table.name,
                iter_indexes(table),
                iter_constraints(table),
            );
            out.dedent(1);
            writeln!(out, "}}, {}Row),", table_name_pascalcase);
        }
        for view in iter_views(module) {
            let type_ref = view.product_type_ref;
            let view_name_pascalcase = view.name.deref().to_case(Case::Pascal);
            writeln!(out, "__table({{");
            out.indent(1);
            write_table_opts(module, out, type_ref, &view.name, iter::empty(), iter::empty());
            out.dedent(1);
            writeln!(out, "}}, {}Row),", view_name_pascalcase);
        }
        out.dedent(1);
        writeln!(out, ");");

        writeln!(out);
        writeln!(out, "/** The schema information for all reducers in this module. This is defined the same way as the reducers would have been defined in the server, except the body of the reducer is omitted in code generation. */");
        writeln!(out, "const reducersSchema = __reducers(");
        out.indent(1);
        for reducer in iter_reducers(module) {
            if !is_reducer_invokable(reducer) {
                // Skip system-defined reducers
                continue;
            }
            let reducer_name = &reducer.name;
            let args_type = reducer_args_type_name(&reducer.name);
            writeln!(out, "__reducerSchema(\"{}\", {}),", reducer_name, args_type);
        }
        out.dedent(1);
        writeln!(out, ");");

        writeln!(out);
        writeln!(
            out,
            "/** The schema information for all procedures in this module. This is defined the same way as the procedures would have been defined in the server. */"
        );
        writeln!(out, "const proceduresSchema = __procedures(");
        out.indent(1);
        for procedure in iter_procedures(module) {
            let procedure_name = &procedure.name;
            let args_type = procedure_args_type_name(&procedure.name);
            writeln!(
                out,
                "__procedureSchema(\"{procedure_name}\", {args_type}.params, {args_type}.returnType),",
            );
        }
        out.dedent(1);
        writeln!(out, ");");

        writeln!(out);
        writeln!(
            out,
            "/** The remote SpacetimeDB module schema, both runtime and type information. */"
        );
        writeln!(out, "const REMOTE_MODULE = {{");
        out.indent(1);
        writeln!(out, "versionInfo: {{");
        out.indent(1);
        writeln!(out, "cliVersion: \"{}\" as const,", spacetimedb_lib_version());
        out.dedent(1);
        writeln!(out, "}},");
        writeln!(out, "tables: tablesSchema.schemaType.tables,");
        writeln!(out, "reducers: reducersSchema.reducersType.reducers,");
        writeln!(out, "...proceduresSchema,");
        out.dedent(1);
        writeln!(out, "}} satisfies __RemoteModule<");
        out.indent(1);
        writeln!(out, "typeof tablesSchema.schemaType,");
        writeln!(out, "typeof reducersSchema.reducersType,");
        writeln!(out, "typeof proceduresSchema");
        out.dedent(1);
        writeln!(out, ">;");
        out.dedent(1);

        writeln!(out);
        writeln!(out, "/** The tables available in this remote SpacetimeDB module. */");
        writeln!(
            out,
            "export const tables = __convertToAccessorMap(tablesSchema.schemaType.tables);"
        );
        writeln!(out);
        writeln!(out, "/** A typed query builder for this remote SpacetimeDB module. */");
        writeln!(
            out,
            "export const query: __QueryBuilder<typeof tablesSchema.schemaType> = __makeQueryBuilder(tablesSchema.schemaType);"
        );
        writeln!(out);
        writeln!(out, "/** The reducers available in this remote SpacetimeDB module. */");
        writeln!(
            out,
            "export const reducers = __convertToAccessorMap(reducersSchema.reducersType.reducers);"
        );

        // Write type aliases for EventContext, ReducerEventContext, SubscriptionEventContext, ErrorContext
        writeln!(out);
        writeln!(
            out,
            "/** The context type returned in callbacks for all possible events. */"
        );
        writeln!(
            out,
            "export type EventContext = __EventContextInterface<typeof REMOTE_MODULE>;"
        );

        writeln!(out, "/** The context type returned in callbacks for reducer events. */");
        writeln!(
            out,
            "export type ReducerEventContext = __ReducerEventContextInterface<typeof REMOTE_MODULE>;"
        );

        writeln!(
            out,
            "/** The context type returned in callbacks for subscription events. */"
        );
        writeln!(
            out,
            "export type SubscriptionEventContext = __SubscriptionEventContextInterface<typeof REMOTE_MODULE>;"
        );

        writeln!(out, "/** The context type returned in callbacks for error events. */");
        writeln!(
            out,
            "export type ErrorContext = __ErrorContextInterface<typeof REMOTE_MODULE>;"
        );

        writeln!(out, "/** The subscription handle type to manage active subscriptions created from a {{@link SubscriptionBuilder}}. */");
        writeln!(
            out,
            "export type SubscriptionHandle = __SubscriptionHandleImpl<typeof REMOTE_MODULE>;"
        );

        writeln!(out);
        writeln!(
            out,
            "/** Builder class to configure a new subscription to the remote SpacetimeDB instance. */"
        );
        writeln!(
            out,
            "export class SubscriptionBuilder extends __SubscriptionBuilderImpl<typeof REMOTE_MODULE> {{}}"
        );

        writeln!(out);
        writeln!(
            out,
            "/** Builder class to configure a new database connection to the remote SpacetimeDB instance. */"
        );
        writeln!(
            out,
            "export class DbConnectionBuilder extends __DbConnectionBuilder<DbConnection> {{}}"
        );

        writeln!(out);
        writeln!(out, "/** The typed database connection to manage connections to the remote SpacetimeDB instance. This class has type information specific to the generated module. */");
        writeln!(
            out,
            "export class DbConnection extends __DbConnectionImpl<typeof REMOTE_MODULE> {{"
        );
        out.indent(1);
        writeln!(out, "/** Creates a new {{@link DbConnectionBuilder}} to configure and connect to the remote SpacetimeDB instance. */");
        writeln!(out, "static builder = (): DbConnectionBuilder => {{");
        out.indent(1);
        writeln!(
            out,
            "return new DbConnectionBuilder(REMOTE_MODULE, (config: __DbConnectionConfig<typeof REMOTE_MODULE>) => new DbConnection(config));"
        );
        out.dedent(1);
        writeln!(out, "}};");

        writeln!(out);
        writeln!(out, "/** Creates a new {{@link SubscriptionBuilder}} to configure a subscription to the remote SpacetimeDB instance. */");
        writeln!(out, "override subscriptionBuilder = (): SubscriptionBuilder => {{");
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

fn print_index_imports(out: &mut Indenter) {
    // All library imports are prefixed with `__` to avoid
    // clashing with the names of user generated types.
    let mut types = [
        "TypeBuilder as __TypeBuilder",
        "type AlgebraicTypeType as __AlgebraicTypeType",
        "Uuid as __Uuid",
        "DbConnectionBuilder as __DbConnectionBuilder",
        "convertToAccessorMap as __convertToAccessorMap",
        "makeQueryBuilder as __makeQueryBuilder",
        "type QueryBuilder as __QueryBuilder",
        "type EventContextInterface as __EventContextInterface",
        "type ReducerEventContextInterface as __ReducerEventContextInterface",
        "type SubscriptionEventContextInterface as __SubscriptionEventContextInterface",
        "type SubscriptionHandleImpl as __SubscriptionHandleImpl",
        "type ErrorContextInterface as __ErrorContextInterface",
        "type RemoteModule as __RemoteModule",
        "SubscriptionBuilderImpl as __SubscriptionBuilderImpl",
        "DbConnectionImpl as __DbConnectionImpl",
        "type Event as __Event",
        "schema as __schema",
        "table as __table",
        "type Infer as __Infer",
        "reducers as __reducers",
        "reducerSchema as __reducerSchema",
        "procedures as __procedures",
        "procedureSchema as __procedureSchema",
        "type DbConnectionConfig as __DbConnectionConfig",
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

fn print_file_header(output: &mut Indenter, include_version: bool, type_builder_only: bool) {
    print_auto_generated_file_comment(output);
    if include_version {
        print_auto_generated_version_comment(output);
    }
    print_lint_suppression(output);
    if type_builder_only {
        print_type_builder_imports(output);
    } else {
        print_index_imports(output);
    }
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
        out.with_indent(|out| write_object_type_builder_fields(module, out, params, None, true, false).unwrap());
        writeln!(out, "}};");
    }
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
        writeln!(out, "}});");
    } else {
        writeln!(out);
        out.with_indent(|out| write_object_type_builder_fields(module, out, elements, None, true, false).unwrap());
        writeln!(out, "}});");
    }
    out.newline();
}

fn write_table_opts<'a>(
    module: &ModuleDef,
    out: &mut Indenter,
    type_ref: AlgebraicTypeRef,
    name: &Identifier,
    indexes: impl Iterator<Item = &'a IndexDef>,
    constraints: impl Iterator<Item = &'a ConstraintDef>,
) {
    let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();
    writeln!(out, "name: '{}',", name.deref());
    writeln!(out, "indexes: [");
    out.indent(1);
    for index_def in indexes {
        if index_def.generated() {
            // Skip system-defined indexes
            continue;
        }

        // We're generating code for the client,
        // and it does not care what the algorithm on the server is,
        // as it an use a btree in all cases.
        let columns = index_def.algorithm.columns();
        let get_name_and_type = |col_pos: ColId| {
            let (field_name, field_type) = &product_def.elements[col_pos.idx()];
            let name_camel = field_name.deref().to_case(Case::Camel);
            (name_camel, field_type)
        };
        // TODO(cloutiertyler):
        // The name users supply is actually the accessor name which will be used
        // in TypeScript to access the index. This will be used verbatim.
        // This is confusing because it is not the index name and there is
        // no actual way for the user to set the actual index name.
        // I think we should standardize: name and accessorName as the way to set
        // the name and accessor name of an index across all SDKs.
        if let Some(accessor_name) = &index_def.accessor_name {
            writeln!(out, "{{ name: '{}', algorithm: 'btree', columns: [", accessor_name);
        } else {
            writeln!(out, "{{ name: '{}', algorithm: 'btree', columns: [", index_def.name);
        }
        out.indent(1);
        for col_id in columns.iter() {
            writeln!(out, "'{}',", get_name_and_type(col_id).0);
        }
        out.dedent(1);
        writeln!(out, "] }},");
    }
    out.dedent(1);
    writeln!(out, "],");
    writeln!(out, "constraints: [");
    out.indent(1);
    // Unique constraints sorted by name for determinism
    for constraint in constraints {
        let columns: Vec<_> = constraint
            .data
            .unique_columns() // Option<&ColSet>
            .into_iter() // Iterator over 0 or 1 item (&ColSet)
            .flat_map(|cs| cs.iter()) // Iterator over the ColIds inside the set
            .map(|col_id| {
                let (field_name, _field_type) = &product_def.elements[col_id.idx()];
                let field_name = field_name.deref().to_case(Case::Camel);
                format!("'{}'", field_name)
            })
            .collect();

        writeln!(
            out,
            "{{ name: '{}', constraint: 'unique', columns: [{}] }},",
            constraint.name,
            columns.join(", ")
        );
    }
    out.dedent(1);
    writeln!(out, "],");
}

/// e.g.
/// ```ts
///   x: __t.f32().primaryKey(),
///   y: __t.f32(),
///   fooBar: __t.string(),
/// ```
fn write_object_type_builder_fields(
    module: &ModuleDef,
    out: &mut Indenter,
    elements: &[(Identifier, AlgebraicTypeUse)],
    primary_key: Option<ColId>,
    convert_case: bool,
    write_original_name: bool,
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
        let original_name = (write_original_name && convert_case && *name != **ident).then_some(&**ident);
        write_type_builder_field(module, out, &name, original_name, ty, is_primary_key)?;
    }

    Ok(())
}

fn write_type_builder_field(
    module: &ModuleDef,
    out: &mut Indenter,
    name: &str,
    original_name: Option<&str>,
    ty: &AlgebraicTypeUse,
    is_primary_key: bool,
) -> fmt::Result {
    // Do we need a getter? (Option/Array only if their inner is a Ref)
    let needs_getter = match ty {
        AlgebraicTypeUse::Ref(_) => true,
        AlgebraicTypeUse::Option(inner) | AlgebraicTypeUse::Array(inner) => {
            matches!(inner.as_ref(), AlgebraicTypeUse::Ref(_))
        }
        _ => false,
    };

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
    match ty {
        AlgebraicTypeUse::Unit => write!(out, "__t.unit()")?,
        AlgebraicTypeUse::Never => write!(out, "__t.never()")?,
        AlgebraicTypeUse::Identity => write!(out, "__t.identity()")?,
        AlgebraicTypeUse::ConnectionId => write!(out, "__t.connectionId()")?,
        AlgebraicTypeUse::Timestamp => write!(out, "__t.timestamp()")?,
        AlgebraicTypeUse::TimeDuration => write!(out, "__t.timeDuration()")?,
        AlgebraicTypeUse::ScheduleAt => write!(out, "__t.scheduleAt()")?,
        AlgebraicTypeUse::Uuid => write!(out, "__t.uuid()")?,
        AlgebraicTypeUse::Option(inner_ty) => {
            write!(out, "__t.option(")?;
            write_type_builder(module, out, inner_ty)?;
            write!(out, ")")?;
        }
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            write!(out, "__t.result(")?;
            write_type_builder(module, out, ok_ty)?;
            write!(out, ", ")?;
            write_type_builder(module, out, err_ty)?;
            write!(out, ")")?;
        }
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
            if matches!(&**elem_ty, AlgebraicTypeUse::Primitive(PrimitiveType::U8)) {
                return write!(out, "__t.byteArray()");
            }
            write!(out, "__t.array(")?;
            write_type_builder(module, out, elem_ty)?;
            write!(out, ")")?;
        }
        AlgebraicTypeUse::Ref(r) => {
            write!(out, "{}", type_ref_name(module, *r))?;
        }
    }
    Ok(())
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
    write!(out, "const {name}");
    if name == "AlgebraicType" {
        write!(out, ": __TypeBuilder<__AlgebraicTypeType, __AlgebraicTypeType>");
    }
    write!(out, " = __t.enum(\"{name}\", {{");
    out.with_indent(|out| write_object_type_builder_fields(module, out, variants, None, false, false).unwrap());
    writeln!(out, "}});");
    out.newline();
    writeln!(out, "export default {name};");
    out.newline();
}

fn type_ref_module_name(module: &ModuleDef, type_ref: AlgebraicTypeRef) -> String {
    let (name, _) = module.type_def_from_ref(type_ref).unwrap();
    type_module_name(name)
}

fn type_module_name(type_name: &ScopedTypeName) -> String {
    collect_case(Case::Snake, type_name.name_segments()) + "_type"
}

fn table_module_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake) + "_table"
}

fn reducer_args_type_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal) + "Reducer"
}

fn procedure_args_type_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Pascal) + "Procedure"
}

fn reducer_module_name(reducer_name: &Identifier) -> String {
    reducer_name.deref().to_case(Case::Snake) + "_reducer"
}

fn procedure_module_name(procedure_name: &Identifier) -> String {
    procedure_name.deref().to_case(Case::Snake) + "_procedure"
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

/// Print imports for each of the `imports`.
fn print_imports(module: &ModuleDef, out: &mut Indenter, imports: Imports, suffix: Option<&str>) {
    for typeref in imports {
        let module_name = type_ref_module_name(module, typeref);
        let type_name = type_ref_name(module, typeref);
        if let Some(suffix) = suffix {
            writeln!(out, "import {type_name}{suffix} from \"./{module_name}\";");
        } else {
            writeln!(out, "import {type_name} from \"./{module_name}\";");
        }
    }
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
    suffix: Option<&str>,
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
