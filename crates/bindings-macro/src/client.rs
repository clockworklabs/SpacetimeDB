//! Client-side expansions of the module macros, for proposal 0040 (module-shaped client bindings).
//!
//! SPIKE: this is a prototype, not production code.
//!
//! In a client build, `spacetimedb::table`, `spacetimedb::reducer` and `spacetimedb::client_module`
//! resolve to the entry points in this module, re-exported from `spacetimedb-sdk`.
//!
//! - `#[table]` and `#[reducer]` expand at the declaration's location to the per-item client API,
//!   the same code that `crates/codegen/src/rust.rs` writes into `{table}_table.rs` and `{reducer}_reducer.rs`.
//!   They never name the module-wide types. Where codegen writes `super::RemoteTables` or `super::EventContext`,
//!   these expansions write projections through `<Row as InModule>::Module`.
//! - `client_module!` lists the module's items and generates the module-wide items, the same code that codegen
//!   writes into `mod.rs`. It links each item to the module by implementing `InModule` for it.
//!
//! The arguments of `#[table]` and `#[reducer]` are parsed by the same code as the server expansion
//! (`TableArgs::parse`, `ColumnAttr::parse`, `ReducerArgs::parse`), so both accept exactly the same syntax.
//!
//! The templates below are ported from `crates/codegen/src/rust.rs` with as few changes as possible,
//! so that they can be compared with the originals. They are string templates, parsed into tokens at the end.
//! The changes are:
//! - inputs are names and type strings from the declaration's tokens instead of a `ModuleDef`;
//! - `super::X` for a module-wide type `X` becomes a projection (`super_*` helpers below);
//! - `{table}_table::register_table` and friends become methods of `ClientTableDef` / `ClientReducerDef`;
//! - `__sdk::` and friends are spelled `spacetimedb::__codegen::` (see `qualify`), because an expansion
//!   cannot add `use` items to the user's module without risking duplicate imports.

use crate::reducer::ReducerArgs;
use crate::table::{ColumnAttr, IndexType, TableArgs};
use convert_case::{Case, Casing};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use std::fmt::Write;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, Token};

// ---------------------------------------------------------------------------------------------------------------------
// Names. These must match codegen's naming functions, and the server's canonical names.
// ---------------------------------------------------------------------------------------------------------------------

/// The canonical name derived from an accessor name under the default `CaseConversionPolicy::SnakeCase`.
/// Must match `spacetimedb_schema::def::validate::v9::convert`, which also uses `convert_case`.
fn canonical(accessor: &str) -> String {
    accessor.to_case(Case::Snake)
}

fn pascal(accessor: &str) -> String {
    accessor.to_case(Case::Pascal)
}

/// codegen: `table_name_pascalcase + "TableAccessor"`. `client_module!` relies on this name to find the table.
fn table_accessor_marker(accessor: &str) -> String {
    pascal(accessor) + "TableAccessor"
}

/// codegen: `function_args_type_name`. `client_module!` relies on this name to find the reducer.
fn args_type_name(accessor: &str) -> String {
    pascal(accessor) + "Args"
}

/// The hidden alias for a reducer parameter's type, which `client_module!` uses for the `Reducer` enum's fields.
/// The alias is defined at the declaration's location, so the type resolves there.
fn param_alias_name(args_type: &str, param: &str) -> String {
    format!("__{args_type}_{param}")
}

/// Spell out the `__sdk`, `__lib`, `__sats` and `__ws` aliases that codegen imports at the top of every file.
fn qualify(code: String) -> String {
    code.replace("__ws::", "spacetimedb::__codegen::__ws::")
        .replace("__sats::", "spacetimedb::__codegen::__sats::")
        .replace("__lib::", "spacetimedb::__codegen::__lib::")
        .replace("crate = __lib)", "crate = spacetimedb::__codegen::__lib)")
        .replace("__sdk::", "spacetimedb::__codegen::")
}

fn parse_code(code: String) -> syn::Result<TokenStream> {
    let code = qualify(code);
    code.parse::<TokenStream>()
        .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), format!("spike template did not lex: {e}")))
}

fn tokens_to_string(t: &impl ToTokens) -> String {
    t.to_token_stream().to_string()
}

/// `super::RemoteModule`, seen from a per-item expansion anchored at `anchor`.
fn super_module(anchor: &str) -> String {
    format!("<{anchor} as __sdk::InModule>::Module")
}

/// `super::{assoc}`, for one of the associated types of `SpacetimeModule`, seen from a per-item expansion.
fn super_assoc(anchor: &str, assoc: &str) -> String {
    format!("<{} as __sdk::SpacetimeModule>::{assoc}", super_module(anchor))
}

// ---------------------------------------------------------------------------------------------------------------------
// `#[table]`
// ---------------------------------------------------------------------------------------------------------------------

struct ColumnModel {
    field: String,
    canonical: String,
    ty: String,
}

struct TableModel {
    /// The canonical table name.
    table_name: String,
    accessor: String,
    row_type: String,
    is_event: bool,
    columns: Vec<ColumnModel>,
    primary_key: Option<usize>,
    /// Indices into `columns`, in column order.
    unique: Vec<usize>,
    /// Indices into `columns` with a single-column index, in column order.
    single_col_indexes: Vec<usize>,
}

pub(crate) fn client_table(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut item: syn::DeriveInput = syn::parse2(item)?;
    let mut args = TableArgs::parse(args, &item.ident)?;

    let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(fields),
        ..
    }) = &mut item.data
    else {
        return Err(syn::Error::new_spanned(&item, "spacetimedb table must be a struct with named fields"));
    };

    let mut columns = vec![];
    let mut primary_key = None;
    let mut unique = vec![];
    for (i, field) in fields.named.iter_mut().enumerate() {
        let ident = field.ident.clone().unwrap();
        let mut is_unique = false;
        let mut keep = vec![];
        for attr in std::mem::take(&mut field.attrs) {
            match ColumnAttr::parse(&attr, &ident)? {
                // Column attributes only describe the server's schema; strip them from the client struct.
                Some(ColumnAttr::Unique(_)) => is_unique = true,
                Some(ColumnAttr::PrimaryKey(_)) => {
                    is_unique = true;
                    primary_key = Some(i);
                }
                Some(ColumnAttr::Index(index)) => args.indices.push(index),
                Some(ColumnAttr::AutoInc(_)) | Some(ColumnAttr::Default(..)) => {}
                None => keep.push(attr),
            }
        }
        field.attrs = keep;
        if is_unique {
            unique.push(i);
        }
        let field_name = ident.unraw().to_string();
        columns.push(ColumnModel {
            canonical: canonical(&field_name),
            field: field_name,
            ty: tokens_to_string(&field.ty),
        });
    }

    // The server creates a btree index for every unique column that no explicit index covers.
    // The query builder exposes every single-column index, so collect them like `table_impl` does.
    let mut single_col_indexes: Vec<usize> = unique.clone();
    for index in &args.indices {
        let col = match &index.kind {
            IndexType::BTree { columns } | IndexType::Hash { columns } if columns.len() == 1 => &columns[0],
            IndexType::Direct { column } => column,
            _ => continue,
        };
        let pos = columns
            .iter()
            .position(|c| *col == c.field)
            .ok_or_else(|| syn::Error::new(col.span(), "not a column of the table"))?;
        if !single_col_indexes.contains(&pos) {
            single_col_indexes.push(pos);
        }
    }
    single_col_indexes.sort();

    let accessor = args.accessor.unraw().to_string();
    let model = TableModel {
        table_name: args.name.as_ref().map(|n| n.value()).unwrap_or_else(|| canonical(&accessor)),
        accessor,
        row_type: item.ident.to_string(),
        is_event: args.event.is_some(),
        columns,
        primary_key,
        unique,
        single_col_indexes,
    };

    // A row type can carry several `#[table]` attributes. The first one to expand adds the derives and the
    // query-builder column types, which belong to the row type rather than the table, and marks the struct
    // with `#[sats(crate = spacetimedb::__codegen::__lib)]`. Later ones skip them.
    let first_table_on_row = !item
        .attrs
        .iter()
        .any(|a| a.path().is_ident("sats") && a.to_token_stream().to_string().contains("__codegen"));

    // codegen: `STRUCT_DERIVES`. Only add the derives the declaration doesn't already have,
    // because module source often derives `Clone` or `Debug` itself.
    let existing_derives = existing_derives(&item.attrs)?;
    let mut derives: Vec<TokenStream> = vec![
        quote!(spacetimedb::__codegen::__lib::ser::Serialize),
        quote!(spacetimedb::__codegen::__lib::de::Deserialize),
    ];
    for std_derive in ["Clone", "PartialEq", "Debug"] {
        if !existing_derives.iter().any(|d| d == std_derive) {
            let ident = Ident::new(std_derive, proc_macro2::Span::call_site());
            derives.push(quote!(#ident));
        }
    }

    let mut code = String::new();
    if first_table_on_row {
        implement_query_col_types_for_table_struct(&mut code, &model);
    }
    generate_table(&mut code, &model);
    implement_query_table_accessor(&mut code, &model);
    let per_table = parse_code(code)?;

    let row_attrs = first_table_on_row.then(|| {
        quote! {
            #[derive(#(#derives),*)]
            #[sats(crate = spacetimedb::__codegen::__lib)]
        }
    });
    Ok(quote! {
        #row_attrs
        #item

        #per_table
    })
}

/// The last path segment of every `#[derive(...)]` entry on the item.
fn existing_derives(attrs: &[syn::Attribute]) -> syn::Result<Vec<String>> {
    let mut out = vec![];
    for attr in attrs.iter().filter(|a| a.path().is_ident("derive")) {
        let paths = attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)?;
        out.extend(paths.iter().filter_map(|p| p.segments.last().map(|s| s.ident.to_string())));
    }
    Ok(out)
}

/// Ported from codegen's `implement_query_col_types_for_struct` and `implement_query_col_types_for_table_struct`,
/// which codegen writes into `{type}_type.rs`.
fn implement_query_col_types_for_table_struct(out: &mut String, t: &TableModel) {
    let struct_name = &t.row_type;
    let cols_struct = struct_name.clone() + "Cols";
    writeln!(
        out,
        "
/// Column accessor struct for the table `{struct_name}`.
///
/// Provides typed access to columns for query building.
pub struct {cols_struct} {{"
    )
    .unwrap();
    for c in &t.columns {
        let (field_name, field_type) = (&c.field, &c.ty);
        writeln!(
            out,
            "    pub {field_name}: __sdk::__query_builder::Col<{struct_name}, {field_type}>,"
        )
        .unwrap();
    }
    writeln!(out, "}}").unwrap();
    writeln!(
        out,
        "
impl __sdk::__query_builder::HasCols for {struct_name} {{
    type Cols = {cols_struct};
    fn cols(table_name: &'static str) -> Self::Cols {{
        {cols_struct} {{"
    )
    .unwrap();
    for c in &t.columns {
        let (field_name, col_name) = (&c.field, &c.canonical);
        writeln!(
            out,
            "            {field_name}: __sdk::__query_builder::Col::new(table_name, {col_name:?}),"
        )
        .unwrap();
    }
    writeln!(out, "        }}\n    }}\n}}").unwrap();

    let cols_ix = struct_name.clone() + "IxCols";
    writeln!(
        out,
        "
/// Indexed column accessor struct for the table `{struct_name}`.
///
/// Provides typed access to indexed columns for query building.
pub struct {cols_ix} {{"
    )
    .unwrap();
    for &i in &t.single_col_indexes {
        let c = &t.columns[i];
        let (field_name, field_type) = (&c.field, &c.ty);
        writeln!(
            out,
            "    pub {field_name}: __sdk::__query_builder::IxCol<{struct_name}, {field_type}>,",
        )
        .unwrap();
    }
    writeln!(out, "}}").unwrap();
    writeln!(
        out,
        "
impl __sdk::__query_builder::HasIxCols for {struct_name} {{
    type IxCols = {cols_ix};
    fn ix_cols(table_name: &'static str) -> Self::IxCols {{
        {cols_ix} {{"
    )
    .unwrap();
    for &i in &t.single_col_indexes {
        let c = &t.columns[i];
        let (field_name, col_name) = (&c.field, &c.canonical);
        writeln!(
            out,
            "            {field_name}: __sdk::__query_builder::IxCol::new(table_name, {col_name:?}),",
        )
        .unwrap();
    }
    writeln!(out, "        }}\n    }}\n}}").unwrap();

    // Event tables cannot be used as lookup tables in semijoins.
    if !t.is_event {
        writeln!(
            out,
            "\nimpl __sdk::__query_builder::CanBeLookupTable for {struct_name} {{}}"
        )
        .unwrap();
    }
}

/// Ported from codegen's `generate_table_file_from_schema`.
fn generate_table(out: &mut String, t: &TableModel) {
    let row_type = &t.row_type;
    let table_name = &t.table_name;
    let table_name_pascalcase = pascal(&t.accessor);
    let table_handle = table_name_pascalcase.clone() + "TableHandle";
    let table_accessor = table_accessor_marker(&t.accessor);
    let insert_callback_id = table_name_pascalcase.clone() + "InsertCallbackId";
    let delete_callback_id = table_name_pascalcase.clone() + "DeleteCallbackId";
    let accessor_trait = table_name_pascalcase.clone() + "TableAccess";
    let accessor_method = &t.accessor;

    // codegen writes `super::RemoteTables`, `super::EventContext` and `super::RemoteModule`.
    let super_remote_tables = super_assoc(row_type, "DbView");
    let super_event_context = super_assoc(row_type, "EventContext");

    write!(
        out,
        "
/// Table handle for the table `{table_name}`.
///
/// Obtain a handle from the [`{accessor_trait}::{accessor_method}`] method on `RemoteTables`,
/// like `ctx.db.{accessor_method}()`.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.{accessor_method}().on_insert(...)`.
#[allow(dead_code)]
pub struct {table_handle}<'ctx> {{
    imp: __sdk::TableHandle<{row_type}>,
    ctx: std::marker::PhantomData<&'ctx {super_remote_tables}>,
}}

/// Lifetime-aware accessor marker for the table `{table_name}`.
pub struct {table_accessor};

impl __sdk::TableAccessor<{super_remote_tables}> for {table_accessor} {{
    type Row = {row_type};
    type Handle<'db> = {table_handle}<'db>;

    fn get<'db>(db: &'db {super_remote_tables}) -> Self::Handle<'db> {{
        db.{accessor_method}()
    }}
}}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `{table_name}`.
///
/// Implemented for `RemoteTables`.
pub trait {accessor_trait} {{
    #[allow(non_snake_case)]
    /// Obtain a [`{table_handle}`], which mediates access to the table `{table_name}`.
    fn {accessor_method}(&self) -> {table_handle}<'_>;
}}

impl {accessor_trait} for {super_remote_tables} {{
    fn {accessor_method}(&self) -> {table_handle}<'_> {{
        {table_handle} {{
            imp: __sdk::HasDbContextImpl::db_context_impl(self).get_table::<{row_type}>({table_name:?}),
            ctx: std::marker::PhantomData,
        }}
    }}
}}

pub struct {insert_callback_id}(__sdk::CallbackId);
"
    )
    .unwrap();

    if t.is_event {
        write!(
            out,
            "
impl<'ctx> __sdk::TableLike for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {super_event_context};

    fn count(&self) -> u64 {{ self.imp.count() }}
    fn iter(&self) -> impl Iterator<Item = {row_type}> + '_ {{ self.imp.iter() }}
}}

impl<'ctx> __sdk::EventTable for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {super_event_context};

    fn count(&self) -> u64 {{ self.imp.count() }}
    fn iter(&self) -> impl Iterator<Item = {row_type}> + '_ {{ self.imp.iter() }}

    type InsertCallbackId = {insert_callback_id};

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {insert_callback_id} {{
        {insert_callback_id}(self.imp.on_insert(Box::new(callback)))
    }}

    fn remove_on_insert(&self, callback: {insert_callback_id}) {{
        self.imp.remove_on_insert(callback.0)
    }}
}}

impl<'ctx> __sdk::WithInsert for {table_handle}<'ctx> {{
    type InsertCallbackId = {insert_callback_id};

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {insert_callback_id} {{
        {insert_callback_id}(self.imp.on_insert(Box::new(callback)))
    }}

    fn remove_on_insert(&self, callback: {insert_callback_id}) {{
        self.imp.remove_on_insert(callback.0)
    }}
}}
"
        )
        .unwrap();
    } else {
        write!(
            out,
            "pub struct {delete_callback_id}(__sdk::CallbackId);

impl<'ctx> __sdk::TableLike for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {super_event_context};

    fn count(&self) -> u64 {{ self.imp.count() }}
    fn iter(&self) -> impl Iterator<Item = {row_type}> + '_ {{ self.imp.iter() }}
}}

impl<'ctx> __sdk::Table for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {super_event_context};

    fn count(&self) -> u64 {{ self.imp.count() }}
    fn iter(&self) -> impl Iterator<Item = {row_type}> + '_ {{ self.imp.iter() }}

    type InsertCallbackId = {insert_callback_id};

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {insert_callback_id} {{
        {insert_callback_id}(self.imp.on_insert(Box::new(callback)))
    }}

    fn remove_on_insert(&self, callback: {insert_callback_id}) {{
        self.imp.remove_on_insert(callback.0)
    }}

    type DeleteCallbackId = {delete_callback_id};

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {delete_callback_id} {{
        {delete_callback_id}(self.imp.on_delete(Box::new(callback)))
    }}

    fn remove_on_delete(&self, callback: {delete_callback_id}) {{
        self.imp.remove_on_delete(callback.0)
    }}
}}

impl<'ctx> __sdk::WithInsert for {table_handle}<'ctx> {{
    type InsertCallbackId = {insert_callback_id};

    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {insert_callback_id} {{
        {insert_callback_id}(self.imp.on_insert(Box::new(callback)))
    }}

    fn remove_on_insert(&self, callback: {insert_callback_id}) {{
        self.imp.remove_on_insert(callback.0)
    }}
}}

impl<'ctx> __sdk::WithDelete for {table_handle}<'ctx> {{
    type DeleteCallbackId = {delete_callback_id};

    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + 'static,
    ) -> {delete_callback_id} {{
        {delete_callback_id}(self.imp.on_delete(Box::new(callback)))
    }}

    fn remove_on_delete(&self, callback: {delete_callback_id}) {{
        self.imp.remove_on_delete(callback.0)
    }}
}}
"
        )
        .unwrap();

        if t.primary_key.is_some() {
            let update_callback_id = table_name_pascalcase.clone() + "UpdateCallbackId";
            write!(
                out,
                "
pub struct {update_callback_id}(__sdk::CallbackId);

impl<'ctx> __sdk::TableWithPrimaryKey for {table_handle}<'ctx> {{
    type UpdateCallbackId = {update_callback_id};

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> {update_callback_id} {{
        {update_callback_id}(self.imp.on_update(Box::new(callback)))
    }}

    fn remove_on_update(&self, callback: {update_callback_id}) {{
        self.imp.remove_on_update(callback.0)
    }}
}}

impl<'ctx> __sdk::WithUpdate for {table_handle}<'ctx> {{
    type UpdateCallbackId = {update_callback_id};

    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + 'static,
    ) -> {update_callback_id} {{
        {update_callback_id}(self.imp.on_update(Box::new(callback)))
    }}

    fn remove_on_update(&self, callback: {update_callback_id}) {{
        self.imp.remove_on_update(callback.0)
    }}
}}
"
            )
            .unwrap();
        }

        for &i in &t.unique {
            let c = &t.columns[i];
            let unique_field_name = &c.field;
            let unique_field_name_pascalcase = pascal(unique_field_name);
            let unique_constraint = table_name_pascalcase.clone() + &unique_field_name_pascalcase + "Unique";
            let unique_field_type = &c.ty;
            write!(
                out,
                "
        /// Access to the `{unique_field_name}` unique index on the table `{table_name}`,
        /// which allows point queries on the field of the same name
        /// via the [`{unique_constraint}::find`] method.
        ///
        /// Users are encouraged not to explicitly reference this type,
        /// but to directly chain method calls,
        /// like `ctx.db.{accessor_method}().{unique_field_name}().find(...)`.
        #[allow(dead_code)]
        pub struct {unique_constraint}<'ctx> {{
            imp: __sdk::UniqueConstraintHandle<{row_type}, {unique_field_type}>,
            phantom: std::marker::PhantomData<&'ctx {super_remote_tables}>,
        }}

        impl<'ctx> {table_handle}<'ctx> {{
            /// Get a handle on the `{unique_field_name}` unique index on the table `{table_name}`.
            pub fn {unique_field_name}(&self) -> {unique_constraint}<'ctx> {{
                {unique_constraint} {{
                    imp: self.imp.get_unique_constraint::<{unique_field_type}>({unique_field_name:?}),
                    phantom: std::marker::PhantomData,
                }}
            }}
        }}

        impl<'ctx> {unique_constraint}<'ctx> {{
            /// Find the subscribed row whose `{unique_field_name}` column value is equal to `col_val`,
            /// if such a row is present in the client cache.
            pub fn find(&self, col_val: &{unique_field_type}) -> Option<{row_type}> {{
                self.imp.find(col_val)
            }}
        }}
        "
            )
            .unwrap();
        }
    }

    // codegen writes `register_table` and `parse_table_update` as `pub(super)` free functions
    // that `mod.rs` calls by path. Here they are methods of `ClientTableDef`, so `client_module!` can find them
    // from anywhere, along with the row type and the code that codegen inlines into `DbUpdate::apply_to_client_cache`.
    let mut register_body = format!("let _table = client_cache.get_or_make_table::<{row_type}>({table_name:?});\n");
    for &i in &t.unique {
        let c = &t.columns[i];
        let (unique_field_name, unique_field_type) = (&c.field, &c.ty);
        writeln!(
            register_body,
            "_table.add_unique_constraint::<{unique_field_type}>({unique_field_name:?}, |row| &row.{unique_field_name});",
        )
        .unwrap();
    }
    let apply_body = if t.is_event {
        // Event tables bypass the client cache entirely.
        "update.into_event_diff()".to_string()
    } else {
        let with_updates = t
            .primary_key
            .map(|i| format!(".with_updates_by_pk(|row| &row.{})", t.columns[i].field))
            .unwrap_or_default();
        format!("cache.apply_diff_to_table::<{row_type}>({table_name:?}, update){with_updates}")
    };
    write!(
        out,
        "
#[doc(hidden)]
impl __sdk::ClientTableDef for {table_accessor} {{
    type Row = {row_type};
    const NAME: &'static str = {table_name:?};

    fn register_table<M: __sdk::SpacetimeModule>(client_cache: &mut __sdk::ClientCache<M>)
    where
        Self::Row: __sdk::InModule<Module = M> + Send + Sync + 'static,
    {{
        {register_body}
    }}

    fn parse_table_update(
        raw_updates: __ws::v2::TableUpdate,
    ) -> __sdk::Result<__sdk::TableUpdate<{row_type}>> {{
        __sdk::TableUpdate::parse_table_update(raw_updates).map_err(|e| {{
            __sdk::InternalError::failed_parse(
                \"TableUpdate<{row_type}>\",
                \"TableUpdate\",
            ).with_cause(e).into()
        }})
    }}

    fn apply_diff<'r, M: __sdk::SpacetimeModule>(
        cache: &mut __sdk::ClientCache<M>,
        update: &'r __sdk::TableUpdate<{row_type}>,
    ) -> __sdk::TableAppliedDiff<'r, {row_type}>
    where
        Self::Row: __sdk::InModule<Module = M> + Clone + std::fmt::Debug + Send + Sync + 'static,
    {{
        let _ = &cache;
        {apply_body}
    }}
}}
"
    )
    .unwrap();
}

/// Ported from codegen's `implement_query_table_accessor`.
fn implement_query_table_accessor(out: &mut String, t: &TableModel) {
    let accessor_method = &t.accessor;
    let table_name = &t.table_name;
    let struct_name = &t.row_type;
    let query_accessor_trait = accessor_method.to_string() + "QueryTableAccess";
    writeln!(
        out,
        "
        #[allow(non_camel_case_types)]
        /// Extension trait for query builder access to the table `{struct_name}`.
        ///
        /// Implemented for [`__sdk::QueryTableAccessor`].
        pub trait {query_accessor_trait} {{
            #[allow(non_snake_case)]
            /// Get a query builder for the table `{struct_name}`.
            fn {accessor_method}(&self) -> __sdk::__query_builder::Table<{struct_name}>;
        }}

        impl {query_accessor_trait} for __sdk::QueryTableAccessor {{
            fn {accessor_method}(&self) -> __sdk::__query_builder::Table<{struct_name}> {{
                __sdk::__query_builder::Table::new({table_name:?})
            }}
        }}
"
    )
    .unwrap();
}

// ---------------------------------------------------------------------------------------------------------------------
// `#[reducer]`
// ---------------------------------------------------------------------------------------------------------------------

/// A free function declaration with or without a body: `pub fn add(ctx: &ReducerContext, name: String);`.
/// `syn::ItemFn` requires a body, so parse the pieces by hand.
struct MaybeBodilessFn {
    sig: syn::Signature,
}

impl Parse for MaybeBodilessFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _attrs = input.call(syn::Attribute::parse_outer)?;
        let _vis: syn::Visibility = input.parse()?;
        let sig: syn::Signature = input.parse()?;
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        } else {
            // The client discards the body, so it is never name-resolved or type-checked.
            input.parse::<syn::Block>()?;
        }
        Ok(Self { sig })
    }
}

struct ReducerModel {
    reducer_name: String,
    func_name: String,
    args_type: String,
    enum_variant_name: String,
    params: Vec<(String, String)>,
}

pub(crate) fn client_reducer(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args = ReducerArgs::parse(args)?;
    let MaybeBodilessFn { sig } = syn::parse2(item)?;

    // The first parameter is the `ReducerContext`, which the client does not pass.
    let mut params = vec![];
    for arg in sig.inputs.iter().skip(1) {
        let syn::FnArg::Typed(pat_ty) = arg else {
            return Err(syn::Error::new_spanned(arg, "expected typed argument"));
        };
        let syn::Pat::Ident(pat) = &*pat_ty.pat else {
            return Err(syn::Error::new_spanned(&pat_ty.pat, "reducer parameters must be identifiers"));
        };
        params.push((pat.ident.unraw().to_string(), tokens_to_string(&pat_ty.ty)));
    }

    let func_name = sig.ident.unraw().to_string();
    let model = ReducerModel {
        reducer_name: args.name.as_ref().map(|n| n.value()).unwrap_or_else(|| canonical(&func_name)),
        args_type: args_type_name(&func_name),
        enum_variant_name: pascal(&func_name),
        func_name,
        params,
    };

    let mut code = String::new();
    generate_reducer(&mut code, &model);
    parse_code(code)
}

/// Ported from codegen's `generate_reducer_file`.
fn generate_reducer(out: &mut String, r: &ReducerModel) {
    let reducer_name = &r.reducer_name;
    let func_name = &r.func_name;
    let args_type = &r.args_type;
    let enum_variant_name = &r.enum_variant_name;
    let super_reducer_event_context = super_assoc(args_type, "ReducerEventContext");
    let super_remote_reducers = super_assoc(args_type, "Reducers");
    let super_reducer = super_assoc(args_type, "Reducer");

    // codegen: `define_struct_for_product(.., "pub(super)")`. `pub(crate)` because `client_module!`
    // may be in any module of the crate.
    let mut fields = String::new();
    let mut arglist_no_delimiters = String::new();
    let mut arg_names = String::new();
    let mut variant_fields = String::new();
    let mut aliases = String::new();
    for (name, ty) in &r.params {
        writeln!(fields, "pub {name}: {ty},").unwrap();
        writeln!(arglist_no_delimiters, "{name}: {ty},").unwrap();
        write!(arg_names, "{name}, ").unwrap();
        writeln!(variant_fields, "{name}: args.{name},").unwrap();
        let alias = param_alias_name(args_type, name);
        writeln!(
            aliases,
            "#[doc(hidden)] #[allow(non_camel_case_types)] pub(crate) type {alias} = {ty};"
        )
        .unwrap();
    }
    let variant_ctor = if r.params.is_empty() {
        format!("Self::{enum_variant_name}")
    } else {
        format!("Self::{enum_variant_name} {{ {variant_fields} }}")
    };
    let variant_pattern = if r.params.is_empty() {
        format!("R::{enum_variant_name}")
    } else {
        format!("R::{enum_variant_name} {{ {arg_names} }}")
    };
    let clone_fields: String = r.params.iter().map(|(n, _)| format!("{n}: {n}.clone(), ")).collect();

    write!(
        out,
        "
#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
#[doc(hidden)]
pub(crate) struct {args_type} {{
    {fields}
}}

{aliases}

impl From<{args_type}> for {super_reducer} {{
    #[allow(unused_variables)]
    fn from(args: {args_type}) -> Self {{
        {variant_ctor}
    }}
}}

#[doc(hidden)]
impl __sdk::ClientReducerDef for {args_type} {{
    const NAME: &'static str = {reducer_name:?};

    #[allow(clippy::clone_on_copy, unreachable_patterns)]
    fn from_reducer(reducer: &{super_reducer}) -> Option<Self> {{
        type R = {super_reducer};
        match reducer {{
            {variant_pattern} => Some(Self {{ {clone_fields} }}),
            _ => None,
        }}
    }}
}}

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `{reducer_name}`.
///
/// Implemented for `RemoteReducers`.
pub trait {func_name} {{
    /// Request that the remote module invoke the reducer `{reducer_name}` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and this method provides no way to listen for its completion status.
    /// /// Use [`{func_name}:{func_name}_then`] to run a callback after the reducer completes.
    fn {func_name}(&self, {arglist_no_delimiters}) -> __sdk::Result<()> {{
        self.{func_name}_then({arg_names} |_, _| {{}})
    }}

    /// Request that the remote module invoke the reducer `{reducer_name}` to run as soon as possible,
    /// registering `callback` to run when we are notified that the reducer completed.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed with the `callback`.
    fn {func_name}_then(
        &self,
        {arglist_no_delimiters}
        callback: impl FnOnce(&{super_reducer_event_context}, Result<Result<(), String>, __sdk::InternalError>)
            + Send
            + 'static,
    ) -> __sdk::Result<()>;
}}

impl {func_name} for {super_remote_reducers} {{
    fn {func_name}_then(
        &self,
        {arglist_no_delimiters}
        callback: impl FnOnce(&{super_reducer_event_context}, Result<Result<(), String>, __sdk::InternalError>)
            + Send
            + 'static,
    ) -> __sdk::Result<()> {{
        __sdk::HasDbContextImpl::db_context_impl(self).invoke_reducer_with_callback({args_type} {{ {arg_names} }}, callback)
    }}
}}
"
    )
    .unwrap();
}

// ---------------------------------------------------------------------------------------------------------------------
// `client_module!`
// ---------------------------------------------------------------------------------------------------------------------

/// ```ignore
/// spacetimedb::client_module! {
///     tables: [connected, other_mod::disconnected],
///     reducers: [identity_connected, emit_test_event(name, value)],
/// }
/// ```
///
/// Each entry is a path to the item's accessor name. A path prefix names the module that contains the declaration.
/// A reducer with parameters lists their names, because the `Reducer` enum has a struct variant per reducer
/// and a macro cannot read another item's parameter names. `client_module!` checks the list against the declaration.
struct ModuleInput {
    tables: Vec<syn::Path>,
    reducers: Vec<(syn::Path, Vec<Ident>)>,
}

impl Parse for ModuleInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut tables = vec![];
        let mut reducers = vec![];
        while !input.is_empty() {
            let section: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let content;
            syn::bracketed!(content in input);
            if section == "tables" {
                tables.extend(Punctuated::<syn::Path, Token![,]>::parse_terminated(&content)?);
            } else if section == "reducers" {
                while !content.is_empty() {
                    let path: syn::Path = content.parse()?;
                    let mut params = vec![];
                    if content.peek(syn::token::Paren) {
                        let p;
                        syn::parenthesized!(p in content);
                        params.extend(Punctuated::<Ident, Token![,]>::parse_terminated(&p)?);
                    }
                    reducers.push((path, params));
                    if content.is_empty() {
                        break;
                    }
                    content.parse::<Token![,]>()?;
                }
            } else {
                return Err(syn::Error::new(section.span(), "expected `tables` or `reducers`"));
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(Self { tables, reducers })
    }
}

/// Split `a::b::name` into (`a::b::`, `name`).
fn split_path(path: &syn::Path) -> (String, String) {
    let mut prefix = String::new();
    if path.leading_colon.is_some() {
        prefix.push_str("::");
    }
    let n = path.segments.len();
    for seg in path.segments.iter().take(n - 1) {
        write!(prefix, "{}::", seg.ident).unwrap();
    }
    (prefix, path.segments[n - 1].ident.unraw().to_string())
}

struct ModTable {
    accessor: String,
    /// `<prefix::FooTableAccessor as __sdk::ClientTableDef>`
    def: String,
}

struct ModReducer {
    variant: String,
    /// `prefix::FooArgs`
    args_path: String,
    /// (name, alias path)
    params: Vec<(String, String)>,
}

pub(crate) fn client_module(input: TokenStream) -> syn::Result<TokenStream> {
    let input: ModuleInput = syn::parse2(input)?;
    let tables: Vec<ModTable> = input
        .tables
        .iter()
        .map(|p| {
            let (prefix, accessor) = split_path(p);
            let def = format!(
                "<{prefix}{} as __sdk::ClientTableDef>",
                table_accessor_marker(&accessor)
            );
            ModTable { accessor, def }
        })
        .collect();
    let reducers: Vec<ModReducer> = input
        .reducers
        .iter()
        .map(|(p, params)| {
            let (prefix, accessor) = split_path(p);
            let args_type = args_type_name(&accessor);
            ModReducer {
                variant: pascal(&accessor),
                args_path: format!("{prefix}{args_type}"),
                params: params
                    .iter()
                    .map(|i| {
                        let name = i.unraw().to_string();
                        let alias = format!("{prefix}{}", param_alias_name(&args_type, &name));
                        (name, alias)
                    })
                    .collect(),
            }
        })
        .collect();

    let mut out = String::new();
    print_reducer_enum_defn(&mut out, &reducers);
    print_db_update_defn(&mut out, &tables);
    print_applied_diff_defn(&mut out, &tables);
    print_const_db_context_types(&mut out);
    print_impl_spacetime_module(&mut out, &tables);
    print_item_links(&mut out, &tables, &reducers);
    parse_code(out)
}

/// New: link every listed item to this module, and check the reducer parameter lists.
fn print_item_links(out: &mut String, tables: &[ModTable], reducers: &[ModReducer]) {
    for t in tables {
        writeln!(
            out,
            "impl __sdk::InModule for {}::Row {{ type Module = RemoteModule; }}",
            t.def
        )
        .unwrap();
    }
    for r in reducers {
        let args = &r.args_path;
        let fields: String = r.params.iter().map(|(n, _)| format!("{n}: _, ")).collect();
        writeln!(
            out,
            "impl __sdk::InModule for {args} {{ type Module = RemoteModule; }}
// The parameter names listed in `client_module!` must be exactly the reducer's parameters.
const _: () = {{ #[allow(dead_code)] fn __check(args: {args}) {{ let {args} {{ {fields} }} = args; }} }};"
        )
        .unwrap();
    }
    for ty in ["RemoteTables", "RemoteReducers", "RemoteProcedures"] {
        writeln!(
            out,
            "impl __sdk::HasDbContextImpl for {ty} {{
    fn db_context_impl(&self) -> &__sdk::DbContextImpl<RemoteModule> {{ &self.imp }}
}}"
        )
        .unwrap();
    }
}

/// Ported from codegen's `print_reducer_enum_defn`.
fn print_reducer_enum_defn(out: &mut String, reducers: &[ModReducer]) {
    writeln!(out, "#[derive(Clone, PartialEq, Debug)]").unwrap();
    writeln!(
        out,
        "
/// One of the reducers defined by this module.
///
/// Contained within a [`__sdk::ReducerEvent`] in [`EventContext`]s for reducer events
/// to indicate which reducer caused the event.
",
    )
    .unwrap();
    writeln!(out, "pub enum Reducer {{").unwrap();
    for r in reducers {
        write!(out, "{} ", r.variant).unwrap();
        if !r.params.is_empty() {
            // codegen: `write_struct_type_fields_in_braces`. The types are the aliases defined by `#[reducer]`.
            let fields: String = r.params.iter().map(|(n, alias)| format!("{n}: {alias}, ")).collect();
            write!(out, "{{ {fields} }}").unwrap();
        }
        writeln!(out, ",").unwrap();
    }
    writeln!(out, "}}\n").unwrap();
    writeln!(
        out,
        "
impl __sdk::InModule for Reducer {{
    type Module = RemoteModule;
}}
",
    )
    .unwrap();

    let mut names = String::new();
    let mut args = String::new();
    for r in reducers {
        // A struct pattern also matches a unit variant, so `{ .. }` works whatever the parameters are.
        writeln!(
            names,
            "Reducer::{} {{ .. }} => <{} as __sdk::ClientReducerDef>::NAME,",
            r.variant, r.args_path
        )
        .unwrap();
        writeln!(
            args,
            "Reducer::{} {{ .. }} => __sats::bsatn::to_vec(&<{} as __sdk::ClientReducerDef>::from_reducer(self).unwrap()),",
            r.variant, r.args_path
        )
        .unwrap();
    }
    write!(
        out,
        "
impl __sdk::Reducer for Reducer {{
    fn reducer_name(&self) -> &'static str {{
        #[allow(unreachable_patterns)]
        match self {{
            {names}
            // Write a catch-all pattern to handle the case where the module defines zero reducers,
            // 'cause references are always considered inhabited,
            // even references to uninhabited types.
            _ => unreachable!(),
        }}
    }}
    #[allow(clippy::clone_on_copy)]
    fn args_bsatn(&self) -> Result<Vec<u8>, __sats::bsatn::EncodeError> {{
        #[allow(unreachable_patterns)]
        match self {{
            {args}
            _ => unreachable!(),
        }}
    }}
}}
"
    )
    .unwrap();
}

/// Ported from codegen's `print_db_update_defn`.
fn print_db_update_defn(out: &mut String, tables: &[ModTable]) {
    let mut fields = String::new();
    let mut try_from_arms = String::new();
    let mut apply = String::new();
    let mut initial_arms = String::new();
    let mut unsub_arms = String::new();
    for t in tables {
        let (f, def) = (&t.accessor, &t.def);
        writeln!(fields, "{f}: __sdk::TableUpdate<{def}::Row>,").unwrap();
        writeln!(
            try_from_arms,
            "name if name == {def}::NAME => db_update.{f}.append({def}::parse_table_update(table_update)?),"
        )
        .unwrap();
        writeln!(apply, "diff.{f} = {def}::apply_diff(cache, &self.{f});").unwrap();
        writeln!(
            initial_arms,
            "name if name == {def}::NAME => db_update.{f}.append(__sdk::parse_row_list_as_inserts(table_rows.rows)?),"
        )
        .unwrap();
        writeln!(
            unsub_arms,
            "name if name == {def}::NAME => db_update.{f}.append(__sdk::parse_row_list_as_deletes(table_rows.rows)?),"
        )
        .unwrap();
    }
    write!(
        out,
        "
#[derive(Default, Debug)]
#[allow(non_snake_case)]
#[doc(hidden)]
pub struct DbUpdate {{
    {fields}
}}

impl TryFrom<__ws::v2::TransactionUpdate> for DbUpdate {{
    type Error = __sdk::Error;
    fn try_from(raw: __ws::v2::TransactionUpdate) -> Result<Self, Self::Error> {{
        let mut db_update = DbUpdate::default();
        for table_update in __sdk::transaction_update_iter_table_updates(raw) {{
            match &table_update.table_name[..] {{
                {try_from_arms}
                unknown => {{
                    return Err(__sdk::InternalError::unknown_name(
                        \"table\",
                        unknown,
                        \"DatabaseUpdate\",
                    ).into());
                }}
            }}
        }}
        Ok(db_update)
    }}
}}

impl __sdk::InModule for DbUpdate {{
    type Module = RemoteModule;
}}

impl __sdk::DbUpdate for DbUpdate {{
    #[allow(unused_mut, unused_variables)]
    fn apply_to_client_cache(&self, cache: &mut __sdk::ClientCache<RemoteModule>) -> AppliedDiff<'_> {{
        let mut diff = AppliedDiff::default();
        {apply}
        diff
    }}
    fn parse_initial_rows(raw: __ws::v2::QueryRows) -> __sdk::Result<Self> {{
        let mut db_update = DbUpdate::default();
        for table_rows in raw.tables {{
            match &table_rows.table[..] {{
                {initial_arms}
                unknown => {{ return Err(__sdk::InternalError::unknown_name(\"table\", unknown, \"QueryRows\").into()); }}
            }}
        }}
        Ok(db_update)
    }}
    fn parse_unsubscribe_rows(raw: __ws::v2::QueryRows) -> __sdk::Result<Self> {{
        let mut db_update = DbUpdate::default();
        for table_rows in raw.tables {{
            match &table_rows.table[..] {{
                {unsub_arms}
                unknown => {{ return Err(__sdk::InternalError::unknown_name(\"table\", unknown, \"QueryRows\").into()); }}
            }}
        }}
        Ok(db_update)
    }}
}}
"
    )
    .unwrap();
}

/// Ported from codegen's `print_applied_diff_defn`.
fn print_applied_diff_defn(out: &mut String, tables: &[ModTable]) {
    let mut fields = String::new();
    let mut invoke = String::new();
    for t in tables {
        let (f, def) = (&t.accessor, &t.def);
        writeln!(fields, "{f}: __sdk::TableAppliedDiff<'r, {def}::Row>,").unwrap();
        writeln!(
            invoke,
            "callbacks.invoke_table_row_callbacks::< {def}::Row >({def}::NAME, &self.{f}, event);"
        )
        .unwrap();
    }
    write!(
        out,
        "
#[derive(Default)]
#[allow(non_snake_case)]
#[doc(hidden)]
pub struct AppliedDiff<'r> {{
    {fields}
    __unused: std::marker::PhantomData<&'r ()>,
}}

impl __sdk::InModule for AppliedDiff<'_> {{
    type Module = RemoteModule;
}}

impl<'r> __sdk::AppliedDiff<'r> for AppliedDiff<'r> {{
    fn invoke_row_callbacks(&self, event: &EventContext, callbacks: &mut __sdk::DbCallbacks<RemoteModule>) {{
        {invoke}
    }}
}}
"
    )
    .unwrap();
}

/// Ported from codegen's `print_impl_spacetime_module`.
fn print_impl_spacetime_module(out: &mut String, tables: &[ModTable]) {
    let register: String = tables
        .iter()
        .map(|t| format!("{}::register_table(client_cache);\n", t.def))
        .collect();
    let names: String = tables.iter().map(|t| format!("{}::NAME,\n", t.def)).collect();
    write!(
        out,
        "
impl __sdk::SpacetimeModule for RemoteModule {{
    type DbConnection = DbConnection;
    type EventContext = EventContext;
    type ReducerEventContext = ReducerEventContext;
    type ProcedureEventContext = ProcedureEventContext;
    type SubscriptionEventContext = SubscriptionEventContext;
    type ErrorContext = ErrorContext;
    type Reducer = Reducer;
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type Procedures = RemoteProcedures;
    type DbUpdate = DbUpdate;
    type AppliedDiff<'r> = AppliedDiff<'r>;
    type SubscriptionHandle = SubscriptionHandle;
    type QueryBuilder = __sdk::QueryBuilder;

    fn register_tables(client_cache: &mut __sdk::ClientCache<Self>) {{
        {register}
    }}
    const ALL_TABLE_NAMES: &'static [&'static str] = &[
        {names}
    ];
}}
"
    )
    .unwrap();
}

/// Ported verbatim from codegen's `print_const_db_context_types` and `define_event_context`.
/// None of this depends on the module's items.
fn print_const_db_context_types(out: &mut String) {
    writeln!(
        out,
        "
#[doc(hidden)]
#[derive(Debug)]
pub struct RemoteModule;

impl __sdk::InModule for RemoteModule {{
    type Module = Self;
}}

/// The `reducers` field of [`EventContext`] and [`DbConnection`],
/// with methods provided by extension traits for each reducer defined by the module.
pub struct RemoteReducers {{
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for RemoteReducers {{
    type Module = RemoteModule;
}}

/// The `procedures` field of [`DbConnection`] and other [`DbContext`] types,
/// with methods provided by extension traits for each procedure defined by the module.
pub struct RemoteProcedures {{
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for RemoteProcedures {{
    type Module = RemoteModule;
}}

/// The `db` field of [`EventContext`] and [`DbConnection`],
/// with methods provided by extension traits for each table defined by the module.
pub struct RemoteTables {{
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for RemoteTables {{
    type Module = RemoteModule;
}}

/// A connection to a remote module, including a materialized view of a subset of the database.
///
/// Connect to a remote module by calling [`DbConnection::builder`]
/// and using the [`__sdk::DbConnectionBuilder`] builder-pattern constructor.
///
/// You must explicitly advance the connection by calling any one of:
///
/// - [`DbConnection::frame_tick`].
#[cfg_attr(not(target_arch = \"wasm32\"), doc = \"- [`DbConnection::run_threaded`].\")]
#[cfg_attr(target_arch = \"wasm32\", doc = \"- [`DbConnection::run_background_task`].\")]
/// - [`DbConnection::run_async`].
/// - [`DbConnection::advance_one_message`].
#[cfg_attr(not(target_arch =  \"wasm32\"), doc = \"- [`DbConnection::advance_one_message_blocking`].\")]
/// - [`DbConnection::advance_one_message_async`].
///
/// Which of these methods you should call depends on the specific needs of your application,
/// but you must call one of them, or else the connection will never progress.
pub struct DbConnection {{
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,
    #[doc(hidden)]

    /// Access to procedures defined by the module via extension traits implemented for [`RemoteProcedures`].
    pub procedures: RemoteProcedures,

    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::InModule for DbConnection {{
    type Module = RemoteModule;
}}

impl __sdk::DbContext for DbConnection {{
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type Procedures = RemoteProcedures;

    fn db(&self) -> &Self::DbView {{
        &self.db
    }}
    fn reducers(&self) -> &Self::Reducers {{
        &self.reducers
    }}
    fn procedures(&self) -> &Self::Procedures {{
        &self.procedures
    }}

    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    fn disconnect(&self) -> __sdk::Result<()> {{
        self.imp.disconnect()
    }}

    type SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {{
        __sdk::SubscriptionBuilder::new(&self.imp)
    }}

    fn try_identity(&self) -> Option<__sdk::Identity> {{
        self.imp.try_identity()
    }}
    fn connection_id(&self) -> __sdk::ConnectionId {{
        self.imp.connection_id()
    }}
    fn try_connection_id(&self) -> Option<__sdk::ConnectionId> {{
        self.imp.try_connection_id()
    }}
}}

impl DbConnection {{
    /// Builder-pattern constructor for a connection to a remote module.
    ///
    /// See [`__sdk::DbConnectionBuilder`] for required and optional configuration for the new connection.
    pub fn builder() -> __sdk::DbConnectionBuilder<RemoteModule> {{
        __sdk::DbConnectionBuilder::new()
    }}

    /// If any WebSocket messages are waiting, process one of them.
    ///
    /// Returns `true` if a message was processed, or `false` if the queue is empty.
    /// Callers should invoke this message in a loop until it returns `false`
    /// or for as much time is available to process messages.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::frame_tick`] each frame
    /// to fully exhaust the queue whenever time is available.
    pub fn advance_one_message(&self) -> __sdk::Result<bool> {{
        self.imp.advance_one_message()
    }}

    /// Process one WebSocket message, potentially blocking the current thread until one is received.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::run_threaded`] to spawn a thread
    /// which advances the connection automatically.
    #[cfg(not(target_arch = \"wasm32\"))]
    pub fn advance_one_message_blocking(&self) -> __sdk::Result<()> {{
        self.imp.advance_one_message_blocking()
    }}

    /// Process one WebSocket message, `await`ing until one is received.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::run_async`] to run an `async` loop
    /// which advances the connection when polled.
    pub async fn advance_one_message_async(&self) -> __sdk::Result<()> {{
        self.imp.advance_one_message_async().await
    }}

    /// Process all WebSocket messages waiting in the queue,
    /// then return without `await`ing or blocking the current thread.
    pub fn frame_tick(&self) -> __sdk::Result<()> {{
        self.imp.frame_tick()
    }}

    /// Spawn a thread which processes WebSocket messages as they are received.
    #[cfg(not(target_arch = \"wasm32\"))]
    pub fn run_threaded(&self) -> std::thread::JoinHandle<()> {{
        self.imp.run_threaded()
    }}

    /// Spawn a background task which processes WebSocket messages as they are received.
    #[cfg(target_arch = \"wasm32\")]
    pub fn run_background_task(&self) {{
        self.imp.run_background_task()
    }}

    /// Run an `async` loop which processes WebSocket messages when polled.
    pub async fn run_async(&self) -> __sdk::Result<()> {{
        self.imp.run_async().await
    }}
}}

impl __sdk::DbConnection for DbConnection {{
    fn new(imp: __sdk::DbContextImpl<RemoteModule>) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            procedures: RemoteProcedures {{ imp: imp.clone() }},
            imp,
        }}
    }}
}}

/// A handle on a subscribed query.
// TODO: Document this better after implementing the new subscription API.
#[derive(Clone)]
pub struct SubscriptionHandle {{
    imp: __sdk::SubscriptionHandleImpl<RemoteModule>,
}}

impl __sdk::InModule for SubscriptionHandle {{
    type Module = RemoteModule;
}}

impl __sdk::SubscriptionHandle for SubscriptionHandle {{
    fn new(imp: __sdk::SubscriptionHandleImpl<RemoteModule>) -> Self {{
        Self {{ imp }}
    }}

    /// Returns true if this subscription has been terminated due to an unsubscribe call or an error.
    fn is_ended(&self) -> bool {{
        self.imp.is_ended()
    }}

    /// Returns true if this subscription has been applied and has not yet been unsubscribed.
    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    /// Unsubscribe from the query controlled by this `SubscriptionHandle`,
    /// then run `on_end` when its rows are removed from the client cache.
    fn unsubscribe_then(self, on_end: __sdk::OnEndedCallback<RemoteModule>) -> __sdk::Result<()> {{
        self.imp.unsubscribe_then(Some(on_end))
    }}

    fn unsubscribe(self) -> __sdk::Result<()> {{
        self.imp.unsubscribe_then(None)
    }}

}}

/// Alias trait for a [`__sdk::DbContext`] connected to this module,
/// with that trait's associated types bounded to this module's concrete types.
///
/// Users can use this trait as a boundary on definitions which should accept
/// either a [`DbConnection`] or an [`EventContext`] and operate on either.
pub trait RemoteDbContext: __sdk::DbContext<
    DbView = RemoteTables,
    Reducers = RemoteReducers,
    SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>,
> {{}}
impl<Ctx: __sdk::DbContext<
    DbView = RemoteTables,
    Reducers = RemoteReducers,
    SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>,
>> RemoteDbContext for Ctx {{}}
",
    )
    .unwrap();

    define_event_context(
        out,
        "EventContext",
        Some("__sdk::Event<Reducer>"),
        "[`__sdk::Table::on_insert`], [`__sdk::Table::on_delete`] and [`__sdk::TableWithPrimaryKey::on_update`] callbacks",
        Some("[`__sdk::Event`]"),
    );
    define_event_context(
        out,
        "ReducerEventContext",
        Some("__sdk::ReducerEvent<Reducer>"),
        "on-reducer callbacks",
        Some("[`__sdk::ReducerEvent`]"),
    );
    define_event_context(out, "ProcedureEventContext", None, "procedure callbacks", None);
    define_event_context(
        out,
        "SubscriptionEventContext",
        None,
        "[`__sdk::SubscriptionBuilder::on_applied`] and [`SubscriptionHandle::unsubscribe_then`] callbacks",
        None,
    );
    define_event_context(
        out,
        "ErrorContext",
        Some("Option<__sdk::Error>"),
        "[`__sdk::DbConnectionBuilder::on_disconnect`], [`__sdk::DbConnectionBuilder::on_connect_error`] and [`__sdk::SubscriptionBuilder::on_error`] callbacks",
        Some("[`__sdk::Error`]"),
    );
}

/// Ported verbatim from codegen's `define_event_context`.
fn define_event_context(
    out: &mut String,
    struct_and_trait_name: &str,
    event_type: Option<&str>,
    passed_to_callbacks_doc_link: &str,
    event_type_doc_link: Option<&str>,
) {
    if let (Some(event_type), Some(event_type_doc_link)) = (event_type, event_type_doc_link) {
        write!(
            out,
            "
/// An [`__sdk::DbContext`] augmented with a {event_type_doc_link},
/// passed to {passed_to_callbacks_doc_link}.
pub struct {struct_and_trait_name} {{
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,
    /// Access to procedures defined by the module via extension traits implemented for [`RemoteProcedures`].
    pub procedures: RemoteProcedures,
    /// The event which caused these callbacks to run.
    pub event: {event_type},
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::AbstractEventContext for {struct_and_trait_name} {{
    type Event = {event_type};
    fn event(&self) -> &Self::Event {{
        &self.event
    }}
    fn new(imp: __sdk::DbContextImpl<RemoteModule>, event: Self::Event) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            procedures: RemoteProcedures {{ imp: imp.clone() }},
            event,
            imp,
        }}
    }}
}}
",
        )
        .unwrap();
    } else {
        write!(
            out,
            "
/// An [`__sdk::DbContext`] passed to {passed_to_callbacks_doc_link}.
pub struct {struct_and_trait_name} {{
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,
    /// Access to procedures defined by the module via extension traits implemented for [`RemoteProcedures`].
    pub procedures: RemoteProcedures,
    imp: __sdk::DbContextImpl<RemoteModule>,
}}

impl __sdk::AbstractEventContext for {struct_and_trait_name} {{
    type Event = ();
    fn event(&self) -> &Self::Event {{
        &()
    }}
    fn new(imp: __sdk::DbContextImpl<RemoteModule>, _event: Self::Event) -> Self {{
        Self {{
            db: RemoteTables {{ imp: imp.clone() }},
            reducers: RemoteReducers {{ imp: imp.clone() }},
            procedures: RemoteProcedures {{ imp: imp.clone() }},
            imp,
        }}
    }}
}}
",
        )
        .unwrap();
    }

    write!(
        out,
        "
impl __sdk::InModule for {struct_and_trait_name} {{
    type Module = RemoteModule;
}}

impl __sdk::DbContext for {struct_and_trait_name} {{
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type Procedures = RemoteProcedures;

    fn db(&self) -> &Self::DbView {{
        &self.db
    }}
    fn reducers(&self) -> &Self::Reducers {{
        &self.reducers
    }}
    fn procedures(&self) -> &Self::Procedures {{
        &self.procedures
    }}

    fn is_active(&self) -> bool {{
        self.imp.is_active()
    }}

    fn disconnect(&self) -> __sdk::Result<()> {{
        self.imp.disconnect()
    }}

    type SubscriptionBuilder = __sdk::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {{
        __sdk::SubscriptionBuilder::new(&self.imp)
    }}

    fn try_identity(&self) -> Option<__sdk::Identity> {{
        self.imp.try_identity()
    }}
    fn connection_id(&self) -> __sdk::ConnectionId {{
        self.imp.connection_id()
    }}
    fn try_connection_id(&self) -> Option<__sdk::ConnectionId> {{
        self.imp.try_connection_id()
    }}
}}

impl __sdk::{struct_and_trait_name} for {struct_and_trait_name} {{}}
"
    )
    .unwrap();
}
