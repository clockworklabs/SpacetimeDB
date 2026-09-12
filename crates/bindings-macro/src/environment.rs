pub(crate) mod value;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ext::IdentExt as _;
use syn::punctuated::Punctuated;
use syn::{Fields, ItemStruct, LitStr, Token};

pub(crate) fn expand(args: TokenStream, mut item: ItemStruct) -> syn::Result<TokenStream> {
    if !args.is_empty() {
        return Err(syn::Error::new_spanned(args, "env does not accept arguments"));
    }
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "environment declarations cannot be generic",
        ));
    }
    if matches!(item.fields, Fields::Unnamed(_)) {
        return Err(syn::Error::new_spanned(
            &item.fields,
            "environment declarations require named fields",
        ));
    }
    let mut declarations = Vec::new();
    let mut signatures = Vec::new();
    let mut methods = Vec::new();
    for field in &mut item.fields {
        let ident = field.ident.as_ref().expect("named fields checked");
        let name = ident.unraw().to_string();
        if name.len() > 256 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(syn::Error::new_spanned(
                ident,
                "environment keys must be POSIX names of at most 256 bytes",
            ));
        }
        let ty = &field.ty;
        let mut values: Option<Vec<LitStr>> = None;
        for attr in field.attrs.iter().filter(|attr| attr.path().is_ident("env")) {
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("values") || values.is_some() {
                    return Err(meta.error("expected one values(\"literal\", ...) constraint"));
                }
                let content;
                syn::parenthesized!(content in meta.input);
                let parsed = Punctuated::<LitStr, Token![,]>::parse_terminated(&content)?;
                if parsed.is_empty() {
                    return Err(meta.error("environment string unions must not be empty"));
                }
                for value in &parsed {
                    if value.value().len() > 8192 {
                        return Err(syn::Error::new_spanned(
                            value,
                            "environment literal exceeds 8192 UTF-8 bytes",
                        ));
                    }
                }
                values = Some(parsed.into_iter().collect());
                Ok(())
            })?;
        }
        field.attrs.retain(|attr| !attr.path().is_ident("env"));
        let constraint = match values.as_deref() {
            None => quote!(<#ty as ::spacetimedb::rt::EnvironmentValue>::constraint()),
            Some([value]) => {
                quote!(::spacetimedb::spacetimedb_lib::environment::EnvironmentConstraint::Literal(#value.into()))
            }
            Some(values) => quote!(
                ::spacetimedb::spacetimedb_lib::environment::EnvironmentConstraint::OneOf(
                    ::std::vec![#(#values.into()),*]
                )
            ),
        };
        let constraint = if values.is_some() {
            quote!(<#ty as ::spacetimedb::rt::StringEnvironmentValue>::with_constraint(#constraint))
        } else {
            constraint
        };
        declarations.push(
            quote!(::spacetimedb::spacetimedb_lib::environment::EnvironmentDeclaration {
                name: #name.into(),
                constraint: #constraint,
                optional: <#ty as ::spacetimedb::rt::EnvironmentValue>::OPTIONAL,
            }),
        );
        if name == "get" {
            continue;
        }
        signatures.push(quote! {
            #[doc = concat!("Read the declared environment key `", #name, "` through the checked host ABI.")]
            fn #ident(&self) -> #ty;
        });
        methods.push(quote! {
            fn #ident(&self) -> #ty {
                <#ty as ::spacetimedb::rt::EnvironmentValue>::get(self, #name)
            }
        });
    }
    let vis = &item.vis;
    let access = format_ident!("{}Access", item.ident.unraw());
    let symbol = format!("__preinit__20_register_environment_{}", item.ident.unraw());
    Ok(quote! {
        #[allow(non_snake_case)]
        #item

        /// Named read-only accessors for this module's environment declaration.
        #[allow(non_snake_case)]
        #vis trait #access {
            #(#signatures)*
        }
        #[allow(non_snake_case)]
        impl #access for ::spacetimedb::Environment {
            #(#methods)*
        }
        const _: () = {
            #[unsafe(export_name = #symbol)]
            extern "C" fn __register_environment() {
                ::spacetimedb::rt::register_environment(|| ::std::vec![#(#declarations),*]);
            }
        };
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_union_invalid_names_and_unsupported_struct_shapes() {
        for item in [
            quote!(
                struct Env {
                    #[env(values())]
                    VALUE: String,
                }
            ),
            quote!(
                struct Env {
                    #[env(values("a"), values("b"))]
                    VALUE: String,
                }
            ),
            quote!(
                struct Env {
                    雪: String,
                }
            ),
            quote!(
                struct Env(String);
            ),
            quote!(
                struct Env<T> {
                    VALUE: T,
                }
            ),
        ] {
            assert!(expand(TokenStream::new(), syn::parse2(item).unwrap()).is_err());
        }
    }

    #[test]
    fn accepts_strings_optionals_constraints_and_generic_accessor_collision() {
        let output = expand(
            TokenStream::new(),
            syn::parse_quote! {
                pub struct Env {
                    VALUE: String,
                    #[env(values("false", "true"))] ENABLED: std::string::String,
                    #[env(values(""))] OPTIONAL: Option<String>,
                    get: Option<String>,
                    r#type: String,
                }
            },
        )
        .unwrap();
        let parsed: syn::File = syn::parse2(output).unwrap();
        let trait_item = parsed
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Trait(item) => Some(item),
                _ => None,
            })
            .unwrap();
        let names: Vec<_> = trait_item
            .items
            .iter()
            .filter_map(|item| match item {
                syn::TraitItem::Fn(method) => Some(method.sig.ident.unraw().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["VALUE", "ENABLED", "OPTIONAL", "type"]);
        assert!(expand(
            TokenStream::new(),
            syn::parse_quote!(
                pub struct Empty {}
            )
        )
        .is_ok());
    }
}
