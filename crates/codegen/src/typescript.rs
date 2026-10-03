use crate::util::{
    is_reducer_invokable, iter_constraints, iter_indexes, iter_procedures, iter_reducers, iter_tables, iter_types,
    iter_views, print_auto_generated_version_comment,
};
use crate::{CodegenOptions, OutputFile};

use super::util::{collect_case, print_auto_generated_file_comment, type_ref_name};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};
use std::iter;
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::{AlgebraicTypeRef, AlgebraicValue};
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{
    ColumnDef, ConstraintDef, IndexDef, ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef, ViewDef,
};
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

impl Lang for TypeScript {
    fn generate_type_files(&self, _module: &ModuleDef, _typ: &TypeDef) -> Vec<OutputFile> {
        vec![]
    }

    /// The row of a table or view, which `module.ts` declares the table or view with.
    ///
    /// e.g.
    /// ```ts
    /// export default __t.row("Player", {
    ///   id: __t.u32().primaryKey().autoInc().name("id"),
    ///   ownerId: __t.string().name("owner_id"),
    ///   get location() {
    ///     return Point.name("location");
    ///   },
    /// });
    /// ```
    fn generate_table_file_from_schema(
        &self,
        module: &ModuleDef,
        table: &TableDef,
        _schema: TableSchema,
    ) -> OutputFile {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_auto_generated_file_comment(out);
        print_lint_suppression(out);
        print_type_builder_imports(out, default_value_imports(module, table));

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

        // The row type keeps its name from the module, as `t.row(name, ...)` declares it.
        let row_type_name = type_ref_name(module, type_ref);
        writeln!(out, "export default __t.row({}, {{", ts_string_literal(&row_type_name));
        out.indent(1);
        write_object_type_builder_fields(module, out, &product_def.elements, table.primary_key, true, Some(table))
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
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;

        print_file_header(out, true, false);

        writeln!(out);
        writeln!(out, "// Import the module's declarations");
        writeln!(out, "import * as __module from \"./module\";");

        // Import row types for submodule namespace tables (public only)
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
        let has_namespaces =
            !ns_tables.is_empty() || !ns_views.is_empty() || !ns_reducers.is_empty() || !ns_procedures.is_empty();
        let (_, fallback) = Functions::partition(module, options);
        let has_fallback = !fallback.is_empty();
        if has_namespaces || has_fallback {
            writeln!(out);
            print_imports(
                out,
                [
                    "schema as __schema",
                    "table as __table",
                    "reducers as __reducers",
                    "reducerSchema as __reducerSchema",
                    "procedures as __procedures",
                    "procedureSchema as __procedureSchema",
                ],
            );
        }
        if !ns_tables.is_empty() || !ns_views.is_empty() {
            writeln!(out);
            writeln!(out, "// Import namespace table schema definitions");
            for (_, owning, table) in &ns_tables {
                let ns_path = submodule_ns_path(owning.accessor_path());
                let file_stem = table_module_name(&table.accessor_name);
                let row_type = submodule_row_type_name(owning.accessor_path(), table.accessor_name.deref());
                writeln!(out, "import {row_type}Row from \"./{ns_path}/{file_stem}\";");
            }
            for (_, owning, view) in &ns_views {
                let ns_path = submodule_ns_path(owning.accessor_path());
                let file_stem = table_module_name(&view.accessor_name);
                let row_type = submodule_row_type_name(owning.accessor_path(), view.accessor_name.deref());
                writeln!(out, "import {row_type}Row from \"./{ns_path}/{file_stem}\";");
            }
        }
        if !ns_reducers.is_empty() {
            writeln!(out);
            writeln!(out, "// Import namespace reducer arg schemas");
            for (_, owning, reducer) in &ns_reducers {
                if !is_reducer_invokable(reducer) {
                    continue;
                }
                let ns_path = submodule_ns_path(owning.accessor_path());
                let module_name = reducer_module_name(&reducer.accessor_name);
                let args_type = submodule_reducer_args_type_name(owning.accessor_path(), &reducer.accessor_name);
                writeln!(out, "import {args_type} from \"./{ns_path}/{module_name}\";");
            }
        }
        if !ns_procedures.is_empty() {
            writeln!(out);
            writeln!(out, "// Import namespace procedure arg schemas");
            for (_, owning, procedure) in &ns_procedures {
                let ns_path = submodule_ns_path(owning.accessor_path());
                let module_name = procedure_module_name(&procedure.accessor_name);
                let args_type = submodule_procedure_args_type_name(owning.accessor_path(), &procedure.accessor_name);
                writeln!(out, "import * as {args_type} from \"./{ns_path}/{module_name}\";");
            }
        }
        if has_fallback {
            writeln!(out);
            writeln!(
                out,
                "// Import the schemas of the functions that module.ts cannot export"
            );
            for reducer in &fallback.reducers {
                let module_name = reducer_module_name(&reducer.accessor_name);
                let args_type = reducer_args_type_name(&reducer.accessor_name);
                writeln!(out, "import {args_type} from \"./{module_name}\";");
            }
            for procedure in &fallback.procedures {
                let module_name = procedure_module_name(&procedure.accessor_name);
                let args_type = procedure_args_type_name(&procedure.accessor_name);
                writeln!(out, "import * as {args_type} from \"./{module_name}\";");
            }
            for view in &fallback.views {
                let module_name = table_module_name(&view.accessor_name);
                let row_type = view.accessor_name.deref().to_case(Case::Pascal);
                writeln!(out, "import {row_type}Row from \"./{module_name}\";");
            }
        }

        writeln!(out);
        writeln!(out, "/** Type-only namespace exports for generated type groups. */");

        // A table or view whose accessor is not camelCase keeps its accessor as a deprecated alias.
        let mut table_accessor_aliases = Vec::new();
        let mut table_accessor_names = BTreeSet::new();
        let accessor_names = iter_tables(module, options.visibility)
            .map(|table| &table.accessor_name)
            .chain(iter_views(module).map(|view| &view.accessor_name));
        for accessor_name in accessor_names {
            let table_accessor = accessor_name.deref().to_case(Case::Camel);
            table_accessor_names.insert(table_accessor.clone());
            if accessor_name.deref() != table_accessor {
                table_accessor_aliases.push((accessor_name.to_string(), table_accessor));
            }
        }
        // A reducer or procedure whose accessor differs from the one that bindings used to derive
        // from its canonical name keeps that one as a deprecated alias.
        let reducer_accessor_aliases = function_accessor_aliases(
            iter_reducers(module, options.visibility)
                .filter(|reducer| is_reducer_invokable(reducer))
                .map(|reducer| (reducer.name.deref(), reducer.accessor_name.deref())),
        );
        let procedure_accessor_aliases = function_accessor_aliases(
            iter_procedures(module, options.visibility)
                .map(|procedure| (procedure.name.deref(), procedure.accessor_name.deref())),
        );

        writeln!(out);
        if !has_namespaces && !has_fallback {
            writeln!(out, "/** The module def, built from the module's declarations. */");
            writeln!(out, "const __moduleDef = __moduleDefFromExports(__module);");
        } else {
            writeln!(
                out,
                "/** The module def, built from the module's declarations, without the items that module.ts cannot declare. */"
            );
            writeln!(out, "const __rootModuleDef = __moduleDefFromExports(__module);");

            // Module syntax cannot declare the items of a mounted submodule yet, and `module.ts`
            // cannot export a function whose accessor is not an export name, such as `default`,
            // so they keep the client schema that bindings used before.
            writeln!(out);
            writeln!(
                out,
                "/** The tables of the mounted submodules, and the views that module.ts cannot export. */"
            );
            writeln!(out, "const __fallbackTables = __schema({{");
            out.indent(1);
            for view in &fallback.views {
                let accessor = view.accessor_name.deref().to_case(Case::Camel);
                let row_type = view.accessor_name.deref().to_case(Case::Pascal);
                writeln!(out, "{}: __table({{", ts_object_key(&accessor));
                out.indent(1);
                write_table_opts(
                    module,
                    out,
                    view.product_type_ref,
                    view.name.deref(),
                    view.is_public,
                    iter::empty(),
                    iter::empty(),
                    false,
                );
                out.dedent(1);
                writeln!(out, "}}, {row_type}Row),");
            }
            // Namespace tables from submodules
            for (prefix, owning_def, table) in &ns_tables {
                let source_name = submodule_source_name(prefix, table.name.deref());
                let row_type = submodule_row_type_name(owning_def.accessor_path(), table.accessor_name.deref());
                let type_ref = table.product_type_ref;
                writeln!(out, "{}: __table({{", ts_object_key(&source_name));
                out.indent(1);
                write_table_opts(
                    owning_def,
                    out,
                    type_ref,
                    &source_name,
                    true,
                    iter_indexes(table),
                    iter_constraints(table),
                    table.is_event,
                );
                out.dedent(1);
                writeln!(out, "}}, {row_type}Row),");
            }
            // Namespace views from submodules.
            // The source name uses the canonical `view.name` (not the accessor) to match the
            // backing table name registered in the database by `create_view_with_prefix`.
            for (prefix, owning_def, view) in &ns_views {
                let source_name = submodule_source_name(prefix, view.name.deref());
                let row_type = submodule_row_type_name(owning_def.accessor_path(), view.accessor_name.deref());
                let type_ref = view.product_type_ref;
                writeln!(out, "{}: __table({{", ts_object_key(&source_name));
                out.indent(1);
                write_table_opts(
                    owning_def,
                    out,
                    type_ref,
                    &source_name,
                    view.is_public,
                    iter::empty(),
                    iter::empty(),
                    false,
                );
                out.dedent(1);
                writeln!(out, "}}, {row_type}Row),");
            }
            out.dedent(1);
            writeln!(out, "}});");

            writeln!(out);
            writeln!(
                out,
                "/** The reducers of the mounted submodules, and those that module.ts cannot export. */"
            );
            writeln!(out, "const __fallbackReducers = __reducers(");
            out.indent(1);
            for reducer in &fallback.reducers {
                let args_type = reducer_args_type_name(&reducer.accessor_name);
                writeln!(
                    out,
                    "__reducerSchema({}, {args_type}, {}),",
                    ts_string_literal(&reducer.name),
                    ts_string_literal(&reducer.accessor_name.deref().to_case(Case::Camel))
                );
            }
            for (_, owning, reducer) in &ns_reducers {
                if !is_reducer_invokable(reducer) {
                    continue;
                }
                // `reducer.name` is already qualified; do not prefix it again.
                let wire_name = reducer.name.to_string();
                let args_type = submodule_reducer_args_type_name(owning.accessor_path(), &reducer.accessor_name);
                let accessor_key = submodule_accessor_key(owning, &reducer.accessor_name);
                writeln!(
                    out,
                    "__reducerSchema(\"{wire_name}\", {args_type}, \"{accessor_key}\"),"
                );
            }
            out.dedent(1);
            writeln!(out, ");");

            writeln!(out);
            writeln!(
                out,
                "/** The procedures of the mounted submodules, and those that module.ts cannot export. */"
            );
            writeln!(out, "const __fallbackProcedures = __procedures(");
            out.indent(1);
            for procedure in &fallback.procedures {
                let args_type = procedure_args_type_name(&procedure.accessor_name);
                writeln!(
                    out,
                    "__procedureSchema({}, {args_type}.params, {args_type}.returnType, {}),",
                    ts_string_literal(&procedure.name),
                    ts_string_literal(&procedure.accessor_name.deref().to_case(Case::Camel))
                );
            }
            for (prefix, owning, procedure) in &ns_procedures {
                let wire_name = format!("{}{}", prefix, procedure.name);
                let args_type = submodule_procedure_args_type_name(owning.accessor_path(), &procedure.accessor_name);
                let accessor_key = submodule_accessor_key(owning, &procedure.accessor_name);
                writeln!(
                    out,
                    "__procedureSchema(\"{wire_name}\", {args_type}.params, {args_type}.returnType, \"{accessor_key}\"),"
                );
            }
            out.dedent(1);
            writeln!(out, ");");

            writeln!(out);
            writeln!(out, "/** The module def, with all of its items. */");
            writeln!(out, "const __moduleDef = {{");
            out.indent(1);
            writeln!(out, "...__rootModuleDef,");
            writeln!(
                out,
                "tables: {{ ...__rootModuleDef.tables, ...__fallbackTables.schemaType.tables }},"
            );
            writeln!(
                out,
                "reducers: [...__rootModuleDef.reducers, ...__fallbackReducers.reducersType.reducers],"
            );
            writeln!(
                out,
                "procedures: [...__rootModuleDef.procedures, ...__fallbackProcedures.procedures],"
            );
            out.dedent(1);
            writeln!(out, "}};");
        }

        table_accessor_aliases.retain(|(deprecated_accessor, _)| !table_accessor_names.contains(deprecated_accessor));
        let has_table_accessor_aliases = !table_accessor_aliases.is_empty();

        if has_table_accessor_aliases {
            writeln!(out);
            writeln!(
                out,
                "type __SchemaWithTableAccessorAliases = Omit<typeof __moduleDef, \"tables\"> & {{"
            );
            out.indent(1);
            writeln!(out, "tables: typeof __moduleDef.tables & {{");
            out.indent(1);
            for (deprecated_accessor, target_accessor) in &table_accessor_aliases {
                writeln!(
                    out,
                    "/** @deprecated Use `{target_accessor}` instead. This alias will be removed in the next major version. */"
                );
                writeln!(
                    out,
                    "readonly {}: Omit<typeof __moduleDef.tables[{}], \"accessorName\"> & {{ readonly accessorName: {} }};",
                    ts_string_literal(deprecated_accessor),
                    ts_string_literal(target_accessor),
                    ts_string_literal(deprecated_accessor)
                );
            }
            out.dedent(1);
            writeln!(out, "}};");
            out.dedent(1);
            writeln!(out, "}};");
        }

        writeln!(out);
        writeln!(
            out,
            "/** The remote SpacetimeDB module schema, both runtime and type information. */"
        );
        writeln!(out, "const REMOTE_MODULE = {{");
        out.indent(1);
        writeln!(out, "...__moduleDef,");
        writeln!(out, "versionInfo: {{");
        out.indent(1);
        writeln!(out, "cliVersion: \"{}\" as const,", spacetimedb_lib_version());
        out.dedent(1);
        writeln!(out, "}},");
        if has_table_accessor_aliases {
            writeln!(
                out,
                "tables: __moduleDef.tables as __SchemaWithTableAccessorAliases[\"tables\"],"
            );
        }
        out.dedent(1);
        writeln!(out, "}};");

        // Each kind of accessor that has deprecated aliases: the connection's field, its type with
        // the aliases, the object that holds the aliases, and the aliases.
        let alias_kinds: Vec<_> = [
            ("db", "DbView", "tableAccessorAliases", &table_accessor_aliases),
            (
                "reducers",
                "__ReducersView",
                "reducerAccessorAliases",
                &reducer_accessor_aliases,
            ),
            (
                "procedures",
                "__ProceduresView",
                "procedureAccessorAliases",
                &procedure_accessor_aliases,
            ),
        ]
        .into_iter()
        .filter(|(.., aliases)| !aliases.is_empty())
        .collect();

        for (_, _, aliases_object, aliases) in &alias_kinds {
            writeln!(out);
            writeln!(out, "const {aliases_object} = {{");
            out.indent(1);
            for (deprecated_accessor, target_accessor) in aliases.iter() {
                writeln!(
                    out,
                    "{}: {},",
                    ts_string_literal(deprecated_accessor),
                    ts_string_literal(target_accessor)
                );
            }
            out.dedent(1);
            writeln!(out, "}} as const;");
        }

        if !alias_kinds.is_empty() {
            writeln!(out);
            writeln!(
                out,
                "function __withAccessorAliases<T extends object>(target: T, aliases: Readonly<Record<string, string>>, freeze = false): T {{"
            );
            out.indent(1);
            writeln!(
                out,
                "const out = Object.create(Object.getPrototypeOf(target)) as T & Record<string, unknown>;"
            );
            writeln!(
                out,
                "Object.defineProperties(out, Object.getOwnPropertyDescriptors(target));"
            );
            writeln!(
                out,
                "for (const [deprecatedAccessor, targetAccessor] of Object.entries(aliases)) {{"
            );
            out.indent(1);
            writeln!(out, "if (deprecatedAccessor in out) {{");
            out.indent(1);
            writeln!(out, "continue;");
            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out, "Object.defineProperty(out, deprecatedAccessor, {{");
            out.indent(1);
            writeln!(out, "enumerable: true,");
            writeln!(out, "configurable: false,");
            writeln!(out, "get: () => out[targetAccessor],");
            out.dedent(1);
            writeln!(out, "}});");
            out.dedent(1);
            writeln!(out, "}}");
            writeln!(out, "return freeze ? Object.freeze(out) : out;");
            out.dedent(1);
            writeln!(out, "}}");
        }

        if has_table_accessor_aliases {
            writeln!(out);
            writeln!(
                out,
                "type __DbViewBase = __DbConnectionImpl<typeof REMOTE_MODULE>[\"db\"];"
            );
            write_with_aliases(out, "export type DbView = ", "__DbViewBase", &table_accessor_aliases);

            writeln!(out);
            writeln!(out, "type __TablesBase = __QueryBuilder<typeof __moduleDef>;");
            write_with_aliases(out, "export type Tables = ", "__TablesBase", &table_accessor_aliases);
        }
        if !reducer_accessor_aliases.is_empty() {
            writeln!(out);
            writeln!(
                out,
                "type __ReducersViewBase = __DbConnectionImpl<typeof REMOTE_MODULE>[\"reducers\"];"
            );
            write_with_aliases(
                out,
                "type __ReducersView = ",
                "__ReducersViewBase",
                &reducer_accessor_aliases,
            );
        }
        if !procedure_accessor_aliases.is_empty() {
            writeln!(out);
            writeln!(
                out,
                "type __ProceduresViewBase = __DbConnectionImpl<typeof REMOTE_MODULE>[\"procedures\"];"
            );
            write_with_aliases(
                out,
                "type __ProceduresView = ",
                "__ProceduresViewBase",
                &procedure_accessor_aliases,
            );
        }

        writeln!(out);
        writeln!(out, "/** The tables available in this remote SpacetimeDB module. Each table reference doubles as a query builder. */");
        if ns_tables.is_empty() && ns_views.is_empty() {
            if has_table_accessor_aliases {
                writeln!(out, "const tablesBase: __TablesBase = __makeQueryBuilder(__moduleDef);");
                writeln!(
                    out,
                    "export const tables: Tables = __withAccessorAliases(tablesBase, tableAccessorAliases, true) as Tables;"
                );
            } else {
                writeln!(
                    out,
                    "export const tables: __QueryBuilder<typeof __moduleDef> = __makeQueryBuilder(__moduleDef);"
                );
            }
        } else {
            writeln!(out, "const __qb = __makeQueryBuilder(__moduleDef);");
            writeln!(out, "export const tables = {{");
            out.indent(1);
            // Root tables (use camelCase accessor, matching the schema keys in `module.ts`)
            for table in iter_tables(module, options.visibility) {
                let key = table.accessor_name.deref().to_case(Case::Camel);
                writeln!(out, "{key}: __qb.{key},");
            }
            // Root views
            for view in iter_views(module) {
                let key = view.accessor_name.deref().to_case(Case::Camel);
                writeln!(out, "{key}: __qb.{key},");
            }
            // Build and emit namespace tree
            let tree = build_ns_tree(&ns_tables, &ns_views);
            emit_ns_tree(out, &tree);
            out.dedent(1);
            writeln!(out, "}} as const;");
        }
        writeln!(out);
        writeln!(out, "/** The reducers available in this remote SpacetimeDB module. */");
        // With aliases, the accessor map is `reducersBase`, which `reducers` exports with them.
        let reducers_decl = if reducer_accessor_aliases.is_empty() {
            "export const reducers"
        } else {
            "const reducersBase"
        };
        if ns_reducers.is_empty() {
            writeln!(out, "{reducers_decl} = __convertToAccessorMap(__moduleDef.reducers);");
        } else {
            writeln!(
                out,
                "const __reducerAccessors = __convertToAccessorMap(__moduleDef.reducers);"
            );
            writeln!(out, "{reducers_decl} = {{");
            out.indent(1);
            for reducer in iter_reducers(module, options.visibility) {
                if !is_reducer_invokable(reducer) {
                    continue;
                }
                let key = reducer.accessor_name.deref().to_case(Case::Camel);
                writeln!(out, "{key}: __reducerAccessors.{key},");
            }
            let tree = build_reducer_ns_tree(&ns_reducers);
            emit_fn_ns_tree(out, "__reducerAccessors", &tree);
            out.dedent(1);
            writeln!(out, "}} as const;");
        }
        if !reducer_accessor_aliases.is_empty() {
            write_with_aliases(
                out,
                "export const reducers = __withAccessorAliases(reducersBase, reducerAccessorAliases) as ",
                "(typeof reducersBase)",
                &reducer_accessor_aliases,
            );
        }

        writeln!(out);
        writeln!(
            out,
            "/** The procedures available in this remote SpacetimeDB module. */"
        );
        // With aliases, the accessor map is `proceduresBase`, which `procedures` exports with them.
        let procedures_decl = if procedure_accessor_aliases.is_empty() {
            "export const procedures"
        } else {
            "const proceduresBase"
        };
        if ns_procedures.is_empty() {
            writeln!(
                out,
                "{procedures_decl} = __convertToAccessorMap(__moduleDef.procedures);"
            );
        } else {
            writeln!(
                out,
                "const __procedureAccessors = __convertToAccessorMap(__moduleDef.procedures);"
            );
            writeln!(out, "{procedures_decl} = {{");
            out.indent(1);
            for procedure in iter_procedures(module, options.visibility) {
                let key = procedure.accessor_name.deref().to_case(Case::Camel);
                writeln!(out, "{key}: __procedureAccessors.{key},");
            }
            let tree = build_procedure_ns_tree(&ns_procedures);
            emit_fn_ns_tree(out, "__procedureAccessors", &tree);
            out.dedent(1);
            writeln!(out, "}} as const;");
        }
        if !procedure_accessor_aliases.is_empty() {
            write_with_aliases(
                out,
                "export const procedures = __withAccessorAliases(proceduresBase, procedureAccessorAliases) as ",
                "(typeof proceduresBase)",
                &procedure_accessor_aliases,
            );
        }

        // Write type aliases for EventContext, ReducerEventContext, SubscriptionEventContext, ErrorContext.
        // They see the tables and reducers with their deprecated aliases.
        let context_overrides: Vec<_> = alias_kinds
            .iter()
            .filter(|(field, ..)| *field != "procedures")
            .map(|(field, ty, ..)| (*field, *ty))
            .collect();
        writeln!(out);
        for (events, context) in [
            ("all possible events", "EventContext"),
            ("reducer events", "ReducerEventContext"),
            ("subscription events", "SubscriptionEventContext"),
            ("error events", "ErrorContext"),
        ] {
            writeln!(out, "/** The context type returned in callbacks for {events}. */");
            let interface = format!("__{context}Interface<typeof REMOTE_MODULE>");
            if context_overrides.is_empty() {
                writeln!(out, "export type {context} = {interface};");
            } else {
                let fields: Vec<_> = context_overrides
                    .iter()
                    .map(|(field, _)| format!("\"{field}\""))
                    .collect();
                let types: Vec<_> = context_overrides
                    .iter()
                    .map(|(field, ty)| format!("{field}: {ty}"))
                    .collect();
                writeln!(
                    out,
                    "export type {context} = Omit<{interface}, {}> & {{ {} }};",
                    fields.join(" | "),
                    types.join("; ")
                );
            }
        }

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
        if !alias_kinds.is_empty() {
            for (field, ty, ..) in &alias_kinds {
                writeln!(out, "declare {field}: {ty};");
            }

            writeln!(out);
            writeln!(
                out,
                "constructor(config: __DbConnectionConfig<typeof REMOTE_MODULE>) {{"
            );
            out.indent(1);
            writeln!(out, "super(config);");
            for (field, ty, aliases_object, _) in &alias_kinds {
                writeln!(
                    out,
                    "this.{field} = __withAccessorAliases(this.{field}, {aliases_object}) as {ty};"
                );
            }
            out.dedent(1);
            writeln!(out, "}}");

            writeln!(out);
        }
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

        let module_file = generate_module_file(module, options);
        let reducers_file = generate_reducers_file(module, options);
        let procedures_file = generate_procedures_file(module, options);
        let types_file = generate_types_file(module);

        let mut files = vec![index_file, module_file, reducers_file, procedures_file, types_file];

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

        files
    }
}

/// The module's declarations, as a TypeScript module declares them, without function bodies.
/// `index.ts` builds the client API from them.
///
/// e.g.
/// ```ts
/// const spacetimedb = __schema({
///   person: __table({
///     name: "person",
///     public: true,
///     indexes: [
///       { accessor: "id", name: "person_id_idx_btree", algorithm: "btree", columns: [
///         "id",
///       ] },
///     ],
///     constraints: [
///       { name: "person_id_key", constraint: "unique", columns: ["id"] },
///     ],
///   }, PersonRow),
/// });
/// export default spacetimedb;
///
/// export const add = spacetimedb.reducer({ name: "add" }, AddReducer);
/// ```
fn generate_module_file(module: &ModuleDef, options: &CodegenOptions) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_auto_generated_file_comment(out);
    print_lint_suppression(out);
    print_imports(out, ["schema as __schema", "table as __table", "t as __t"]);

    // `index.ts` declares the fallback functions, which this file cannot export.
    let (exported, _) = Functions::partition(module, options);

    writeln!(out);
    writeln!(out, "// Import all reducer arg schemas");
    for reducer in &exported.reducers {
        let reducer_module_name = reducer_module_name(&reducer.accessor_name);
        let args_type = reducer_args_type_name(&reducer.accessor_name);
        writeln!(out, "import {args_type} from \"./{reducer_module_name}\";");
    }

    writeln!(out);
    writeln!(out, "// Import all procedure arg schemas");
    for procedure in &exported.procedures {
        let procedure_module_name = procedure_module_name(&procedure.accessor_name);
        let args_type = procedure_args_type_name(&procedure.accessor_name);
        writeln!(out, "import * as {args_type} from \"./{procedure_module_name}\";");
    }

    writeln!(out);
    writeln!(out, "// Import all table schema definitions");
    let row_accessor_names = iter_tables(module, options.visibility)
        .map(|table| &table.accessor_name)
        .chain(exported.views.iter().map(|view| &view.accessor_name));
    for accessor_name in row_accessor_names {
        let table_module_name = table_module_name(accessor_name);
        let table_name_pascalcase = accessor_name.deref().to_case(Case::Pascal);
        // TODO: This really shouldn't be necessary. We could also have `table()` accept
        // `__t.object(...)`s.
        writeln!(out, "import {table_name_pascalcase}Row from \"./{table_module_name}\";");
    }

    writeln!(out);
    writeln!(out, "/** The module's schema, with its tables. */");
    writeln!(out, "const spacetimedb = __schema({{");
    out.indent(1);
    for table in iter_tables(module, options.visibility) {
        let type_ref = table.product_type_ref;
        let table_accessor = table.accessor_name.deref().to_case(Case::Camel);
        let table_name_pascalcase = table.accessor_name.deref().to_case(Case::Pascal);
        writeln!(out, "{}: __table({{", ts_object_key(&table_accessor));
        out.indent(1);
        write_table_opts(
            module,
            out,
            type_ref,
            table.name.deref(),
            table.table_access == TableAccess::Public,
            iter_indexes(table),
            iter_constraints(table),
            table.is_event,
        );
        out.dedent(1);
        writeln!(out, "}}, {}Row),", table_name_pascalcase);
    }
    out.dedent(1);
    writeln!(out, "}});");
    writeln!(out, "export default spacetimedb;");

    writeln!(out);
    writeln!(
        out,
        "// The module's reducers, procedures, and views, without their bodies"
    );
    for reducer in exported.reducers {
        let args_type = reducer_args_type_name(&reducer.accessor_name);
        let name = ts_string_literal(&reducer.name);
        write_export(
            out,
            &reducer.accessor_name,
            format_args!("spacetimedb.reducer({{ name: {name} }}, {args_type})"),
        );
    }
    for procedure in exported.procedures {
        let args_type = procedure_args_type_name(&procedure.accessor_name);
        let name = ts_string_literal(&procedure.name);
        write_export(
            out,
            &procedure.accessor_name,
            format_args!("spacetimedb.procedure({{ name: {name} }}, {args_type}.params, {args_type}.returnType)"),
        );
    }
    for view in exported.views {
        let declare = if view.is_anonymous { "anonymousView" } else { "view" };
        // A view returns an array or an option of its rows.
        let wrap = match view.return_type_for_generate {
            AlgebraicTypeUse::Option(_) => "option",
            _ => "array",
        };
        let row_type = view.accessor_name.deref().to_case(Case::Pascal) + "Row";
        let name = ts_string_literal(&view.name);
        let public = if view.is_public { ", public: true" } else { "" };
        write_export(
            out,
            &view.accessor_name,
            format_args!("spacetimedb.{declare}({{ name: {name}{public} }}, __t.{wrap}({row_type}))"),
        );
    }

    OutputFile {
        filename: "module.ts".to_string(),
        code: output.into_inner(),
    }
}

/// The module's client-callable reducers, its procedures, and its views.
struct Functions<'a> {
    reducers: Vec<&'a ReducerDef>,
    procedures: Vec<&'a ProcedureDef>,
    views: Vec<&'a ViewDef>,
}

impl<'a> Functions<'a> {
    /// Splits the module's functions into those that `module.ts` declares and exports, and the
    /// fallback ones, whose accessors are not export names, which `index.ts` declares with the
    /// client schema that bindings used before.
    fn partition(module: &'a ModuleDef, options: &CodegenOptions) -> (Self, Self) {
        let (reducers, fallback_reducers) = iter_reducers(module, options.visibility)
            .filter(|reducer| is_reducer_invokable(reducer))
            .partition(|reducer| is_export_name(&reducer.accessor_name));
        let (procedures, fallback_procedures) =
            iter_procedures(module, options.visibility).partition(|procedure| is_export_name(&procedure.accessor_name));
        let (views, fallback_views) = iter_views(module).partition(|view| is_export_name(&view.accessor_name));
        let exported = Self {
            reducers,
            procedures,
            views,
        };
        let fallback = Self {
            reducers: fallback_reducers,
            procedures: fallback_procedures,
            views: fallback_views,
        };
        (exported, fallback)
    }

    fn is_empty(&self) -> bool {
        self.reducers.is_empty() && self.procedures.is_empty() && self.views.is_empty()
    }
}

/// Whether `module.ts` can export a declaration under the camelCase form of `accessor_name`:
/// an identifier name other than `default`, which is the schema's export.
fn is_export_name(accessor_name: &str) -> bool {
    let name = accessor_name.to_case(Case::Camel);
    is_identifier_name(&name) && name != "default"
}

/// Whether `name` is an ASCII identifier name, which an object literal takes as a key unquoted.
fn is_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// `key` as an object literal's key: unquoted if it is an identifier name, and quoted otherwise.
fn ts_object_key(key: &str) -> String {
    if is_identifier_name(key) {
        key.to_owned()
    } else {
        ts_string_literal(key)
    }
}

/// Exports `declaration` under the camelCase form of `accessor_name`, as `index.ts` expects.
/// A name that cannot be a variable, such as a reserved word, is exported through an alias.
fn write_export(out: &mut Indenter, accessor_name: &str, declaration: fmt::Arguments) {
    let name = accessor_name.to_case(Case::Camel);
    // `spacetimedb` is the schema's variable in `module.ts`, and TypeScript reserves `require`
    // and `exports` at the top level of a module.
    if RESERVED_KEYWORDS.contains(&name.as_str()) || matches!(name.as_str(), "spacetimedb" | "require" | "exports") {
        writeln!(out, "const __{name} = {declaration};");
        writeln!(out, "export {{ __{name} as {name} }};");
    } else {
        writeln!(out, "export const {name} = {declaration};");
    }
}

/// Writes `{prefix}{base} & { ... };`, where the object type declares each deprecated alias as
/// the member of `base` that it stands for.
fn write_with_aliases(out: &mut Indenter, prefix: &str, base: &str, aliases: &[(String, String)]) {
    writeln!(out, "{prefix}{base} & {{");
    out.indent(1);
    for (deprecated_accessor, target_accessor) in aliases {
        writeln!(
            out,
            "/** @deprecated Use `{target_accessor}` instead. This alias will be removed in the next major version. */"
        );
        writeln!(
            out,
            "readonly {}: {base}[{}];",
            ts_string_literal(deprecated_accessor),
            ts_string_literal(target_accessor)
        );
    }
    out.dedent(1);
    writeln!(out, "}};");
}

/// For functions given as `(canonical name, accessor)`, the accessors that bindings used to derive
/// from their canonical names, paired with the camelCase accessors that replace them, where the two
/// differ.
fn function_accessor_aliases<'a>(functions: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<(String, String)> {
    let functions: Vec<_> = functions
        .map(|(name, accessor)| (sdk_to_camel_case(name), accessor.to_case(Case::Camel)))
        .collect();
    let accessors: BTreeSet<_> = functions.iter().map(|(_, accessor)| accessor.clone()).collect();
    functions
        .into_iter()
        .filter(|(old, new)| old != new && !accessors.contains(old))
        .collect()
}

/// The SDK's `toCamelCase`: collapses each run of `-` and `_` to one `_`, removes each `_` that
/// precedes an ASCII letter or digit and uppercases that character, and lowercases the first
/// UTF-16 code unit.
fn sdk_to_camel_case(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '-' && c != '_' {
            out.push(c);
            continue;
        }
        while chars.next_if(|&c| c == '-' || c == '_').is_some() {}
        match chars.next_if(char::is_ascii_alphanumeric) {
            Some(next) => out.push(next.to_ascii_uppercase()),
            None => out.push('_'),
        }
    }
    let mut chars = out.chars();
    match chars.next() {
        Some(first) if first.len_utf16() == 1 => first.to_lowercase().chain(chars).collect(),
        _ => out,
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

fn print_index_imports(out: &mut Indenter) {
    let types = [
        "TypeBuilder as __TypeBuilder",
        "type AlgebraicTypeType as __AlgebraicTypeType",
        "Uuid as __Uuid",
        "DbConnectionBuilder as __DbConnectionBuilder",
        "convertToAccessorMap as __convertToAccessorMap",
        "makeQueryBuilder as __makeQueryBuilder",
        "moduleDefFromExports as __moduleDefFromExports",
        "type QueryBuilder as __QueryBuilder",
        "type EventContextInterface as __EventContextInterface",
        "type ReducerEventContextInterface as __ReducerEventContextInterface",
        "type SubscriptionEventContextInterface as __SubscriptionEventContextInterface",
        "type SubscriptionHandleImpl as __SubscriptionHandleImpl",
        "type ErrorContextInterface as __ErrorContextInterface",
        "SubscriptionBuilderImpl as __SubscriptionBuilderImpl",
        "DbConnectionImpl as __DbConnectionImpl",
        "type Event as __Event",
        "type Infer as __Infer",
        "type DbConnectionConfig as __DbConnectionConfig",
        "t as __t",
    ];
    print_imports(out, types);
}

fn print_type_builder_imports<'a>(out: &mut Indenter, extra: impl IntoIterator<Item = &'a str>) {
    let types = [
        "TypeBuilder as __TypeBuilder",
        "type AlgebraicTypeType as __AlgebraicTypeType",
        "type Infer as __Infer",
        "t as __t",
    ];
    print_imports(out, types.into_iter().chain(extra));
}

fn print_imports<'a>(out: &mut Indenter, types: impl IntoIterator<Item = &'a str>) {
    // All library imports are prefixed with `__` to avoid
    // clashing with the names of user generated types.
    let mut types: Vec<_> = types.into_iter().collect();
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
        print_type_builder_imports(output, []);
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

#[allow(clippy::too_many_arguments)]
fn write_table_opts<'a>(
    module: &ModuleDef,
    out: &mut Indenter,
    type_ref: AlgebraicTypeRef,
    name: &str,
    is_public: bool,
    indexes: impl Iterator<Item = &'a IndexDef>,
    constraints: impl Iterator<Item = &'a ConstraintDef>,
    is_event: bool,
) {
    let product_def = module.typespace_for_generate()[type_ref].as_product().unwrap();
    writeln!(out, "name: {},", ts_string_literal(name));
    if is_public {
        writeln!(out, "public: true,");
    }
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
            "{{ accessor: {}, name: {}, algorithm: {}, columns: [",
            ts_string_literal(accessor_name),
            ts_string_literal(&index_def.name),
            ts_string_literal("btree")
        );
        out.indent(1);
        for col_id in columns.iter() {
            writeln!(out, "{},", ts_string_literal(&get_name_and_type(col_id).0));
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
                ts_string_literal(&field_name.deref().to_case(Case::Camel))
            })
            .collect();

        writeln!(
            out,
            "{{ name: {}, constraint: {}, columns: [{}] }},",
            ts_string_literal(&constraint.name),
            ts_string_literal("unique"),
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
///
/// For the row of `table`, each column also declares the attributes the table records for it:
/// `.unique()`, `.autoInc()`, `.default(..)`, and its canonical name, `.name(..)`.
fn write_object_type_builder_fields(
    module: &ModuleDef,
    out: &mut Indenter,
    elements: &[(Identifier, AlgebraicTypeUse)],
    primary_key: Option<ColId>,
    convert_case: bool,
    table: Option<&TableDef>,
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
        // The `.name(..)` value is the in-database (canonical) column name. It is always
        // explicit, so the client never derives a column's canonical name from its accessor.
        let column = table.and_then(|table| {
            let column = table.columns.get(i)?;
            Some(ColumnAttrs {
                name: column.name.deref(),
                unique: table.constraints.values().any(|constraint| {
                    constraint.data.unique_columns().and_then(|cols| cols.as_singleton()) == Some(column.col_id)
                }),
                auto_inc: table.sequences.values().any(|seq| seq.column == column.col_id),
                default_value: ts_default_value(module, column),
            })
        });
        write_type_builder_field(module, out, &name, ty, is_primary_key, column)?;
    }

    Ok(())
}

/// What a table records about a column beyond its type.
struct ColumnAttrs<'a> {
    /// The canonical name.
    name: &'a str,
    /// Whether a unique constraint covers just this column.
    unique: bool,
    auto_inc: bool,
    default_value: Option<&'a AlgebraicValue>,
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
    ty: &AlgebraicTypeUse,
    is_primary_key: bool,
    column: Option<ColumnAttrs>,
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
    // `__t.unit()` takes no column attributes, and `__t.result(..)` has no `.name(..)`,
    // so on the client these columns keep their accessor as their name.
    if let Some(column) = column.filter(|_| !matches!(ty, AlgebraicTypeUse::Unit)) {
        // A module declares such a column `.unique()`, which also types its index as unique on
        // the client. The builders of other types, such as enums, have no `.unique()`.
        let has_unique = matches!(
            ty,
            AlgebraicTypeUse::String
                | AlgebraicTypeUse::Primitive(_)
                | AlgebraicTypeUse::Identity
                | AlgebraicTypeUse::ConnectionId
                | AlgebraicTypeUse::Timestamp
                | AlgebraicTypeUse::TimeDuration
                | AlgebraicTypeUse::Uuid
        );
        if column.unique && !is_primary_key && has_unique {
            write!(out, ".unique()");
        }
        if column.auto_inc {
            write!(out, ".autoInc()");
        }
        if let Some(value) = column.default_value {
            write!(out, ".default(");
            write_value(module, out, ty, value, &mut BTreeSet::new())?;
            write!(out, ")");
        }
        if !matches!(ty, AlgebraicTypeUse::Result { .. }) {
            write!(out, ".name({})", ts_string_literal(column.name));
        }
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

/// The SDK classes that the default values of `table`'s columns use.
fn default_value_imports(module: &ModuleDef, table: &TableDef) -> BTreeSet<&'static str> {
    let mut imports = BTreeSet::new();
    for column in &table.columns {
        if let Some(value) = ts_default_value(module, column) {
            write_value(module, &mut String::new(), &column.ty_for_generate, value, &mut imports).unwrap();
        }
    }
    imports
}

/// The default value of `column`, unless it has a `Some(None)`, which `.default(..)` cannot
/// declare: TypeScript represents both `None` and `Some(None)` as `undefined`.
fn ts_default_value<'a>(module: &ModuleDef, column: &'a ColumnDef) -> Option<&'a AlgebraicValue> {
    let value = column.default_value.as_ref()?;
    (!has_some_none(module, &column.ty_for_generate, value)).then_some(value)
}

/// Whether `value`, of type `ty`, has a `Some(None)` anywhere in it.
fn has_some_none(module: &ModuleDef, ty: &AlgebraicTypeUse, value: &AlgebraicValue) -> bool {
    match (ty, value) {
        (AlgebraicTypeUse::Option(inner), AlgebraicValue::Sum(sum)) if sum.tag == 0 => {
            matches!((&**inner, &*sum.value), (AlgebraicTypeUse::Option(_), AlgebraicValue::Sum(none)) if none.tag != 0)
                || has_some_none(module, inner, &sum.value)
        }
        (AlgebraicTypeUse::Result { ok_ty, err_ty }, AlgebraicValue::Sum(sum)) => {
            has_some_none(module, if sum.tag == 0 { ok_ty } else { err_ty }, &sum.value)
        }
        (AlgebraicTypeUse::Array(elem_ty), AlgebraicValue::Array(array)) => {
            array.iter_cloned().any(|elem| has_some_none(module, elem_ty, &elem))
        }
        (AlgebraicTypeUse::Ref(r), _) => match (&module.typespace_for_generate()[*r], value) {
            (AlgebraicTypeDef::Product(product), AlgebraicValue::Product(fields)) => product
                .elements
                .iter()
                .zip(&*fields.elements)
                .any(|((_, ty), field)| has_some_none(module, ty, field)),
            (AlgebraicTypeDef::Sum(sum_def), AlgebraicValue::Sum(sum)) => {
                has_some_none(module, &sum_def.variants[sum.tag as usize].1, &sum.value)
            }
            _ => false,
        },
        _ => false,
    }
}

/// Writes `value` as the TypeScript value that the type builder for `ty` takes, e.g. for
/// `.default(..)`. Adds the SDK classes it uses to `imports`.
fn write_value<W: Write>(
    module: &ModuleDef,
    out: &mut W,
    ty: &AlgebraicTypeUse,
    value: &AlgebraicValue,
    imports: &mut BTreeSet<&'static str>,
) -> fmt::Result {
    // The special types are products of one integer, which their SDK class takes.
    let mut write_class = |class: &'static str, import: &'static str| {
        imports.insert(import);
        let AlgebraicValue::Product(product) = value else {
            panic!("a {class} value must be a product");
        };
        write!(out, "new __{class}({})", bigint_literal(&product.elements[0]))
    };
    match ty {
        AlgebraicTypeUse::Identity => return write_class("Identity", "Identity as __Identity"),
        AlgebraicTypeUse::ConnectionId => return write_class("ConnectionId", "ConnectionId as __ConnectionId"),
        AlgebraicTypeUse::Timestamp => return write_class("Timestamp", "Timestamp as __Timestamp"),
        AlgebraicTypeUse::TimeDuration => return write_class("TimeDuration", "TimeDuration as __TimeDuration"),
        AlgebraicTypeUse::Uuid => return write_class("Uuid", "Uuid as __Uuid"),
        _ => {}
    }
    match (ty, value) {
        (AlgebraicTypeUse::Unit, _) => write!(out, "{{}}"),
        (AlgebraicTypeUse::Option(inner), AlgebraicValue::Sum(sum)) if sum.tag == 0 => {
            write_value(module, out, inner, &sum.value, imports)
        }
        (AlgebraicTypeUse::Option(_), _) => write!(out, "undefined"),
        (AlgebraicTypeUse::Result { ok_ty, err_ty }, AlgebraicValue::Sum(sum)) => {
            let (key, ty) = if sum.tag == 0 { ("ok", ok_ty) } else { ("err", err_ty) };
            write!(out, "{{ {key}: ")?;
            write_value(module, out, ty, &sum.value, imports)?;
            write!(out, " }}")
        }
        (AlgebraicTypeUse::ScheduleAt, AlgebraicValue::Sum(sum)) => {
            let (tag, ty) = if sum.tag == 0 {
                ("Interval", AlgebraicTypeUse::TimeDuration)
            } else {
                ("Time", AlgebraicTypeUse::Timestamp)
            };
            write!(out, "{{ tag: \"{tag}\", value: ")?;
            write_value(module, out, &ty, &sum.value, imports)?;
            write!(out, " }}")
        }
        (AlgebraicTypeUse::String, AlgebraicValue::String(s)) => write!(out, "{}", ts_string_literal(s)),
        (AlgebraicTypeUse::Primitive(_), _) => write!(out, "{}", primitive_literal(value)),
        (AlgebraicTypeUse::Array(elem_ty), AlgebraicValue::Array(array)) => {
            let is_bytes = matches!(&**elem_ty, AlgebraicTypeUse::Primitive(PrimitiveType::U8));
            write!(out, "{}[", if is_bytes { "new Uint8Array(" } else { "" })?;
            for (i, elem) in array.iter_cloned().enumerate() {
                write!(out, "{}", if i == 0 { "" } else { ", " })?;
                write_value(module, out, elem_ty, &elem, imports)?;
            }
            write!(out, "]{}", if is_bytes { ")" } else { "" })
        }
        // Products and sums use the names that `types.ts` gives their fields and variants.
        (AlgebraicTypeUse::Ref(r), _) => match (&module.typespace_for_generate()[*r], value) {
            (AlgebraicTypeDef::Product(product), AlgebraicValue::Product(fields)) => {
                write!(out, "{{ ")?;
                for ((name, ty), field) in product.elements.iter().zip(&*fields.elements) {
                    write!(out, "{}: ", name.deref().to_case(Case::Camel))?;
                    write_value(module, out, ty, field, imports)?;
                    write!(out, ", ")?;
                }
                write!(out, "}}")
            }
            (AlgebraicTypeDef::Sum(sum_def), AlgebraicValue::Sum(sum)) => {
                let (name, ty) = &sum_def.variants[sum.tag as usize];
                write!(out, "{{ tag: \"{}\"", name.deref().to_case(Case::Pascal))?;
                if !matches!(ty, AlgebraicTypeUse::Unit) {
                    write!(out, ", value: ")?;
                    write_value(module, out, ty, &sum.value, imports)?;
                }
                write!(out, " }}")
            }
            (AlgebraicTypeDef::PlainEnum(plain_enum), AlgebraicValue::Sum(sum)) => {
                let name = &plain_enum.variants[sum.tag as usize];
                write!(out, "{{ tag: \"{}\" }}", name.deref().to_case(Case::Pascal))
            }
            _ => panic!("value {value:?} does not match its type"),
        },
        _ => panic!("value {value:?} does not match its type {ty:?}"),
    }
}

/// A number, or a bigint for the integers that TypeScript represents as bigints.
fn primitive_literal(value: &AlgebraicValue) -> String {
    match value {
        AlgebraicValue::Bool(b) => b.to_string(),
        AlgebraicValue::I8(n) => n.to_string(),
        AlgebraicValue::U8(n) => n.to_string(),
        AlgebraicValue::I16(n) => n.to_string(),
        AlgebraicValue::U16(n) => n.to_string(),
        AlgebraicValue::I32(n) => n.to_string(),
        AlgebraicValue::U32(n) => n.to_string(),
        AlgebraicValue::F32(f) => float_literal(f.into_inner()),
        AlgebraicValue::F64(f) => float_literal(f.into_inner()),
        _ => bigint_literal(value),
    }
}

fn bigint_literal(value: &AlgebraicValue) -> String {
    match value {
        AlgebraicValue::I64(n) => format!("{n}n"),
        AlgebraicValue::U64(n) => format!("{n}n"),
        AlgebraicValue::I128(n) => format!("{}n", { n.0 }),
        AlgebraicValue::U128(n) => format!("{}n", { n.0 }),
        AlgebraicValue::I256(n) => format!("{n}n"),
        AlgebraicValue::U256(n) => format!("{n}n"),
        _ => panic!("{value:?} is not a bigint"),
    }
}

fn float_literal<F: Into<f64> + fmt::Debug + Copy>(f: F) -> String {
    match f.into() {
        f if f.is_nan() => "NaN".into(),
        f if f.is_infinite() => if f > 0.0 { "Infinity" } else { "-Infinity" }.into(),
        _ => format!("{f:?}"),
    }
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
fn submodule_source_name(namespace: &NamespacePath, canonical_name: &str) -> String {
    format!("{}{}", namespace, canonical_name)
}

/// TypeScript import symbol for a submodule namespace table/view row type.
/// Uses `_` separator to avoid colliding with root tables that share the same PascalCase prefix.
/// `namespace` is the *accessor* path, since this names a client-side symbol.
/// E.g. namespace="myLib.", accessor_name="library_table" → "MyLib_LibraryTable"
fn submodule_row_type_name(namespace: &NamespacePath, accessor_name: &str) -> String {
    let ns_part = namespace.join_segments("_").to_case(Case::Pascal);
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

/// Converts a namespace path like `"lib."` or `"lib.sublib."` to a directory path like `"lib"` or `"lib/sublib"`.
/// Callers pass the *accessor* path, so generated directories follow the names used in module code.
fn submodule_ns_path(namespace: &NamespacePath) -> String {
    namespace.join_segments("/")
}

/// The key under which a submodule reducer or procedure is registered in the SDK's
/// accessor map. This corresponds to the owning module's accessor path and the camelCase
/// accessor name, joined by `.`, e.g. `myLib.libInsert`.
fn submodule_accessor_key(owning: &ModuleDef, accessor_name: &str) -> String {
    owning
        .accessor_path()
        .segments()
        .iter()
        .map(|segment| segment.to_string())
        .chain(std::iter::once(accessor_name.to_case(Case::Camel)))
        .collect::<Vec<_>>()
        .join(".")
}

/// TypeScript import symbol for a submodule namespace reducer/procedure.
/// Uses `_` separator to avoid colliding with root reducers/procedures sharing the same prefix.
/// `prefix` is the *accessor* path, since this names a client-side symbol.
/// E.g. prefix="myLib.", accessor_name="library_reducer" → "MyLib_LibraryReducer"
fn submodule_fn_type_name(prefix: &NamespacePath, accessor_name: &str) -> String {
    let ns_part = prefix.join_segments("_").to_case(Case::Pascal);
    format!("{}_{}", ns_part, accessor_name.to_case(Case::Pascal))
}

fn submodule_reducer_args_type_name(prefix: &NamespacePath, accessor_name: &ReducerName) -> String {
    submodule_fn_type_name(prefix, accessor_name.deref()) + "Reducer"
}

fn submodule_procedure_args_type_name(prefix: &NamespacePath, accessor_name: &Identifier) -> String {
    submodule_fn_type_name(prefix, accessor_name.deref()) + "Procedure"
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
        NsTree {
            entries: Vec::new(),
            children: BTreeMap::new(),
        }
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

/// Build the namespace tree from all submodule tables and views.
fn build_ns_tree<'a>(
    ns_tables: &[(NamespacePath, &'a ModuleDef, &'a TableDef)],
    ns_views: &[(NamespacePath, &'a ModuleDef, &'a ViewDef)],
) -> BTreeMap<String, NsTree> {
    // Object keys follow the accessor path (`tables.myLib.x`), while the query builder keys
    // are the canonical wire names (`__qb["my_lib.x"]`) that match the `__fallbackTables` entries.
    let mut tree: BTreeMap<String, NsTree> = BTreeMap::new();
    for (prefix, owning, table) in ns_tables {
        let source_name = submodule_source_name(prefix, table.name.deref());
        let local = table.accessor_name.deref().to_case(Case::Camel);
        let segs: Vec<&str> = owning.accessor_path().segments().iter().map(|s| &**s).collect();
        if let Some((first, rest)) = segs.split_first() {
            tree.entry(first.to_string())
                .or_insert_with(NsTree::new)
                .insert(rest, source_name, local);
        }
    }
    for (prefix, owning, view) in ns_views {
        // Canonical name: must match the `__fallbackTables` key and the DB backing table name.
        let source_name = submodule_source_name(prefix, view.name.deref());
        let local = view.accessor_name.deref().to_case(Case::Camel);
        let segs: Vec<&str> = owning.accessor_path().segments().iter().map(|s| &**s).collect();
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
            writeln!(out, "{local_key}: __qb[\"{qb_key}\"],");
        }
        emit_ns_tree(out, &node.children);
        out.dedent(1);
        writeln!(out, "}},");
    }
}

/// Build namespace tree for submodule reducers (uses `.` path separator).
/// Object keys follow the accessor path; `flat_key` is the accessor-map key the schema
/// entry was registered under (see [`submodule_accessor_key`]). It contains dots, so
/// bracket notation is required.
fn build_reducer_ns_tree<'a>(
    ns_reducers: &[(NamespacePath, &'a ModuleDef, &'a ReducerDef)],
) -> BTreeMap<String, NsTree> {
    let mut tree: BTreeMap<String, NsTree> = BTreeMap::new();
    for (_, owning, reducer) in ns_reducers {
        if !is_reducer_invokable(reducer) {
            continue;
        }
        let flat_key = submodule_accessor_key(owning, &reducer.accessor_name);
        let local = reducer.accessor_name.deref().to_case(Case::Camel);
        let segs: Vec<&str> = owning.accessor_path().segments().iter().map(|s| &**s).collect();
        if let Some((first, rest)) = segs.split_first() {
            tree.entry(first.to_string())
                .or_insert_with(NsTree::new)
                .insert(rest, flat_key, local);
        }
    }
    tree
}

/// Build namespace tree for submodule procedures (uses `.` path separator).
fn build_procedure_ns_tree<'a>(
    ns_procedures: &[(NamespacePath, &'a ModuleDef, &'a ProcedureDef)],
) -> BTreeMap<String, NsTree> {
    let mut tree: BTreeMap<String, NsTree> = BTreeMap::new();
    for (_, owning, procedure) in ns_procedures {
        let flat_key = submodule_accessor_key(owning, &procedure.accessor_name);
        let local = procedure.accessor_name.deref().to_case(Case::Camel);
        let segs: Vec<&str> = owning.accessor_path().segments().iter().map(|s| &**s).collect();
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

/// Words that cannot name a variable in an ES module, which is strict mode code.
const RESERVED_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    // Reserved in strict mode code and in modules.
    "await",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
    "arguments",
    "eval",
];

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

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9Builder;
    use spacetimedb_lib::sats::algebraic_value::ser::value_serialize;
    use spacetimedb_lib::sats::{i256, u256, AlgebraicType, ProductValue, SumValue};
    use spacetimedb_lib::{ConnectionId, Identity, ScheduleAt, TimeDuration, Timestamp, Uuid};
    use AlgebraicTypeUse as T;
    use AlgebraicValue as V;

    /// A module with the types `Point { x: i32, y: i32 }`, `Shape { Circle(u32), Empty }`,
    /// `Color { Red, Blue }`, and `Nested { maybe: Option<Option<i32>> }`.
    fn module() -> (ModuleDef, [T; 4]) {
        let mut builder = RawModuleDefV9Builder::new();
        let i32_option_option = AlgebraicType::option(AlgebraicType::option(AlgebraicType::I32));
        let types = [
            (
                "Point",
                AlgebraicType::product([("x", AlgebraicType::I32), ("y", AlgebraicType::I32)]),
            ),
            (
                "Shape",
                AlgebraicType::sum([("Circle", AlgebraicType::U32), ("Empty", AlgebraicType::unit())]),
            ),
            ("Color", AlgebraicType::simple_enum(["Red", "Blue"].into_iter())),
            ("Nested", AlgebraicType::product([("maybe", i32_option_option)])),
        ]
        .map(|(name, ty)| T::Ref(builder.add_algebraic_type([], name, ty, true)));
        (builder.finish().try_into().unwrap(), types)
    }

    fn prim(ty: PrimitiveType) -> T {
        T::Primitive(ty)
    }

    fn option(ty: T) -> T {
        T::Option(ty.into())
    }

    fn array(ty: T) -> T {
        T::Array(ty.into())
    }

    fn result(ok_ty: T, err_ty: T) -> T {
        T::Result {
            ok_ty: ok_ty.into(),
            err_ty: err_ty.into(),
        }
    }

    #[test]
    fn write_value_writes_default_values() {
        use PrimitiveType::*;
        let (module, [point, shape, color, _]) = module();
        let u256_max = "115792089237316195423570985008687907853269984665640564039457584007913129639935n";
        let cases = [
            (prim(Bool), V::Bool(true), "true"),
            (prim(I8), V::I8(i8::MIN), "-128"),
            (prim(U32), V::U32(u32::MAX), "4294967295"),
            (prim(I64), V::I64(i64::MIN), "-9223372036854775808n"),
            (prim(U64), V::U64(u64::MAX), "18446744073709551615n"),
            (
                prim(I128),
                V::I128(i128::MIN.into()),
                "-170141183460469231731687303715884105728n",
            ),
            (
                prim(U128),
                V::U128(u128::MAX.into()),
                "340282366920938463463374607431768211455n",
            ),
            (
                prim(I256),
                V::I256(i256::MIN.into()),
                "-57896044618658097711785492504343953926634992332820282019728792003956564819968n",
            ),
            (prim(U256), V::U256(u256::MAX.into()), u256_max),
            (prim(F32), V::F32(0.1.into()), "0.1"),
            (prim(F64), V::F64(1e300.into()), "1e300"),
            (prim(F64), V::F64((-0.0).into()), "-0.0"),
            (prim(F64), V::F64(f64::NAN.into()), "NaN"),
            (prim(F64), V::F64(f64::INFINITY.into()), "Infinity"),
            (prim(F32), V::F32(f32::NEG_INFINITY.into()), "-Infinity"),
            (
                T::String,
                V::String("q\"b\\l\u{2028}".into()),
                "\"q\\\"b\\\\l\u{2028}\"",
            ),
            (T::Unit, V::unit(), "{}"),
            (
                T::Identity,
                Identity::from_u256(u256::MAX).into(),
                &format!("new __Identity({u256_max})"),
            ),
            (
                T::ConnectionId,
                ConnectionId::from_u128(7).into(),
                "new __ConnectionId(7n)",
            ),
            (
                T::Timestamp,
                Timestamp::from_micros_since_unix_epoch(-1).into(),
                "new __Timestamp(-1n)",
            ),
            (
                T::TimeDuration,
                TimeDuration::from_micros(5).into(),
                "new __TimeDuration(5n)",
            ),
            (T::Uuid, Uuid::from_u128(9).into(), "new __Uuid(9n)"),
            (
                T::ScheduleAt,
                value_serialize(&ScheduleAt::Interval(TimeDuration::from_micros(5))),
                "{ tag: \"Interval\", value: new __TimeDuration(5n) }",
            ),
            (option(T::String), V::OptionSome("x".into()), "\"x\""),
            (option(T::String), V::OptionNone(), "undefined"),
            (option(option(prim(I32))), V::OptionSome(V::OptionSome(V::I32(1))), "1"),
            (result(prim(U32), T::String), V::sum(0, V::U32(7)), "{ ok: 7 }"),
            (result(prim(U32), T::String), V::sum(1, "no".into()), "{ err: \"no\" }"),
            (point.clone(), V::product([V::I32(1), V::I32(-2)]), "{ x: 1, y: -2, }"),
            (shape.clone(), V::sum(0, V::U32(3)), "{ tag: \"Circle\", value: 3 }"),
            (shape, V::sum(1, V::unit()), "{ tag: \"Empty\" }"),
            (color, V::sum(1, V::unit()), "{ tag: \"Blue\" }"),
            (
                array(point),
                V::Array([ProductValue::from([V::I32(0), V::I32(0)])].into()),
                "[{ x: 0, y: 0, }]",
            ),
            (array(prim(U8)), V::Bytes([1, 2].into()), "new Uint8Array([1, 2])"),
            (array(T::String), V::Array(Box::<[Box<str>]>::default().into()), "[]"),
        ];
        let mut imports = BTreeSet::new();
        for (ty, value, expected) in &cases {
            let mut out = String::new();
            write_value(&module, &mut out, ty, value, &mut imports).unwrap();
            assert_eq!(out, *expected, "{ty:?}");
        }
        let classes = [
            "ConnectionId as __ConnectionId",
            "Identity as __Identity",
            "TimeDuration as __TimeDuration",
            "Timestamp as __Timestamp",
            "Uuid as __Uuid",
        ];
        assert_eq!(imports, BTreeSet::from(classes));
    }

    #[test]
    fn defaults_with_some_none_are_omitted() {
        let (module, [_, _, _, nested]) = module();
        let some_none = V::OptionSome(V::OptionNone());
        let cases = [
            (option(option(prim(PrimitiveType::I32))), some_none.clone(), true),
            (option(option(prim(PrimitiveType::I32))), V::OptionNone(), false),
            (
                option(option(prim(PrimitiveType::I32))),
                V::OptionSome(V::OptionSome(V::I32(1))),
                false,
            ),
            (
                array(option(option(T::String))),
                V::Array([SumValue::new(0, V::OptionNone())].into()),
                true,
            ),
            (
                result(option(option(T::String)), T::String),
                V::sum(0, some_none.clone()),
                true,
            ),
            (nested.clone(), V::product([some_none]), true),
            (nested, V::product([V::OptionNone()]), false),
        ];
        for (ty, value, expected) in &cases {
            assert_eq!(has_some_none(&module, ty, value), *expected, "{ty:?} {value:?}");
        }
    }

    #[test]
    fn function_accessor_aliases_match_the_sdk() {
        // The SDK's `toCamelCase` of each input, as `node` computes it.
        for (input, sdk) in [
            ("say_hello", "sayHello"),
            ("set_hp_2x", "setHp2x"),
            ("load_URL_list", "loadURLList"),
            ("__init__", "init_"),
            ("a--b_c", "aBC"),
            ("_1x", "1x"),
            ("ÀBC_d", "àBCD"),
            ("𝒳_y", "𝒳Y"),
            ("PascalName", "pascalName"),
        ] {
            assert_eq!(sdk_to_camel_case(input), sdk, "{input}");
        }

        // A function keeps the accessor that bindings used to derive from its canonical name as an
        // alias, unless the accessor is the same, as for `say_hello`, or another function's
        // accessor, as for `old_name`.
        let aliases = function_accessor_aliases(
            [
                ("grant_item", "giveItem"),
                ("load_URL_list", "load_URL_list"),
                ("say_hello", "say_hello"),
                ("set_hp_2x", "set_hp_2x"),
                ("old_name", "renamed"),
                ("other", "oldName"),
            ]
            .into_iter(),
        );
        let aliases: Vec<_> = aliases.iter().map(|(old, new)| (&**old, &**new)).collect();
        assert_eq!(
            aliases,
            [
                ("grantItem", "giveItem"),
                ("loadURLList", "loadUrlList"),
                ("setHp2x", "setHp2X"),
                ("other", "oldName"),
            ]
        );
    }

    #[test]
    fn export_names() {
        for (accessor, exported) in [
            ("say_hello", Some("export const sayHello = d;")),
            ("delete", Some("const __delete = d;\nexport { __delete as delete };")),
            (
                "require",
                Some("const __require = d;\nexport { __require as require };"),
            ),
            (
                "exports",
                Some("const __exports = d;\nexport { __exports as exports };"),
            ),
            (
                "spacetimedb",
                Some("const __spacetimedb = d;\nexport { __spacetimedb as spacetimedb };"),
            ),
            ("default", None),
            ("_default", None),
            ("café", None),
            ("_1x", None),
        ] {
            assert_eq!(is_export_name(accessor), exported.is_some(), "{accessor}");
            if let Some(exported) = exported {
                let mut out = CodeIndenter::new(String::new(), INDENT);
                write_export(&mut out, accessor, format_args!("d"));
                assert_eq!(out.into_inner().trim_end(), exported);
            }
        }
        for (key, written) in [
            ("person", "person"),
            ("default", "default"),
            ("1x", "\"1x\""),
            ("lib.x", "\"lib.x\""),
        ] {
            assert_eq!(ts_object_key(key), written);
        }
    }
}
