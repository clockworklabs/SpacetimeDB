//! Defines procedural macros like `#[spacetimedb(table)]`,
//! simplifying writing SpacetimeDB modules in Rust.

#![crate_type = "proc-macro"]

#[macro_use]
mod macros;

mod module;

extern crate core;
extern crate proc_macro;

use bitflags::Flags;
use module::{derive_deserialize, derive_satstype, derive_serialize};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, TokenStreamExt};
use spacetimedb_primitives::ColumnAttribute;
use std::collections::HashMap;
use std::time::Duration;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    parse_quote, BinOp, Expr, ExprBinary, ExprLit, ExprUnary, FnArg, Ident, ItemFn, ItemStruct, Member, Token, Type,
    UnOp,
};

mod sym {
    /// A symbol known at compile-time against
    /// which identifiers and paths may be matched.
    pub struct Symbol(&'static str);

    /// Matches `autoinc`.
    pub const AUTOINC: Symbol = Symbol("autoinc");

    /// Matches `crate`.
    pub const CRATE: Symbol = Symbol("crate");

    /// Matches `name`.
    pub const NAME: Symbol = Symbol("name");

    /// Matches `primarykey`.
    pub const PRIMARYKEY: Symbol = Symbol("primarykey");

    /// Matches `sats`.
    pub const SATS: Symbol = Symbol("sats");

    /// Matches `unique`.
    pub const UNIQUE: Symbol = Symbol("unique");

    impl PartialEq<Symbol> for syn::Ident {
        fn eq(&self, sym: &Symbol) -> bool {
            self == sym.0
        }
    }
    impl PartialEq<Symbol> for &syn::Ident {
        fn eq(&self, sym: &Symbol) -> bool {
            *self == sym.0
        }
    }
    impl PartialEq<Symbol> for syn::Path {
        fn eq(&self, sym: &Symbol) -> bool {
            self.is_ident(sym)
        }
    }
    impl PartialEq<Symbol> for &syn::Path {
        fn eq(&self, sym: &Symbol) -> bool {
            self.is_ident(sym)
        }
    }
}

/// Defines the `#[spacetimedb(input)]` procedural attribute.
///
/// The macro takes this `input`, which defines what the attribute does,
/// and it is structured roughly like so:
/// ```ignore
/// input = table | init | connect | disconnect | migrate
///       | reducer [, repeat = Duration]
///       | index(btree | hash [, name = string] [, field_name:ident]*)
/// ```
///
/// For description of the field attributes on `#[spacetimedb(table)]` structs,
/// see [`TableType`](spacetimedb_tabletype).
#[proc_macro_attribute]
pub fn spacetimedb(macro_args: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item: TokenStream = item.into();
    let orig_input = item.clone();

    syn::parse::<MacroInput>(macro_args)
        .and_then(|input| route_input(input, item))
        .unwrap_or_else(|x| {
            let mut out = orig_input;
            out.extend(x.into_compile_error());
            out
        })
        .into()
}

/// On `item`, route the macro `input` to the various interpretations.
fn route_input(input: MacroInput, item: TokenStream) -> syn::Result<TokenStream> {
    match input {
        MacroInput::Table => spacetimedb_table(item),
        MacroInput::Init => spacetimedb_init(item),
        MacroInput::Reducer { repeat } => spacetimedb_reducer(repeat, item),
        MacroInput::Connect => spacetimedb_special_reducer("__identity_connected__", item),
        MacroInput::Disconnect => spacetimedb_special_reducer("__identity_disconnected__", item),
        MacroInput::Migrate => spacetimedb_special_reducer("__migrate__", item),
        MacroInput::Index { ty, name, field_names } => spacetimedb_index(ty, name, field_names, item),
        MacroInput::Update => spacetimedb_special_reducer("__update__", item),
    }
}

/// Convert the `dur`ation to a `TokenStream` corresponding to it.
fn duration_totokens(dur: Duration) -> TokenStream {
    let (secs, nanos) = (dur.as_secs(), dur.subsec_nanos());
    quote!({
        const DUR: ::core::time::Duration = ::core::time::Duration::new(#secs, #nanos);
        DUR
    })
}

/// Defines the input space of the `spacetimedb` macro.
enum MacroInput {
    Table,
    Init,
    Reducer {
        repeat: Option<Duration>,
    },
    Connect,
    Disconnect,
    Migrate,
    Index {
        ty: IndexType,
        name: Option<String>,
        field_names: Vec<Ident>,
    },
    Update,
}

/// Parse `f()` delimited by `,` until `input` is empty.
///
/// ` `; `,`; `, f()`; `, f(),`; are some valid parses.
fn comma_then_comma_delimited(
    input: syn::parse::ParseStream,
    mut f: impl FnMut() -> syn::Result<()>,
) -> syn::Result<()> {
    while !input.is_empty() {
        input.parse::<Token![,]>()?;
        if input.is_empty() {
            break;
        }
        f()?;
    }
    Ok(())
}

/// Ensures that `x` is `None` or returns an error.
fn check_duplicate<T>(x: &Option<T>, span: Span) -> syn::Result<()> {
    if x.is_none() {
        Ok(())
    } else {
        Err(syn::Error::new(span, "duplicate attribute"))
    }
}
fn check_duplicate_meta<T>(x: &Option<T>, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
    if x.is_none() {
        Ok(())
    } else {
        Err(meta.error("duplicate attribute"))
    }
}

impl syn::parse::Parse for MacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(match_tok!(match input {
            kw::table => Self::Table,
            kw::init => Self::Init,
            kw::reducer => {
                // Eat an optional comma, and then if anything follows,
                // it has to be `repeat = Duration`.
                let mut repeat = None;
                comma_then_comma_delimited(input, || {
                    match_tok!(match input {
                        tok @ kw::repeat => {
                            check_duplicate(&repeat, tok.span)?;
                            input.parse::<Token![=]>()?;
                            repeat = Some(input.call(parse_duration)?);
                        }
                    });
                    Ok(())
                })?;
                Self::Reducer { repeat }
            }
            kw::connect => Self::Connect,
            kw::disconnect => Self::Disconnect,
            kw::migrate => Self::Migrate,
            kw::index => {
                // Extract stuff in parens.
                let in_parens;
                syn::parenthesized!(in_parens in input);
                let in_parens = &in_parens;

                // Parse `btree` or `hash`.
                let ty: IndexType = in_parens.parse()?;

                // Find `name = $string_literal`.
                // Also find plain identifiers that become field names to index.
                let mut name = None;
                let mut field_names = Vec::new();
                comma_then_comma_delimited(in_parens, || {
                    match_tok!(match in_parens {
                        (tok, _) @ (kw::name, Token![=]) => {
                            check_duplicate(&name, tok.span)?;
                            let v = in_parens.parse::<syn::LitStr>()?;
                            name = Some(v.value())
                        }
                        ident @ Ident => field_names.push(ident),
                    });
                    Ok(())
                })?;
                Self::Index { ty, name, field_names }
            }
            kw::update => Self::Update,
        }))
    }
}

#[derive(Debug)]
enum IndexType {
    BTree,
    Hash,
}

impl syn::parse::Parse for IndexType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(match_tok!(match input {
            kw::btree => Self::BTree,
            kw::hash => Self::Hash,
        }))
    }
}

impl quote::ToTokens for IndexType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append(Ident::new(&format!("{self:?}"), Span::call_site()))
    }
}

mod kw {
    syn::custom_keyword!(table);
    syn::custom_keyword!(init);
    syn::custom_keyword!(reducer);
    syn::custom_keyword!(connect);
    syn::custom_keyword!(disconnect);
    syn::custom_keyword!(migrate);
    syn::custom_keyword!(index);
    syn::custom_keyword!(btree);
    syn::custom_keyword!(hash);
    syn::custom_keyword!(name);
    syn::custom_keyword!(repeat);
    syn::custom_keyword!(update);
}

/// Generates a reducer in place of `item`.
fn spacetimedb_reducer(repeat: Option<Duration>, item: TokenStream) -> syn::Result<TokenStream> {
    let repeat_dur = repeat.map_or(ReducerExtra::Schedule, ReducerExtra::Repeat);
    let original_function = syn::parse2::<ItemFn>(item)?;

    // Extract reducer name, making sure it's not `__XXX__` as that's the form we reserve for special reducers.
    let reducer_name = original_function.sig.ident.to_string();
    if reducer_name.starts_with("__") && reducer_name.ends_with("__") {
        return Err(syn::Error::new_spanned(
            &original_function.sig.ident,
            "reserved reducer name",
        ));
    }

    gen_reducer(original_function, &reducer_name, repeat_dur)
}

/// Generates the special `__init__` "reducer" in place of `item`.
fn spacetimedb_init(item: TokenStream) -> syn::Result<TokenStream> {
    let original_function = syn::parse2::<ItemFn>(item)?;

    gen_reducer(original_function, "__init__", ReducerExtra::None)
}

enum ReducerExtra {
    None,
    Schedule,
    Repeat(Duration),
}

fn gen_reducer(original_function: ItemFn, reducer_name: &str, extra: ReducerExtra) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    // let errmsg = "reducer should have at least 2 arguments: (identity: Identity, timestamp: u64, ...)";
    // let ([arg1, arg2], args) = validate_reducer_args(&original_function.sig, errmsg)?;

    // // TODO: better (non-string-based) validation for these
    // if !matches!(
    //     &*arg1.to_token_stream().to_string(),
    //     "spacetimedb::spacetimedb_sats::hash::Hash" | "Hash"
    // ) {
    //     return Err(syn::Error::new_spanned(
    //         &arg1,
    //         "1st parameter of a reducer must be of type \'u64\'.",
    //     ));
    // }
    // if arg2.to_token_stream().to_string() != "u64" {
    //     return Err(syn::Error::new_spanned(
    //         &arg2,
    //         "2nd parameter of a reducer must be of type \'u64\'.",
    //     ));
    // }

    // Extract all function parameters, except for `self` ones that aren't allowed.
    let typed_args = original_function
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => Ok(arg),
            _ => Err(syn::Error::new_spanned(arg, "expected typed argument")),
        })
        .collect::<syn::Result<Vec<_>>>()?;

    // Extract all function parameter names.
    let opt_arg_names = typed_args.iter().map(|arg| {
        if let syn::Pat::Ident(i) = &*arg.pat {
            let name = i.ident.to_string();
            quote!(Some(#name))
        } else {
            quote!(None)
        }
    });

    let arg_tys = typed_args.iter().map(|arg| arg.ty.as_ref()).collect::<Vec<_>>();

    // Extract the return type.
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let register_describer_symbol = format!("__preinit__20_register_describer_{reducer_name}");

    let mut epilogue = TokenStream::new();
    let mut extra_impls = TokenStream::new();

    if !matches!(extra, ReducerExtra::None) {
        let arg_names = typed_args
            .iter()
            .enumerate()
            .map(|(i, arg)| match &*arg.pat {
                syn::Pat::Ident(pat) => pat.ident.clone(),
                _ => format_ident!("__arg{}", i),
            })
            .collect::<Vec<_>>();

        extra_impls.extend(quote!(impl #func_name {
            pub fn schedule(__time: spacetimedb::Timestamp #(, #arg_names: #arg_tys)*) -> spacetimedb::ScheduleToken<#func_name> {
                spacetimedb::rt::schedule(__time, (#(#arg_names,)*))
            }
        }));
    }

    if let ReducerExtra::Repeat(repeat_dur) = &extra {
        let repeat_dur = duration_totokens(*repeat_dur);
        epilogue.extend(quote! {
            if _res.is_ok() {
                spacetimedb::rt::schedule_repeater::<_, _, #func_name>(#func_name)
            }
        });
        extra_impls.extend(quote! {
            impl spacetimedb::rt::RepeaterInfo for #func_name {
                const REPEAT_INTERVAL: ::core::time::Duration = #repeat_dur;
            }
        });
    }

    let generated_function = quote! {
        fn __reducer(
            __sender: spacetimedb::sys::Buffer,
            __caller_address: spacetimedb::sys::Buffer,
            __timestamp: u64,
            __args: &[u8]
        ) -> spacetimedb::sys::Buffer {
            #(spacetimedb::rt::assert_reducer_arg::<#arg_tys>();)*
            #(spacetimedb::rt::assert_reducer_ret::<#ret_ty>();)*
            spacetimedb::rt::invoke_reducer(
                #func_name,
                __sender,
                __caller_address,
                __timestamp,
                __args,
                |_res| { #epilogue },
            )
        }
    };

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_reducer::<_, _, #func_name>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        impl spacetimedb::rt::ReducerInfo for #func_name {
            const NAME: &'static str = #reducer_name;
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];
            const INVOKE: spacetimedb::rt::ReducerFn = {
                #generated_function
                __reducer
            };
        }
        #extra_impls
        #original_function
    })
}

// TODO: We actually need to add a constraint that requires this column to be unique!
struct Column<'a> {
    index: u8,
    field: &'a module::SatsField<'a>,
    attr: ColumnAttribute,
}

fn spacetimedb_table(item: TokenStream) -> syn::Result<TokenStream> {
    Ok(quote! {
        #[derive(spacetimedb::TableType)]
        #item
    })
}

/// Generates code for treating this type as a table.
///
/// Among other things, this derives `Serialize`, `Deserialize`,
/// `SpacetimeType`, and `TableType` for our type.
///
/// A table type must be a `struct`, whose fields may be annotated with the following attributes:
///
/// * `#[autoinc]`
///
///    Creates a database sequence.
///
///    When a row is inserted with the annotated field set to `0` (zero),
///    the sequence is incremented, and this value is used instead.
///    Can only be used on numeric types and may be combined with indexes.
///
///    Note that using `#[autoinc]` on a field does not also imply `#[primarykey]` or `#[unique]`.
///    If those semantics are desired, those attributes should also be used.
///
/// * `#[unique]`
///
///    Creates an index and unique constraint for the annotated field.
///
/// * `#[primarykey]`
///
///    Similar to `#[unique]`, but generates additional CRUD methods.
#[proc_macro_derive(TableType, attributes(sats, unique, autoinc, primarykey))]
pub fn spacetimedb_tabletype(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(item as syn::DeriveInput);
    spacetimedb_tabletype_impl(item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

enum ColumnAttr {
    Unique(Span),
    Autoinc(Span),
    Primarykey(Span),
}

impl ColumnAttr {
    fn parse(attr: &syn::Attribute) -> syn::Result<Option<Self>> {
        let Some(ident) = attr.path().get_ident() else {
            return Ok(None);
        };
        Ok(if ident == sym::UNIQUE {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Unique(ident.span()))
        } else if ident == sym::AUTOINC {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Autoinc(ident.span()))
        } else if ident == sym::PRIMARYKEY {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Primarykey(ident.span()))
        } else {
            None
        })
    }
}

fn spacetimedb_tabletype_impl(item: syn::DeriveInput) -> syn::Result<TokenStream> {
    let sats_ty = module::sats_type_from_derive(&item, quote!(spacetimedb::spacetimedb_lib))?;

    let original_struct_ident = sats_ty.ident;
    let table_name = &sats_ty.name;
    let module::SatsTypeData::Product(fields) = &sats_ty.data else {
        return Err(syn::Error::new(Span::call_site(), "spacetimedb table must be a struct"));
    };

    let mut columns = Vec::<Column>::new();

    let get_table_id_func = quote! {
        fn table_id() -> spacetimedb::TableId {
            static TABLE_ID: spacetimedb::rt::OnceCell<spacetimedb::TableId> = spacetimedb::rt::OnceCell::new();
            *TABLE_ID.get_or_init(|| {
                spacetimedb::get_table_id(<Self as spacetimedb::TableType>::TABLE_NAME)
            })
        }
    };

    for (i, field) in fields.iter().enumerate() {
        let col_num: u8 = i
            .try_into()
            .map_err(|_| syn::Error::new_spanned(field.ident, "too many columns; the most a table can have is 256"))?;

        let mut col_attr = ColumnAttribute::UNSET;
        for attr in field.original_attrs {
            let Some(attr) = ColumnAttr::parse(attr)? else { continue };
            let duplicate = |span| syn::Error::new(span, "duplicate attribute");
            let (extra_col_attr, span) = match attr {
                ColumnAttr::Unique(span) => (ColumnAttribute::UNIQUE, span),
                ColumnAttr::Autoinc(span) => (ColumnAttribute::AUTO_INC, span),
                ColumnAttr::Primarykey(span) => (ColumnAttribute::PRIMARY_KEY, span),
            };
            // do those attributes intersect (not counting the INDEXED bit which is present in all attributes)?
            // this will check that no two attributes both have UNIQUE, AUTOINC or PRIMARY_KEY bits set
            if !(col_attr & extra_col_attr)
                .difference(ColumnAttribute::INDEXED)
                .is_empty()
            {
                return Err(duplicate(span));
            }
            col_attr |= extra_col_attr;
        }

        if col_attr.contains(ColumnAttribute::AUTO_INC) {
            let valid_for_autoinc = if let syn::Type::Path(p) = field.ty {
                // TODO: this is janky as heck
                matches!(
                    &*p.path.segments.last().unwrap().ident.to_string(),
                    "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "u128" | "i128"
                )
            } else {
                false
            };
            if !valid_for_autoinc {
                return Err(syn::Error::new(field.ident.unwrap().span(), "An `autoinc` or `identity` column must be one of the integer types: u8, i8, u16, i16, u32, i32, u64, i64, u128, i128"));
            }
        }

        let column = Column {
            index: col_num,
            field,
            attr: col_attr,
        };

        columns.push(column);
    }

    let mut indexes = vec![];

    for attr in sats_ty.original_attrs {
        if attr.path().segments.last().unwrap().ident != "spacetimedb" {
            continue;
        }
        let args = attr.parse_args::<MacroInput>()?;
        let MacroInput::Index { ty, name, field_names } = args else {
            continue;
        };
        let col_ids = field_names
            .iter()
            .map(|ident| {
                let col = columns
                    .iter()
                    .find(|col| col.field.ident == Some(ident))
                    .ok_or_else(|| syn::Error::new(ident.span(), "not a column of the table"))?;
                Ok(col.index)
            })
            .collect::<syn::Result<Vec<_>>>()?;
        let name = name.as_deref().unwrap_or("default_index");
        indexes.push(quote!(spacetimedb::IndexDesc {
            name: #name,
            ty: spacetimedb::sats::db::def::IndexType::#ty,
            col_ids: &[#(#col_ids),*],
        }));
    }

    let (unique_columns, nonunique_columns): (Vec<_>, Vec<_>) =
        columns.iter().partition(|x| x.attr.contains(ColumnAttribute::UNIQUE));

    let has_unique = !unique_columns.is_empty();

    let mut unique_filter_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_update_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_delete_funcs = Vec::with_capacity(unique_columns.len());
    let mut unique_fields = Vec::with_capacity(unique_columns.len());
    for unique in unique_columns {
        let column_index = unique.index;
        let vis = unique.field.vis;
        let column_type = unique.field.ty;
        let column_ident = unique.field.ident.unwrap();

        let filter_func_ident = format_ident!("filter_by_{}", column_ident);
        let update_func_ident = format_ident!("update_by_{}", column_ident);
        let delete_func_ident = format_ident!("delete_by_{}", column_ident);

        unique_fields.push(column_index);

        unique_filter_funcs.push(quote! {
            #vis fn #filter_func_ident(#column_ident: &#column_type) -> Option<Self> {
                spacetimedb::query::filter_by_unique_field::<Self, #column_type, #column_index>(#column_ident)
            }
        });

        unique_update_funcs.push(quote! {
            #vis fn #update_func_ident(#column_ident: &#column_type, value: Self) -> bool {
                spacetimedb::query::update_by_field::<Self, #column_type, #column_index>(#column_ident, value)
            }
        });

        unique_delete_funcs.push(quote! {
            #vis fn #delete_func_ident(#column_ident: &#column_type) -> bool {
                spacetimedb::query::delete_by_field::<Self, #column_type, #column_index>(#column_ident)
            }
        });
    }

    let non_primary_filter_func = nonunique_columns.into_iter().filter_map(|column| {
        let vis = column.field.vis;
        let column_ident = column.field.ident.unwrap();
        let column_type = column.field.ty;
        let column_index = column.index;

        let filter_func_ident = format_ident!("filter_by_{}", column_ident);

        let skip = if let syn::Type::Path(p) = column_type {
            // TODO: this is janky as heck
            !matches!(
                &*p.path.segments.last().unwrap().ident.to_string(),
                "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "Hash" | "Identity" | "String" | "bool"
            )
        } else {
            true
        };

        if skip {
            return None;
        }

        Some(quote! {
            // TODO: should we expose spacetimedb::query::FilterByIter ?
            #vis fn #filter_func_ident<'a>(#column_ident: &#column_type) -> impl Iterator<Item = Self> {
                spacetimedb::query::filter_by_field::<Self, #column_type, #column_index>(#column_ident)
            }
        })
    });
    let non_primary_filter_func = non_primary_filter_func.collect::<Vec<_>>();

    let insert_result = if has_unique {
        quote!(std::result::Result<Self, spacetimedb::UniqueConstraintViolation<Self>>)
    } else {
        quote!(Self)
    };

    let db_insert = quote! {
        #[allow(unused_variables)]
        pub fn insert(ins: #original_struct_ident) -> #insert_result {
            <Self as spacetimedb::TableType>::insert(ins)
        }
    };

    let db_iter = quote! {
        #[allow(unused_variables)]
        pub fn iter() -> spacetimedb::TableIter<Self> {
            <Self as spacetimedb::TableType>::iter()
        }
    };

    let deserialize_impl = derive_deserialize(&sats_ty);
    let serialize_impl = derive_serialize(&sats_ty);
    let schema_impl = derive_satstype(&sats_ty, false);
    let column_attrs = columns.iter().map(|col| {
        Ident::new(
            ColumnAttribute::FLAGS
                .iter()
                .find_map(|f| (col.attr == *f.value()).then_some(f.name()))
                .expect("Invalid column attribute"),
            Span::call_site(),
        )
    });
    let tabletype_impl = quote! {
        impl spacetimedb::TableType for #original_struct_ident {
            const TABLE_NAME: &'static str = #table_name;
            const COLUMN_ATTRS: &'static [spacetimedb::sats::db::attr::ColumnAttribute] = &[
                #(spacetimedb::sats::db::attr::ColumnAttribute::#column_attrs),*
            ];
            const INDEXES: &'static [spacetimedb::IndexDesc<'static>] = &[#(#indexes),*];
            type InsertResult = #insert_result;
            #get_table_id_func
        }
    };

    let register_describer_symbol = format!("__preinit__20_register_describer_{table_name}");

    let describe_table_func = quote! {
        #[export_name = #register_describer_symbol]
        extern "C" fn __register_describer() {
            spacetimedb::rt::register_table::<#original_struct_ident>()
        }
    };

    let field_names = fields.iter().map(|f| f.ident.unwrap()).collect::<Vec<_>>();
    let field_types = fields.iter().map(|f| f.ty).collect::<Vec<_>>();

    let col_num = 0u8..;
    let field_access_impls = quote! {
        #(impl spacetimedb::query::FieldAccess<#col_num> for #original_struct_ident {
            type Field = #field_types;
            fn get_field(&self) -> &Self::Field {
                &self.#field_names
            }
        })*
    };

    let filter_impl = quote! {
        const _: () = {
            #[derive(Debug, spacetimedb::Serialize, spacetimedb::Deserialize)]
            #[sats(crate = spacetimedb::spacetimedb_lib)]
            #[repr(u8)]
            #[allow(non_camel_case_types)]
            pub enum FieldIndex {
                #(#field_names),*
            }

            impl spacetimedb::spacetimedb_lib::filter::Table for #original_struct_ident {
                type FieldIndex = FieldIndex;
            }
        };
    };

    // Attempt to improve the compile error when a table field doesn't satisfy
    // the supertraits of `TableType`. We make it so the field span indicates
    // which fields are offenders, and error reporting stops if the field doesn't
    // implement `SpacetimeType` (which happens to be the derive macro one is
    // supposed to use). That is, the user doesn't see errors about `Serialize`,
    // `Deserialize` not being satisfied, which they wouldn't know what to do
    // about.
    let assert_fields_are_spacetimetypes = {
        let trait_ident = Ident::new("AssertSpacetimeFields", Span::call_site());
        let field_impls = fields
            .iter()
            .map(|field| (field.ty, field.span))
            .collect::<HashMap<_, _>>()
            .into_iter()
            .map(|(ty, span)| quote_spanned!(span=> impl #trait_ident for #ty {}));

        quote_spanned! {item.span()=>
            trait #trait_ident: spacetimedb::SpacetimeType {}
            #(#field_impls)*
        }
    };

    // Output all macro data
    let emission = quote! {
        const _: () = {
            #describe_table_func
        };

        const _: () = {
            #assert_fields_are_spacetimetypes
        };

        impl #original_struct_ident {
            #db_insert
            #(#unique_filter_funcs)*
            #(#unique_update_funcs)*
            #(#unique_delete_funcs)*

            #db_iter
            #(#non_primary_filter_func)*
        }

        #schema_impl
        #deserialize_impl
        #serialize_impl
        #tabletype_impl

        #field_access_impls
        #filter_impl
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        {
            #![allow(clippy::disallowed_macros)]
            println!("{}", emission);
        }
    }

    Ok(emission)
}

fn spacetimedb_index(
    _index_type: IndexType,
    _index_name: Option<String>,
    _field_names: Vec<Ident>,
    item: TokenStream,
) -> syn::Result<TokenStream> {
    let original_struct = syn::parse2::<ItemStruct>(item)?;

    let original_struct_name = &original_struct.ident;

    let output = quote! {
        #original_struct

        const _: () = spacetimedb::rt::assert_table::<#original_struct_name>();
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        {
            #![allow(clippy::disallowed_macros)]
            println!("{}", output);
        }
    }

    Ok(output)
}

fn spacetimedb_special_reducer(name: &str, item: TokenStream) -> syn::Result<TokenStream> {
    let original_function = syn::parse2::<ItemFn>(item)?;
    gen_reducer(original_function, name, ReducerExtra::None)
}

#[proc_macro]
pub fn duration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let dur = syn::parse_macro_input!(input with parse_duration);
    duration_totokens(dur).into()
}

fn parse_duration(input: ParseStream) -> syn::Result<Duration> {
    let (s, span) = match_tok!(match input {
        s @ syn::LitStr => (s.value(), s.span()),
        i @ syn::LitInt => (i.to_string(), i.span()),
    });
    humantime::parse_duration(&s).map_err(|e| syn::Error::new(span, format_args!("can't parse as duration: {e}")))
}

#[proc_macro_derive(Deserialize, attributes(sats))]
pub fn deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    module::sats_type_from_derive(&input, quote!(spacetimedb_lib))
        .map(|ty| derive_deserialize(&ty))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Serialize, attributes(sats))]
pub fn serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    module::sats_type_from_derive(&input, quote!(spacetimedb_lib))
        .map(|ty| derive_serialize(&ty))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(SpacetimeType, attributes(sats))]
pub fn schema_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    (|| {
        let ty = module::sats_type_from_derive(&input, quote!(spacetimedb::spacetimedb_lib))?;

        let ident = ty.ident;

        let schema_impl = derive_satstype(&ty, true);
        let deserialize_impl = derive_deserialize(&ty);
        let serialize_impl = derive_serialize(&ty);

        let register_describer_symbol = format!("__preinit__20_register_describer_{}", ty.name);

        let emission = quote! {
            #schema_impl
            #deserialize_impl
            #serialize_impl

            const _: () = {
                #[export_name = #register_describer_symbol]
                extern "C" fn __register_describer() {
                    spacetimedb::rt::register_reftype::<#ident>()
                }
            };
        };

        if std::env::var("PROC_MACRO_DEBUG").is_ok() {
            {
                #![allow(clippy::disallowed_macros)]
                println!("{}", emission);
            }
        }

        Ok(emission)
    })()
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}

struct ClosureArg {
    // only ident for now as we want to do scope analysis and for now this makes things easier
    row_name: Ident,
    table_ty: Type,
}

impl Parse for ClosureArg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse::<Token![|]>()?;
        let row_name = input.parse()?;
        input.parse::<Token![:]>()?;
        let table_ty = input.parse()?;
        input.parse::<Token![|]>()?;
        Ok(Self { row_name, table_ty })
    }
}

impl ClosureArg {
    fn expr_as_table_field<'e>(&self, expr: &'e Expr) -> syn::Result<&'e Ident> {
        match expr {
            Expr::Field(field)
                if match field.base.as_ref() {
                    Expr::Path(path) => path.path.is_ident(&self.row_name),
                    _ => false,
                } =>
            {
                match &field.member {
                    Member::Named(ident) => Ok(ident),
                    Member::Unnamed(index) => Err(syn::Error::new_spanned(index, "unnamed members are not allowed")),
                }
            }
            _ => Err(syn::Error::new_spanned(expr, "expected table field access")),
        }
    }

    fn make_rhs(&self, e: &mut Expr) -> syn::Result<()> {
        match e {
            // support `E::A`, `foobar`, etc. - any path except the `row` argument
            Expr::Path(path) if !path.path.is_ident(&self.row_name) => Ok(()),
            // support any field of a valid RHS expression - this makes it work like
            // Rust 2021 closures where `|| foo.bar.baz` captures only `foo.bar.baz`
            Expr::Field(field) => self.make_rhs(&mut field.base),
            // string literals need to be converted to their owned version for serialization
            Expr::Lit(ExprLit {
                lit: syn::Lit::Str(_), ..
            }) => {
                *e = parse_quote!(#e.to_owned());
                Ok(())
            }
            // other literals can be inlined into the AST as-is
            Expr::Lit(_) => Ok(()),
            // unary expressions can be also hoisted out to AST builder, in particulal this
            // is important to support negative literals like `-123`
            Expr::Unary(ExprUnary { expr: arg, .. }) => self.make_rhs(arg),
            Expr::Group(group) => self.make_rhs(&mut group.expr),
            Expr::Paren(paren) => self.make_rhs(&mut paren.expr),
            _ => Err(syn::Error::new_spanned(
                e,
                "this expression is not supported in the right-hand side of the comparison",
            )),
        }
    }

    fn handle_cmp(&self, expr: &ExprBinary) -> syn::Result<TokenStream> {
        let left = self.expr_as_table_field(&expr.left)?;

        let mut right = expr.right.clone();
        self.make_rhs(&mut right)?;

        let table_ty = &self.table_ty;

        let lhs_field = quote_spanned!(left.span()=> <#table_ty as spacetimedb::spacetimedb_lib::filter::Table>::FieldIndex::#left as u8);

        let rhs = quote_spanned!(right.span()=> spacetimedb::spacetimedb_lib::filter::Rhs::Value(
            std::convert::identity::<<#table_ty as spacetimedb::query::FieldAccess::<{#lhs_field}>>::Field>(#right).into()
        ));

        let op = match expr.op {
            BinOp::Lt(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::Lt),
            BinOp::Le(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::LtEq),
            BinOp::Eq(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::Eq),
            BinOp::Ne(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::NotEq),
            BinOp::Ge(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::GtEq),
            BinOp::Gt(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpCmp::Gt),
            _ => unreachable!(),
        };

        Ok(
            quote_spanned!(expr.span()=> spacetimedb::spacetimedb_lib::filter::Expr::Cmp(spacetimedb::spacetimedb_lib::filter::Cmp {
                op: #op,
                args: spacetimedb::spacetimedb_lib::filter::CmpArgs {
                    lhs_field: #lhs_field,
                    rhs: #rhs,
                },
            })),
        )
    }

    fn handle_logic(&self, expr: &ExprBinary) -> syn::Result<TokenStream> {
        let op = match expr.op {
            BinOp::And(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpLogic::And),
            BinOp::Or(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpLogic::Or),
            _ => unreachable!(),
        };

        let left = self.handle_expr(&expr.left)?;
        let right = self.handle_expr(&expr.right)?;

        Ok(
            quote_spanned!(expr.span()=> spacetimedb::spacetimedb_lib::filter::Expr::Logic(spacetimedb::spacetimedb_lib::filter::Logic {
                lhs: Box::new(#left),
                op: #op,
                rhs: Box::new(#right),
            })),
        )
    }

    fn handle_binop(&self, expr: &ExprBinary) -> syn::Result<TokenStream> {
        match expr.op {
            BinOp::Lt(_) | BinOp::Le(_) | BinOp::Eq(_) | BinOp::Ne(_) | BinOp::Ge(_) | BinOp::Gt(_) => {
                self.handle_cmp(expr)
            }
            BinOp::And(_) | BinOp::Or(_) => self.handle_logic(expr),
            _ => Err(syn::Error::new_spanned(expr.op, "unsupported binary operator")),
        }
    }

    fn handle_unop(&self, expr: &ExprUnary) -> syn::Result<TokenStream> {
        let op = match expr.op {
            UnOp::Not(op) => quote_spanned!(op.span()=> spacetimedb::spacetimedb_lib::operator::OpUnary::Not),
            _ => return Err(syn::Error::new_spanned(expr.op, "unsupported unary operator")),
        };

        let arg = self.handle_expr(&expr.expr)?;

        Ok(
            quote_spanned!(expr.span()=> spacetimedb::spacetimedb_lib::filter::Expr::Unary(spacetimedb::spacetimedb_lib::filter::Unary {
                op: #op,
                arg: Box::new(#arg),
            })),
        )
    }

    fn handle_expr(&self, expr: &Expr) -> syn::Result<TokenStream> {
        Ok(match expr {
            Expr::Binary(expr) => self.handle_binop(expr)?,
            Expr::Unary(expr) => self.handle_unop(expr)?,
            Expr::Group(group) => self.handle_expr(&group.expr)?,
            Expr::Paren(paren) => self.handle_expr(&paren.expr)?,
            expr => return Err(syn::Error::new_spanned(expr, "unsupported expression")),
        })
    }
}

struct ClosureLike {
    arg: ClosureArg,
    body: Box<Expr>,
}

impl Parse for ClosureLike {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            arg: input.parse()?,
            body: input.parse()?,
        })
    }
}

impl ClosureLike {
    pub fn handle(&self) -> syn::Result<TokenStream> {
        let table_ty = &self.arg.table_ty;
        let expr = self.arg.handle_expr(&self.body)?;

        Ok(quote_spanned!(self.body.span()=> {
            <#table_ty as spacetimedb::TableType>::iter_filtered(#expr)
        }))
    }
}

/// Implements query!(|row| ...) macro for filtering rows.
///
/// # Example
///
/// ```ignore // unfortunately, doctest doesn't work well inside proc-macro
/// use spacetimedb::{spacetimedb, query};
///
/// #[spacetimedb(table)]
/// pub struct Person {
///     name: String,
///     age: u32,
/// }
///
/// for person in query!(|person: Person| person.age >= 18) {
///    println!("{person:?}");
/// }
/// ```
///
/// # Syntax
///
/// Supports Rust-like closure syntax, with the following limitations:
///
/// - Only one argument is supported.
/// - Argument must be an identifier (destructuring is not yet implemented).
/// - Argument must have an explicit table type annotation.
/// - Left hand side of any comparison must be a table field access.
/// - Right hand side of any comparison must be a literal or a captured variable `foo` or a property `foo.bar.baz` (which will be inlined as its value).
///   In the future field-to-field comparisons will be supported too.
/// - Comparisons can be combined with `&&` and `||` operators.
/// - Parentheses are supported.
/// - Unary `!` operator is supported at the syntax level but not yet implemented by the VM so it will panic at translation phase.
#[proc_macro]
pub fn query(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let closure_like = syn::parse_macro_input!(input as ClosureLike);

    closure_like
        .handle()
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
