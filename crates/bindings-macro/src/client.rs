//! Client expansions of the module macros (proposal 0040, module-shaped client bindings).
//!
//! `spacetimedb-sdk` re-exports these entry points as `spacetimedb_sdk::{table, reducer, procedure, view,
//! SpacetimeType, client_module}`. Generated bindings name them as `spacetimedb::table` and so on,
//! with `use spacetimedb_sdk as spacetimedb;` in scope.
//!
//! - `#[table]`, `#[view]`, `#[reducer]` and `#[procedure]` expand at the declaration's location
//!   to the client API for that one item.
//!   They never name the module-wide types (`RemoteModule`, `RemoteTables`, `Reducer`, ...).
//!   They reach them through `<Anchor as InModule>::Module`, where the anchor is the item's
//!   conventionally named `{Table}TableAccessor` marker or `__{Function}Args` struct.
//! - `#[derive(SpacetimeType)]` implements serialization, plus query-builder columns for product types.
//! - `client_module!` lists the module's items and generates the module-wide items.
//!   It links each item to the module by implementing `InModule` for its anchor.
//!
//! Every impl of a trait with many impls, such as `InModule`, `From` or `ClientTableDecl`, has a concrete self type.
//! An impl of a widely implemented trait whose self type is a projection cannot be fast-rejected,
//! and made type checking several times slower in a module with a few hundred items.
//! The exceptions are the extension traits of a single table, reducer or procedure, such as `{Table}TableAccess`.
//! Each has exactly one impl, for a projection such as
//! `<<{Table}TableAccessor as InModule>::Module as RemoteModuleDecl>::DbView`, so there is nothing to reject.
//!
//! The items each expansion writes have the declaration's visibility, so that module source
//! with private items compiles. `client_module!` must then be where it can name those items.
//! The fields of a table's row are the exception: `#[table]` makes them `pub`, as generated bindings declare them,
//! so that client code can read the same fields with either (see `make_fields_pub`).
//!
//! The arguments of each attribute are parsed by the same code as the server expansion,
//! so both accept the same syntax, including proposal 0022's `table = ...` modifier on column attributes
//! (see `table::select_table_attrs`), and proposal 0032's `#[name("...")]` on a field (see `column_name`),
//! which generated bindings use where a column's canonical name differs from the default.
//!
//! The templates are string templates, ported from codegen's former Rust backend and parsed into tokens at the end.
//! They write `__sdk::`, `__lib::`, `__sats::` and `__ws::`, which `qualify` spells out as
//! `spacetimedb::__codegen::...`, because an expansion cannot add `use` items to the user's module
//! without risking duplicate imports.

use crate::procedure::ProcedureArgs;
use crate::reducer::ReducerArgs;
use crate::sats::{self, SatsTypeData};
use crate::table::{analyze_columns, select_table_attrs, TableArgs, TableColumns};
use crate::view::{extract_view_return_row_type, ViewArgs};
use convert_case::{Case, Casing};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use std::fmt::Write;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Ident, Token};

// ---------------------------------------------------------------------------------------------------------------------
// Names. These must match each other across the expansions, and the server's canonical names.
// ---------------------------------------------------------------------------------------------------------------------

/// The canonical name derived from an accessor name under the default `CaseConversionPolicy::SnakeCase`.
/// Must match `spacetimedb_schema::def::validate::v9::convert`, which also uses `convert_case`.
/// A per-item expansion cannot see the module's case conversion policy,
/// so a declaration whose canonical name differs from this states it with `name = "..."`.
fn canonical(accessor: &str) -> String {
    accessor.to_case(Case::Snake)
}

fn pascal(accessor: &str) -> String {
    accessor.to_case(Case::Pascal)
}

/// `name` as a Rust identifier, such as `r#type` for the field `r#type`.
/// The expansions keep names without `r#`, because they derive other names and canonical names from them,
/// and turn them back into identifiers where the templates write a field, method or trait.
fn rust_ident(name: &str) -> String {
    if syn::parse_str::<Ident>(name).is_ok() {
        name.to_string()
    } else {
        format!("r#{name}")
    }
}

/// The marker type of a table or view. `client_module!` finds the table through this name.
fn table_accessor_marker(accessor: &str) -> String {
    pascal(accessor) + "TableAccessor"
}

/// The arguments struct of a reducer or procedure. `client_module!` finds the function through this name.
/// The leading underscores keep it from colliding with the module's own types, such as a `MovePlayerArgs`.
fn args_type_name(accessor: &str) -> String {
    format!("__{}Args", pascal(accessor))
}

/// The hidden alias for a reducer parameter's type, which `client_module!` uses for the `Reducer` enum's fields.
/// The alias is defined at the declaration's location, so the type resolves there.
fn param_alias_name(args_type: &str, param: &str) -> String {
    format!("{args_type}_{param}")
}

/// The key of a field in `__sdk::FieldType<KEY>`, a 128-bit FNV-1a hash of its name.
/// `#[view(primary_key = col)]` names the type of `col` as `<Row as FieldType<KEY>>::Ty`,
/// because the view declaration does not contain the row's field types.
fn field_key(field: &str) -> String {
    let mut hash: u128 = 0x6c62272e07bb014262b821756295c58d;
    for byte in field.bytes() {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000000001000000000000000000013b);
    }
    format!("{hash:#x}u128")
}

/// Spell out the `__sdk`, `__lib`, `__sats` and `__ws` aliases that the templates write.
fn qualify(code: String) -> String {
    code.replace("__ws::", "spacetimedb::__codegen::__ws::")
        .replace("__sats::", "spacetimedb::__codegen::__sats::")
        .replace("__lib::", "spacetimedb::__codegen::__lib::")
        .replace("crate = __lib)", "crate = spacetimedb::__codegen::__lib)")
        .replace("__sdk::", "spacetimedb::__codegen::")
}

fn parse_code(code: String) -> syn::Result<TokenStream> {
    qualify(code).parse::<TokenStream>().map_err(|e| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("client expansion did not lex: {e}"),
        )
    })
}

fn tokens_to_string(t: &impl ToTokens) -> String {
    t.to_token_stream().to_string()
}

/// The module that `client_module!` links `anchor` to, named from a per-item expansion.
fn module_of(anchor: &str) -> String {
    format!("<{anchor} as __sdk::InModule>::Module")
}

/// One of the associated types of `RemoteModuleDecl`, such as `DbView`, for the module that `anchor` is linked to.
fn module_type(anchor: &str, assoc: &str) -> String {
    format!("<{} as __sdk::RemoteModuleDecl>::{assoc}", module_of(anchor))
}

/// Whether the client API exposes a unique column of this type through a `find` accessor.
/// Mirrors `is_type_filterable` in `crates/codegen/src/util.rs`, which decides this from the column's
/// `AlgebraicTypeUse`. Here only the type's tokens are available, so a named type is assumed to be a plain enum,
/// which the server requires of a unique column of a named type.
/// The SDK's `Uuid`, `Timestamp`, `TimeDuration` and `ScheduleAt` are recognized by name, unless the path starts with
/// `self`, `super` or `crate`: codegen writes a module type with one of those names as `self::Timestamp`.
fn is_filterable_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    let local = path.path.segments.len() > 1
        && ["self", "super", "crate"]
            .iter()
            .any(|root| path.path.segments[0].ident == root);
    match &*last.ident.to_string() {
        "Uuid" | "Timestamp" | "TimeDuration" | "ScheduleAt" => local,
        "f32" | "f64" | "Vec" | "Result" => false,
        // `Option<()>` is a sum of two unit variants, like a plain enum.
        "Option" => match &last.arguments {
            syn::PathArguments::AngleBracketed(args) => {
                args.args.len() == 1
                    && matches!(&args.args[0], syn::GenericArgument::Type(syn::Type::Tuple(unit)) if unit.elems.is_empty())
            }
            _ => false,
        },
        _ => true,
    }
}

/// A column, or a field of a product type, as the query-builder templates need it.
struct ColInfo {
    /// The field name, without `r#`.
    name: String,
    ty: String,
    /// The canonical column name.
    column_name: String,
    /// Whether to implement `FieldType` for the field. See `field_type_is_nameable`.
    field_type: bool,
}

impl ColInfo {
    fn new(row_vis: &syn::Visibility, field: &sats::SatsField<'_>) -> syn::Result<Self> {
        let ident = field.ident.expect("named field");
        Ok(Self {
            name: ident.unraw().to_string(),
            ty: tokens_to_string(field.ty),
            column_name: column_name(ident, field.original_attrs)?,
            field_type: field_type_is_nameable(row_vis, field.vis, field.ty),
        })
    }
}

/// The canonical name of a column.
///
/// A per-item expansion cannot see the module's case conversion policy, so it assumes the default.
/// Codegen writes proposal 0032's `#[name("...")]` on a field whose canonical name differs,
/// such as `#[name("playerRef")]` on `player_ref` under `CaseConversionPolicy::None`.
fn column_name(ident: &Ident, attrs: &[syn::Attribute]) -> syn::Result<String> {
    match sats::explicit_name(attrs)? {
        Some(name) => Ok(name.value()),
        None => Ok(canonical(&ident.unraw().to_string())),
    }
}

/// Whether the `FieldType` impl for a field can name the field's type.
///
/// The impl is as visible as the row, and an associated type that is more private than its impl is an error (E0446).
/// The expansion cannot see the visibility of the field's type, so it assumes that the type is visible enough
/// if the row is private, if the field is not private, or if the type is one the SDK exports.
/// A view's primary key on any other field does not compile on the client.
fn field_type_is_nameable(row_vis: &syn::Visibility, field_vis: &syn::Visibility, ty: &syn::Type) -> bool {
    matches!(row_vis, syn::Visibility::Inherited) || !matches!(field_vis, syn::Visibility::Inherited) || is_sdk_type(ty)
}

/// Whether `ty` is a primitive or a type that the SDK exports, or an `Option` or `Vec` of one.
fn is_sdk_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    match &last.arguments {
        syn::PathArguments::None => matches!(
            &*last.ident.to_string(),
            "bool"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "f32"
                | "f64"
                | "String"
                | "u256"
                | "i256"
                | "Identity"
                | "ConnectionId"
                | "Timestamp"
                | "TimeDuration"
                | "Uuid"
                | "ScheduleAt"
        ),
        syn::PathArguments::AngleBracketed(args) => {
            matches!(&*last.ident.to_string(), "Option" | "Vec")
                && args.args.iter().all(|arg| match arg {
                    syn::GenericArgument::Type(ty) => is_sdk_type(ty),
                    _ => false,
                })
        }
        syn::PathArguments::Parenthesized(_) => false,
    }
}

/// The last path segment of every `#[derive(...)]` entry on the item.
///
/// A `#[derive(...)]` written before `#[table]` has already been expanded and removed when `#[table]` runs,
/// so `#[table]` adds `Clone`, `PartialEq` and `Debug` again, and the duplicate impls conflict (E0119).
/// Module source that derives any of them must write `#[derive(...)]` after `#[table]`.
fn existing_derives(attrs: &[syn::Attribute]) -> syn::Result<Vec<String>> {
    let mut out = vec![];
    for attr in attrs.iter().filter(|a| a.path().is_ident("derive")) {
        let paths = attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)?;
        out.extend(
            paths
                .iter()
                .filter_map(|p| p.segments.last().map(|s| s.ident.to_string())),
        );
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------
// `#[table]`
// ---------------------------------------------------------------------------------------------------------------------

/// A table or a view, as the per-table template needs it.
struct TableModel {
    /// The declaration's visibility, which the generated items get.
    vis: String,
    /// The canonical table name.
    table_name: String,
    /// The accessor, without `r#`.
    accessor: String,
    row_type: String,
    is_event: bool,
    /// The primary key's field name, without `r#`.
    primary_key: Option<String>,
    /// The unique columns that have a `find` accessor, as (field name without `r#`, type), in column order.
    unique: Vec<(String, String)>,
}

/// The derive the first `#[table]` on a struct adds, after every other attribute.
/// Later `#[table]`s on the same struct see it and skip the row-level items, like the server expansion does.
/// It also declares the column attributes as helper attributes, so they stay on the fields for later `#[table]`s.
const TABLE_HELPER: &str = "__ClientTableHelper";

/// Remove the client-only `omitted` flag from the arguments of `#[table]`, and return whether it was there.
///
/// Codegen omits the tables that the client cannot access, such as private tables without `--include-private`,
/// but still declares their row types, which a view may return. In Rust, a row type's declaration
/// carries the `#[table]` attributes of its tables, so codegen writes them with `omitted`.
/// An omitted table gets the row-level items, such as its `IxCols` for semijoins, but no table handle or accessor,
/// and `client_module!` lists its row type under `types` instead of the table under `tables`.
/// Codegen writes the attributes of omitted tables after those of the row's other tables,
/// so that the first `#[table]`, which writes the row-level items, is not omitted if any is not.
fn take_omitted_flag(args: TokenStream) -> syn::Result<(TokenStream, bool)> {
    let metas = Punctuated::<syn::Meta, Token![,]>::parse_terminated.parse2(args)?;
    let mut omitted = false;
    let mut rest = Punctuated::<syn::Meta, Token![,]>::new();
    for meta in metas {
        match meta {
            syn::Meta::Path(path) if path.is_ident("omitted") => omitted = true,
            meta => rest.push(meta),
        }
    }
    Ok((rest.into_token_stream(), omitted))
}

pub(crate) fn client_table(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut item: syn::DeriveInput = syn::parse2(item)?;
    let (args, omitted) = take_omitted_flag(args)?;
    let mut args = TableArgs::parse(args, &item.ident)?;

    let derives = existing_derives(&item.attrs)?;
    let first_table_on_row = !derives.iter().any(|d| d == TABLE_HELPER);

    let selected = select_table_attrs(&item, &args.accessor, first_table_on_row)?;
    let sats_ty = sats::sats_type_from_derive(&selected, quote!(spacetimedb::__codegen::__lib))?;
    let SatsTypeData::Product(fields) = &sats_ty.data else {
        return Err(syn::Error::new_spanned(&item, "spacetimedb table must be a struct"));
    };
    let TableColumns {
        columns,
        unique_columns,
        primary_key_column,
        ..
    } = analyze_columns(&mut args.indices, fields)?;

    let accessor = args.accessor.unraw().to_string();
    let row_type = item.ident.to_string();
    let model = TableModel {
        vis: tokens_to_string(&item.vis),
        table_name: args
            .name
            .as_ref()
            .map(|n| n.value())
            .unwrap_or_else(|| canonical(&accessor)),
        accessor,
        row_type: row_type.clone(),
        is_event: args.event.is_some(),
        primary_key: primary_key_column.map(|c| c.ident.unraw().to_string()),
        unique: unique_columns
            .iter()
            .filter(|c| is_filterable_type(c.ty))
            .map(|c| (c.ident.unraw().to_string(), tokens_to_string(c.ty)))
            .collect(),
    };

    let mut code = String::new();
    let mut new_derives = vec![];
    if first_table_on_row {
        // The query builder exposes every single-column index,
        // including the ones the server creates for unique columns.
        let mut indexed = vec![];
        for index in &args.indices {
            if let Some(col) = index.validate(&model.accessor, &columns)?.kind.one_col() {
                indexed.push(col.index);
            }
        }
        indexed.sort();
        indexed.dedup();
        let cols = fields
            .iter()
            .map(|field| ColInfo::new(&item.vis, field))
            .collect::<syn::Result<Vec<_>>>()?;
        let ix_cols: Vec<&ColInfo> = indexed.iter().map(|&i| &cols[usize::from(i)]).collect();
        implement_query_col_types(&mut code, &model.vis, "table", &row_type, &cols);
        implement_query_ix_col_types(&mut code, &model.vis, &row_type, &ix_cols, model.is_event);
        implement_field_types(&mut code, &row_type, &cols);
        // An omitted table has no marker, so `client_module!` links its row type directly.
        if !omitted {
            let marker = table_accessor_marker(&model.accessor);
            writeln!(
                code,
                "
// `client_module!` links the table's marker to the module, and the row's `InModule` impl delegates to it.
impl __sdk::InModule for {row_type} {{
    type Module = <{marker} as __sdk::InModule>::Module;
}}"
            )
            .unwrap();
        }

        // Add only the derives the declaration doesn't already have,
        // because module source often derives `Clone` or `Debug` itself.
        new_derives.push(quote!(spacetimedb::__codegen::__ClientTableHelper));
        for std_derive in ["Clone", "PartialEq", "Debug"] {
            if !derives.iter().any(|d| d == std_derive) {
                let ident = Ident::new(std_derive, proc_macro2::Span::call_site());
                new_derives.push(quote!(#ident));
            }
        }
    }
    if !omitted {
        generate_table(&mut code, &model);
        implement_query_table_accessor(&mut code, &model);
    }
    let per_table = parse_code(code)?;

    // The derives go after every other attribute, so that rustc expands all `#[table]`s before them.
    if !new_derives.is_empty() {
        item.attrs.push(syn::parse_quote!(#[derive(#(#new_derives),*)]));
    }
    make_fields_pub(&mut item);
    Ok(quote! {
        #item
        #per_table
    })
}

/// Make every field of a table's row `pub`, as in generated bindings,
/// so that client code can read the fields that module source keeps private.
///
/// The `pub` has the expansion's span, so a field of a private type doesn't trip `private_interfaces`.
/// The `FieldType` impls still follow the declared visibility (see `field_type_is_nameable`).
/// `#[derive(SpacetimeType)]` cannot do the same, because a derive cannot change its item.
fn make_fields_pub(item: &mut syn::DeriveInput) {
    let syn::Data::Struct(data) = &mut item.data else {
        return;
    };
    for field in data.fields.iter_mut() {
        field.vis = syn::parse_quote!(pub);
    }
}

/// The row-level half of `#[table]`: serialization, with the column attributes as helper attributes.
pub(crate) fn client_table_helper(input: syn::DeriveInput) -> syn::Result<TokenStream> {
    let ty = sats::sats_type_from_derive(&input, quote!(spacetimedb::__codegen::__lib))?;
    Ok(TokenStream::from_iter([
        sats::derive_deserialize(&ty),
        sats::derive_serialize(&ty),
    ]))
}

/// `{Struct}Cols` and its `HasCols` impl: the query builder's columns of a row type.
/// `kind` is `table` for a table's row type and `type` otherwise.
fn implement_query_col_types(out: &mut String, vis: &str, kind: &str, struct_name: &str, cols: &[ColInfo]) {
    let cols_struct = format!("{struct_name}Cols");
    writeln!(
        out,
        "
/// Column accessor struct for the {kind} `{struct_name}`.
///
/// Provides typed access to columns for query building.
{vis} struct {cols_struct} {{"
    )
    .unwrap();
    for col in cols {
        let (field_name, field_type) = (rust_ident(&col.name), &col.ty);
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
    for col in cols {
        let (field_name, col_name) = (rust_ident(&col.name), &col.column_name);
        writeln!(
            out,
            "            {field_name}: __sdk::__query_builder::Col::new(table_name, {col_name:?}),"
        )
        .unwrap();
    }
    writeln!(out, "        }}\n    }}\n}}").unwrap();
}

/// `{Struct}IxCols` and its `HasIxCols` impl: the query builder's indexed columns of a table's row type.
/// Also `CanBeLookupTable`, unless the table is an event table.
fn implement_query_ix_col_types(out: &mut String, vis: &str, struct_name: &str, ix_cols: &[&ColInfo], is_event: bool) {
    let cols_ix = format!("{struct_name}IxCols");
    writeln!(
        out,
        "
/// Indexed column accessor struct for the table `{struct_name}`.
///
/// Provides typed access to indexed columns for query building.
{vis} struct {cols_ix} {{"
    )
    .unwrap();
    for col in ix_cols {
        let (field_name, field_type) = (rust_ident(&col.name), &col.ty);
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
    for col in ix_cols {
        let (field_name, col_name) = (rust_ident(&col.name), &col.column_name);
        writeln!(
            out,
            "            {field_name}: __sdk::__query_builder::IxCol::new(table_name, {col_name:?}),",
        )
        .unwrap();
    }
    writeln!(out, "        }}\n    }}\n}}").unwrap();

    // Event tables cannot be used as lookup tables in semijoins.
    if !is_event {
        writeln!(
            out,
            "\nimpl __sdk::__query_builder::CanBeLookupTable for {struct_name} {{}}"
        )
        .unwrap();
    }
}

/// `FieldType` impls, so that a `#[view]` with a primary key can name the type of its key column.
fn implement_field_types(out: &mut String, struct_name: &str, cols: &[ColInfo]) {
    for col in cols.iter().filter(|col| col.field_type) {
        let (key, field_type) = (field_key(&col.name), &col.ty);
        writeln!(
            out,
            "#[doc(hidden)] impl __sdk::FieldType<{key}> for {struct_name} {{ type Ty = {field_type}; }}"
        )
        .unwrap();
    }
}

/// The client API of a table or view: its handle, accessor marker and accessor trait, its callbacks,
/// its unique indexes, and its `ClientTableDecl` impl.
fn generate_table(out: &mut String, t: &TableModel) {
    let row_type = &t.row_type;
    let table_name = &t.table_name;
    let table_name_pascalcase = pascal(&t.accessor);
    let table_handle = table_name_pascalcase.clone() + "TableHandle";
    let table_accessor = table_accessor_marker(&t.accessor);
    let insert_callback_id = table_name_pascalcase.clone() + "InsertCallbackId";
    let delete_callback_id = table_name_pascalcase.clone() + "DeleteCallbackId";
    let accessor_trait = table_name_pascalcase.clone() + "TableAccess";
    let accessor_method = rust_ident(&t.accessor);
    let vis = &t.vis;

    // `client_module!` implements `InModule` for the accessor marker, so it is the anchor for the module's types.
    let remote_tables = module_type(&table_accessor, "DbView");
    let event_context = module_type(&table_accessor, "EventContext");

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
{vis} struct {table_handle}<'ctx> {{
    imp: __sdk::TableHandle<{row_type}>,
    ctx: std::marker::PhantomData<&'ctx {remote_tables}>,
}}

/// Lifetime-aware accessor marker for the table `{table_name}`.
{vis} struct {table_accessor};

impl __sdk::TableAccessor<{remote_tables}> for {table_accessor} {{
    type Row = {row_type};
    type Handle<'db> = {table_handle}<'db>;

    fn get<'db>(db: &'db {remote_tables}) -> Self::Handle<'db> {{
        db.{accessor_method}()
    }}
}}

#[allow(non_camel_case_types)]
/// Extension trait for access to the table `{table_name}`.
///
/// Implemented for `RemoteTables`.
{vis} trait {accessor_trait} {{
    #[allow(non_snake_case)]
    /// Obtain a [`{table_handle}`], which mediates access to the table `{table_name}`.
    fn {accessor_method}(&self) -> {table_handle}<'_>;
}}

impl {accessor_trait} for {remote_tables} {{
    fn {accessor_method}(&self) -> {table_handle}<'_> {{
        {table_handle} {{
            imp: __sdk::HasDbContextImpl::db_context_impl(self).get_table::<{row_type}>({table_name:?}),
            ctx: std::marker::PhantomData,
        }}
    }}
}}

{vis} struct {insert_callback_id}(__sdk::CallbackId);
"
    )
    .unwrap();

    if t.is_event {
        // Event tables: implement the `EventTable` trait, which exposes only on-insert callbacks,
        // and no unique index accessors, because event tables never have resident rows.
        write!(
            out,
            "
impl<'ctx> __sdk::TableLike for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {event_context};

    fn count(&self) -> u64 {{ self.imp.count() }}
    fn iter(&self) -> impl Iterator<Item = {row_type}> + '_ {{ self.imp.iter() }}
}}

impl<'ctx> __sdk::EventTable for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {event_context};

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
            "{vis} struct {delete_callback_id}(__sdk::CallbackId);

impl<'ctx> __sdk::TableLike for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {event_context};

    fn count(&self) -> u64 {{ self.imp.count() }}
    fn iter(&self) -> impl Iterator<Item = {row_type}> + '_ {{ self.imp.iter() }}
}}

impl<'ctx> __sdk::Table for {table_handle}<'ctx> {{
    type Row = {row_type};
    type EventContext = {event_context};

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
{vis} struct {update_callback_id}(__sdk::CallbackId);

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

        for (unique_field_name, unique_field_type) in &t.unique {
            let unique_constraint = table_name_pascalcase.clone() + &pascal(unique_field_name) + "Unique";
            let unique_field_method = rust_ident(unique_field_name);
            write!(
                out,
                "
/// Access to the `{unique_field_name}` unique index on the table `{table_name}`,
/// which allows point queries on the field of the same name
/// via the [`{unique_constraint}::find`] method.
///
/// Users are encouraged not to explicitly reference this type,
/// but to directly chain method calls,
/// like `ctx.db.{accessor_method}().{unique_field_method}().find(...)`.
#[allow(dead_code)]
{vis} struct {unique_constraint}<'ctx> {{
    imp: __sdk::UniqueConstraintHandle<{row_type}, {unique_field_type}>,
    phantom: std::marker::PhantomData<&'ctx {remote_tables}>,
}}

impl<'ctx> {table_handle}<'ctx> {{
    /// Get a handle on the `{unique_field_name}` unique index on the table `{table_name}`.
    pub fn {unique_field_method}(&self) -> {unique_constraint}<'ctx> {{
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

    // The table's parts of the module-wide items are methods of `ClientTableDecl`,
    // so that `client_module!` can reach them through the accessor marker, wherever the table is declared.
    let mut register_body = format!("let _table = client_cache.get_or_make_table::<{row_type}>({table_name:?});\n");
    for (unique_field_name, unique_field_type) in &t.unique {
        writeln!(
            register_body,
            "_table.add_unique_constraint::<{unique_field_type}>({unique_field_name:?}, |row| &row.{});",
            rust_ident(unique_field_name)
        )
        .unwrap();
    }
    let apply_body = if t.is_event {
        // Event tables bypass the client cache entirely.
        "update.into_event_diff()".to_string()
    } else {
        let with_updates = t
            .primary_key
            .as_ref()
            .map(|pk| format!(".with_updates_by_pk(|row| &row.{})", rust_ident(pk)))
            .unwrap_or_default();
        format!("cache.apply_diff_to_table::<{row_type}>({table_name:?}, update){with_updates}")
    };
    write!(
        out,
        "
#[doc(hidden)]
impl __sdk::ClientTableDecl for {table_accessor} {{
    type Row = {row_type};
    const NAME: &'static str = {table_name:?};

    fn register_table<M: __sdk::RemoteModuleDecl>(client_cache: &mut __sdk::ClientCache<M>)
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

    fn apply_diff<'r, M: __sdk::RemoteModuleDecl>(
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

/// The query builder's accessor for a table or view, an extension trait of `QueryTableAccessor`.
fn implement_query_table_accessor(out: &mut String, t: &TableModel) {
    let accessor_method = rust_ident(&t.accessor);
    let table_name = &t.table_name;
    let struct_name = &t.row_type;
    let query_accessor_trait = t.accessor.clone() + "QueryTableAccess";
    let vis = &t.vis;
    writeln!(
        out,
        "
#[allow(non_camel_case_types)]
/// Extension trait for query builder access to the table `{struct_name}`.
///
/// Implemented for [`__sdk::QueryTableAccessor`].
{vis} trait {query_accessor_trait} {{
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
// `#[derive(SpacetimeType)]`
// ---------------------------------------------------------------------------------------------------------------------

/// Serialization, plus, for a product type, the query-builder columns, which a view's row type needs,
/// and `FieldType` impls for `#[view(primary_key = ...)]`.
///
/// A derive cannot add other derives, so a declaration that needs `Clone`, `PartialEq` or `Debug`
/// derives them itself, as generated bindings do.
/// Nor can it make the fields `pub`, as `#[table]` does, so client code cannot read a private field
/// of such a type from outside its module, though it can through generated bindings.
pub(crate) fn client_spacetime_type(input: syn::DeriveInput) -> syn::Result<TokenStream> {
    let ty = sats::sats_type_from_derive(&input, quote!(spacetimedb::__codegen::__lib))?;
    let mut out = TokenStream::from_iter([sats::derive_deserialize(&ty), sats::derive_serialize(&ty)]);
    if let SatsTypeData::Product(fields) = &ty.data
        && input.generics.params.is_empty()
        && fields.iter().all(|f| f.ident.is_some())
    {
        let cols = fields
            .iter()
            .map(|field| ColInfo::new(&input.vis, field))
            .collect::<syn::Result<Vec<_>>>()?;
        let name = input.ident.to_string();
        let mut code = String::new();
        implement_query_col_types(&mut code, &tokens_to_string(&input.vis), "type", &name, &cols);
        implement_field_types(&mut code, &name, &cols);
        out.extend(parse_code(code)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------
// Function declarations: `#[reducer]`, `#[procedure]` and `#[view]`
// ---------------------------------------------------------------------------------------------------------------------

/// A free function declaration with or without a body: `pub fn add(ctx: &ReducerContext, name: String);`.
/// `syn::ItemFn` requires a body, so parse the pieces by hand.
/// The client discards the body, so it is never name-resolved or type-checked.
struct MaybeBodilessFn {
    vis: syn::Visibility,
    sig: syn::Signature,
}

impl Parse for MaybeBodilessFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _attrs = input.call(syn::Attribute::parse_outer)?;
        let vis: syn::Visibility = input.parse()?;
        let sig: syn::Signature = input.parse()?;
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        } else {
            input.parse::<syn::Block>()?;
        }
        Ok(Self { vis, sig })
    }
}

/// The visibility of a function's hidden helpers, its `__{Function}Args` struct and parameter aliases:
/// the function's own, but at most `pub(crate)`, because they are not public API.
fn helper_vis(vis: &syn::Visibility) -> String {
    match vis {
        syn::Visibility::Public(_) => "pub(crate)".to_string(),
        vis => tokens_to_string(vis),
    }
}

/// The parameters after the context, as (name without `r#`, type).
fn function_params(sig: &syn::Signature, kind: &str) -> syn::Result<Vec<(String, String)>> {
    let mut params = vec![];
    for arg in sig.inputs.iter().skip(1) {
        let syn::FnArg::Typed(pat_ty) = arg else {
            return Err(syn::Error::new_spanned(arg, "expected typed argument"));
        };
        let syn::Pat::Ident(pat) = &*pat_ty.pat else {
            return Err(syn::Error::new_spanned(
                &pat_ty.pat,
                format!("{kind} parameters must be identifiers"),
            ));
        };
        params.push((pat.ident.unraw().to_string(), tokens_to_string(&pat_ty.ty)));
    }
    Ok(params)
}

/// The arguments struct of a reducer or procedure, and the argument list in two forms.
/// The struct has `helper_vis`, because `client_module!` may be in any module that can see the function.
fn define_args_struct(
    out: &mut String,
    vis: &syn::Visibility,
    args_type: &str,
    params: &[(String, String)],
) -> (String, String) {
    let mut fields = String::new();
    let mut arglist_no_delimiters = String::new();
    let mut arg_names = String::new();
    for (name, ty) in params {
        let name = rust_ident(name);
        writeln!(fields, "pub {name}: {ty},").unwrap();
        writeln!(arglist_no_delimiters, "{name}: {ty},").unwrap();
        write!(arg_names, "{name}, ").unwrap();
    }
    let vis = helper_vis(vis);
    write!(
        out,
        "
#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
#[doc(hidden)]
{vis} struct {args_type} {{
    {fields}
}}
"
    )
    .unwrap();
    (arglist_no_delimiters, arg_names)
}

pub(crate) fn client_reducer(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    // A lifecycle kind is parsed and ignored: a lifecycle reducer gets the same client API as any other reducer.
    let args = ReducerArgs::parse(args)?;
    let MaybeBodilessFn { vis, sig } = syn::parse2(item)?;
    let func_name = sig.ident.unraw().to_string();
    let func_ident = rust_ident(&func_name);
    let reducer_name = args
        .name
        .as_ref()
        .map(|n| n.value())
        .unwrap_or_else(|| canonical(&func_name));
    let params = function_params(&sig, "reducer")?;
    let args_type = args_type_name(&func_name);
    let enum_variant_name = pascal(&func_name);
    let reducer_event_context = module_type(&args_type, "ReducerEventContext");
    let remote_reducers = module_type(&args_type, "Reducers");
    let reducer_enum = module_type(&args_type, "Reducer");

    let mut out = String::new();
    let (arglist_no_delimiters, arg_names) = define_args_struct(&mut out, &vis, &args_type, &params);
    let mut aliases = String::new();
    for (name, ty) in &params {
        let alias = param_alias_name(&args_type, name);
        writeln!(
            aliases,
            "#[doc(hidden)] #[allow(non_camel_case_types)] {} type {alias} = {ty};",
            helper_vis(&vis)
        )
        .unwrap();
    }
    let variant_pattern = if params.is_empty() {
        format!("R::{enum_variant_name}")
    } else {
        format!("R::{enum_variant_name} {{ {arg_names} }}")
    };
    let clone_fields: String = params
        .iter()
        .map(|(n, _)| format!("{0}: {0}.clone(), ", rust_ident(n)))
        .collect();
    let vis = tokens_to_string(&vis);

    write!(
        out,
        "
{aliases}

#[doc(hidden)]
impl __sdk::ClientReducerDecl for {args_type} {{
    const NAME: &'static str = {reducer_name:?};

    #[allow(clippy::clone_on_copy, unreachable_patterns)]
    fn from_reducer(reducer: &{reducer_enum}) -> Option<Self> {{
        type R = {reducer_enum};
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
{vis} trait {func_ident} {{
    /// Request that the remote module invoke the reducer `{reducer_name}` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and this method provides no way to listen for its completion status.
    /// Use [`Self::{func_name}_then`] to run a callback after the reducer completes.
    fn {func_ident}(&self, {arglist_no_delimiters}) -> __sdk::Result<()> {{
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
        callback: impl FnOnce(&{reducer_event_context}, Result<Result<(), String>, __sdk::InternalError>)
            + Send
            + 'static,
    ) -> __sdk::Result<()>;
}}

impl {func_ident} for {remote_reducers} {{
    fn {func_name}_then(
        &self,
        {arglist_no_delimiters}
        callback: impl FnOnce(&{reducer_event_context}, Result<Result<(), String>, __sdk::InternalError>)
            + Send
            + 'static,
    ) -> __sdk::Result<()> {{
        __sdk::HasDbContextImpl::db_context_impl(self).invoke_reducer_with_callback({args_type} {{ {arg_names} }}, callback)
    }}
}}
"
    )
    .unwrap();
    parse_code(out)
}

pub(crate) fn client_procedure(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args = ProcedureArgs::parse(args)?;
    let MaybeBodilessFn { vis, sig } = syn::parse2(item)?;
    let func_name = sig.ident.unraw().to_string();
    let func_ident = rust_ident(&func_name);
    let procedure_name = args
        .name
        .as_ref()
        .map(|n| n.value())
        .unwrap_or_else(|| canonical(&func_name));
    let params = function_params(&sig, "procedure")?;
    let res_ty_name = match &sig.output {
        syn::ReturnType::Default => "()".to_string(),
        syn::ReturnType::Type(_, ty) => tokens_to_string(ty),
    };
    let args_type = args_type_name(&func_name);
    let func_name_with_callback = format!("{func_name}_then");
    let procedure_event_context = module_type(&args_type, "ProcedureEventContext");
    let remote_procedures = module_type(&args_type, "Procedures");

    let mut out = String::new();
    let (arglist_no_delimiters, arg_names) = define_args_struct(&mut out, &vis, &args_type, &params);
    let vis = tokens_to_string(&vis);
    write!(
        out,
        "
#[allow(non_camel_case_types)]
/// Extension trait for access to the procedure `{procedure_name}`.
///
/// Implemented for `RemoteProcedures`.
{vis} trait {func_ident} {{
    fn {func_ident}(&self, {arglist_no_delimiters}) {{
        self.{func_name_with_callback}({arg_names} |_, _| {{}});
    }}

    fn {func_name_with_callback}(
        &self,
        {arglist_no_delimiters}
        __callback: impl FnOnce(&{procedure_event_context}, Result<{res_ty_name}, __sdk::InternalError>) + Send + 'static,
    );
}}

impl {func_ident} for {remote_procedures} {{
    fn {func_name_with_callback}(
        &self,
        {arglist_no_delimiters}
        __callback: impl FnOnce(&{procedure_event_context}, Result<{res_ty_name}, __sdk::InternalError>) + Send + 'static,
    ) {{
        __sdk::HasDbContextImpl::db_context_impl(self).invoke_procedure_with_callback::<_, {res_ty_name}>(
            {procedure_name:?},
            {args_type} {{ {arg_names} }},
            __callback,
        );
    }}
}}
"
    )
    .unwrap();
    parse_code(out)
}

pub(crate) fn client_view(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let MaybeBodilessFn { vis, sig } = syn::parse2(item)?;
    let args = ViewArgs::parse(args, &sig.ident)?;
    let return_error = "views must return `Vec<T>`, `Option<T>` or `impl Query<T>`";
    let syn::ReturnType::Type(_, ret_ty) = &sig.output else {
        return Err(syn::Error::new_spanned(&sig, return_error));
    };
    let row_ty = extract_view_return_row_type(ret_ty).ok_or_else(|| syn::Error::new_spanned(ret_ty, return_error))?;
    let row_type = tokens_to_string(row_ty);
    let accessor = args.accessor.unraw().to_string();
    let primary_key = args.primary_key.as_ref().map(|pk| pk.name());
    // The view declaration does not contain the row's field types,
    // so it names the key's type through the row's `FieldType` impl.
    let unique = primary_key
        .iter()
        .map(|pk| {
            let key = field_key(pk);
            (pk.clone(), format!("<{row_type} as __sdk::FieldType<{key}>>::Ty"))
        })
        .collect();
    let model = TableModel {
        vis: tokens_to_string(&vis),
        table_name: args
            .name
            .as_ref()
            .map(|n| n.value())
            .unwrap_or_else(|| canonical(&accessor)),
        accessor,
        row_type,
        is_event: false,
        primary_key,
        unique,
    };

    // A view is a table to the client.
    let mut code = String::new();
    generate_table(&mut code, &model);
    implement_query_table_accessor(&mut code, &model);
    parse_code(code)
}

// ---------------------------------------------------------------------------------------------------------------------
// `client_module!`
// ---------------------------------------------------------------------------------------------------------------------

/// ```ignore
/// spacetimedb::client_module! {
///     types: [point_type::Point],
///     tables: [connected, other_mod::disconnected],
///     views: [my_player],
///     reducers: [identity_connected, emit_test_event(name, value)],
///     procedures: [return_value],
/// }
/// ```
///
/// Each entry is a path to the item's accessor name, or for `types`, to the type.
/// A path prefix names the module that contains the declaration. Every section is optional.
///
/// A reducer with parameters lists their names, because the `Reducer` enum has a struct variant per reducer
/// and a macro cannot read another item's parameter names. `client_module!` checks the list against the declaration.
///
/// `types` lists types that are not the rows of listed tables. Only the row type of a view
/// and the row type of an omitted table (see `take_omitted_flag`) need to be listed,
/// but generated bindings list every one, so that each implements `InModule` as before.
#[derive(Default)]
struct ModuleInput {
    types: Vec<syn::Path>,
    tables: Vec<syn::Path>,
    reducers: Vec<(syn::Path, Vec<Ident>)>,
    procedures: Vec<syn::Path>,
}

impl Parse for ModuleInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut module = Self::default();
        let paths = Punctuated::<syn::Path, Token![,]>::parse_terminated;
        while !input.is_empty() {
            let section: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let content;
            syn::bracketed!(content in input);
            match &*section.to_string() {
                "types" => module.types.extend(paths(&content)?),
                // A view is a table to the client.
                "tables" | "views" => module.tables.extend(paths(&content)?),
                "procedures" => module.procedures.extend(paths(&content)?),
                "reducers" => {
                    while !content.is_empty() {
                        let path: syn::Path = content.parse()?;
                        let mut params = vec![];
                        if content.peek(syn::token::Paren) {
                            let p;
                            syn::parenthesized!(p in content);
                            params.extend(Punctuated::<Ident, Token![,]>::parse_terminated(&p)?);
                        }
                        module.reducers.push((path, params));
                        if content.is_empty() {
                            break;
                        }
                        content.parse::<Token![,]>()?;
                    }
                }
                _ => {
                    return Err(syn::Error::new(
                        section.span(),
                        "expected `types`, `tables`, `views`, `reducers` or `procedures`",
                    ))
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(module)
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
    /// The table's field in `DbUpdate` and `AppliedDiff`: the accessor, as a Rust identifier.
    field: String,
    /// `prefix::FooTableAccessor`
    marker: String,
    /// `<prefix::FooTableAccessor as __sdk::ClientTableDecl>`
    def: String,
}

struct ModReducer {
    variant: String,
    /// `prefix::__FooArgs`
    args_path: String,
    /// (name as a Rust identifier, alias path)
    params: Vec<(String, String)>,
    /// A check that the parameter names listed in `client_module!` are exactly the reducer's.
    /// It has the listed names' spans, so that the error for a wrong name points at it.
    check: TokenStream,
}

pub(crate) fn client_module(input: TokenStream) -> syn::Result<TokenStream> {
    let input: ModuleInput = syn::parse2(input)?;
    let tables: Vec<ModTable> = input
        .tables
        .iter()
        .map(|p| {
            let (prefix, accessor) = split_path(p);
            let marker = format!("{prefix}{}", table_accessor_marker(&accessor));
            let def = format!("<{marker} as __sdk::ClientTableDecl>");
            let field = rust_ident(&accessor);
            ModTable { field, marker, def }
        })
        .collect();
    let reducers: Vec<ModReducer> = input
        .reducers
        .iter()
        .map(|(p, params)| {
            let (prefix, accessor) = split_path(p);
            let args_type = args_type_name(&accessor);
            let mut args_ty = p.clone();
            let last = args_ty.segments.last_mut().expect("a path has a segment");
            last.ident = Ident::new(&args_type, last.ident.span());
            let check = quote! {
                const _: () = {
                    #[allow(dead_code, unreachable_code)]
                    fn __check(never: ::core::convert::Infallible) -> #args_ty {
                        #args_ty { #(#params: match never {},)* }
                    }
                };
            };
            ModReducer {
                variant: pascal(&accessor),
                args_path: format!("{prefix}{args_type}"),
                params: params
                    .iter()
                    .map(|i| {
                        let name = i.unraw().to_string();
                        let alias = format!("{prefix}{}", param_alias_name(&args_type, &name));
                        (rust_ident(&name), alias)
                    })
                    .collect(),
                check,
            }
        })
        .collect();
    let procedure_args: Vec<String> = input
        .procedures
        .iter()
        .map(|p| {
            let (prefix, accessor) = split_path(p);
            format!("{prefix}{}", args_type_name(&accessor))
        })
        .collect();
    let types: Vec<String> = input.types.iter().map(tokens_to_string).collect();

    let mut out = String::new();
    print_reducer_enum_defn(&mut out, &reducers);
    print_db_update_defn(&mut out, &tables);
    print_applied_diff_defn(&mut out, &tables);
    print_const_db_context_types(&mut out);
    print_impl_remote_module_decl(&mut out, &tables);
    print_item_links(&mut out, &types, &tables, &reducers, &procedure_args);
    let mut tokens = parse_code(out)?;
    tokens.extend(reducers.iter().map(|r| r.check.clone()));
    Ok(tokens)
}

/// Link every listed item to this module.
fn print_item_links(
    out: &mut String,
    types: &[String],
    tables: &[ModTable],
    reducers: &[ModReducer],
    procedure_args: &[String],
) {
    let linked = types
        .iter()
        .chain(tables.iter().map(|t| &t.marker))
        .chain(reducers.iter().map(|r| &r.args_path))
        .chain(procedure_args);
    for ty in linked {
        writeln!(out, "impl __sdk::InModule for {ty} {{ type Module = RemoteModule; }}").unwrap();
    }
    for r in reducers {
        let args = &r.args_path;
        let variant = &r.variant;
        // This `From` impl is here rather than in the reducer's expansion, so that its self type is concrete.
        let ctor = if r.params.is_empty() {
            format!("Reducer::{variant}")
        } else {
            let f: String = r.params.iter().map(|(n, _)| format!("{n}: args.{n}, ")).collect();
            format!("Reducer::{variant} {{ {f} }}")
        };
        writeln!(
            out,
            "impl From<{args}> for Reducer {{
    #[allow(unused_variables)]
    fn from(args: {args}) -> Self {{ {ctor} }}
}}"
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

/// The `Reducer` enum, with a variant for each reducer, and its impls.
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
            // The fields' types are the aliases defined by `#[reducer]`.
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
            "Reducer::{} {{ .. }} => <{} as __sdk::ClientReducerDecl>::NAME,",
            r.variant, r.args_path
        )
        .unwrap();
        writeln!(
            args,
            "Reducer::{} {{ .. }} => __sats::bsatn::to_vec(&<{} as __sdk::ClientReducerDecl>::from_reducer(self).unwrap()),",
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

/// `DbUpdate`, with a field for each table, and its impls.
fn print_db_update_defn(out: &mut String, tables: &[ModTable]) {
    let mut fields = String::new();
    let mut try_from_arms = String::new();
    let mut apply = String::new();
    let mut initial_arms = String::new();
    let mut unsub_arms = String::new();
    for t in tables {
        let (f, def) = (&t.field, &t.def);
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

/// `AppliedDiff`, with a field for each table, and its impls.
fn print_applied_diff_defn(out: &mut String, tables: &[ModTable]) {
    let mut fields = String::new();
    let mut invoke = String::new();
    for t in tables {
        let (f, def) = (&t.field, &t.def);
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

/// `impl RemoteModuleDecl for RemoteModule`.
fn print_impl_remote_module_decl(out: &mut String, tables: &[ModTable]) {
    let register: String = tables
        .iter()
        .map(|t| format!("{}::register_table(client_cache);\n", t.def))
        .collect();
    let names: String = tables.iter().map(|t| format!("{}::NAME,\n", t.def)).collect();
    write!(
        out,
        "
impl __sdk::RemoteModuleDecl for RemoteModule {{
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

/// `RemoteModule`, `RemoteTables`, `RemoteReducers`, `RemoteProcedures`, `DbConnection`, `SubscriptionHandle`,
/// `RemoteDbContext` and the event contexts. None of this depends on the module's items.
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

/// One of the event context types, such as `EventContext`, and its impls.
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
