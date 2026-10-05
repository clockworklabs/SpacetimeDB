//! The Rust backend. It emits the module's declarations in Rust module syntax (proposal 0040):
//! `#[spacetimedb::table]` structs, `#[derive(spacetimedb::SpacetimeType)]` types,
//! and bodiless `#[spacetimedb::reducer]`, `#[spacetimedb::procedure]` and `#[spacetimedb::view]` functions,
//! plus a `mod.rs` that lists them in `spacetimedb::client_module!`.
//! Each file starts with `use spacetimedb_sdk as spacetimedb;`, so the client SDK's macros expand them.
//!
//! The client API is defined by those expansions, in `crates/bindings-macro/src/client.rs`.
//! This backend only has to write declarations that are complete and produce the module's canonical names.

use super::code_indenter::{CodeIndenter, Indenter};
use super::util::{collect_case, iter_reducers, print_lines, type_ref_name};
use super::Lang;
use crate::util::{
    iter_indexes, iter_procedures, iter_tables, iter_types, iter_views, print_auto_generated_file_comment,
    print_auto_generated_version_comment, CodegenVisibility,
};
use crate::CodegenOptions;
use crate::OutputFile;
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::db::raw_def::v9::{Lifecycle, TableAccess};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::{AlgebraicTypeRef, AlgebraicValue, ArrayValue};
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{
    ConstraintData, IndexAlgorithm, ModuleDef, ProcedureDef, ReducerDef, ScopedTypeName, TableDef, TypeDef, ViewDef,
};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};
use std::ops::Deref;

const INDENT: &str = "    ";

/// The arguments of one table's `#[spacetimedb::table(...)]`, and that table's attributes for each column.
type TableArgs = (Vec<String>, BTreeMap<ColId, Vec<String>>);

pub struct Rust;

impl Lang for Rust {
    fn generate_module(&self, module: &ModuleDef, options: &CodegenOptions) -> Option<Vec<OutputFile>> {
        Some(Declarations::new(module, options.visibility).generate())
    }

    // The per-item methods are unused, because `generate_module` generates every file.
    // A row type is declared together with all of its tables, so its file depends on which tables are emitted.

    fn generate_table_file_from_schema(&self, _: &ModuleDef, _: &TableDef, _: TableSchema) -> OutputFile {
        unreachable!("the Rust backend generates every file in `generate_module`")
    }
    fn generate_type_files(&self, _: &ModuleDef, _: &TypeDef) -> Vec<OutputFile> {
        unreachable!("the Rust backend generates every file in `generate_module`")
    }
    fn generate_reducer_file(&self, _: &ModuleDef, _: &ReducerDef) -> OutputFile {
        unreachable!("the Rust backend generates every file in `generate_module`")
    }
    fn generate_procedure_file(&self, _: &ModuleDef, _: &ProcedureDef) -> OutputFile {
        unreachable!("the Rust backend generates every file in `generate_module`")
    }
    fn generate_global_files(&self, _: &ModuleDef, _: &CodegenOptions) -> Vec<OutputFile> {
        unreachable!("the Rust backend generates every file in `generate_module`")
    }
}

/// The declarations of a module, and the file that declares each type.
struct Declarations<'a> {
    module: &'a ModuleDef,
    visibility: CodegenVisibility,
    /// The emitted tables of each row type, in order of table name.
    tables_by_row: BTreeMap<AlgebraicTypeRef, Vec<&'a TableDef>>,
    /// The omitted tables of each row type, such as private tables without `--include-private`, in order of table name.
    /// A row type's declaration still carries their `#[table]` attributes, marked `omitted`.
    omitted_tables_by_row: BTreeMap<AlgebraicTypeRef, Vec<&'a TableDef>>,
    /// The Rust module that declares each type.
    type_modules: BTreeMap<AlgebraicTypeRef, String>,
}

impl<'a> Declarations<'a> {
    fn new(module: &'a ModuleDef, visibility: CodegenVisibility) -> Self {
        let mut tables_by_row = BTreeMap::<_, Vec<_>>::new();
        for table in iter_tables(module, visibility) {
            tables_by_row.entry(table.product_type_ref).or_default().push(table);
        }
        let mut omitted_tables_by_row = BTreeMap::<_, Vec<_>>::new();
        for table in iter_tables(module, CodegenVisibility::IncludePrivate) {
            let emitted = tables_by_row.get(&table.product_type_ref);
            if !emitted.is_some_and(|tables| tables.iter().any(|t| std::ptr::eq(*t, table))) {
                omitted_tables_by_row
                    .entry(table.product_type_ref)
                    .or_default()
                    .push(table);
            }
        }
        let mut type_modules = BTreeMap::new();
        for typ in iter_types(module) {
            let module_name = match tables_by_row.get(&typ.ty) {
                Some(tables) => table_module_name(&row_owner(module, typ.ty, tables).accessor_name),
                None => type_module_name(&typ.accessor_name),
            };
            type_modules.insert(typ.ty, module_name);
        }
        Self {
            module,
            visibility,
            tables_by_row,
            omitted_tables_by_row,
            type_modules,
        }
    }

    /// The omitted tables of the row type `row`.
    fn omitted_tables(&self, row: AlgebraicTypeRef) -> &[&'a TableDef] {
        self.omitted_tables_by_row.get(&row).map_or(&[], |tables| &tables[..])
    }

    /// The types that are not the row type of an emitted table.
    fn plain_types(&self) -> impl Iterator<Item = &'a TypeDef> + '_ {
        iter_types(self.module).filter(|typ| !self.tables_by_row.contains_key(&typ.ty))
    }

    /// The tables that declare a row type, with all of the row type's emitted tables, in order of file name.
    fn table_declarations(&self) -> impl Iterator<Item = (&'a TableDef, &[&'a TableDef])> + '_ {
        self.tables_by_row
            .iter()
            .map(|(&row, tables)| (row_owner(self.module, row, tables), &tables[..]))
            .sorted_by_key(|(owner, _)| &owner.name)
    }

    fn generate(&self) -> Vec<OutputFile> {
        let mut files = vec![];
        for typ in self.plain_types() {
            files.push(OutputFile {
                filename: type_module_name(&typ.accessor_name) + ".rs",
                code: self.type_file(typ),
            });
        }
        for (owner, tables) in self.table_declarations() {
            let module_name = table_module_name(&owner.accessor_name);
            files.push(OutputFile {
                code: self.table_file(&module_name, tables, self.omitted_tables(owner.product_type_ref)),
                filename: module_name + ".rs",
            });
        }
        for view in iter_views(self.module) {
            files.push(OutputFile {
                filename: table_module_name(&view.accessor_name) + ".rs",
                code: self.view_file(view),
            });
        }
        for reducer in iter_reducers(self.module, self.visibility) {
            files.push(OutputFile {
                filename: reducer_module_name(reducer) + ".rs",
                code: self.reducer_file(reducer),
            });
        }
        for procedure in iter_procedures(self.module, self.visibility) {
            files.push(OutputFile {
                filename: procedure_module_name(procedure) + ".rs",
                code: self.procedure_file(procedure),
            });
        }
        files.push(OutputFile {
            filename: "mod.rs".to_string(),
            code: self.mod_file(),
        });
        files
    }

    /// Start the file of the Rust module `this_module`: the header,
    /// and a `use` item for each type in `roots` that another file declares.
    fn file_header<'t>(
        &self,
        out: &mut Indenter,
        this_module: &str,
        roots: impl IntoIterator<Item = &'t AlgebraicTypeUse>,
    ) {
        print_file_header(out, false);
        let mut imports = BTreeSet::new();
        for root in roots {
            root.for_each_ref(|r| {
                imports.insert(r);
            });
        }
        let imports = imports
            .into_iter()
            .map(|r| (&self.type_modules[&r], type_ref_name(self.module, r)))
            .filter(|(module_name, _)| *module_name != this_module)
            .collect::<BTreeSet<_>>();
        if !imports.is_empty() {
            out.newline();
        }
        for (module_name, type_name) in imports {
            writeln!(out, "use super::{module_name}::{type_name};");
        }
        out.newline();
    }

    fn type_file(&self, typ: &TypeDef) -> String {
        // The row type of omitted tables only, which is declared with their `#[table]` attributes.
        let omitted = self.omitted_tables(typ.ty);
        if !omitted.is_empty() {
            return self.table_file(&type_module_name(&typ.accessor_name), &[], omitted);
        }
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;
        let type_name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
        let ty = &self.module.typespace_for_generate()[typ.ty];
        let roots: Vec<&AlgebraicTypeUse> = match ty {
            AlgebraicTypeDef::Product(product) => product.elements.iter().map(|(_, ty)| ty).collect(),
            AlgebraicTypeDef::Sum(sum) => sum.variants.iter().map(|(_, ty)| ty).collect(),
            AlgebraicTypeDef::PlainEnum(_) => vec![],
        };
        self.file_header(out, &type_module_name(&typ.accessor_name), roots);

        match ty {
            AlgebraicTypeDef::Product(product) => {
                writeln!(out, "#[derive(spacetimedb::SpacetimeType, Clone, PartialEq, Debug)]");
                print_type_name_attr(out, &typ.accessor_name, &type_name);
                write!(out, "pub struct {type_name} ");
                // The canonical field names, which the typespace has after case conversion.
                let canonical_names: Vec<Option<&str>> = self.module.typespace()[typ.ty]
                    .as_product()
                    .map(|product| product.elements.iter().map(|e| e.name().map(|n| &**n)).collect())
                    .unwrap_or_default();
                out.delimited_block(
                    "{",
                    |out| {
                        for (i, (ident, ty)) in product.elements.iter().enumerate() {
                            let field = field_name(ident);
                            if let Some(Some(name)) = canonical_names.get(i)
                                && let Some(attr) = column_name_attr(&field, name)
                            {
                                writeln!(out, "#[{attr}]");
                            }
                            writeln!(out, "pub {field}: {},", self.type_name(ty));
                        }
                    },
                    "}\n",
                );
            }
            AlgebraicTypeDef::Sum(sum) => {
                writeln!(out, "#[derive(spacetimedb::SpacetimeType, Clone, PartialEq, Debug)]");
                print_type_name_attr(out, &typ.accessor_name, &type_name);
                write!(out, "pub enum {type_name} ");
                out.delimited_block(
                    "{",
                    |out| {
                        for (ident, ty) in &sum.variants {
                            match ty {
                                AlgebraicTypeUse::Unit => writeln!(out, "{},", variant_name(ident)),
                                ty => writeln!(out, "{}({}),", variant_name(ident), self.type_name(ty)),
                            }
                        }
                    },
                    "}\n",
                );
            }
            AlgebraicTypeDef::PlainEnum(plain) => {
                writeln!(
                    out,
                    "#[derive(spacetimedb::SpacetimeType, Clone, Copy, PartialEq, Eq, Hash, Debug)]"
                );
                print_type_name_attr(out, &typ.accessor_name, &type_name);
                write!(out, "pub enum {type_name} ");
                out.delimited_block(
                    "{",
                    |out| {
                        for ident in &plain.variants {
                            writeln!(out, "{},", variant_name(ident));
                        }
                    },
                    "}\n",
                );
            }
        }
        output.into_inner()
    }

    /// A row type with all of its tables: `#[spacetimedb::table(...)]` for each table, then the struct.
    /// The attributes of the `omitted` tables come last, marked `omitted`.
    fn table_file(&self, this_module: &str, emitted: &[&TableDef], omitted: &[&TableDef]) -> String {
        let tables: Vec<&TableDef> = emitted.iter().chain(omitted).copied().collect();
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;
        // Tables that share a row type have the same columns, but may differ in their column attributes.
        let first = tables[0];
        self.file_header(out, this_module, first.columns.iter().map(|c| &c.ty_for_generate));

        // The first `#[table]` determines the row's `Cols` and `IxCols`.
        // It is the first emitted table by name, or the first omitted one if every table is omitted.
        let tables_args: Vec<_> = tables.iter().map(|table| self.table_args(table)).collect();
        for (i, (args, _)) in tables_args.iter().enumerate() {
            let omitted = if i < emitted.len() { "" } else { ", omitted" };
            writeln!(out, "#[spacetimedb::table({}{omitted})]", args.join(", "));
        }
        let (type_name, typ) = self.type_def(first.product_type_ref);
        print_type_name_attr(out, &typ.accessor_name, &type_name);
        write!(out, "pub struct {type_name} ");
        out.delimited_block(
            "{",
            |out| {
                for column in &first.columns {
                    for attr in row_column_attrs(&tables, &tables_args, column.col_id) {
                        writeln!(out, "#[{attr}]");
                    }
                    writeln!(
                        out,
                        "pub {}: {},",
                        field_name(&column.accessor_name),
                        self.type_name(&column.ty_for_generate)
                    );
                }
            },
            "}\n",
        );
        output.into_inner()
    }

    /// The arguments of `#[spacetimedb::table(...)]` for `table`,
    /// and the column attributes of `table`, such as `primary_key`, for each column.
    fn table_args(&self, table: &TableDef) -> TableArgs {
        let mut args = vec![];
        let accessor = table_method_name(&table.accessor_name);
        args.push(format!("accessor = {accessor}"));
        if canonical(&accessor) != *table.name {
            args.push(format!("name = {:?}", table.name.deref()));
        }
        if table.table_access == TableAccess::Public {
            args.push("public".to_string());
        }
        if table.is_event {
            args.push("event".to_string());
        }
        if let Some(schedule) = &table.schedule {
            let function = self
                .module
                .reducers()
                .find(|r| r.name.deref() == schedule.function_name.deref())
                .map(|r| r.accessor_name.deref().to_case(Case::Snake))
                .or_else(|| {
                    self.module
                        .procedures()
                        .find(|p| p.name == schedule.function_name)
                        .map(|p| p.accessor_name.deref().to_case(Case::Snake))
                })
                .unwrap_or_else(|| schedule.function_name.deref().to_case(Case::Snake));
            let at = self.column_field_name(table, schedule.at_column);
            if at == "scheduled_at" {
                args.push(format!("scheduled({function})"));
            } else {
                args.push(format!("scheduled({function}, at = {at})"));
            }
        }

        let unique_cols: BTreeSet<ColId> = table
            .constraints
            .values()
            .filter_map(|c| match &c.data {
                ConstraintData::Unique(unique) => unique.columns.as_singleton(),
                _ => None,
            })
            .collect();
        let mut column_attrs: BTreeMap<ColId, Vec<String>> = BTreeMap::new();
        // An index without an accessor can repeat another index's attribute on the same field.
        let mut push_attr = |col: ColId, attr: String| {
            let attrs = column_attrs.entry(col).or_default();
            if !attrs.contains(&attr) {
                attrs.push(attr);
            }
        };
        for column in &table.columns {
            if let Some(attr) = column_name_attr(&field_name(&column.accessor_name), &column.name) {
                push_attr(column.col_id, attr);
            }
        }
        for col in table.columns.iter().map(|c| c.col_id) {
            if table.primary_key == Some(col) {
                push_attr(col, "primary_key".to_string());
            } else if unique_cols.contains(&col) {
                push_attr(col, "unique".to_string());
            }
        }
        for sequence in table.sequences.values().sorted_by_key(|s| s.column) {
            push_attr(sequence.column, "auto_inc".to_string());
        }
        for index in iter_indexes(table) {
            let index_accessor = index.accessor_name.as_ref().map(|a| a.deref().to_case(Case::Snake));
            let (kind, columns) = match &index.algorithm {
                IndexAlgorithm::BTree(btree) => ("btree", btree.columns.iter().collect::<Vec<_>>()),
                IndexAlgorithm::Hash(hash) => ("hash", hash.columns.iter().collect()),
                IndexAlgorithm::Direct(direct) => ("direct", vec![direct.column]),
                _ => continue,
            };
            let default_name = format!(
                "{}_{}_idx_{kind}",
                table.name.deref(),
                columns
                    .iter()
                    .map(|&col| table.get_column(col).unwrap().name.deref())
                    .join("_")
            );
            let explicit_name = (*index.name != *default_name).then_some(&index.name);
            if let [col] = &columns[..]
                && match &index_accessor {
                    Some(accessor) => explicit_name.is_none() && self.column_field_name(table, *col) == *accessor,
                    None => true,
                }
            {
                // An index on one column, named after it, is declared on the field.
                // So is one without an accessor, which the client uses only for the query builder's `IxCols`.
                // A unique column's btree index is implied by `#[unique]` or `#[primary_key]`.
                let implied = kind == "btree" && (table.primary_key == Some(*col) || unique_cols.contains(col));
                if !implied {
                    push_attr(*col, format!("index({kind})"));
                }
                continue;
            }
            // `IxCols` lists only indexes on one column, so an index on several columns without an accessor is left out.
            let Some(index_accessor) = index_accessor else {
                continue;
            };
            let field_names = columns.iter().map(|&col| self.column_field_name(table, col)).join(", ");
            let algorithm = match kind {
                "direct" => format!("direct(column = [{field_names}])"),
                kind => format!("{kind}(columns = [{field_names}])"),
            };
            let name = explicit_name
                .map(|name| format!("name = {:?}, ", name.deref()))
                .unwrap_or_default();
            args.push(format!("index(accessor = {index_accessor}, {name}{algorithm})"));
        }
        for column in &table.columns {
            // Module syntax doesn't allow a default on a primary key, unique or auto-increment column,
            // so it is left out there; the client does not use defaults.
            let col = column.col_id;
            if table.primary_key == Some(col)
                || unique_cols.contains(&col)
                || table.sequences.values().any(|s| s.column == col)
            {
                continue;
            }
            if let Some(default) = &column.default_value {
                // A default value that has no Rust expression here is left out. See `value_expr`.
                if let Some(expr) = self.value_expr(&column.ty_for_generate, default) {
                    push_attr(column.col_id, format!("default({expr})"));
                }
            }
        }
        (args, column_attrs)
    }

    fn view_file(&self, view: &ViewDef) -> String {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;
        let row = AlgebraicTypeUse::Ref(view.product_type_ref);
        self.file_header(out, &table_module_name(&view.accessor_name), [&row]);

        let accessor = table_method_name(&view.accessor_name);
        let mut args = vec![format!("accessor = {accessor}")];
        if canonical(&accessor) != *view.name {
            args.push(format!("name = {:?}", view.name.deref()));
        }
        if view.is_public {
            args.push("public".to_string());
        }
        if let Some(pk) = view.primary_key {
            args.push(format!(
                "primary_key = {}",
                field_name(&view.return_columns[pk.idx()].accessor_name)
            ));
        }
        let ctx = if view.is_anonymous {
            "spacetimedb::AnonymousViewContext"
        } else {
            "spacetimedb::ViewContext"
        };
        let ret = if view.is_procedural() {
            self.type_name(&view.return_type_for_generate)
        } else {
            format!("impl spacetimedb::Query<{}>", self.type_name(&row))
        };
        writeln!(out, "#[spacetimedb::view({})]", args.join(", "));
        writeln!(out, "pub fn {accessor}(ctx: &{ctx}) -> {ret};");
        output.into_inner()
    }

    fn reducer_file(&self, reducer: &ReducerDef) -> String {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;
        let params = &reducer.params_for_generate.elements;
        self.file_header(out, &reducer_module_name(reducer), params.iter().map(|(_, ty)| ty));

        let func_name = reducer_function_name(reducer);
        let mut args = vec![];
        match reducer.lifecycle {
            Some(Lifecycle::Init) => args.push("init".to_string()),
            Some(Lifecycle::OnConnect) => args.push("client_connected".to_string()),
            Some(Lifecycle::OnDisconnect) => args.push("client_disconnected".to_string()),
            _ => {}
        }
        if canonical(&func_name) != *reducer.name.deref() {
            args.push(format!("name = {:?}", reducer.name.deref()));
        }
        print_function_attr(out, "reducer", &args);
        let ctx = context_param_name(params);
        writeln!(
            out,
            "pub fn {func_name}({ctx}: &spacetimedb::ReducerContext{});",
            self.params(params)
        );
        output.into_inner()
    }

    fn procedure_file(&self, procedure: &ProcedureDef) -> String {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;
        let params = &procedure.params_for_generate.elements;
        self.file_header(
            out,
            &procedure_module_name(procedure),
            params
                .iter()
                .map(|(_, ty)| ty)
                .chain([&procedure.return_type_for_generate]),
        );

        let func_name = procedure_function_name(procedure);
        let mut args = vec![];
        if canonical(&func_name) != *procedure.name {
            args.push(format!("name = {:?}", procedure.name.deref()));
        }
        print_function_attr(out, "procedure", &args);
        let ret = match &procedure.return_type_for_generate {
            AlgebraicTypeUse::Unit => String::new(),
            ty => format!(" -> {}", self.type_name(ty)),
        };
        let ctx = context_param_name(params);
        writeln!(
            out,
            "pub fn {func_name}({ctx}: &mut spacetimedb::ProcedureContext{}){ret};",
            self.params(params)
        );
        output.into_inner()
    }

    fn mod_file(&self) -> String {
        let mut output = CodeIndenter::new(String::new(), INDENT);
        let out = &mut output;
        print_file_header(out, true);
        out.newline();

        let types: Vec<_> = self.plain_types().collect();
        let tables: Vec<_> = self.table_declarations().collect();
        let views: Vec<_> = iter_views(self.module).collect();
        let reducers: Vec<_> = iter_reducers(self.module, self.visibility).collect();
        let procedures: Vec<_> = iter_procedures(self.module, self.visibility).collect();

        // Declare `pub mod` for each of the files generated.
        let module_names = itertools::chain!(
            types.iter().map(|typ| type_module_name(&typ.accessor_name)),
            tables.iter().map(|(owner, _)| table_module_name(&owner.accessor_name)),
            views.iter().map(|view| table_module_name(&view.accessor_name)),
            reducers.iter().map(|r| reducer_module_name(r)),
            procedures.iter().map(|p| procedure_module_name(p)),
        );
        for module_name in module_names {
            writeln!(out, "pub mod {module_name};");
        }
        out.newline();

        // Re-export the declarations and the client API they expand to.
        for typ in &types {
            let type_name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
            writeln!(out, "pub use {}::{type_name};", type_module_name(&typ.accessor_name));
        }
        for (owner, _) in &tables {
            writeln!(out, "pub use {}::*;", table_module_name(&owner.accessor_name));
        }
        for view in &views {
            writeln!(out, "pub use {}::*;", table_module_name(&view.accessor_name));
        }
        for reducer in &reducers {
            writeln!(
                out,
                "pub use {}::{};",
                reducer_module_name(reducer),
                reducer_function_name(reducer)
            );
        }
        for procedure in &procedures {
            writeln!(
                out,
                "pub use {}::{};",
                procedure_module_name(procedure),
                procedure_function_name(procedure)
            );
        }
        out.newline();

        // Generate the module-wide items, such as `DbConnection`, `RemoteTables` and the `Reducer` enum.
        let mut sections: Vec<(&str, Vec<String>)> = vec![];
        sections.push((
            "types",
            types
                .iter()
                .map(|typ| {
                    let type_name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
                    format!("{}::{type_name}", type_module_name(&typ.accessor_name))
                })
                .collect(),
        ));
        // A view is a table to the client. `client_module!` invokes row callbacks and lists `ALL_TABLE_NAMES`
        // in the order of its tables, so the tables and views are listed together, in order of accessor name.
        let table_entries = tables.iter().flat_map(|(owner, tables)| {
            let module_name = table_module_name(&owner.accessor_name);
            tables.iter().map(move |t| (&t.accessor_name, module_name.clone()))
        });
        let view_entries = views
            .iter()
            .map(|view| (&view.accessor_name, table_module_name(&view.accessor_name)));
        sections.push((
            "tables",
            table_entries
                .chain(view_entries)
                .sorted_by_key(|(accessor, _)| *accessor)
                .map(|(accessor, module_name)| format!("{module_name}::{}", table_method_name(accessor)))
                .collect(),
        ));
        sections.push((
            "reducers",
            reducers
                .iter()
                .map(|reducer| {
                    let params = &reducer.params_for_generate.elements;
                    let params = if params.is_empty() {
                        String::new()
                    } else {
                        format!("({})", params.iter().map(|(ident, _)| field_name(ident)).join(", "))
                    };
                    format!(
                        "{}::{}{params}",
                        reducer_module_name(reducer),
                        reducer_function_name(reducer)
                    )
                })
                .collect(),
        ));
        sections.push((
            "procedures",
            procedures
                .iter()
                .map(|p| format!("{}::{}", procedure_module_name(p), procedure_function_name(p)))
                .collect(),
        ));
        // `delimited_block` does not indent nested blocks, and rustfmt does not format macro invocations.
        writeln!(out, "spacetimedb::client_module! {{");
        for (section, entries) in &sections {
            if entries.is_empty() {
                continue;
            }
            writeln!(out, "{INDENT}{section}: [");
            for entry in entries {
                writeln!(out, "{INDENT}{INDENT}{entry},");
            }
            writeln!(out, "{INDENT}],");
        }
        writeln!(out, "}}");
        output.into_inner()
    }

    fn type_def(&self, r: AlgebraicTypeRef) -> (String, &'a TypeDef) {
        let typ = self
            .module
            .types()
            .find(|t| t.ty == r)
            .expect("row type should be declared");
        (type_ref_name(self.module, r), typ)
    }

    fn column_field_name(&self, table: &TableDef, col: ColId) -> String {
        field_name(&table.get_column(col).unwrap().accessor_name)
    }

    /// `, name: ty, name: ty` for function parameters after the context.
    fn params(&self, params: &[(Identifier, AlgebraicTypeUse)]) -> String {
        params
            .iter()
            .map(|(ident, ty)| format!(", {}: {}", field_name(ident), self.type_name(ty)))
            .collect()
    }

    fn type_name(&self, ty: &AlgebraicTypeUse) -> String {
        let mut s = String::new();
        self.write_type(&mut s, ty).unwrap();
        s
    }

    fn write_type(&self, out: &mut impl Write, ty: &AlgebraicTypeUse) -> fmt::Result {
        match ty {
            AlgebraicTypeUse::Unit => write!(out, "()")?,
            AlgebraicTypeUse::Never => write!(out, "std::convert::Infallible")?,
            AlgebraicTypeUse::Identity => write!(out, "spacetimedb::Identity")?,
            AlgebraicTypeUse::ConnectionId => write!(out, "spacetimedb::ConnectionId")?,
            AlgebraicTypeUse::Timestamp => write!(out, "spacetimedb::Timestamp")?,
            AlgebraicTypeUse::TimeDuration => write!(out, "spacetimedb::TimeDuration")?,
            AlgebraicTypeUse::Uuid => write!(out, "spacetimedb::Uuid")?,
            AlgebraicTypeUse::ScheduleAt => write!(out, "spacetimedb::ScheduleAt")?,
            AlgebraicTypeUse::Option(inner_ty) => {
                write!(out, "Option<")?;
                self.write_type(out, inner_ty)?;
                write!(out, ">")?;
            }
            AlgebraicTypeUse::Result { ok_ty, err_ty } => {
                write!(out, "Result<")?;
                self.write_type(out, ok_ty)?;
                write!(out, ", ")?;
                self.write_type(out, err_ty)?;
                write!(out, ">")?;
            }
            AlgebraicTypeUse::Primitive(prim) => match prim {
                PrimitiveType::Bool => write!(out, "bool")?,
                PrimitiveType::I8 => write!(out, "i8")?,
                PrimitiveType::U8 => write!(out, "u8")?,
                PrimitiveType::I16 => write!(out, "i16")?,
                PrimitiveType::U16 => write!(out, "u16")?,
                PrimitiveType::I32 => write!(out, "i32")?,
                PrimitiveType::U32 => write!(out, "u32")?,
                PrimitiveType::I64 => write!(out, "i64")?,
                PrimitiveType::U64 => write!(out, "u64")?,
                PrimitiveType::I128 => write!(out, "i128")?,
                PrimitiveType::U128 => write!(out, "u128")?,
                PrimitiveType::I256 => write!(out, "spacetimedb::i256")?,
                PrimitiveType::U256 => write!(out, "spacetimedb::u256")?,
                PrimitiveType::F32 => write!(out, "f32")?,
                PrimitiveType::F64 => write!(out, "f64")?,
            },
            AlgebraicTypeUse::String => write!(out, "String")?,
            AlgebraicTypeUse::Array(elem_ty) => {
                write!(out, "Vec<")?;
                self.write_type(out, elem_ty)?;
                write!(out, ">")?;
            }
            AlgebraicTypeUse::Ref(r) => {
                let name = type_ref_name(self.module, *r);
                // The client's `#[table]` takes these names for the SDK's types, unless the path starts with `self`.
                // See `is_filterable_type` in `crates/bindings-macro/src/client.rs`.
                if ["Uuid", "Timestamp", "TimeDuration", "ScheduleAt"].contains(&&*name) {
                    write!(out, "self::")?;
                }
                write!(out, "{name}")?
            }
        }
        Ok(())
    }

    /// A Rust expression for `value`, for `#[default(...)]`.
    /// Returns `None` for a value that has no simple expression, such as an `Identity` or an `i256`.
    fn value_expr(&self, ty: &AlgebraicTypeUse, value: &AlgebraicValue) -> Option<String> {
        Some(match (ty, value) {
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::Bool(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::I8(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::U8(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::I16(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::U16(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::I32(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::U32(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::I64(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::U64(v)) => v.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::I128(v)) => { v.0 }.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::U128(v)) => { v.0 }.to_string(),
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::F32(v)) if v.into_inner().is_finite() => {
                format!("{:?}", v.into_inner())
            }
            (AlgebraicTypeUse::Primitive(_), AlgebraicValue::F64(v)) if v.into_inner().is_finite() => {
                format!("{:?}", v.into_inner())
            }
            (AlgebraicTypeUse::String, AlgebraicValue::String(v)) => format!("{v:?}"),
            (AlgebraicTypeUse::Option(inner), AlgebraicValue::Sum(sum)) => match sum.tag {
                0 => format!("Some({})", self.value_expr(inner, &sum.value)?),
                _ => "None".to_string(),
            },
            (AlgebraicTypeUse::Array(elem), AlgebraicValue::Array(array)) => format!(
                "vec![{}]",
                array_elements(array)
                    .iter()
                    .map(|v| self.value_expr(elem, v))
                    .collect::<Option<Vec<_>>>()?
                    .join(", ")
            ),
            (AlgebraicTypeUse::Ref(r), value) => {
                let type_name = type_ref_name(self.module, *r);
                match (&self.module.typespace_for_generate()[*r], value) {
                    (AlgebraicTypeDef::PlainEnum(plain), AlgebraicValue::Sum(sum)) => {
                        format!("{type_name}::{}", variant_name(plain.variants.get(sum.tag as usize)?))
                    }
                    (AlgebraicTypeDef::Sum(def), AlgebraicValue::Sum(sum)) => {
                        let (ident, ty) = def.variants.get(sum.tag as usize)?;
                        match ty {
                            AlgebraicTypeUse::Unit => format!("{type_name}::{}", variant_name(ident)),
                            ty => format!(
                                "{type_name}::{}({})",
                                variant_name(ident),
                                self.value_expr(ty, &sum.value)?
                            ),
                        }
                    }
                    (AlgebraicTypeDef::Product(def), AlgebraicValue::Product(product)) => format!(
                        "{type_name} {{ {} }}",
                        def.elements
                            .iter()
                            .zip(&product.elements)
                            .map(|((ident, ty), v)| Some(format!("{}: {}", field_name(ident), self.value_expr(ty, v)?)))
                            .collect::<Option<Vec<_>>>()?
                            .join(", ")
                    ),
                    _ => return None,
                }
            }
            _ => return None,
        })
    }
}

fn array_elements(array: &ArrayValue) -> Vec<AlgebraicValue> {
    array.iter_cloned().collect()
}

/// The table whose file declares a row type: the table named after the type, or else the first by name.
fn row_owner<'a>(module: &ModuleDef, row: AlgebraicTypeRef, tables: &[&'a TableDef]) -> &'a TableDef {
    let type_name = type_ref_name(module, row);
    tables
        .iter()
        .find(|t| t.accessor_name.deref().to_case(Case::Pascal) == type_name)
        .unwrap_or(&tables[0])
}

/// The canonical name the client expansions derive from an accessor name.
/// Must match `canonical` in `crates/bindings-macro/src/client.rs`.
/// A declaration whose canonical name differs states it with `name = "..."`.
fn canonical(accessor: &str) -> String {
    accessor.to_case(Case::Snake)
}

/// `name("...")` for the field `field`, if the column's canonical name differs from the one the client derives,
/// as under `CaseConversionPolicy::None`. Module syntax has no column name yet:
/// this is proposal 0032's form, which only the client expansions accept.
fn column_name_attr(field: &str, canonical_name: &str) -> Option<String> {
    (canonical(field) != canonical_name).then(|| format!("name({canonical_name:?})"))
}

/// The attributes of the column `col` of a row type, merged from the column attributes of each of its tables.
///
/// Tables that share a row type can differ in their column attributes: C# and C++ modules declare
/// primary keys, unique constraints, sequences and indexes per table. An attribute that only some of the tables have
/// gets proposal 0022's `table = ...` modifier, such as `#[primary_key(table = player)]`.
/// A default cannot be declared per table, so it is left out unless every table has it;
/// the client does not use defaults.
fn row_column_attrs(tables: &[&TableDef], tables_args: &[TableArgs], col: ColId) -> Vec<String> {
    // Each attribute, with the accessors of the tables that have it, in order of first appearance.
    let mut attrs: Vec<(&String, Vec<String>)> = vec![];
    for (table, (_, column_attrs)) in tables.iter().zip(tables_args) {
        for attr in column_attrs.get(&col).into_iter().flatten() {
            let accessor = table_method_name(&table.accessor_name);
            match attrs.iter_mut().find(|(a, _)| *a == attr) {
                Some((_, accessors)) => accessors.push(accessor),
                None => attrs.push((attr, vec![accessor])),
            }
        }
    }
    attrs
        .into_iter()
        .filter_map(|(attr, accessors)| {
            if accessors.len() == tables.len() {
                return Some(attr.clone());
            }
            if attr.starts_with("default(") {
                return None;
            }
            let active = match &accessors[..] {
                [accessor] => accessor.clone(),
                accessors => format!("[{}]", accessors.join(", ")),
            };
            // `index(btree)` becomes `index(btree, table = ...)`, and `unique` becomes `unique(table = ...)`.
            Some(match attr.strip_suffix(')') {
                Some(args) => format!("{args}, table = {active})"),
                None => format!("{attr}(table = {active})"),
            })
        })
        .collect()
}

/// `#[sats(name = "...")]`, when the type's name in the module is not its Rust name,
/// such as for a type in a namespace.
fn print_type_name_attr(out: &mut Indenter, name: &ScopedTypeName, rust_name: &str) {
    let full_name = name.name_segments().join(".");
    if full_name != rust_name {
        writeln!(out, "#[sats(name = {full_name:?})]");
    }
}

fn print_function_attr(out: &mut Indenter, kind: &str, args: &[String]) {
    if args.is_empty() {
        writeln!(out, "#[spacetimedb::{kind}]");
    } else {
        writeln!(out, "#[spacetimedb::{kind}({})]", args.join(", "));
    }
}

/// The context parameter's name, which must differ from the other parameters' names.
fn context_param_name(params: &[(Identifier, AlgebraicTypeUse)]) -> &'static str {
    if params.iter().any(|(ident, _)| field_name(ident) == "ctx") {
        "_ctx"
    } else {
        "ctx"
    }
}

const ALLOW_LINTS: &str = "#![allow(unused, clippy::all)]";

const SPACETIMEDB_IMPORTS: &[&str] = &["use spacetimedb_sdk as spacetimedb;"];

fn print_file_header(output: &mut Indenter, include_version: bool) {
    print_auto_generated_file_comment(output);
    if include_version {
        print_auto_generated_version_comment(output);
    }
    writeln!(output, "{ALLOW_LINTS}");
    print_lines(output, SPACETIMEDB_IMPORTS);
}

fn field_name(ident: &Identifier) -> String {
    ident.deref().to_case(Case::Snake)
}

fn variant_name(ident: &Identifier) -> String {
    ident.deref().to_case(Case::Pascal)
}

fn type_module_name(type_name: &ScopedTypeName) -> String {
    collect_case(Case::Snake, type_name.name_segments()) + "_type"
}

fn table_module_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake) + "_table"
}

fn table_method_name(table_name: &Identifier) -> String {
    table_name.deref().to_case(Case::Snake)
}

fn reducer_module_name(reducer: &ReducerDef) -> String {
    reducer.accessor_name.deref().to_case(Case::Snake) + "_reducer"
}

fn reducer_function_name(reducer: &ReducerDef) -> String {
    reducer.accessor_name.deref().to_case(Case::Snake)
}

fn procedure_module_name(procedure: &ProcedureDef) -> String {
    procedure.accessor_name.deref().to_case(Case::Snake) + "_procedure"
}

fn procedure_function_name(procedure: &ProcedureDef) -> String {
    procedure.accessor_name.deref().to_case(Case::Snake)
}
