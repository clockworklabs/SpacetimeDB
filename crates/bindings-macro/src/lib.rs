//! Defines procedural macros like `#[spacetimedb::table]`,
//! simplifying writing SpacetimeDB modules in Rust.

mod reducer;
mod sats;
mod table;
mod util;

use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::TokenStream;
use quote::quote;
use std::time::Duration;
use syn::{parse::ParseStream, Attribute};
use syn::{ItemConst, ItemFn};
use util::{cvt_attr, ok_or_compile_error};

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

    symbol!(at);
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
        let args = reducer::ReducerArgs::parse(args)?;
        reducer::reducer_impl(args, original_function)
    })
}

/// It turns out to be shockingly difficult to construct an [`Attribute`].
/// That type is not [`Parse`], instead having two distinct methods
/// for parsing "inner" vs "outer" attributes.
///
/// We need this [`Attribute`] in [`table`] so that we can "pushnew" it
/// onto the end of a list of attributes. See comments within [`table`].
fn derive_table_helper_attr() -> Attribute {
    let source = quote!(#[derive(spacetimedb::__TableHelper)]);

    syn::parse::Parser::parse2(Attribute::parse_outer, source)
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
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
    let derive_table_helper: syn::Attribute = derive_table_helper_attr();

    ok_or_compile_error(|| {
        let item = TokenStream::from(item);
        let mut derive_input: syn::DeriveInput = syn::parse2(item.clone())?;

        // Add `derive(__TableHelper)` only if it's not already in the attributes of the `derive_input.`
        // If multiple `#[table]` attributes are applied to the same `struct` item,
        // this will ensure that we don't emit multiple conflicting implementations
        // for traits like `SpacetimeType`, `Serialize` and `Deserialize`.
        //
        // We need to push at the end, rather than the beginning,
        // because rustc expands attribute macros (including derives) top-to-bottom,
        // and we need *all* `#[table]` attributes *before* the `derive(__TableHelper)`.
        // This way, the first `table` will insert a `derive(__TableHelper)`,
        // and all subsequent `#[table]`s on the same `struct` will see it,
        // and not add another.
        //
        // Note, thank goodness, that `syn`'s `PartialEq` impls (provided with the `extra-traits` feature)
        // skip any [`Span`]s contained in the items,
        // thereby comparing for syntactic rather than structural equality. This shouldn't matter,
        // since we expect that the `derive_table_helper` will always have the same [`Span`]s,
        // but it's nice to know.
        if !derive_input.attrs.contains(&derive_table_helper) {
            derive_input.attrs.push(derive_table_helper);
        }

        let args = table::TableArgs::parse(args.into(), &derive_input.ident)?;
        let generated = table::table_impl(args, &derive_input)?;
        Ok(TokenStream::from_iter([quote!(#derive_input), generated]))
    })
}

/// Special alias for `derive(SpacetimeType)`, aka [`schema_type`], for use by [`table`].
///
/// Provides helper attributes for `#[spacetimedb::table]`, so that we don't get unknown attribute errors.
#[doc(hidden)]
#[proc_macro_derive(__TableHelper, attributes(sats, unique, auto_inc, primary_key, index))]
pub fn table_helper(input: StdTokenStream) -> StdTokenStream {
    schema_type(input)
}

#[proc_macro]
pub fn duration(input: StdTokenStream) -> StdTokenStream {
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

/// A helper for the common bits of the derive macros.
fn sats_derive(
    input: StdTokenStream,
    assume_in_module: bool,
    logic: impl FnOnce(&sats::SatsType) -> TokenStream,
) -> StdTokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    let crate_fallback = if assume_in_module {
        quote!(spacetimedb::spacetimedb_lib)
    } else {
        quote!(spacetimedb_lib)
    };
    sats::sats_type_from_derive(&input, crate_fallback)
        .map(|ty| logic(&ty))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Deserialize, attributes(sats))]
pub fn deserialize(input: StdTokenStream) -> StdTokenStream {
    sats_derive(input, false, sats::derive_deserialize)
}

#[proc_macro_derive(Serialize, attributes(sats))]
pub fn serialize(input: StdTokenStream) -> StdTokenStream {
    sats_derive(input, false, sats::derive_serialize)
}

#[proc_macro_derive(SpacetimeType, attributes(sats))]
pub fn schema_type(input: StdTokenStream) -> StdTokenStream {
    sats_derive(input, true, |ty| {
        let ident = ty.ident;
        let name = &ty.name;
        let krate = &ty.krate;
        TokenStream::from_iter([
            sats::derive_satstype(ty),
            sats::derive_deserialize(ty),
            sats::derive_serialize(ty),
            // unfortunately, generic types don't work in modules at the moment.
            quote!(#krate::__make_register_reftype!(#ident, #name);),
        ])
    })
}

/// Generates code for registering a row-level security rule.
///
/// This attribute must be applied to a `const` binding of type [`Filter`].
/// It will be interpreted as a filter on the table to which it applies, for all client queries.
/// If a module contains multiple `client_visibility_filter`s for the same table,
/// they will be unioned together as if by SQL `OR`,
/// so that any row permitted by at least one filter is visible.
///
/// The `const` binding's identifier must be unique within the module.
///
/// The query follows the same syntax as a subscription query.
///
/// ## Example:
///
/// ```rust,ignore
/// /// Players can only see what's in their chunk
/// #[spacetimedb::client_visibility_filter]
/// const PLAYERS_SEE_ENTITIES_IN_SAME_CHUNK: Filter = Filter::Sql("
///     SELECT * FROM LocationState WHERE chunk_index IN (
///         SELECT chunk_index FROM LocationState WHERE entity_id IN (
///             SELECT entity_id FROM UserState WHERE identity = @sender
///         )
///     )
/// ");
/// ```
///
/// Queries are not checked for syntactic or semantic validity
/// until they are processed by the SpacetimeDB host.
/// This means that errors in queries, such as syntax errors, type errors or unknown tables,
/// will be reported during `spacetime publish`, not at compile time.
#[doc(hidden)] // TODO: RLS filters are currently unimplemented, and are not enforced.
#[proc_macro_attribute]
pub fn client_visibility_filter(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
    ok_or_compile_error(|| {
        if !args.is_empty() {
            return Err(syn::Error::new_spanned(
                TokenStream::from(args),
                "The `client_visibility_filter` attribute does not accept arguments",
            ));
        }

        let item: ItemConst = syn::parse(item)?;
        let rls_ident = item.ident.clone();
        let register_rls_symbol = format!("__preinit__20_register_row_level_security_{rls_ident}");

        Ok(quote! {
            #item

            const _: () = {
                #[export_name = #register_rls_symbol]
                extern "C" fn __register_client_visibility_filter() {
                    spacetimedb::rt::register_row_level_security(#rls_ident.sql_text())
                }
            };
        })
    })
}
