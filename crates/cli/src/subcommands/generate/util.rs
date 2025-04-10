//! Various utility functions that the generate modules have in common.

use std::{
    fmt::{Display, Formatter, Result},
    ops::Deref,
};

use super::code_indenter::Indenter;
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::{db::raw_def::v9::Lifecycle, sats::AlgebraicTypeRef};
use spacetimedb_primitives::ColList;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::ProductTypeDef;
use spacetimedb_schema::{
    def::{IndexDef, TableDef, TypeDef},
    type_for_generate::TypespaceForGenerate,
};
use spacetimedb_schema::{
    def::{ModuleDef, ReducerDef},
    identifier::Identifier,
    type_for_generate::{AlgebraicTypeUse, PrimitiveType},
};

/// Turns a closure `f: Fn(&mut Formatter) -> Result` into `fmt::Display`.
pub(super) fn fmt_fn(f: impl Fn(&mut Formatter) -> Result) -> impl Display {
    struct FDisplay<F>(F);
    impl<F: Fn(&mut Formatter) -> Result> Display for FDisplay<F> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result {
            (self.0)(f)
        }
    }
    FDisplay(f)
}

pub(super) fn collect_case<'a>(case: Case, segs: impl Iterator<Item = &'a Identifier>) -> String {
    segs.map(|s| s.deref().to_case(case)).join(case.delim())
}

pub(super) fn print_lines(output: &mut Indenter, lines: &[&str]) {
    for line in lines {
        writeln!(output, "{line}");
    }
}

pub(super) const AUTO_GENERATED_PREFIX: &str = "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB";

const AUTO_GENERATED_FILE_COMMENT: &[&str] = &[
    "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    "// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.",
    "",
];

pub(super) fn print_auto_generated_file_comment(output: &mut Indenter) {
    print_lines(output, AUTO_GENERATED_FILE_COMMENT);
}

pub(super) fn type_ref_name(module: &ModuleDef, typeref: AlgebraicTypeRef) -> String {
    let (name, _def) = module.type_def_from_ref(typeref).unwrap();
    collect_case(Case::Pascal, name.name_segments())
}

pub(super) fn is_type_filterable(typespace: &TypespaceForGenerate, ty: &AlgebraicTypeUse) -> bool {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => !matches!(prim, PrimitiveType::F32 | PrimitiveType::F64),
        AlgebraicTypeUse::String | AlgebraicTypeUse::Identity | AlgebraicTypeUse::ConnectionId => true,
        // Sum types with all unit variants:
        AlgebraicTypeUse::Never => true,
        AlgebraicTypeUse::Option(inner) => matches!(&**inner, AlgebraicTypeUse::Unit),
        AlgebraicTypeUse::Ref(r) => typespace[r].is_plain_enum(),
        _ => false,
    }
}

pub(super) fn is_reducer_invokable(reducer: &ReducerDef) -> bool {
    reducer.lifecycle.is_none()
}

/// Iterate over all the [`ReducerDef`]s defined by the module, in alphabetical order by name.
///
/// The init reducer is skipped because it should never be visible to the clients.
/// Sorting is not necessary for reducers because they are already stored in an IndexMap.
pub(super) fn iter_reducers(module: &ModuleDef) -> impl Iterator<Item = &ReducerDef> {
    module
        .reducers()
        .filter(|reducer| reducer.lifecycle != Some(Lifecycle::Init))
}

/// Iterate over all the [`TableDef`]s defined by the module, in alphabetical order by name.
///
/// Sorting is necessary to have deterministic reproducable codegen.
pub(super) fn iter_tables(module: &ModuleDef) -> impl Iterator<Item = &TableDef> {
    module.tables().sorted_by_key(|table| &table.name)
}

pub(super) fn iter_unique_cols<'a>(
    typespace: &'a TypespaceForGenerate,
    schema: &'a TableSchema,
    product_def: &'a ProductTypeDef,
) -> impl Iterator<Item = &'a (Identifier, AlgebraicTypeUse)> + 'a {
    let constraints = schema.backcompat_column_constraints();
    schema.columns().iter().filter_map(move |field| {
        constraints[&ColList::from(field.col_pos)]
            .has_unique()
            .then(|| {
                let res @ (_, ref ty) = &product_def.elements[field.col_pos.idx()];
                is_type_filterable(typespace, ty).then_some(res)
            })
            .flatten()
    })
}

pub(super) fn iter_indexes(table: &TableDef) -> impl Iterator<Item = &IndexDef> {
    table.indexes.values().sorted_by_key(|index| &index.name)
}

/// Iterate over all the [`TypeDef`]s defined by the module, in alphabetical order by name.
///
/// Sorting is necessary to have deterministic reproducable codegen.
pub fn iter_types(module: &ModuleDef) -> impl Iterator<Item = &TypeDef> {
    module.types().sorted_by_key(|table| &table.name)
}
