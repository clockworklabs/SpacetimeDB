use crate::util::{
    is_reducer_invokable, iter_constraints, iter_indexes, iter_procedures, iter_reducers, iter_table_names_and_types,
    iter_tables, iter_types, iter_views, print_auto_generated_version_comment,
};
use crate::{CodegenOptions, OutputFile};

use super::util::{collect_case, print_auto_generated_file_comment, type_ref_name};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};
use std::iter;
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::ColId;
use spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb_schema::def::{ConstraintDef, IndexDef, ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef, ViewDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::reducer_name::ReducerName;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};

use super::code_indenter::{CodeIndenter, Indenter};
use super::Lang;
use spacetimedb_lib::version::spacetimedb_lib_version;

const INDENT: &str = "  ";

pub struct TypeScript;

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
        );

        writeln!(out);

        writeln!(out, "export default __t.row({{");
        out.indent(1);
        write_object_type_builder_fields(module, out, "", &product_def.elements, table.primary_key, true, true)
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

        print_file_header(out, false, true);

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
        );

        writeln!(out, "export const params = {{");
        out.with_indent(|out| {
            write_object_type_builder_fields(
                module,
                out,
                "",
                &procedure.params_for_generate.elements,
                None,
                true,
                false,
            )
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
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, true, false);

        writeln!(out);
        writeln!(out, "// Import all reducer arg schemas");
        for reducer in iter_reducers(module, options.visibility) {
            if !is_reducer_invokable(reducer) {
                // Skip system-defined reducers
                continue;
            }
            let reducer_module_name = reducer_module_name(&reducer.accessor_name);
            let args_type = reducer_args_type_name(&reducer.accessor_name);
            writeln!(out, "import {args_type} from \"./{reducer_module_name}\";");
        }

        writeln!(out);
        writeln!(out, "// Import all procedure arg schemas");
        for procedure in iter_procedures(module, options.visibility) {
            let procedure_module_name = procedure_module_name(&procedure.accessor_name);
            let args_type = procedure_args_type_name(&procedure.accessor_name);
            writeln!(out, "import * as {args_type} from \"./{procedure_module_name}\";");
        }

        writeln!(out);
        writeln!(out, "// Import all table schema definitions");
        for (_, accessor_name, _) in iter_table_names_and_types(module, options.visibility) {
            let table_module_name = table_module_name(accessor_name);
            let table_name_pascalcase = accessor_name.deref().to_case(Case::Pascal);
            // TODO: This really shouldn't be necessary. We could also have `table()` accept
            // `__t.object(...)`s.
            writeln!(out, "import {table_name_pascalcase}Row from \"./{table_module_name}\";");
        }

        // Import row types for mounted namespace tables (public only)
        let ns_tables: Vec<_> = module
            .all_tables_with_prefix()
            .into_iter()
            .filter(|(prefix, _, table)| !prefix.is_empty() && table.table_access == TableAccess::Public)
            .collect();
        let ns_views: Vec<_> = module
            .all_views_with_prefix()
            .into_iter()
            .filter(|(prefix, _, _)| !prefix.is_empty())
            .collect();
        let ns_reducers: Vec<_> = module
            .all_reducers_with_prefix()
            .into_iter()
            .filter(|(prefix, _, reducer)| !prefix.is_empty() && !reducer.visibility.is_private())
            .collect();
        let ns_procedures: Vec<_> = module
            .all_procedures_with_prefix()
            .into_iter()
            .filter(|(prefix, _, procedure)| !prefix.is_empty() && !procedure.visibility.is_private())
            .collect();
        if !ns_tables.is_empty() || !ns_views.is_empty() {
            writeln!(out);
            writeln!(out, "// Import namespace table schema definitions");
            for (prefix, _, table) in &ns_tables {
                let ns_path = mounted_ns_path(prefix);
                let file_stem = table_module_name(&table.accessor_name);
                let row_type = mounted_row_type_name(prefix, table.accessor_name.deref());
                writeln!(out, "import {row_type}Row from \"./{ns_path}/{file_stem}\";");
            }
            for (prefix, _, view) in &ns_views {
                let ns_path = mounted_ns_path(prefix);
                let file_stem = table_module_name(&view.accessor_name);
                let row_type = mounted_row_type_name(prefix, view.accessor_name.deref());
                writeln!(out, "import {row_type}Row from \"./{ns_path}/{file_stem}\";");
            }
        }
        if !ns_reducers.is_empty() {
            writeln!(out);
            writeln!(out, "// Import namespace reducer arg schemas");
            for (prefix, _, reducer) in &ns_reducers {
                if !is_reducer_invokable(reducer) {
                    continue;
                }
                let ns_path = mounted_ns_path(prefix);
                let module_name = reducer_module_name(&reducer.accessor_name);
                let args_type = mounted_reducer_args_type_name(prefix, &reducer.accessor_name);
                writeln!(out, "import {args_type} from \"./{ns_path}/{module_name}\";");
            }
        }
        if !ns_procedures.is_empty() {
            writeln!(out);
            writeln!(out, "// Import namespace procedure arg schemas");
            for (prefix, _, procedure) in &ns_procedures {
                let ns_path = mounted_ns_path(prefix);
                let module_name = procedure_module_name(&procedure.accessor_name);
                let args_type = mounted_procedure_args_type_name(prefix, &procedure.accessor_name);
                writeln!(out, "import * as {args_type} from \"./{ns_path}/{module_name}\";");
            }
        }

        writeln!(out);
        writeln!(out, "/** Type-only namespace exports for generated type groups. */");

        writeln!(out);
        writeln!(out, "/** The schema information for all tables in this module. This is defined the same was as the tables would have been defined in the server. */");
        writeln!(out, "const tablesSchema = __schema({{");
        out.indent(1);
        for table in iter_tables(module, options.visibility) {
            let type_ref = table.product_type_ref;
            let table_name_pascalcase = table.accessor_name.deref().to_case(Case::Pascal);
            writeln!(out, "{}: __table({{", table.accessor_name);
            out.indent(1);
            write_table_opts(
                module,
                out,
                type_ref,
                table.name.deref(),
                iter_indexes(table),
                iter_constraints(table),
                table.is_event,
            );
            out.dedent(1);
            writeln!(out, "}}, {}Row),", table_name_pascalcase);
        }
        for view in iter_views(module) {
            let type_ref = view.product_type_ref;
            let view_name_pascalcase = view.accessor_name.deref().to_case(Case::Pascal);
            writeln!(out, "{}: __table({{", view.accessor_name);
            out.indent(1);
            write_table_opts(module, out, type_ref, view.name.deref(), iter::empty(), iter::empty(), false);
            out.dedent(1);
            writeln!(out, "}}, {}Row),", view_name_pascalcase);
        }
        // Namespace tables from mounted submodules
        for (prefix, owning_def, table) in &ns_tables {
            let source_name = mounted_source_name(prefix, table.accessor_name.deref());
            let row_type = mounted_row_type_name(prefix, table.accessor_name.deref());
            let type_ref = table.product_type_ref;
            writeln!(out, "\"{source_name}\": __table({{");
            out.indent(1);
            write_table_opts(owning_def, out, type_ref, &source_name, iter_indexes(table), iter_constraints(table), table.is_event);
            out.dedent(1);
            writeln!(out, "}}, {row_type}Row),");
        }
        // Namespace views from mounted submodules
        for (prefix, owning_def, view) in &ns_views {
            let source_name = mounted_source_name(prefix, view.accessor_name.deref());
            let row_type = mounted_row_type_name(prefix, view.accessor_name.deref());
            let type_ref = view.product_type_ref;
            writeln!(out, "\"{source_name}\": __table({{");
            out.indent(1);
            write_table_opts(owning_def, out, type_ref, &source_name, iter::empty(), iter::empty(), false);
            out.dedent(1);
            writeln!(out, "}}, {row_type}Row),");
        }
        out.dedent(1);
        writeln!(out, "}});");

        writeln!(out);
        writeln!(out, "/** The schema information for all reducers in this module. This is defined the same way as the reducers would have been defined in the server, except the body of the reducer is omitted in code generation. */");
        writeln!(out, "const reducersSchema = __reducers(");
        out.indent(1);
        for reducer in iter_reducers(module, options.visibility) {
            if !is_reducer_invokable(reducer) {
                // Skip system-defined reducers
                continue;
            }
            let args_type = reducer_args_type_name(&reducer.accessor_name);
            writeln!(out, "__reducerSchema(\"{}\", {}),", reducer.name, args_type);
        }
        for (prefix, _, reducer) in &ns_reducers {
            if !is_reducer_invokable(reducer) {
                continue;
            }
            let wire_name = format!("{}{}", prefix, reducer.name);
            let args_type = mounted_reducer_args_type_name(prefix, &reducer.accessor_name);
            writeln!(out, "__reducerSchema(\"{wire_name}\", {args_type}),");
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
        for procedure in iter_procedures(module, options.visibility) {
            let args_type = procedure_args_type_name(&procedure.accessor_name);
            writeln!(
                out,
                "__procedureSchema(\"{}\", {args_type}.params, {args_type}.returnType),",
                procedure.name,
            );
        }
        for (prefix, _, procedure) in &ns_procedures {
            let wire_name = format!("{}{}", prefix, procedure.name);
            let args_type = mounted_procedure_args_type_name(prefix, &procedure.accessor_name);
            writeln!(out, "__procedureSchema(\"{wire_name}\", {args_type}.params, {args_type}.returnType),");
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
        writeln!(out, "/** The tables available in this remote SpacetimeDB module. Each table reference doubles as a query builder. */");
        if ns_tables.is_empty() && ns_views.is_empty() {
            writeln!(
                out,
                "export const tables: __QueryBuilder<typeof tablesSchema.schemaType> = __makeQueryBuilder(tablesSchema.schemaType);"
            );
        } else {
            writeln!(out, "const _qb = __makeQueryBuilder(tablesSchema.schemaType);");
            writeln!(out, "export const tables = {{");
            out.indent(1);
            // Root tables
            for table in iter_tables(module, options.visibility) {
                let key = table.accessor_name.deref();
                writeln!(out, "{key}: _qb.{key},");
            }
            // Root views
            for view in iter_views(module) {
                let key = view.accessor_name.deref();
                writeln!(out, "{key}: _qb.{key},");
            }
            // Build and emit namespace tree
            let tree = build_ns_tree(&ns_tables, &ns_views);
            emit_ns_tree(out, &tree);
            out.dedent(1);
            writeln!(out, "}} as const;");
        }
        writeln!(out);
        writeln!(out, "/** The reducers available in this remote SpacetimeDB module. */");
        if ns_reducers.is_empty() {
            writeln!(
                out,
                "export const reducers = __convertToAccessorMap(reducersSchema.reducersType.reducers);"
            );
        } else {
            writeln!(
                out,
                "const _reducers = __convertToAccessorMap(reducersSchema.reducersType.reducers);"
            );
            writeln!(out, "export const reducers = {{");
            out.indent(1);
            for reducer in iter_reducers(module, options.visibility) {
                if !is_reducer_invokable(reducer) {
                    continue;
                }
                let key = reducer.accessor_name.deref().to_case(Case::Camel);
                writeln!(out, "{key}: _reducers.{key},");
            }
            let tree = build_reducer_ns_tree(&ns_reducers);
            emit_fn_ns_tree(out, "_reducers", &tree);
            out.dedent(1);
            writeln!(out, "}} as const;");
        }

        writeln!(out);
        writeln!(
            out,
            "/** The procedures available in this remote SpacetimeDB module. */"
        );
        if ns_procedures.is_empty() {
            writeln!(
                out,
                "export const procedures = __convertToAccessorMap(proceduresSchema.procedures);"
            );
        } else {
            writeln!(
                out,
                "const _procedures = __convertToAccessorMap(proceduresSchema.procedures);"
            );
            writeln!(out, "export const procedures = {{");
            out.indent(1);
            for procedure in iter_procedures(module, options.visibility) {
                let key = procedure.accessor_name.deref().to_case(Case::Camel);
                writeln!(out, "{key}: _procedures.{key},");
            }
            let tree = build_procedure_ns_tree(&ns_procedures);
            emit_fn_ns_tree(out, "_procedures", &tree);
            out.dedent(1);
            writeln!(out, "}} as const;");
        }

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

        let index_file = OutputFile {
            filename: "index.ts".to_string(),
            code: output.into_inner(),
        };

        let reducers_file = generate_reducers_file(module, options);
        let procedures_file = generate_procedures_file(module, options);
        let types_file = generate_types_file(module);

        let mut files = vec![index_file, reducers_file, procedures_file, types_file];

        // Generate types.ts for each mounted submodule namespace so that the
        // namespace-scoped reducer/procedure/table files can resolve their
        // `import { … } from "./types"` imports.
        let mut mounted_namespaces: BTreeMap<String, &ModuleDef> = BTreeMap::new();
        collect_mounted_namespaces(module, "", &mut mounted_namespaces);
        for (prefix, owning_def) in &mounted_namespaces {
            let ns_path = mounted_ns_path(prefix);
            let filename = format!("{ns_path}/types.ts");
            files.push(generate_types_file_with_path(owning_def, filename));
        }

        files
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
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_file_header(out, false, true);
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

/// Recursively collect all mounted namespaces in depth-first order.
/// Keys are dot-terminated prefix strings (e.g. `"lib."`, `"lib.sublib."`).
/// Values are references to the `ModuleDef` that owns that namespace.
fn collect_mounted_namespaces<'a>(module: &'a ModuleDef, prefix: &str, out: &mut BTreeMap<String, &'a ModuleDef>) {
    for (ns, mounted_def) in module.mounts() {
        let full_prefix = format!("{prefix}{ns}.");
        out.insert(full_prefix.clone(), mounted_def);
        collect_mounted_namespaces(mounted_def, &full_prefix, out);
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
        out.with_indent(|out| write_object_type_builder_fields(module, out, "", params, None, true, false).unwrap());
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
        out.with_indent(|out| {
            write_object_type_builder_fields(module, out, name, elements, None, true, false).unwrap()
        });
        writeln!(out, "}});");
    }
    writeln!(out, "export type {name} = __Infer<typeof {name}>;");
    out.newline();
}

fn write_table_opts<'a>(
    module: &ModuleDef,
    out: &mut Indenter,
    type_ref: AlgebraicTypeRef,
    name: &str,
    indexes: impl Iterator<Item = &'a IndexDef>,
    constraints: impl Iterator<Item = &'a ConstraintDef>,
    is_event: bool,
) {
    let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();
    writeln!(out, "name: '{}',", name);
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
        let accessor_name = index_def.accessor_name.as_deref().unwrap_or(&index_def.name);
        writeln!(
            out,
            "{{ accessor: '{}', name: '{}', algorithm: 'btree', columns: [",
            accessor_name, index_def.name
        );
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
    if is_event {
        writeln!(out, "event: true,");
    }
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
    type_name: &str,
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
        write_type_builder_field(module, out, type_name, &name, original_name, ty, is_primary_key)?;
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
    type_name: &str,
    name: &str,
    original_name: Option<&str>,
    ty: &AlgebraicTypeUse,
    is_primary_key: bool,
) -> fmt::Result {
    // If the type contains a ref, we need to use a getter to prevent access-before-initialization.
    let needs_getter = type_contains_ref(ty);

    if needs_getter {
        if type_name == "RawModuleMountV10" && name == "module" {
            // HACK: Fixes a type inference error (TS7022/TS7023) for const types in typescript due to the recursive
            // type: RawModuleDefV10 -> ModuleMountsV10 -> RawModuleDefV10
            // Annotating this getter with `: any` breaks the cycle without affecting other types.
            writeln!(out, "get {name}(): any {{");
        } else {
            writeln!(out, "get {name}() {{");
        }
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
    out.with_indent(|out| {
        write_object_type_builder_fields(module, out, name, &pascal_variants, None, false, false).unwrap()
    });
    writeln!(out, "}});");
    writeln!(out, "export type {name} = __Infer<typeof {name}>;");
    out.newline();
}

fn table_module_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake) + "_table"
}

/// Combined accessor name for a mounted namespace table/view.
/// E.g. namespace="alias.", accessor_name="tableName" → "aliasTableName"
/// Source name (wire name) for a mounted namespace table/view.
/// E.g. namespace="alias.", accessor_name="tableName" → "alias.tableName"
fn mounted_source_name(namespace: &str, accessor_name: &str) -> String {
    format!("{}{}", namespace, accessor_name)
}

/// TypeScript import symbol for a mounted namespace table/view row type.
/// Uses `_` separator to avoid colliding with root tables that share the same PascalCase prefix.
/// E.g. namespace="lib.", accessor_name="library_table" → "Lib_LibraryTable"
fn mounted_row_type_name(namespace: &str, accessor_name: &str) -> String {
    let ns_part = namespace.trim_end_matches('.').replace('.', "_").to_case(Case::Pascal);
    format!("{}_{}", ns_part, accessor_name.to_case(Case::Pascal))
}

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

/// Converts a dot-terminated namespace like `"lib."` or `"lib.sublib."` to a path like `"lib"` or `"lib/sublib"`.
fn mounted_ns_path(namespace: &str) -> String {
    namespace.trim_end_matches('.').replace('.', "/")
}

/// TypeScript import symbol for a mounted namespace reducer/procedure.
/// Uses `_` separator to avoid colliding with root reducers/procedures sharing the same prefix.
/// E.g. prefix="lib.", accessor_name="library_reducer" → "Lib_LibraryReducer"
fn mounted_fn_type_name(prefix: &str, accessor_name: &str) -> String {
    let ns_part = prefix.trim_end_matches('.').replace('.', "_").to_case(Case::Pascal);
    format!("{}_{}", ns_part, accessor_name.to_case(Case::Pascal))
}

fn mounted_reducer_args_type_name(prefix: &str, accessor_name: &ReducerName) -> String {
    mounted_fn_type_name(prefix, accessor_name.deref()) + "Reducer"
}

fn mounted_procedure_args_type_name(prefix: &str, accessor_name: &Identifier) -> String {
    mounted_fn_type_name(prefix, accessor_name.deref()) + "Procedure"
}

/// A node in the recursive namespace tree used to emit the nested `tables` export.
struct NsTree {
    /// (combined_qb_key, local_ts_key) for table/view entries at this level.
    entries: Vec<(String, String)>,
    /// Child namespace nodes keyed by namespace segment.
    children: BTreeMap<String, NsTree>,
}

impl NsTree {
    fn new() -> Self {
        NsTree { entries: Vec::new(), children: BTreeMap::new() }
    }

    fn insert(&mut self, path_segs: &[&str], combined_qb_key: String, local_ts_key: String) {
        if path_segs.is_empty() {
            self.entries.push((combined_qb_key, local_ts_key));
        } else {
            self.children
                .entry(path_segs[0].to_string())
                .or_insert_with(NsTree::new)
                .insert(&path_segs[1..], combined_qb_key, local_ts_key);
        }
    }
}

/// Build the namespace tree from all mounted tables and views.
fn build_ns_tree<'a>(
    ns_tables: &[(String, &'a ModuleDef, &'a TableDef)],
    ns_views: &[(String, &'a ModuleDef, &'a ViewDef)],
) -> BTreeMap<String, NsTree> {
    let mut tree: BTreeMap<String, NsTree> = BTreeMap::new();
    for (prefix, _, table) in ns_tables {
        let source_name = mounted_source_name(prefix, table.accessor_name.deref());
        let local = table.accessor_name.deref().to_case(Case::Camel);
        // prefix like "lib." → segments ["lib"], or "lib.sublib." → ["lib", "sublib"]
        let segs: Vec<&str> = prefix.trim_end_matches('.').split('.').collect();
        if let Some((first, rest)) = segs.split_first() {
            tree.entry(first.to_string())
                .or_insert_with(NsTree::new)
                .insert(rest, source_name, local);
        }
    }
    for (prefix, _, view) in ns_views {
        let source_name = mounted_source_name(prefix, view.accessor_name.deref());
        let local = view.accessor_name.deref().to_case(Case::Camel);
        let segs: Vec<&str> = prefix.trim_end_matches('.').split('.').collect();
        if let Some((first, rest)) = segs.split_first() {
            tree.entry(first.to_string())
                .or_insert_with(NsTree::new)
                .insert(rest, source_name, local);
        }
    }
    tree
}

/// Recursively emit the namespace tree as nested TypeScript object blocks.
fn emit_ns_tree(out: &mut Indenter, tree: &BTreeMap<String, NsTree>) {
    for (ns, node) in tree {
        writeln!(out, "{ns}: {{");
        out.indent(1);
        for (qb_key, local_key) in &node.entries {
            writeln!(out, "{local_key}: _qb[\"{qb_key}\"],");
        }
        emit_ns_tree(out, &node.children);
        out.dedent(1);
        writeln!(out, "}},");
    }
}

/// Build namespace tree for mounted reducers (uses `.` path separator).
/// `flat_key` matches SDK's `accessorName = toCamelCase(wireName)`.
/// SDK toCamelCase only splits on `_`/`-`, so `/` is kept verbatim:
/// `"lib.library_reducer"` → `"lib.libraryReducer"`.  Bracket notation is required.
fn build_reducer_ns_tree<'a>(
    ns_reducers: &[(String, &'a ModuleDef, &'a ReducerDef)],
) -> BTreeMap<String, NsTree> {
    let mut tree: BTreeMap<String, NsTree> = BTreeMap::new();
    for (prefix, _, reducer) in ns_reducers {
        if !is_reducer_invokable(reducer) {
            continue;
        }
        let flat_key = format!("{}{}", prefix, reducer.accessor_name.deref().to_case(Case::Camel));
        let local = reducer.accessor_name.deref().to_case(Case::Camel);
        let segs: Vec<&str> = prefix.trim_end_matches('.').split('.').collect();
        if let Some((first, rest)) = segs.split_first() {
            tree.entry(first.to_string())
                .or_insert_with(NsTree::new)
                .insert(rest, flat_key, local);
        }
    }
    tree
}

/// Build namespace tree for mounted procedures (uses `.` path separator).
fn build_procedure_ns_tree<'a>(
    ns_procedures: &[(String, &'a ModuleDef, &'a ProcedureDef)],
) -> BTreeMap<String, NsTree> {
    let mut tree: BTreeMap<String, NsTree> = BTreeMap::new();
    for (prefix, _, procedure) in ns_procedures {
        let flat_key = format!("{}{}", prefix, procedure.accessor_name.deref().to_case(Case::Camel));
        let local = procedure.accessor_name.deref().to_case(Case::Camel);
        let segs: Vec<&str> = prefix.trim_end_matches('.').split('.').collect();
        if let Some((first, rest)) = segs.split_first() {
            tree.entry(first.to_string())
                .or_insert_with(NsTree::new)
                .insert(rest, flat_key, local);
        }
    }
    tree
}

/// Emit a namespace tree for reducers/procedures using bracket notation.
/// Flat keys contain `/` (e.g. `"lib/libraryReducer"`) so dot notation is invalid JS.
fn emit_fn_ns_tree(out: &mut Indenter, map_var: &str, tree: &BTreeMap<String, NsTree>) {
    for (ns, node) in tree {
        writeln!(out, "{ns}: {{");
        out.indent(1);
        for (flat_key, local_key) in &node.entries {
            writeln!(out, "{local_key}: {map_var}[\"{flat_key}\"],");
        }
        emit_fn_ns_tree(out, map_var, &node.children);
        out.dedent(1);
        writeln!(out, "}},");
    }
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
