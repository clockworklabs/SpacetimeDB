//! Module-shaped declarations and the compatibility entry point.
use super::*;
use spacetimedb_schema::def::IndexAlgorithm;

pub(super) fn generate(module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
    let mut files = crate::generate_item_files(module, &TypeScript, options);
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
pub(super) fn types_only(module: &ModuleDef) -> OutputFile {
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
