//! Defines procedural macros like `#[spacetimedb::table]`,
//! simplifying writing SpacetimeDB modules in Rust.

#![crate_type = "proc-macro"]

#[macro_use]
mod macros;

mod module;

extern crate core;
extern crate proc_macro;

use bitflags::Flags;
use module::{derive_deserialize, derive_satstype, derive_serialize};
use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, TokenStreamExt};
use spacetimedb_primitives::ColumnAttribute;
use std::collections::HashMap;
use std::time::Duration;
use syn::meta::ParseNestedMeta;
use syn::parse::{Nothing, Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    parse_quote, BinOp, Expr, ExprBinary, ExprLit, ExprUnary, FnArg, Ident, ItemFn, LitStr, Member, Path, Token, Type,
    TypePath, UnOp,
};

mod sym {

    /// A symbol known at compile-time against
    /// which identifiers and paths may be matched.
    pub struct Symbol(&'static str);

    macro_rules! symbol {
        ($ident:ident) => {
            symbol!($ident, $ident);
        };
        ($const:ident, $ident:ident) => {
            #[allow(non_upper_case_globals)]
            #[doc = concat!("Matches `", stringify!($ident), "`.")]
            pub const $const: Symbol = Symbol(stringify!($ident));
        };
    }

    symbol!(auto_inc);
    symbol!(btree);
    symbol!(columns);
    symbol!(crate_, crate);
    symbol!(hash);
    symbol!(index);
    symbol!(name);
    symbol!(primary_key);
    symbol!(private);
    symbol!(public);
    symbol!(sats);
    symbol!(unique);

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
    impl std::fmt::Display for Symbol {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::borrow::Borrow<str> for Symbol {
        fn borrow(&self) -> &str {
            self.0
        }
    }
}

/// Parses `item`, passing it and `args` to `f`,
/// which should return only whats newly added, excluding the `item`.
/// Returns the full token stream `extra_attr item newly_added`.
fn cvt_attr<Item: Parse>(
    args: StdTokenStream,
    item: StdTokenStream,
    extra_attr: TokenStream,
    f: impl FnOnce(TokenStream, Item) -> syn::Result<TokenStream>,
) -> StdTokenStream {
    let item: TokenStream = item.into();
    let parsed_item = match syn::parse2::<Item>(item.clone()) {
        Ok(i) => i,
        Err(e) => return TokenStream::from_iter([item, e.into_compile_error()]).into(),
    };
    let generated = f(args.into(), parsed_item).unwrap_or_else(syn::Error::into_compile_error);
    TokenStream::from_iter([extra_attr, item, generated]).into()
}

/// Convert the `dur`ation to a `TokenStream` corresponding to it.
fn duration_totokens(dur: Duration) -> TokenStream {
    let (secs, nanos) = (dur.as_secs(), dur.subsec_nanos());
    quote!({
        const DUR: ::core::time::Duration = ::core::time::Duration::new(#secs, #nanos);
        DUR
    })
}

trait ErrorSource {
    fn error(self, msg: impl std::fmt::Display) -> syn::Error;
}
impl ErrorSource for Span {
    fn error(self, msg: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, msg)
    }
}
impl ErrorSource for &syn::meta::ParseNestedMeta<'_> {
    fn error(self, msg: impl std::fmt::Display) -> syn::Error {
        self.error(msg)
    }
}

/// Ensures that `x` is `None` or returns an error.
fn check_duplicate<T>(x: &Option<T>, src: impl ErrorSource) -> syn::Result<()> {
    check_duplicate_msg(x, src, "duplicate attribute")
}
fn check_duplicate_msg<T>(x: &Option<T>, src: impl ErrorSource, msg: impl std::fmt::Display) -> syn::Result<()> {
    if x.is_none() {
        Ok(())
    } else {
        Err(src.error(msg))
    }
}

#[derive(Debug)]
enum IndexType {
    BTree,
    Hash,
}

impl quote::ToTokens for IndexType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append(Ident::new(&format!("{self:?}"), Span::call_site()))
    }
}

/// Marks a function as a spacetimedb reducer.
///
/// A reducer is a function which traverses and updates the database,
/// a sort of stored procedure that lives in the database, and which can be invoked remotely.
/// Each reducer call runs in its own transaction,
/// and its updates to the database are only committed if the reducer returns successfully.
///
/// A reducer may take no arguments, like so:
///
/// ```rust,ignore
/// #[spacetimedb::reducer]
/// pub fn hello_world() {
///     println!("Hello, World!");
/// }
/// ```
///
/// But it may also take some:
/// ```rust,ignore
/// #[spacetimedb::reducer]
/// pub fn add_person(name: String, age: u16) {
///     // Logic to add a person with `name` and `age`.
/// }
/// ```
///
/// Reducers cannot return values, but can return errors.
/// To do so, a reducer must have a return type of `Result<(), impl Debug>`.
/// When such an error occurs, it will be formatted and printed out to logs,
/// resulting in an aborted transaction.
#[proc_macro_attribute]
pub fn reducer(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
    cvt_attr(args, item, quote!(), |args, original_function: ItemFn| {
        syn::parse2::<Nothing>(args)?;

        // Extract reducer name, making sure it's not `__XXX__` as that's the form we reserve for special reducers.
        let reducer_name = original_function.sig.ident.to_string();
        if reducer_name.starts_with("__") && reducer_name.ends_with("__") {
            return Err(syn::Error::new_spanned(
                &original_function.sig.ident,
                "reserved reducer name",
            ));
        }

        gen_reducer(original_function, &reducer_name, ReducerExtra::Schedule)
    })
}

enum ReducerExtra {
    None,
    Schedule,
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
    })
}

#[derive(Default)]
struct TableArgs {
    public: Option<Span>,
    name: Option<LitStr>,
    indices: Vec<IndexArg>,
}

struct IndexArg {
    kind: IndexType,
    name: Option<LitStr>,
    columns: Vec<Ident>,
}

impl TableArgs {
    fn parse(input: TokenStream) -> syn::Result<Self> {
        let mut specified_access = false;
        let mut args = TableArgs::default();
        syn::meta::parser(|meta| {
            let mut specified_access = || {
                if specified_access {
                    return Err(meta.error("already specified access level"));
                }
                specified_access = true;
                Ok(())
            };
            match_meta!(match meta {
                sym::public => {
                    specified_access()?;
                    args.public = Some(meta.path.span());
                }
                sym::private => {
                    specified_access()?;
                }
                sym::name => {
                    check_duplicate(&args.name, &meta)?;
                    let value = meta.value()?;
                    args.name = Some(value.parse()?);
                }
                sym::index => args.indices.push(IndexArg::parse_meta(meta)?),
            });
            Ok(())
        })
        .parse2(input)?;
        Ok(args)
    }
}

impl IndexArg {
    fn parse_meta(meta: ParseNestedMeta) -> syn::Result<Self> {
        let mut kind = None;
        let mut name = None;
        let mut columns = None;
        meta.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::btree => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::BTree);
                }
                sym::hash => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::Hash);
                }
                sym::name => {
                    check_duplicate(&name, &meta)?;
                    let value = meta.value()?;
                    name = Some(value.parse()?);
                }
                sym::columns => {
                    check_duplicate(&columns, &meta)?;
                    let value = meta.value()?;
                    let inner;
                    syn::bracketed!(inner in value);
                    let cols = Punctuated::<Ident, Token![,]>::parse_terminated(&inner)?;
                    columns = Some(cols.into_iter().collect());
                }
            });
            Ok(())
        })?;
        let kind = kind.ok_or_else(|| meta.error("must specify either `btree` or `hash` for index"))?;
        let columns = columns.ok_or_else(|| meta.error("must specify columns = [col1, col2] for index"))?;
        Ok(IndexArg { kind, name, columns })
    }

    /// Parses an inline `#[index(btree | hash)]` attribute on a field.
    fn parse_index_attr(field: &Ident, attr: &syn::Attribute) -> syn::Result<Self> {
        let mut kind = None;
        attr.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::btree => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::BTree);
                }
                sym::hash => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::Hash);
                }
            });
            Ok(())
        })?;
        let kind =
            kind.ok_or_else(|| syn::Error::new_spanned(&attr.meta, "must specify kind of index (`btree` or `hash`)"))?;
        Ok(IndexArg {
            kind,
            name: None,
            columns: vec![field.clone()],
        })
    }
}

/// Generates code for treating this struct type as a table.
///
/// Among other things, this derives [`Serialize`], [`Deserialize`],
/// [`SpacetimeType`], and [`TableType`] for our type.
///
/// # Example
///
/// ```ignore
/// #[spacetimedb::table(public, name = "Users")]
/// pub struct User {
///     #[auto_inc]
///     #[primary_key]
///     pub id: u32,
///     #[unique]
///     pub username: String,
///     #[index(btree)]
///     pub popularity: u32,
/// }
/// ```
///
/// # Macro arguments
///
/// * `public` and `private`
///
///    Tables are private by default. If you'd like to make your table publically
///    accessible by anyone, put `public` in the macro arguments (e.g.
///    `#[spacetimedb::table(public)]`). You can also specify `private` if
///    you'd like to be specific. This is fully separate from Rust's module visibility
///    system; `pub struct` or `pub(crate) struct` do not affect the table visibility, only
///    the visibility of the items in your own source code.
///
/// * `index(btree | hash, name = "...", columns = [a, b, c])`
///
///    You can specify an index on 1 or more of the table's columns with the above syntax.
///    You can also just put `#[index(btree | hash)]` on the field itself if you only need
///    a single-column attribute; see column attributes below.
///
/// * `name = "..."`
///
///    Specify the name of the table in the database, if you want it to be different from
///    the name of the struct.
///
/// # Column (field) attributes
///
/// * `#[auto_inc]`
///
///    Creates a database sequence.
///
///    When a row is inserted with the annotated field set to `0` (zero),
///    the sequence is incremented, and this value is used instead.
///    Can only be used on numeric types and may be combined with indexes.
///
///    Note that using `#[auto_inc]` on a field does not also imply `#[primary_key]` or `#[unique]`.
///    If those semantics are desired, those attributes should also be used.
///
/// * `#[unique]`
///
///    Creates an index and unique constraint for the annotated field.
///
/// * `#[primary_key]`
///
///    Similar to `#[unique]`, but generates additional CRUD methods.
///
/// * `#[index(btree | hash)]`
///
///    Creates a single-column index with the specified algorithm.
///
/// [`Serialize`]: https://docs.rs/spacetimedb/latest/spacetimedb/trait.Serialize.html
/// [`Deserialize`]: https://docs.rs/spacetimedb/latest/spacetimedb/trait.Deserialize.html
/// [`SpacetimeType`]: https://docs.rs/spacetimedb/latest/spacetimedb/trait.SpacetimeType.html
/// [`TableType`]: https://docs.rs/spacetimedb/latest/spacetimedb/trait.TableType.html
#[proc_macro_attribute]
pub fn table(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
    // put this on the struct so we don't get unknown attribute errors
    let extra_attr = quote!(#[derive(spacetimedb::__TableHelper)]);
    cvt_attr(args, item, extra_attr, |args, item: syn::DeriveInput| {
        let args = TableArgs::parse(args)?;
        table_impl(args, item)
    })
}

#[doc(hidden)]
#[proc_macro_derive(__TableHelper, attributes(sats, unique, auto_inc, primary_key, index))]
pub fn table_helper(_input: StdTokenStream) -> StdTokenStream {
    Default::default()
}

// TODO: We actually need to add a constraint that requires this column to be unique!
struct Column<'a> {
    index: u8,
    field: &'a module::SatsField<'a>,
    attr: ColumnAttribute,
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
        Ok(if ident == sym::unique {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Unique(ident.span()))
        } else if ident == sym::auto_inc {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Autoinc(ident.span()))
        } else if ident == sym::primary_key {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Primarykey(ident.span()))
        } else {
            None
        })
    }
}

/// Heuristically determine if the path `p` is one of Rust's primitive integer types.
/// This is an approximation, as the user could do `use String as u8`.
fn is_integer_type(p: &Path) -> bool {
    p.get_ident().map_or(false, |i| {
        matches!(
            i.to_string().as_str(),
            "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "u128" | "i128"
        )
    })
}

fn table_impl(mut args: TableArgs, item: syn::DeriveInput) -> syn::Result<TokenStream> {
    let mut sats_ty = module::sats_type_from_derive(&item, quote!(spacetimedb::spacetimedb_lib))?;

    let original_struct_ident = sats_ty.ident;
    // TODO: error on setting sats name for a table
    let table_name = args
        .name
        .map(|s| s.value())
        .unwrap_or_else(|| original_struct_ident.to_string());
    sats_ty.name.clone_from(&table_name);
    let module::SatsTypeData::Product(fields) = &sats_ty.data else {
        return Err(syn::Error::new(Span::call_site(), "spacetimedb table must be a struct"));
    };

    let mut columns = Vec::<Column>::new();

    let get_table_id_func = quote! {
        fn table_id() -> spacetimedb::TableId {
            static TABLE_ID: std::sync::OnceLock<spacetimedb::TableId> = std::sync::OnceLock::new();
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
            if attr.path() == sym::index {
                let index = IndexArg::parse_index_attr(field.ident.unwrap(), attr)?;
                args.indices.push(index);
                continue;
            }
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

        if col_attr.contains(ColumnAttribute::AUTO_INC)
            && !matches!(field.ty, syn::Type::Path(p) if is_integer_type(&p.path))
        {
            return Err(syn::Error::new(field.ident.unwrap().span(), "An `auto_inc` or `identity` column must be one of the integer types: u8, i8, u16, i16, u32, i32, u64, i64, u128, i128"));
        }

        let column = Column {
            index: col_num,
            field,
            attr: col_attr,
        };

        columns.push(column);
    }

    let mut indexes = vec![];

    for index in args.indices {
        let cols = index
            .columns
            .iter()
            .map(|ident| {
                let col = columns
                    .iter()
                    .find(|col| col.field.ident == Some(ident))
                    .ok_or_else(|| syn::Error::new(ident.span(), "not a column of the table"))?;
                Ok(col)
            })
            .collect::<syn::Result<Vec<_>>>()?;
        let name = index.name.map(|s| s.value()).unwrap_or_else(|| {
            [&*table_name]
                .into_iter()
                .chain(cols.iter().map(|col| col.field.name.as_deref().unwrap()))
                .collect::<Vec<_>>()
                .join("_")
        });
        let col_ids = cols.iter().map(|col| col.index);
        let ty = index.kind;
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
                spacetimedb::query::delete_by_unique_field::<Self, #column_type, #column_index>(#column_ident)
            }
        });
    }

    let non_primary_filter_func = nonunique_columns.into_iter().filter_map(|column| {
        let vis = column.field.vis;
        let column_ident = column.field.ident.unwrap();
        let column_type = column.field.ty;
        let column_index = column.index;

        let filter_func_ident = format_ident!("filter_by_{}", column_ident);
        let delete_func_ident = format_ident!("delete_by_{}", column_ident);

        let is_filterable = if let syn::Type::Path(TypePath { path, .. }) = column_type {
            // TODO: this is janky as heck
            is_integer_type(path)
                || path.is_ident("String")
                || path.is_ident("bool")
                // For these we use the last element of the path because they can be more commonly namespaced.
                || matches!(
                    &*path.segments.last().unwrap().ident.to_string(),
                    "Address" | "Identity"
                )
        } else {
            false
        };

        if !is_filterable {
            return None;
        }

        Some(quote! {
            // TODO: should we expose spacetimedb::query::FilterByIter ?
            #vis fn #filter_func_ident<'a>(#column_ident: &#column_type) -> impl Iterator<Item = Self> {
                spacetimedb::query::filter_by_field::<Self, #column_type, #column_index>(#column_ident)
            }
            #vis fn #delete_func_ident(#column_ident: &#column_type) -> u32 {
                spacetimedb::query::delete_by_field::<Self, #column_type, #column_index>(#column_ident)
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

    let table_access = if let Some(span) = args.public {
        quote_spanned!(span=> spacetimedb::sats::db::auth::StAccess::Public)
    } else {
        quote!(spacetimedb::sats::db::auth::StAccess::Private)
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
            const TABLE_ACCESS: spacetimedb::sats::db::auth::StAccess = #table_access;
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

macro_rules! special_reducer {
    ($(#[$attr:meta])* $name:ident = $reducer_name:literal) => {
        #[proc_macro_attribute]
        pub fn $name(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
            cvt_attr(args, item, quote!(), |args, original_function: ItemFn| {
                syn::parse2::<Nothing>(args)?;
                gen_reducer(original_function, $reducer_name, ReducerExtra::None)
            })
        }
    };
}

special_reducer!(
    /// Marks the function as a reducer run the first time a module is published
    /// and anytime the database is cleared.
    ///
    /// The reducer cannot be called manually
    /// and may not have any parameters except for `ReducerContext`.
    /// As with normal [`reducer`]s, an `init` reducer may return a `Result`.
    /// If an error occurs when initializing, the module will not be published.
    init = "__init__"
);

special_reducer!(
    /// Marks the function as a reducer run when a client connects to the SpacetimeDB module.
    /// Their identity can be found in the sender value of the `ReducerContext`.
    ///
    /// The reducer cannot be called manually
    /// and may not have any parameters except for `ReducerContext`.
    /// If an error occurs in the reducer, the client will be disconnected.
    connect = "__identity_connected__"
);

special_reducer!(
    /// Marks the function as a reducer run when a client disconnects from the SpacetimeDB module.
    /// Their identity can be found in the sender value of the `ReducerContext`.
    ///
    /// The reducer cannot be called manually
    /// and may not have any parameters except for `ReducerContext`.
    /// If an error occurs in the disconnect reducer,
    /// the client is still recorded as disconnected.
    disconnect = "__identity_disconnected__"
);

special_reducer!(
    /// Marks the function as a reducer to run when the module is updated,
    /// i.e., when publishing a module for a database that has already been initialized.
    ///
    /// The reducer cannot be called manually and may not have any parameters.
    /// As with normal [`reducer`]s, an `init` reducer may return a `Result`.
    /// If an error occurs when initializing, the module will not be published.
    update = "__update__"
);

#[proc_macro]
pub fn duration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let dur = syn::parse_macro_input!(input with parse_duration);
    duration_totokens(dur).into()
}

fn parse_duration(input: ParseStream) -> syn::Result<Duration> {
    let lookahead = input.lookahead1();
    let (s, span) = if lookahead.peek(syn::LitStr) {
        let s = input.parse::<syn::LitStr>()?;
        (s.value(), s.span())
    } else if lookahead.peek(syn::LitInt) {
        let i = input.parse::<syn::LitInt>()?;
        (i.to_string(), i.span())
    } else {
        return Err(lookahead.error());
    };
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
            // unary expressions can be also hoisted out to AST builder, in particular this
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
/// use spacetimedb::query;
///
/// #[spacetimedb::table]
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
