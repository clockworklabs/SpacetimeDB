//! Defines procedural macros like `#[spacetimedb::table]`,
//! simplifying writing SpacetimeDB modules in Rust.

// DO NOT WRITE (public) DOCS IN THIS MODULE.
// Docs should be written in the `spacetimedb` crate (i.e. `bindings/`) at reexport sites
// using `#[doc(inline)]`.
// We do this so that links to library traits, structs, etc can resolve correctly.
//
// (private documentation for the macro authors is totally fine here and you SHOULD write that!)

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
use spacetimedb_bindings_macro_input as input;

mod sym {
    use crate::input::sym::{symbol, Symbol};

    symbol!(client_connected);
    symbol!(client_disconnected);
    symbol!(init);
    symbol!(update);

    symbol!(u8);
    symbol!(i8);
    symbol!(u16);
    symbol!(i16);
    symbol!(u32);
    symbol!(i32);
    symbol!(u64);
    symbol!(i64);
    symbol!(u128);
    symbol!(i128);
    symbol!(f32);
    symbol!(f64);
}

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

        let table = input::table::TableArgs::parse(args.into(), &derive_input)?;
        let (table, columns) = input::table::ColumnArgs::parse(table, &derive_input)?;
        let generated = table::table_impl(table, columns, &derive_input)?;
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
    logic: impl FnOnce(&input::sats::SatsType) -> TokenStream,
) -> StdTokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    let crate_fallback = if assume_in_module {
        quote!(spacetimedb::spacetimedb_lib)
    } else {
        quote!(spacetimedb_lib)
    };
    input::sats::sats_type_from_derive(&input, crate_fallback)
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
