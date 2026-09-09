use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeSet;
use syn::ext::IdentExt as _;
use syn::{Data, DeriveInput, Fields, LitStr};

pub(crate) fn derive(item: DeriveInput) -> syn::Result<TokenStream> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "environment value enums cannot be generic",
        ));
    }
    if let Some(attr) = item.attrs.iter().find(|attr| attr.path().is_ident("env")) {
        return Err(syn::Error::new_spanned(
            attr,
            "place `#[env(value = \"...\")]` on enum variants",
        ));
    }
    let Data::Enum(data) = &item.data else {
        return Err(syn::Error::new_spanned(
            &item.ident,
            "EnvironmentValue requires an enum with unit variants",
        ));
    };
    if data.variants.is_empty() || data.variants.len() > 256 {
        return Err(syn::Error::new_spanned(
            &item.ident,
            "environment value enums require 1 to 256 variants",
        ));
    }
    let mut variants = Vec::new();
    let mut values = Vec::new();
    let mut unique = BTreeSet::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "environment value variants cannot have payloads",
            ));
        }
        let mut value: Option<LitStr> = None;
        for attr in variant.attrs.iter().filter(|attr| attr.path().is_ident("env")) {
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("value") || value.is_some() {
                    return Err(meta.error("expected one value = \"literal\" mapping"));
                }
                value = Some(meta.value()?.parse()?);
                Ok(())
            })?;
            if value.is_none() {
                return Err(syn::Error::new_spanned(attr, "expected value = \"literal\" mapping"));
            }
        }
        let value = value.unwrap_or_else(|| LitStr::new(&variant.ident.unraw().to_string(), variant.ident.span()));
        if value.value().len() > 8192 {
            return Err(syn::Error::new_spanned(
                value,
                "environment literal exceeds 8192 UTF-8 bytes",
            ));
        }
        if !unique.insert(value.value()) {
            return Err(syn::Error::new_spanned(
                value,
                "environment variants must map to distinct strings",
            ));
        }
        variants.push(&variant.ident);
        values.push(value);
    }
    let constraint = match values.as_slice() {
        [value] => quote!(::spacetimedb::spacetimedb_lib::environment::EnvironmentConstraint::Literal(#value.into())),
        values => quote!(
            ::spacetimedb::spacetimedb_lib::environment::EnvironmentConstraint::OneOf(::std::vec![#(#values.into()),*])
        ),
    };
    let ident = &item.ident;
    Ok(quote! {
        impl ::spacetimedb::rt::EnvironmentValue for #ident {
            const OPTIONAL: bool = false;

            fn constraint() -> ::spacetimedb::spacetimedb_lib::environment::EnvironmentConstraint {
                #constraint
            }

            fn from_environment(value: ::std::option::Option<::std::string::String>, key: &str) -> Self {
                match value.as_deref() {
                    #(::std::option::Option::Some(#values) => Self::#variants,)*
                    ::std::option::Option::Some(_) => ::core::panic!("environment value does not match its declared enum: {}", key),
                    ::std::option::Option::None => ::core::panic!("required environment key is missing: {}", key),
                }
            }
        }
        impl ::spacetimedb::rt::RequiredEnvironmentValue for #ident {}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_enum_shapes_mappings_and_limits() {
        for input in [
            quote!(
                struct Value;
            ),
            quote!(
                enum Value {}
            ),
            quote!(
                enum Value<T> {
                    Item(T),
                }
            ),
            quote!(
                enum Value {
                    Item(String),
                }
            ),
            quote!(
                enum Value {
                    Item { field: String },
                }
            ),
            quote!(
                enum Value {
                    #[env()]
                    Item,
                }
            ),
            quote!(
                enum Value {
                    #[env(values("x"))]
                    Item,
                }
            ),
            quote!(
                enum Value {
                    #[env(value = "x", value = "y")]
                    Item,
                }
            ),
            quote!(
                enum Value {
                    #[env(value = "x")]
                    #[env(value = "y")]
                    Item,
                }
            ),
            quote!(
                enum Value {
                    #[env(value = "Same")]
                    First,
                    Same,
                }
            ),
            quote!(
                #[env(value = "x")]
                enum Value {
                    Item,
                }
            ),
        ] {
            assert!(derive(syn::parse2(input).unwrap()).is_err());
        }
        let oversized = LitStr::new(&"é".repeat(4097), proc_macro2::Span::call_site());
        assert!(derive(syn::parse_quote!(
            enum Value {
                #[env(value = #oversized)]
                Item,
            }
        ))
        .is_err());
        let variants = (0..257).map(|n| quote::format_ident!("V{n}"));
        assert!(derive(syn::parse_quote!(enum Value { #(#variants),* })).is_err());
    }

    #[test]
    fn accepts_exact_strings_and_ordinary_enum_attributes() {
        let output = derive(syn::parse_quote! {
            #[derive(Debug, PartialEq)]
            enum Value {
                #[env(value = "in progress")] InProgress,
                #[env(value = "")] Empty,
                #[env(value = "héllo\0世界")] Unicode,
                r#type,
            }
        })
        .unwrap();
        syn::parse2::<syn::File>(output).unwrap();
    }
}
