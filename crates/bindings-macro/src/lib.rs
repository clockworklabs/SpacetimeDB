//! Defines procedural macros like `#[spacetimedb::table]`,
//! simplifying writing SpacetimeDB modules in Rust.

#![crate_type = "proc-macro"]

#[macro_use]
mod macros;

mod module;

extern crate core;
extern crate proc_macro;

use heck::ToSnakeCase;
use module::{derive_deserialize, derive_satstype, derive_serialize};
use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;
use syn::ext::IdentExt;
use syn::meta::ParseNestedMeta;
use syn::parse::{Parse, ParseStream, Parser as _};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_quote, FnArg, Ident, ItemFn, Path, Token};

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
    symbol!(client_connected);
    symbol!(client_disconnected);
    symbol!(columns);
    symbol!(crate_, crate);
    symbol!(index);
    symbol!(init);
    symbol!(name);
    symbol!(primary_key);
    symbol!(private);
    symbol!(public);
    symbol!(sats);
    symbol!(scheduled);
    symbol!(unique);
    symbol!(update);

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
fn cvt_attr<Item: Parse + quote::ToTokens>(
    args: StdTokenStream,
    item: StdTokenStream,
    extra_attr: TokenStream,
    f: impl FnOnce(TokenStream, MutItem<'_, Item>) -> syn::Result<TokenStream>,
) -> StdTokenStream {
    let item: TokenStream = item.into();
    let mut parsed_item = match syn::parse2::<Item>(item.clone()) {
        Ok(i) => i,
        Err(e) => return TokenStream::from_iter([item, e.into_compile_error()]).into(),
    };
    let mut modified = false;
    let mut_item = MutItem {
        val: &mut parsed_item,
        modified: &mut modified,
    };
    let generated = f(args.into(), mut_item).unwrap_or_else(syn::Error::into_compile_error);
    let item = if modified {
        parsed_item.into_token_stream()
    } else {
        item
    };
    TokenStream::from_iter([extra_attr, item, generated]).into()
}

fn ident_to_litstr(ident: &Ident) -> syn::LitStr {
    syn::LitStr::new(&ident.to_string(), ident.span())
}

struct MutItem<'a, T> {
    val: &'a mut T,
    modified: &'a mut bool,
}
impl<T> std::ops::Deref for MutItem<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.val
    }
}
impl<T> std::ops::DerefMut for MutItem<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        *self.modified = true;
        self.val
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

#[derive(Default)]
struct ReducerArgs {
    lifecycle: Option<LifecycleReducer>,
}

enum LifecycleReducer {
    Init(Span),
    ClientConnected(Span),
    ClientDisconnected(Span),
    Update(Span),
}
impl LifecycleReducer {
    fn reducer_name(&self) -> &'static str {
        match self {
            Self::Init(_) => "__init__",
            Self::ClientConnected(_) => "__identity_connected__",
            Self::ClientDisconnected(_) => "__identity_disconnected__",
            Self::Update(_) => "__update__",
        }
    }
    fn to_lifecycle_value(&self) -> Option<TokenStream> {
        let (Self::Init(span) | Self::ClientConnected(span) | Self::ClientDisconnected(span) | Self::Update(span)) =
            *self;
        let name = match self {
            Self::Init(_) => "Init",
            Self::ClientConnected(_) => "OnConnect",
            Self::ClientDisconnected(_) => "OnDisconnect",
            Self::Update(_) => return None,
        };
        let ident = Ident::new(name, span);
        Some(quote_spanned!(span => spacetimedb::rt::LifecycleReducer::#ident))
    }
}

impl ReducerArgs {
    fn parse(input: TokenStream) -> syn::Result<Self> {
        let mut args = Self::default();
        syn::meta::parser(|meta| {
            let mut set_lifecycle = |kind: fn(Span) -> _| -> syn::Result<()> {
                check_duplicate_msg(&args.lifecycle, &meta, "already specified a lifecycle reducer kind")?;
                args.lifecycle = Some(kind(meta.path.span()));
                Ok(())
            };
            match_meta!(match meta {
                sym::init => set_lifecycle(LifecycleReducer::Init)?,
                sym::client_connected => set_lifecycle(LifecycleReducer::ClientConnected)?,
                sym::client_disconnected => set_lifecycle(LifecycleReducer::ClientDisconnected)?,
                sym::update => set_lifecycle(LifecycleReducer::Update)?,
            });
            Ok(())
        })
        .parse2(input)?;
        Ok(args)
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
///
/// # Lifecycle Reducers
///
/// You can specify special lifecycle reducers that are run at set points in
/// the module's lifecycle. You can have one each per module.
///
/// ## `#[spacetimedb::reducer(init)]`
///
/// This reducer is run the first time a module is published
/// and anytime the database is cleared.
///
/// The reducer cannot be called manually
/// and may not have any parameters except for `ReducerContext`.
/// If an error occurs when initializing, the module will not be published.
///
/// ## `#[spacetimedb::reducer(client_connected)]`
///
/// This reducer is run when a client connects to the SpacetimeDB module.
/// Their identity can be found in the sender value of the `ReducerContext`.
///
/// The reducer cannot be called manually
/// and may not have any parameters except for `ReducerContext`.
/// If an error occurs in the reducer, the client will be disconnected.
///
///
/// ## `#[spacetimedb::reducer(client_disconnected)]`
///
/// This reducer is run when a client disconnects from the SpacetimeDB module.
/// Their identity can be found in the sender value of the `ReducerContext`.
///
/// The reducer cannot be called manually
/// and may not have any parameters except for `ReducerContext`.
/// If an error occurs in the disconnect reducer,
/// the client is still recorded as disconnected.
///
/// ## `#[spacetimedb::reducer(update)]`
///
/// This reducer is run when the module is updated,
/// i.e., when publishing a module for a database that has already been initialized.
///
/// The reducer cannot be called manually and may not have any parameters.
/// If an error occurs when initializing, the module will not be published.
#[proc_macro_attribute]
pub fn reducer(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
    cvt_attr::<ItemFn>(args, item, quote!(), |args, original_function| {
        let args = ReducerArgs::parse(args)?;
        reducer_impl(args, &original_function)
    })
}

fn reducer_impl(args: ReducerArgs, original_function: &ItemFn) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    // Extract reducer name, making sure it's not `__XXX__` as that's the form we reserve for special reducers.
    let reducer_name;
    let reducer_name = match &args.lifecycle {
        Some(lifecycle) => lifecycle.reducer_name(),
        None => {
            reducer_name = func_name.to_string();
            if reducer_name.starts_with("__") && reducer_name.ends_with("__") {
                return Err(syn::Error::new_spanned(
                    &original_function.sig.ident,
                    "reserved reducer name",
                ));
            }
            &reducer_name
        }
    };

    for param in &original_function.sig.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => return Err(err("type parameters are not allowed on reducers")),
            syn::GenericParam::Const(_) => return Err(err("const parameters are not allowed on reducers")),
        }
    }

    let lifecycle = args.lifecycle.iter().filter_map(|lc| lc.to_lifecycle_value());

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
    let first_arg_ty = arg_tys.first().into_iter();
    let rest_arg_tys = arg_tys.iter().skip(1);

    // Extract the return type.
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let register_describer_symbol = format!("__preinit__20_register_describer_{reducer_name}");

    let lt_params = &original_function.sig.generics;
    let lt_where_clause = &lt_params.where_clause;

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_reducer::<_, #func_name>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        const _: () = {
            fn _assert_args #lt_params () #lt_where_clause {
                #(let _ = <#first_arg_ty as spacetimedb::rt::ReducerContextArg>::_ITEM;)*
                #(let _ = <#rest_arg_tys as spacetimedb::rt::ReducerArg>::_ITEM;)*
                #(let _ = <#ret_ty as spacetimedb::rt::IntoReducerResult>::into_result;)*
            }
        };
        impl #func_name {
            fn invoke(__ctx: spacetimedb::ReducerContext, __args: &[u8]) -> spacetimedb::ReducerResult {
                spacetimedb::rt::invoke_reducer(#func_name, __ctx, __args)
            }
        }
        #[automatically_derived]
        impl spacetimedb::rt::ReducerInfo for #func_name {
            const NAME: &'static str = #reducer_name;
            #(const LIFECYCLE: Option<spacetimedb::rt::LifecycleReducer> = Some(#lifecycle);)*
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];
            const INVOKE: spacetimedb::rt::ReducerFn = #func_name::invoke;
        }
    })
}

struct TableArgs {
    access: Option<TableAccess>,
    scheduled: Option<Path>,
    name: Ident,
    indices: Vec<IndexArg>,
}

enum TableAccess {
    Public(Span),
    Private(Span),
}

impl TableAccess {
    fn to_value(&self) -> TokenStream {
        let (TableAccess::Public(span) | TableAccess::Private(span)) = *self;
        let name = match self {
            TableAccess::Public(_) => "Public",
            TableAccess::Private(_) => "Private",
        };
        let ident = Ident::new(name, span);
        quote_spanned!(span => spacetimedb::table::TableAccess::#ident)
    }
}

// add scheduled_id and scheduled_at fields to the struct
fn add_scheduled_fields(item: &mut syn::DeriveInput) {
    if let syn::Data::Struct(struct_data) = &mut item.data {
        if let syn::Fields::Named(fields) = &mut struct_data.fields {
            let extra_fields: syn::FieldsNamed = parse_quote!({
                #[primary_key]
                #[auto_inc]
                pub scheduled_id: u64,
                pub scheduled_at: spacetimedb::spacetimedb_lib::ScheduleAt,
            });
            fields.named.extend(extra_fields.named);
        }
    }
}

struct IndexArg {
    name: Ident,
    kind: IndexType,
}

enum IndexType {
    BTree { columns: Vec<Ident> },
    UniqueBTree { column: Ident },
}

impl TableArgs {
    fn parse(input: TokenStream, struct_ident: &Ident) -> syn::Result<Self> {
        let mut access = None;
        let mut scheduled = None;
        let mut name = None;
        let mut indices = Vec::new();
        syn::meta::parser(|meta| {
            match_meta!(match meta {
                sym::public => {
                    check_duplicate_msg(&access, &meta, "already specified access level")?;
                    access = Some(TableAccess::Public(meta.path.span()));
                }
                sym::private => {
                    check_duplicate_msg(&access, &meta, "already specified access level")?;
                    access = Some(TableAccess::Private(meta.path.span()));
                }
                sym::name => {
                    check_duplicate(&name, &meta)?;
                    let value = meta.value()?;
                    name = Some(value.parse()?);
                }
                sym::index => indices.push(IndexArg::parse_meta(meta)?),
                sym::scheduled => {
                    check_duplicate(&scheduled, &meta)?;
                    let in_parens;
                    syn::parenthesized!(in_parens in meta.input);
                    scheduled = Some(in_parens.parse::<Path>()?);
                }
            });
            Ok(())
        })
        .parse2(input)?;
        let name = name.ok_or_else(|| {
            let table = struct_ident.to_string().to_snake_case();
            syn::Error::new(
                Span::call_site(),
                format_args!("must specify table name, e.g. `#[spacetimedb::table(name = {table})]"),
            )
        })?;
        Ok(TableArgs {
            access,
            scheduled,
            name,
            indices,
        })
    }
}

impl IndexArg {
    fn parse_meta(meta: ParseNestedMeta) -> syn::Result<Self> {
        let mut name = None;
        let mut algo = None;

        meta.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::name => {
                    check_duplicate(&name, &meta)?;
                    name = Some(meta.value()?.parse()?);
                }
                sym::btree => {
                    check_duplicate_msg(&algo, &meta, "index algorithm specified twice")?;
                    algo = Some(Self::parse_btree(meta)?);
                }
            });
            Ok(())
        })?;
        let name = name.ok_or_else(|| meta.error("missing index name, e.g. name = my_index"))?;
        let kind = algo.ok_or_else(|| meta.error("missing index algorithm, e.g., `btree(columns = [col1, col2])`"))?;

        Ok(IndexArg { name, kind })
    }

    fn parse_btree(meta: ParseNestedMeta) -> syn::Result<IndexType> {
        let mut columns = None;
        meta.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::columns => {
                    check_duplicate(&columns, &meta)?;
                    let value = meta.value()?;
                    let inner;
                    syn::bracketed!(inner in value);
                    columns = Some(
                        Punctuated::<Ident, Token![,]>::parse_terminated(&inner)?
                            .into_iter()
                            .collect::<Vec<_>>(),
                    );
                }
            });
            Ok(())
        })?;
        let columns = columns
            .ok_or_else(|| meta.error("must specify columns for btree index, e.g. `btree(columns = [col1, col2])`"))?;
        Ok(IndexType::BTree { columns })
    }

    /// Parses an inline `#[index(btree)]` attribute on a field.
    fn parse_index_attr(field: &Ident, attr: &syn::Attribute) -> syn::Result<Self> {
        let mut kind = None;
        attr.parse_nested_meta(|meta| {
            match_meta!(match meta {
                sym::btree => {
                    check_duplicate_msg(&kind, &meta, "index type specified twice")?;
                    kind = Some(IndexType::BTree {
                        columns: vec![field.clone()],
                    });
                }
            });
            Ok(())
        })?;
        let kind = kind.ok_or_else(|| syn::Error::new_spanned(&attr.meta, "must specify kind of index (`btree`)"))?;
        let name = field.clone();
        Ok(IndexArg { kind, name })
    }

    fn validate<'a>(&'a self, table_name: &str, cols: &'a [Column<'a>]) -> syn::Result<ValidatedIndex<'_>> {
        let find_column = |ident| {
            cols.iter()
                .find(|col| col.field.ident == Some(ident))
                .ok_or_else(|| syn::Error::new(ident.span(), "not a column of the table"))
        };
        let kind = match &self.kind {
            IndexType::BTree { columns } => {
                let cols = columns.iter().map(find_column).collect::<syn::Result<Vec<_>>>()?;
                ValidatedIndexType::BTree { cols }
            }
            IndexType::UniqueBTree { column } => {
                let col = find_column(column)?;
                ValidatedIndexType::UniqueBTree { col }
            }
        };
        let index_name = match &kind {
            ValidatedIndexType::BTree { cols } => ([table_name, "btree"].into_iter())
                .chain(cols.iter().map(|col| col.field.name.as_deref().unwrap()))
                .collect::<Vec<_>>()
                .join("_"),
            ValidatedIndexType::UniqueBTree { col } => {
                [table_name, "btree", col.field.name.as_deref().unwrap()].join("_")
            }
        };
        Ok(ValidatedIndex {
            index_name,
            accessor_name: &self.name,
            kind,
        })
    }
}

struct ValidatedIndex<'a> {
    index_name: String,
    accessor_name: &'a Ident,
    kind: ValidatedIndexType<'a>,
}

enum ValidatedIndexType<'a> {
    BTree { cols: Vec<&'a Column<'a>> },
    UniqueBTree { col: &'a Column<'a> },
}

impl ValidatedIndex<'_> {
    fn desc(&self) -> TokenStream {
        let algo = match &self.kind {
            ValidatedIndexType::BTree { cols } => {
                let col_ids = cols.iter().map(|col| col.index);
                quote!(spacetimedb::table::IndexAlgo::BTree {
                    columns: &[#(#col_ids),*]
                })
            }
            ValidatedIndexType::UniqueBTree { col } => {
                let col_id = col.index;
                quote!(spacetimedb::table::IndexAlgo::BTree {
                    columns: &[#col_id]
                })
            }
        };
        let index_name = &self.index_name;
        let accessor_name = ident_to_litstr(self.accessor_name);
        quote!(spacetimedb::table::IndexDesc {
            name: #index_name,
            accessor_name: #accessor_name,
            algo: #algo,
        })
    }

    fn accessor(&self, vis: &syn::Visibility, row_type_ident: &Ident) -> TokenStream {
        let index_ident = self.accessor_name;
        match &self.kind {
            ValidatedIndexType::BTree { cols } => {
                let col_tys = cols.iter().map(|col| col.ty);
                let mut doc = format!(
                    "Gets the `{index_ident}` [`BTreeIndex`][spacetimedb::BTreeIndex] as defined \
                     on this table. \n\
                     \n\
                     This B-tree index is defined on the following columns, in order:\n"
                );
                for col in cols {
                    use std::fmt::Write;
                    writeln!(
                        doc,
                        "- [`{ident}`][{row_type_ident}#structfield.{ident}]: [`{ty}`]",
                        ident = col.field.ident.unwrap(),
                        ty = col.ty.to_token_stream()
                    )
                    .unwrap();
                }
                quote! {
                    #[doc = #doc]
                    #vis fn #index_ident(&self) -> spacetimedb::BTreeIndex<Self, (#(#col_tys,)*), __indices::#index_ident> {
                        spacetimedb::BTreeIndex::__NEW
                    }
                }
            }
            ValidatedIndexType::UniqueBTree { col } => {
                let vis = col.field.vis;
                let col_ty = col.field.ty;
                let column_ident = col.field.ident.unwrap();

                let doc = format!(
                    "Gets the [`UniqueColumn`][spacetimedb::UniqueColumn] for the \
                     [`{column_ident}`][{row_type_ident}::{column_ident}] column."
                );
                quote! {
                    #[doc = #doc]
                    #vis fn #column_ident(&self) -> spacetimedb::UniqueColumn<Self, #col_ty, __indices::#index_ident> {
                        spacetimedb::UniqueColumn::__NEW
                    }
                }
            }
        }
    }

    fn marker_type(&self, vis: &syn::Visibility, row_type_ident: &Ident) -> TokenStream {
        let index_ident = self.accessor_name;
        let index_name = &self.index_name;
        let vis = if let ValidatedIndexType::UniqueBTree { col } = self.kind {
            col.field.vis
        } else {
            vis
        };
        let vis = superize_vis(vis);
        let mut decl = quote! {
            #vis struct #index_ident;
            impl spacetimedb::table::Index for #index_ident {
                fn index_id() -> spacetimedb::table::IndexId {
                    static INDEX_ID: std::sync::OnceLock<spacetimedb::table::IndexId> = std::sync::OnceLock::new();
                    *INDEX_ID.get_or_init(|| {
                        spacetimedb::sys::index_id_from_name(#index_name).unwrap()
                    })
                }
            }
        };
        if let ValidatedIndexType::UniqueBTree { col } = self.kind {
            let col_ty = col.ty;
            let col_name = col.field.name.as_deref().unwrap();
            let field_ident = col.field.ident.unwrap();
            decl.extend(quote! {
                impl spacetimedb::table::Column for #index_ident {
                    type Row = #row_type_ident;
                    type ColType = #col_ty;
                    const COLUMN_NAME: &'static str = #col_name;
                    fn get_field(row: &Self::Row) -> &Self::ColType {
                        &row.#field_ident
                    }
                }
            });
        }
        decl
    }
}

/// Transform a visibility marker to one with the same effective visibility, but
/// for use in a child module of the module of the original marker.
fn superize_vis(vis: &syn::Visibility) -> Cow<'_, syn::Visibility> {
    match vis {
        syn::Visibility::Public(_) => Cow::Borrowed(vis),
        syn::Visibility::Restricted(vis_r) => {
            let first = &vis_r.path.segments[0];
            if first.ident == "crate" || vis_r.path.leading_colon.is_some() {
                Cow::Borrowed(vis)
            } else {
                let mut vis_r = vis_r.clone();
                if first.ident == "super" {
                    vis_r.path.segments.insert(0, first.clone())
                } else if first.ident == "self" {
                    vis_r.path.segments[0].ident = Ident::new("super", Span::call_site())
                }
                Cow::Owned(syn::Visibility::Restricted(vis_r))
            }
        }
        syn::Visibility::Inherited => Cow::Owned(parse_quote!(pub(super))),
    }
}

/// Generates code for treating this struct type as a table.
///
/// Among other things, this derives `Serialize`, `Deserialize`,
/// `SpacetimeType`, and `Table` for our type.
///
/// # Example
///
/// ```ignore
/// #[spacetimedb::table(name = users, public)]
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
/// * `index(name = my_index, btree(columns = [a, b, c]))`
///
///    You can specify an index on 1 or more of the table's columns with the above syntax.
///    You can also just put `#[index(btree)]` on the field itself if you only need
///    a single-column attribute; see column attributes below.
///
/// * `name = my_table`
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
/// * `#[index(btree)]`
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
    cvt_attr::<syn::DeriveInput>(args, item, extra_attr, |args, item| {
        let args = TableArgs::parse(args, &item.ident)?;
        table_impl(args, item)
    })
}

#[doc(hidden)]
#[proc_macro_derive(__TableHelper, attributes(sats, unique, auto_inc, primary_key, index))]
pub fn table_helper(_input: StdTokenStream) -> StdTokenStream {
    Default::default()
}

#[derive(Copy, Clone)]
struct Column<'a> {
    index: u16,
    field: &'a module::SatsField<'a>,
    ty: &'a syn::Type,
}

enum ColumnAttr {
    Unique(Span),
    AutoInc(Span),
    PrimaryKey(Span),
    Index(IndexArg),
}

impl ColumnAttr {
    fn parse(attr: &syn::Attribute, field_ident: &Ident) -> syn::Result<Option<Self>> {
        let Some(ident) = attr.path().get_ident() else {
            return Ok(None);
        };
        Ok(if ident == sym::index {
            let index = IndexArg::parse_index_attr(field_ident, attr)?;
            Some(ColumnAttr::Index(index))
        } else if ident == sym::unique {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::Unique(ident.span()))
        } else if ident == sym::auto_inc {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::AutoInc(ident.span()))
        } else if ident == sym::primary_key {
            attr.meta.require_path_only()?;
            Some(ColumnAttr::PrimaryKey(ident.span()))
        } else {
            None
        })
    }
}

fn table_impl(mut args: TableArgs, mut item: MutItem<syn::DeriveInput>) -> syn::Result<TokenStream> {
    let scheduled_reducer_type_check = args.scheduled.as_ref().map(|reducer| {
        add_scheduled_fields(&mut item);
        let struct_name = &item.ident;
        quote! {
            const _: () = spacetimedb::rt::scheduled_reducer_typecheck::<#struct_name>(#reducer);
        }
    });

    let vis = &item.vis;
    let sats_ty = module::sats_type_from_derive(&item, quote!(spacetimedb::spacetimedb_lib))?;

    let original_struct_ident = sats_ty.ident;
    let table_ident = &args.name;
    let table_name = table_ident.unraw().to_string();
    let module::SatsTypeData::Product(fields) = &sats_ty.data else {
        return Err(syn::Error::new(Span::call_site(), "spacetimedb table must be a struct"));
    };

    for param in &item.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => return Err(err("type parameters are not allowed on tables")),
            syn::GenericParam::Const(_) => return Err(err("const parameters are not allowed on tables")),
        }
    }

    let table_id_from_name_func = quote! {
        fn table_id() -> spacetimedb::TableId {
            static TABLE_ID: std::sync::OnceLock<spacetimedb::TableId> = std::sync::OnceLock::new();
            *TABLE_ID.get_or_init(|| {
                spacetimedb::table_id_from_name(<Self as spacetimedb::table::TableInternal>::TABLE_NAME)
            })
        }
    };

    if fields.len() > u16::MAX.into() {
        return Err(syn::Error::new_spanned(
            &*item,
            "too many columns; the most a table can have is 2^16",
        ));
    }

    let mut columns = vec![];
    let mut unique_columns = vec![];
    let mut sequenced_columns = vec![];
    let mut primary_key_column = None;

    for (i, field) in fields.iter().enumerate() {
        let col_num = i as u16;
        let field_ident = field.ident.unwrap();

        let mut unique = None;
        let mut auto_inc = None;
        let mut primary_key = None;
        for attr in field.original_attrs {
            let Some(attr) = ColumnAttr::parse(attr, field_ident)? else {
                continue;
            };
            match attr {
                ColumnAttr::Unique(span) => {
                    check_duplicate(&unique, span)?;
                    unique = Some(span);
                }
                ColumnAttr::AutoInc(span) => {
                    check_duplicate(&auto_inc, span)?;
                    auto_inc = Some(span);
                }
                ColumnAttr::PrimaryKey(span) => {
                    check_duplicate(&primary_key, span)?;
                    primary_key = Some(span);
                }
                ColumnAttr::Index(index_arg) => args.indices.push(index_arg),
            }
        }

        let column = Column {
            index: col_num,
            field,
            ty: field.ty,
        };

        if unique.is_some() || primary_key.is_some() {
            unique_columns.push(column);
            args.indices.push(IndexArg {
                name: field_ident.clone(),
                kind: IndexType::UniqueBTree {
                    column: field_ident.clone(),
                },
            });
        }
        if auto_inc.is_some() {
            sequenced_columns.push(column);
        }
        if let Some(span) = primary_key {
            check_duplicate_msg(&primary_key_column, span, "can only have one primary key per table")?;
            primary_key_column = Some(column);
        }

        columns.push(column);
    }

    let row_type = quote!(#original_struct_ident);

    let mut indices = args
        .indices
        .iter()
        .map(|index| index.validate(&table_name, &columns))
        .collect::<syn::Result<Vec<_>>>()?;

    // order unique accessors before index accessors
    indices.sort_by(|a, b| match (&a.kind, &b.kind) {
        (ValidatedIndexType::UniqueBTree { .. }, ValidatedIndexType::UniqueBTree { .. }) => std::cmp::Ordering::Equal,
        (_, ValidatedIndexType::UniqueBTree { .. }) => std::cmp::Ordering::Greater,
        (ValidatedIndexType::UniqueBTree { .. }, _) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    });

    let index_descs = indices.iter().map(|index| index.desc());
    let index_accessors = indices.iter().map(|index| index.accessor(vis, original_struct_ident));
    let index_marker_types = indices
        .iter()
        .map(|index| index.marker_type(vis, original_struct_ident));

    let tablehandle_ident = format_ident!("{}__TableHandle", table_ident);

    let deserialize_impl = derive_deserialize(&sats_ty);
    let serialize_impl = derive_serialize(&sats_ty);
    let schema_impl = derive_satstype(&sats_ty);

    // Generate `integrate_generated_columns`
    // which will integrate all generated auto-inc col values into `_row`.
    let integrate_gen_col = sequenced_columns.iter().map(|col| {
        let field = col.field.ident.unwrap();
        quote_spanned!(field.span()=>
            if spacetimedb::table::IsSequenceTrigger::is_sequence_trigger(&_row.#field) {
                _row.#field = spacetimedb::sats::bsatn::from_reader(_in).unwrap();
            }
        )
    });
    let integrate_generated_columns = quote_spanned!(item.span() =>
        fn integrate_generated_columns(_row: &mut #row_type, mut _generated_cols: &[u8]) {
            let mut _in = &mut _generated_cols;
            #(#integrate_gen_col)*
        }
    );

    let table_access = args.access.iter().map(|acc| acc.to_value());
    let unique_col_ids = unique_columns.iter().map(|col| col.index);
    let primary_col_id = primary_key_column.iter().map(|col| col.index);
    let sequence_col_ids = sequenced_columns.iter().map(|col| col.index);

    let schedule = args
        .scheduled
        .as_ref()
        .map(|reducer| {
            // scheduled_at was inserted as the last field
            let scheduled_at_id = (fields.len() - 1) as u16;
            quote!(spacetimedb::table::ScheduleDesc {
                reducer_name: <#reducer as spacetimedb::rt::ReducerInfo>::NAME,
                scheduled_at_column: #scheduled_at_id,
            })
        })
        .into_iter();

    let unique_err = if !unique_columns.is_empty() {
        quote!(spacetimedb::UniqueConstraintViolation)
    } else {
        quote!(::core::convert::Infallible)
    };
    let autoinc_err = if !sequenced_columns.is_empty() {
        quote!(spacetimedb::AutoIncOverflow)
    } else {
        quote!(::core::convert::Infallible)
    };

    let field_names = fields.iter().map(|f| f.ident.unwrap()).collect::<Vec<_>>();
    let field_types = fields.iter().map(|f| f.ty).collect::<Vec<_>>();

    let tabletype_impl = quote! {
        impl spacetimedb::Table for #tablehandle_ident {
            type Row = #row_type;

            type UniqueConstraintViolation = #unique_err;
            type AutoIncOverflow = #autoinc_err;

            #integrate_generated_columns
        }
        impl spacetimedb::table::TableInternal for #tablehandle_ident {
            const TABLE_NAME: &'static str = #table_name;
            // the default value if not specified is Private
            #(const TABLE_ACCESS: spacetimedb::table::TableAccess = #table_access;)*
            const UNIQUE_COLUMNS: &'static [u16] = &[#(#unique_col_ids),*];
            const INDEXES: &'static [spacetimedb::table::IndexDesc<'static>] = &[#(#index_descs),*];
            #(const PRIMARY_KEY: Option<u16> = Some(#primary_col_id);)*
            const SEQUENCES: &'static [u16] = &[#(#sequence_col_ids),*];
            #(const SCHEDULE: Option<spacetimedb::table::ScheduleDesc<'static>> = Some(#schedule);)*

            #table_id_from_name_func
        }
    };

    let register_describer_symbol = format!("__preinit__20_register_describer_{table_ident}");

    let describe_table_func = quote! {
        #[export_name = #register_describer_symbol]
        extern "C" fn __register_describer() {
            spacetimedb::rt::register_table::<#tablehandle_ident>()
        }
    };

    let col_num = 0u16..;
    let field_access_impls = quote! {
        #(impl spacetimedb::table::FieldAccess<#col_num> for #original_struct_ident {
            type Field = #field_types;
            fn get_field(&self) -> &Self::Field {
                &self.#field_names
            }
        })*
    };

    let row_type_to_table = quote!(<#row_type as spacetimedb::table::__MapRowTypeToTable>::Table);

    // Output all macro data
    let trait_def = quote_spanned! {table_ident.span()=>
        #[allow(non_camel_case_types, dead_code)]
        #vis trait #table_ident {
            fn #table_ident(&self) -> &#row_type_to_table;
        }
        impl #table_ident for spacetimedb::Local {
            fn #table_ident(&self) -> &#row_type_to_table {
                #[allow(non_camel_case_types)]
                type #tablehandle_ident = #row_type_to_table;
                &#tablehandle_ident {}
            }
        }
    };

    let tablehandle_def = quote! {
        #[allow(non_camel_case_types)]
        #[non_exhaustive]
        #vis struct #tablehandle_ident {}
    };

    let emission = quote! {
        const _: () = {
            #(let _ = <#field_types as spacetimedb::rt::TableColumn>::_ITEM;)*
        };

        #trait_def

        #[cfg(doc)]
        #tablehandle_def

        const _: () = {
            #[cfg(not(doc))]
            #tablehandle_def

            impl spacetimedb::table::__MapRowTypeToTable for #row_type {
                type Table = #tablehandle_ident;
            }

            impl #tablehandle_ident {
                #(#index_accessors)*
            }

            #tabletype_impl

            #[allow(non_camel_case_types)]
            mod __indices {
                #[allow(unused)]
                use super::*;
                #(#index_marker_types)*
            }

            #describe_table_func
        };

        #schema_impl
        #deserialize_impl
        #serialize_impl

        #field_access_impls

        #scheduled_reducer_type_check
    };

    if std::env::var("PROC_MACRO_DEBUG").is_ok() {
        {
            #![allow(clippy::disallowed_macros)]
            println!("{}", emission);
        }
    }

    Ok(emission)
}

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
        let name = &ty.name;
        let krate = &ty.krate;

        let schema_impl = derive_satstype(&ty);
        let deserialize_impl = derive_deserialize(&ty);
        let serialize_impl = derive_serialize(&ty);

        let emission = quote! {
            #schema_impl
            #deserialize_impl
            #serialize_impl

            // unfortunately, generic types don't work in modules at the moment.
            #krate::__make_register_reftype!(#ident, #name);
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

fn parse_sql(input: ParseStream) -> syn::Result<String> {
    use spacetimedb_sql_parser::parser::sub;

    let lookahead = input.lookahead1();
    let sql = if lookahead.peek(syn::LitStr) {
        let s = input.parse::<syn::LitStr>()?;
        // Checks the query is syntactically valid
        let _ = sub::parse_subscription(&s.value()).map_err(|e| syn::Error::new(s.span(), format_args!("{e}")))?;

        s.value()
    } else {
        return Err(lookahead.error());
    };

    Ok(sql)
}

/// Generates code for registering a row-level security `SQL` function.
///
/// A row-level security function takes a `SQL` query expression that is used to filter rows.
///
/// The query follows the same syntax as a subscription query.
///
/// **Example:**
///
/// ```rust,ignore
/// /// Players can only see what's in their chunk
/// spacetimedb::filter!("
///     SELECT * FROM LocationState WHERE chunk_index IN (
///         SELECT chunk_index FROM LocationState WHERE entity_id IN (
///             SELECT entity_id FROM UserState WHERE identity = @sender
///         )
///     )
/// ");
/// ```
///
/// **NOTE:** The `SQL` query expression is pre-parsed at compile time, but only check is a valid
/// subscription query *syntactically*, not that the query is valid when executed.
///
/// For example, it could refer to a non-existent table.
#[proc_macro]
pub fn filter(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rls_sql = syn::parse_macro_input!(input with parse_sql);

    let mut hasher = DefaultHasher::new();
    rls_sql.hash(&mut hasher);
    let rls_name = format_ident!("rls_{}", hasher.finish());

    let register_rls_symbol = format!("__preinit__20_register_{rls_name}");

    let generated_describe_function = quote! {
        #[export_name = #register_rls_symbol]
        extern "C" fn __register_rls() {
            spacetimedb::rt::register_row_level_security::<#rls_name>()
        }
    };

    let emission = quote! {
        const _: () = {
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        struct #rls_name { _never: ::core::convert::Infallible }
        impl spacetimedb::rt::RowLevelSecurityInfo for #rls_name {
            const SQL: &'static str = #rls_sql;
        }
    };

    emission.into()
}
