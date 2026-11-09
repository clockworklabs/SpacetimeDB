//! Generates the skeleton of a manual migration, as Rust module code.
//!
//! The output declares the dropped tables of the old schema in a `#[spacetimedb::migration]` module,
//! and a `migrate` function for the user to fill in.
//! Unlike the rest of this crate, the output is *module* code, not client code,
//! and the user is expected to edit it.

use super::{type_ref_name, write_type_in, Indenter, INDENT, MODULE_PATHS};
use crate::code_indenter::CodeIndenter;
use crate::{DroppedSchema, OutputFile};
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_schema::def::{ConstraintData, IndexAlgorithm, IndexDef, ModuleDef, TableDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Deref;

pub(super) fn generate_migration(dropped: &DroppedSchema) -> OutputFile {
    let module = dropped.old;
    let hash = dropped.hash.to_hex();
    let mod_name = format!("migration_{}", &hash[..16]);

    let tables = name_row_structs(module, &dropped.tables);
    let row_types: HashSet<_> = dropped.tables.iter().map(|t| t.product_type_ref).collect();

    // The other types the dropped tables use, which must also be declared in their old shape.
    let mut types = BTreeSet::new();
    for (table, _) in &tables {
        for column in &table.columns {
            collect_type_refs(module, &column.ty_for_generate, &mut types);
        }
    }
    let types = types
        .into_iter()
        .filter(|r| !row_types.contains(r))
        .map(|r| (type_ref_name(module, r), r))
        .collect::<BTreeMap<_, _>>();

    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;

    print_header(out, &hash, &mod_name);
    writeln!(out);
    writeln!(out, "use spacetimedb::MigrationContext;");
    writeln!(out);

    writeln!(out, "#[spacetimedb::migration(");
    writeln!(out, "{INDENT}function = migrate,");
    writeln!(out, "{INDENT}hash = 0x{hash},");
    writeln!(out, ")]");
    if tables.is_empty() {
        writeln!(out, "mod dropped {{}}");
    } else {
        block(out, "mod dropped", |out| {
            let items = tables
                .iter()
                .map(|(table, name)| Item::Table(table, name))
                .chain(types.iter().map(|(name, &r)| Item::Type(name, r)));
            for (i, item) in items.enumerate() {
                if i != 0 {
                    writeln!(out);
                }
                match item {
                    Item::Table(table, name) => define_table(module, out, table, name),
                    Item::Type(name, r) => define_type(module, out, name, r),
                }
            }
        });
    }
    writeln!(out);

    define_migrate_fn(out, &tables, &dropped.removed);

    OutputFile {
        filename: format!("{mod_name}.rs"),
        code: output.into_inner(),
    }
}

enum Item<'a> {
    Table(&'a TableDef, &'a str),
    Type(&'a str, AlgebraicTypeRef),
}

/// Writes `header {`, then the body written by `f` indented one level, then `}`.
fn block(out: &mut Indenter, header: &str, f: impl FnOnce(&mut Indenter)) {
    writeln!(out, "{header} {{");
    f(&mut out.indented(1));
    writeln!(out, "}}");
}

/// Names the row struct of each table.
///
/// A struct in a migration module can only be the row type of one table,
/// so when tables share a row type, the one whose accessor matches the type's name keeps it,
/// and the rest are named after their accessors.
fn name_row_structs<'a>(module: &ModuleDef, tables: &'a [TableDef]) -> Vec<(&'a TableDef, String)> {
    let pascal = |table: &TableDef| table.accessor_name.deref().to_case(Case::Pascal);
    let mut claimed = HashSet::new();
    // Tables whose accessor matches their row type's name claim it first.
    let (matching, rest): (Vec<&TableDef>, Vec<_>) = tables
        .iter()
        .partition(|t| pascal(t) == type_ref_name(module, t.product_type_ref));
    let mut names: BTreeMap<_, _> = matching
        .into_iter()
        .chain(rest)
        .map(|table| {
            let name = if claimed.insert(table.product_type_ref) {
                type_ref_name(module, table.product_type_ref)
            } else {
                pascal(table)
            };
            (&table.name, name)
        })
        .collect();
    tables
        .iter()
        .map(|table| (table, names.remove(&table.name).unwrap()))
        .collect()
}

fn print_header(out: &mut Indenter, hash: &str, mod_name: &str) {
    writeln!(
        out,
        "\
// A migration from the database schema with hash
// {hash}.
//
// Include it in your module with `mod {mod_name};`, then fill in `migrate`.
//
// When this module is published to a database with that schema,
// every table that the new schema removes or changes incompatibly is moved under `dropped`,
// and the new tables are created empty. `migrate` then runs, in the same transaction as the rest
// of the update, to copy the rows worth keeping into the new tables. The dropped tables are
// deleted after it returns. If it fails, the publish is aborted and the database is unchanged.
//
// `mod dropped` declares the old schema of those tables. Don't change it, but you can remove
// tables you don't need to read from."
    );
}

fn define_migrate_fn(out: &mut Indenter, tables: &[(&TableDef, String)], removed: &[Identifier]) {
    writeln!(
        out,
        "\
/// Copies the rows the new schema should keep out of the tables in [`dropped`].
///
/// Anything not copied is lost once this returns."
    );
    if tables.is_empty() {
        block(
            out,
            "fn migrate(_ctx: &MigrationContext<Dropped>) -> Result<(), String>",
            |out| {
                writeln!(out, "// No tables are dropped by this migration.");
                writeln!(out, "Ok(())");
            },
        );
        return;
    }
    let is_removed = |table: &TableDef| removed.contains(&table.name);
    block(
        out,
        "fn migrate(ctx: &MigrationContext<Dropped>) -> Result<(), String>",
        |out| {
            writeln!(out, "// TODO: Copy the rows of each dropped table into the new tables.");
            // Only a changed table has a new version to copy rows into.
            if let Some((example, _)) = tables.iter().find(|(table, _)| !is_removed(table)) {
                let example = ident(&example.accessor_name);
                writeln!(
                    out,
                    "\
// For example, with `spacetimedb::Table` and the new table's accessor in scope:
//
//     for row in ctx.dropped.{example}().iter() {{
//         ctx.db.{example}().insert(..);
//     }}
//"
                );
            }
            writeln!(out, "// Until then, refuse to delete any rows.");
            for (table, _) in tables {
                let accessor = ident(&table.accessor_name);
                let message = if is_removed(table) {
                    format!(
                        "the new schema removes the `{}` table; delete this check to discard its rows",
                        table.accessor_name
                    )
                } else {
                    format!("rows of the old `{}` table have not been migrated", table.accessor_name)
                };
                block(out, &format!("if ctx.dropped.{accessor}().count() != 0"), |out| {
                    writeln!(out, "return Err({message:?}.into());");
                });
            }
            writeln!(out, "Ok(())");
        },
    );
}

/// Declares `table` as a struct with a `#[spacetimedb::table]` attribute.
fn define_table(module: &ModuleDef, out: &mut Indenter, table: &TableDef, struct_name: &str) {
    let col_ident = |col: ColId| ident(&table.columns[col.idx()].accessor_name);

    let unique_cols: HashSet<ColId> = table
        .constraints
        .values()
        .filter_map(|c| match &c.data {
            ConstraintData::Unique(unique) => ColList::from(unique.columns.clone()).as_singleton(),
            _ => None,
        })
        .chain(table.primary_key)
        .collect();
    let auto_inc_cols: HashSet<ColId> = table.sequences.values().map(|s| s.column).collect();

    // Indexes go on their column where they can, like `#[index(btree)]`,
    // and otherwise in the table attribute, like `index(accessor = .., btree(columns = [..]))`.
    let mut column_indexes: BTreeMap<ColId, Vec<&str>> = BTreeMap::new();
    let mut table_indexes = vec![];
    for index in table.indexes.values().sorted_by_key(|index| &index.name) {
        // An index the attribute can't express isn't needed to read the table, so leave it out.
        let Some((kind, cols)) = index_kind_and_columns(index) else {
            continue;
        };
        // Indexes added by the automatic migration come from the new schema,
        // so they may refer to columns the old table doesn't have.
        if cols.iter().any(|col| col.idx() >= table.columns.len()) {
            continue;
        }
        let accessor = index.accessor_name.as_deref();
        if let Some(col) = cols.as_singleton() {
            let col_accessor = &*table.columns[col.idx()].accessor_name;
            if accessor.is_none_or(|a| a == col_accessor) {
                // A btree index on a unique column is created by the `#[unique]` attribute itself.
                if !(kind == "btree" && unique_cols.contains(&col)) {
                    column_indexes.entry(col).or_default().push(kind);
                }
                continue;
            }
        }
        let accessor = accessor
            .map(ident)
            .unwrap_or_else(|| cols.iter().map(col_ident).join("_"));
        let cols = cols.iter().map(col_ident).join(", ");
        let cols = match index.algorithm {
            IndexAlgorithm::Direct(_) => format!("column = [{cols}]"),
            _ => format!("columns = [{cols}]"),
        };
        table_indexes.push(format!("index(accessor = {accessor}, {kind}({cols}))"));
    }

    let mut args = vec![format!("accessor = {}", ident(&table.accessor_name))];
    if table.name != table.accessor_name {
        args.push(format!("name = {:?}", &*table.name));
    }
    if table.table_access == TableAccess::Public {
        args.push("public".into());
    }
    if table.is_event {
        args.push("event".into());
    }
    args.extend(table_indexes);

    let one_line = format!("#[spacetimedb::table({})]", args.join(", "));
    if one_line.len() <= 100 {
        writeln!(out, "{one_line}");
    } else {
        writeln!(out, "#[spacetimedb::table(");
        for arg in &args {
            writeln!(out, "{INDENT}{arg},");
        }
        writeln!(out, ")]");
    }

    block(out, &format!("pub struct {struct_name}"), |out| {
        for column in &table.columns {
            let col = column.col_id;
            if table.primary_key == Some(col) {
                writeln!(out, "#[primary_key]");
            } else if unique_cols.contains(&col) {
                writeln!(out, "#[unique]");
            }
            if auto_inc_cols.contains(&col) {
                writeln!(out, "#[auto_inc]");
            }
            for kind in column_indexes.get(&col).into_iter().flatten() {
                writeln!(out, "#[index({kind})]");
            }
            write!(out, "pub {}: ", ident(&column.accessor_name));
            write_type_in(module, out, &column.ty_for_generate, &MODULE_PATHS).unwrap();
            writeln!(out, ",");
        }
    });
}

fn index_kind_and_columns(index: &IndexDef) -> Option<(&'static str, ColList)> {
    Some(match &index.algorithm {
        IndexAlgorithm::BTree(algo) => ("btree", algo.columns.clone()),
        IndexAlgorithm::Hash(algo) => ("hash", algo.columns.clone()),
        IndexAlgorithm::Direct(algo) => ("direct", algo.column.into()),
        _ => return None,
    })
}

/// Declares the type `r` as a struct or enum deriving `SpacetimeType`.
fn define_type(module: &ModuleDef, out: &mut Indenter, name: &str, r: AlgebraicTypeRef) {
    writeln!(out, "#[derive(spacetimedb::SpacetimeType)]");
    match &module.typespace_for_generate()[r] {
        AlgebraicTypeDef::Product(product) => block(out, &format!("pub struct {name}"), |out| {
            for (field, ty) in product.elements.iter() {
                write!(out, "pub {}: ", ident(&field.deref().to_case(Case::Snake)));
                write_type_in(module, out, ty, &MODULE_PATHS).unwrap();
                writeln!(out, ",");
            }
        }),
        AlgebraicTypeDef::Sum(sum) => block(out, &format!("pub enum {name}"), |out| {
            for (variant, ty) in sum.variants.iter() {
                write!(out, "{}", ident(&variant.deref().to_case(Case::Pascal)));
                if !matches!(ty, AlgebraicTypeUse::Unit) {
                    write!(out, "(");
                    write_type_in(module, out, ty, &MODULE_PATHS).unwrap();
                    write!(out, ")");
                }
                writeln!(out, ",");
            }
        }),
        AlgebraicTypeDef::PlainEnum(plain) => block(out, &format!("pub enum {name}"), |out| {
            for variant in plain.variants.iter() {
                writeln!(out, "{},", ident(&variant.deref().to_case(Case::Pascal)));
            }
        }),
    }
}

/// Adds every type that `ty` refers to, directly or transitively, to `refs`.
fn collect_type_refs(module: &ModuleDef, ty: &AlgebraicTypeUse, refs: &mut BTreeSet<AlgebraicTypeRef>) {
    match ty {
        AlgebraicTypeUse::Ref(r) => {
            if !refs.insert(*r) {
                return;
            }
            match &module.typespace_for_generate()[*r] {
                AlgebraicTypeDef::Product(product) => {
                    for (_, ty) in product.elements.iter() {
                        collect_type_refs(module, ty, refs);
                    }
                }
                AlgebraicTypeDef::Sum(sum) => {
                    for (_, ty) in sum.variants.iter() {
                        collect_type_refs(module, ty, refs);
                    }
                }
                AlgebraicTypeDef::PlainEnum(_) => {}
            }
        }
        AlgebraicTypeUse::Array(elem) | AlgebraicTypeUse::Option(elem) => collect_type_refs(module, elem, refs),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => {
            collect_type_refs(module, ok_ty, refs);
            collect_type_refs(module, err_ty, refs);
        }
        _ => {}
    }
}

/// Renders `name` as a Rust identifier, escaping it if it's a keyword.
fn ident(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn", "else", "enum",
        "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "macro", "match", "mod",
        "move", "mut", "override", "priv", "pub", "ref", "return", "static", "struct", "trait", "true", "try", "type",
        "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
    ];
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_owned()
    }
}
