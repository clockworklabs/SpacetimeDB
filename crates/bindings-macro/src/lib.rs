//! Defines procedural macros like `#[spacetimedb::table]`,
//! simplifying writing SpacetimeDB modules in Rust.

#![crate_type = "proc-macro"]
extern crate proc_macro;

#[macro_use]
mod util;

mod module_items;
mod query;
mod sats;

use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::TokenStream;
use quote::quote;
use std::time::Duration;
use syn::parse::{Parse, ParseStream};

use sats::{derive_deserialize, derive_satstype, derive_serialize};

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

    symbol!(autoinc);
    symbol!(btree);
    symbol!(columns);
    symbol!(crate_, crate);
    symbol!(hash);
    symbol!(index);
    symbol!(name);
    symbol!(primarykey);
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

trait ParseArgs: Sized {
    fn parse_from_args(input: TokenStream) -> syn::Result<Self>;
}

impl ParseArgs for syn::parse::Nothing {
    fn parse_from_args(input: TokenStream) -> syn::Result<Self> {
        syn::parse2(input)
    }
}

/// Parses `item`, passing it and `args` to `f`,
/// which should return only whats newly added, excluding the `item`.
/// Returns the full token stream `extra_attr item newly_added`.
fn cvt_attr<Args: ParseArgs, Item: Parse>(
    args: StdTokenStream,
    item: StdTokenStream,
    extra_attr: TokenStream,
    f: impl FnOnce(Args, Item) -> syn::Result<TokenStream>,
) -> StdTokenStream {
    let item: TokenStream = item.into();
    let parsed_item = match syn::parse2::<Item>(item.clone()) {
        Ok(i) => i,
        Err(e) => return TokenStream::from_iter([item, e.into_compile_error()]).into(),
    };
    let generated = Args::parse_from_args(args.into())
        .and_then(|args| f(args, parsed_item))
        .unwrap_or_else(syn::Error::into_compile_error);
    TokenStream::from_iter([extra_attr, item, generated]).into()
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
    cvt_attr(args, item, quote!(), module_items::reducer_impl)
}

/// Generates code for treating this struct type as a table.
///
/// Among other things, this derives [`Serialize`], [`Deserialize`],
/// [`SpacetimeType`], and [`TableType`] for our type.
///
/// # Example
///
/// ```no_run
/// #[spacetimedb::table(public, name = "Users")]
/// pub struct User {
///     #[autoinc]
///     #[primarykey]
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
    cvt_attr(args, item, extra_attr, module_items::table_impl)
}

#[doc(hidden)]
#[proc_macro_derive(__TableHelper, attributes(sats, unique, autoinc, primarykey, index))]
pub fn table_helper(_input: StdTokenStream) -> StdTokenStream {
    Default::default()
}

macro_rules! special_reducer {
    ($(#[$attr:meta])* $name:ident = $reducer_name:literal) => {
        #[proc_macro_attribute]
        pub fn $name(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
            cvt_attr(args, item, quote!(), module_items::special_reducer($reducer_name))
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
    let (secs, nanos) = (dur.as_secs(), dur.subsec_nanos());
    quote!({
        const DUR: ::core::time::Duration = ::core::time::Duration::new(#secs, #nanos);
        DUR
    })
    .into()
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
    sats::sats_type_from_derive(&input, quote!(spacetimedb_lib))
        .map(|ty| derive_deserialize(&ty))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Serialize, attributes(sats))]
pub fn serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    sats::sats_type_from_derive(&input, quote!(spacetimedb_lib))
        .map(|ty| derive_serialize(&ty))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(SpacetimeType, attributes(sats))]
pub fn schema_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    (|| {
        let ty = sats::sats_type_from_derive(&input, quote!(spacetimedb::spacetimedb_lib))?;

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
    query::query_impl(input.into()).into()
}
