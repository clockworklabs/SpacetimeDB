//! Module-shaped declarations and the compatibility entry point.
use super::*;
use spacetimedb_schema::def::IndexAlgorithm;

pub(super) fn generate(module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
    let mut files = Vec::new();
    generate_module(module, options, "", &mut files);
    files
}

fn generate_module(module: &ModuleDef, options: &CodegenOptions, dir: &str, files: &mut Vec<OutputFile>) {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;
    print_auto_generated_file_comment(out);
    writeln!(
        out,
        "import {{ schema as __schema, table as __table }} from 'spacetimedb/schema';"
    );
    let tables: Vec<_> = iter_tables(module, options.visibility).collect();
    // Private tables needed by schedules remain local metadata, not public accessors.
    let schedule_tables: Vec<_> = module
        .tables()
        .filter(|table| {
            table.schedule.as_ref().is_some_and(|schedule| {
                iter_reducers(module, options.visibility)
                    .any(|r| is_reducer_invokable(r) && local_name(&r.name) == local_name(&schedule.function_name))
                    || iter_procedures(module, options.visibility)
                        .any(|p| local_name(&p.name) == local_name(&schedule.function_name))
            })
        })
        .collect();
    let mut rows: Vec<&TableDef> = tables.clone();
    for table in &schedule_tables {
        if !rows.iter().any(|t| t.name == table.name) {
            rows.push(table);
        }
    }
    for (i, table) in rows.iter().enumerate() {
        let stem = table_module_name(&table.accessor_name);
        writeln!(out, "import __row{i} from './{stem}';");
        files.push(row_file(module, table, format!("{dir}{stem}.ts")));
    }
    for (i, view) in module.views().enumerate() {
        let row = row_file(
            module,
            &TableDef::from(view.clone()),
            format!("{dir}{}.ts", table_module_name(&view.accessor_name)),
        );
        // The schema's view definition supplies the view kind and result cardinality.
        files.push(row);
        writeln!(
            out,
            "import __viewRow{i} from './{}';",
            table_module_name(&view.accessor_name)
        );
    }
    for reducer in iter_reducers(module, options.visibility) {
        let file = TypeScript.generate_reducer_file(module, reducer);
        writeln!(
            out,
            "import {} from './{}';",
            reducer_args_type_name(&reducer.accessor_name),
            reducer_module_name(&reducer.accessor_name)
        );
        files.push(OutputFile {
            filename: format!("{dir}{}", file.filename),
            ..file
        });
    }
    for procedure in iter_procedures(module, options.visibility) {
        let file = TypeScript.generate_procedure_file(module, procedure);
        writeln!(
            out,
            "import * as {} from './{}';",
            procedure_args_type_name(&procedure.accessor_name),
            procedure_module_name(&procedure.accessor_name)
        );
        files.push(OutputFile {
            filename: format!("{dir}{}", file.filename),
            ..file
        });
    }
    writeln!(out, "import {{ t as __t }} from 'spacetimedb';");
    gen_and_print_imports(
        module,
        out,
        module.views().flat_map(|v| v.params_for_generate.element_types()),
        &[],
    );
    for (i, (canonical, _)) in module.submodules().iter().enumerate() {
        writeln!(out, "import * as __child{i} from './{canonical}/declarations';");
    }
    for (i, table) in rows.iter().enumerate() {
        write!(out, "const __table{i} = __table(");
        table_options(out, table);
        writeln!(out, ", __row{i});");
    }
    writeln!(out, "const spacetimedb = __schema({{");
    out.indent(1);
    for table in &tables {
        let i = rows.iter().position(|t| t.name == table.name).unwrap();
        writeln!(
            out,
            "{}: __table{i},",
            ts_string_literal(&table.accessor_name.deref().to_case(Case::Camel))
        );
    }
    for (i, (canonical, child)) in module.submodules().iter().enumerate() {
        let accessor = child.mount_accessor_name().unwrap_or(canonical);
        writeln!(
            out,
            "{}: {{ name: {}, module: __child{i} }},",
            ts_string_literal(accessor),
            ts_string_literal(canonical)
        );
        generate_module(child, options, &format!("{dir}{canonical}/"), files);
    }
    out.dedent(1);
    writeln!(out, "}});");
    writeln!(out, "export default spacetimedb;");
    for (i, reducer) in iter_reducers(module, options.visibility)
        .filter(|r| is_reducer_invokable(r))
        .enumerate()
    {
        let accessor = reducer.accessor_name.deref().to_case(Case::Camel);
        write!(
            out,
            "const __reducer{i} = spacetimedb.reducer({{ name: {}",
            ts_string_literal(local_name(&reducer.name))
        );
        let scheduled = rows.iter().enumerate().find(|(_, t)| {
            t.schedule
                .as_ref()
                .is_some_and(|s| local_name(&s.function_name) == local_name(&reducer.name))
        });
        if let Some((i, _)) = scheduled {
            write!(out, ", onSchedule: __table{i}");
        }
        write!(out, " }}, ");
        if let Some((i, _)) = scheduled {
            write!(
                out,
                "{{ {}: __row{i} }}",
                reducer.params_for_generate.elements[0].0.deref().to_case(Case::Camel)
            );
        } else {
            write!(out, "{}", reducer_args_type_name(&reducer.accessor_name));
        }
        writeln!(out, ");");
        export_function(out, &accessor, &format!("__reducer{i}"));
    }
    for (i, procedure) in iter_procedures(module, options.visibility).enumerate() {
        let args = procedure_args_type_name(&procedure.accessor_name);
        write!(
            out,
            "const __procedure{i} = spacetimedb.procedure({{ name: {}",
            ts_string_literal(local_name(&procedure.name))
        );
        let scheduled = rows.iter().enumerate().find(|(_, t)| {
            t.schedule
                .as_ref()
                .is_some_and(|s| local_name(&s.function_name) == local_name(&procedure.name))
        });
        if let Some((i, _)) = scheduled {
            write!(out, ", onSchedule: __table{i}");
        }
        write!(out, " }}, ");
        if let Some((i, _)) = scheduled {
            write!(
                out,
                "{{ {}: __row{i} }}",
                procedure.params_for_generate.elements[0].0.deref().to_case(Case::Camel)
            );
        } else {
            write!(out, "{args}.params");
        }
        writeln!(out, ", {args}.returnType);");
        export_function(
            out,
            &procedure.accessor_name.deref().to_case(Case::Camel),
            &format!("__procedure{i}"),
        );
    }
    for (i, view) in module.views().enumerate() {
        let kind = if view.is_anonymous { "anonymousView" } else { "view" };
        let cardinality = if matches!(view.return_type_for_generate, AlgebraicTypeUse::Option(_)) {
            "option"
        } else {
            "array"
        };
        write!(
            out,
            "const __view{i} = spacetimedb.{kind}({{ name: {}, public: true }}, ",
            ts_string_literal(local_name(&view.name))
        );
        if !view.params_for_generate.elements.is_empty() {
            writeln!(out, "{{");
            write_object_type_builder_fields(module, out, &view.params_for_generate.elements, None, true, None)
                .unwrap();
            write!(out, "}}, ");
        }
        writeln!(out, "__t.{cardinality}(__viewRow{i}));");
        export_function(
            out,
            &view.accessor_name.deref().to_case(Case::Camel),
            &format!("__view{i}"),
        );
    }
    files.push(OutputFile {
        filename: format!("{dir}declarations.ts"),
        code: output.into_inner(),
    });
    files.push(generate_types_file_with_path(module, format!("{dir}types.ts")));
    // Preserve the existing parameter type import paths.
    let mut reducers = generate_reducers_file(module, options);
    reducers.filename = format!("{dir}{}", reducers.filename);
    files.push(reducers);
    let mut procedures = generate_procedures_file(module, options);
    procedures.filename = format!("{dir}{}", procedures.filename);
    files.push(procedures);
    files.push(entry_point(module, options, dir));
}

fn export_function(out: &mut Indenter, accessor: &str, binding: &str) {
    if accessor == "default" {
        writeln!(
            out,
            "export const __defaultFunction = spacetimedb.exportGroup({{ default: {binding} }});"
        );
    } else {
        writeln!(out, "export {{ {binding} as {accessor} }};");
    }
}

fn local_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap()
}

fn row_file(module: &ModuleDef, table: &TableDef, filename: String) -> OutputFile {
    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;
    print_file_header(out, false, true);
    writeln!(out, "import {{ BinaryReader as __BinaryReader }} from 'spacetimedb';");
    let product = module.typespace_for_generate()[table.product_type_ref]
        .as_product()
        .unwrap();
    gen_and_print_imports(module, out, product.element_types(), &[]);
    writeln!(out, "export default __t.row({{");
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
        write_type_builder(module, out, ty).unwrap();
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
            let bytes = spacetimedb_lib::sats::bsatn::to_vec(value).unwrap();
            write!(out, ".default(");
            write_type_builder(module, out, ty).unwrap();
            write!(out, ".deserialize(new __BinaryReader(new Uint8Array({bytes:?}))))");
        }
        if getter {
            writeln!(out, "; }},");
        } else {
            writeln!(out, ",");
        }
    }
    out.dedent(1);
    writeln!(out, "}});");
    OutputFile {
        filename,
        code: output.into_inner(),
    }
}

fn table_options(out: &mut Indenter, table: &TableDef) {
    writeln!(
        out,
        "{{ name: {}, public: {}, event: {}, indexes: [",
        ts_string_literal(&table.name),
        table.table_access == TableAccess::Public,
        table.is_event
    );
    out.indent(1);
    for index in iter_indexes(table).filter(|i| !i.generated()) {
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
    writeln!(out, "], constraints: [");
    out.indent(1);
    for constraint in iter_constraints(table) {
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
    write!(out, "] }}");
}

fn entry_point(module: &ModuleDef, options: &CodegenOptions, dir: &str) -> OutputFile {
    let mut code = String::new();
    writeln!(code, "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB.").unwrap();
    writeln!(code, "import * as declarations from './declarations';").unwrap();
    writeln!(code, "import {{ createClient, type EventContextInterface, type ReducerEventContextInterface, type SubscriptionEventContextInterface, type ErrorContextInterface, type SubscriptionHandleImpl, type RemoteModuleOfDeclarations }} from 'spacetimedb';").unwrap();
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
    writeln!(code, "type RemoteModule = RemoteModuleOfDeclarations<typeof declarations>;\nexport type DbView = DbConnection['db'];\nexport type Tables = typeof tables;").unwrap();
    for (name, interface) in [
        ("EventContext", "EventContextInterface"),
        ("ReducerEventContext", "ReducerEventContextInterface"),
        ("SubscriptionEventContext", "SubscriptionEventContextInterface"),
        ("ErrorContext", "ErrorContextInterface"),
    ] {
        writeln!(
            code,
            "export type {name} = Omit<{interface}<RemoteModule>, 'db'> & {{ db: DbView }};"
        )
        .unwrap();
    }
    writeln!(
        code,
        "export type SubscriptionHandle = SubscriptionHandleImpl<RemoteModule>;"
    )
    .unwrap();
    // Match the old type namespaces as well as ordinary row-type import paths.
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
