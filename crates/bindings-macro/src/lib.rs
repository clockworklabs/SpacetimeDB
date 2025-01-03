//! Defines procedural macros like `#[spacetimedb::table]`,
//! simplifying writing SpacetimeDB modules in Rust.

// DO NOT WRITE (public) DOCS IN THIS MODULE.
// Docs should be written in the `spacetimedb` crate (i.e. `bindings/`) at reexport sites
// using `#[doc(inline)]`.
// We do this so that links to library traits, structs, etc can resolve correctly.
//
// (private documentation for the macro authors is totally fine here and you SHOULD write that!)

mod filter;
mod reducer;
mod sats;
mod table;
mod util;

use crate::util::cvt_attr;
use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::TokenStream;
use quote::quote;
use std::time::Duration;
use syn::parse::ParseStream;
use syn::ItemFn;

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
    symbol!(scheduled_at);
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

#[proc_macro_attribute]
pub fn reducer(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
    cvt_attr::<ItemFn>(args, item, quote!(), |args, original_function| {
        let args = reducer::ReducerArgs::parse(args)?;
        reducer::reducer_impl(args, original_function)
    })
}


#[proc_macro_attribute]
pub fn table(args: StdTokenStream, item: StdTokenStream) -> StdTokenStream {
    // put this on the struct so we don't get unknown attribute errors
    let extra_attr = quote!(#[derive(spacetimedb::__TableHelper)]);
    cvt_attr::<syn::DeriveInput>(args, item, extra_attr, |args, item| {
        let args = table::TableArgs::parse(args, &item.ident)?;
        table::table_impl(args, item)
    })
}

/// Provides helper attributes for `#[spacetimedb::table]`, so that we don't get unknown attribute errors.
#[doc(hidden)]
#[proc_macro_derive(__TableHelper, attributes(sats, unique, auto_inc, primary_key, index, scheduled_at))]
pub fn table_helper(_input: StdTokenStream) -> StdTokenStream {
    Default::default()
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

#[proc_macro]
pub fn filter(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let arg = syn::parse_macro_input!(input);
    filter::filter_impl(arg)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
