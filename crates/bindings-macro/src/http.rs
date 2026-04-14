use crate::reducer::{assert_only_lifetime_generics, extract_typed_args};
use crate::util::ident_to_litstr;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, ReturnType};

pub(crate) fn handler_impl(args: TokenStream, original_function: &ItemFn) -> syn::Result<TokenStream> {
    if !args.is_empty() {
        return Err(syn::Error::new_spanned(
            args,
            "The `handler` attribute does not accept arguments",
        ));
    }

    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;
    let handler_name = ident_to_litstr(func_name);

    assert_only_lifetime_generics(original_function, "http handlers")?;

    let typed_args = extract_typed_args(original_function)?;
    if typed_args.len() != 2 {
        return Err(syn::Error::new_spanned(
            original_function.sig.clone(),
            "HTTP handlers must take exactly two arguments",
        ));
    }

    let arg_tys = typed_args.iter().map(|arg| arg.ty.as_ref()).collect::<Vec<_>>();
    let first_arg_ty = &arg_tys[0];
    let second_arg_ty = &arg_tys[1];

    let ret_ty = match &original_function.sig.output {
        ReturnType::Type(_, t) => t.as_ref(),
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                original_function.sig.clone(),
                "HTTP handlers must return `spacetimedb::http::Response`",
            ));
        }
    };

    let internal_ident = syn::Ident::new(&format!("__spacetimedb_http_handler_{func_name}"), func_name.span());
    let mut inner_fn = original_function.clone();
    inner_fn.sig.ident = internal_ident.clone();

    let register_describer_symbol = format!("__preinit__20_register_http_handler_{}", handler_name.value());

    let lifetime_params = &original_function.sig.generics;
    let lifetime_where_clause = &lifetime_params.where_clause;

    let generated_describe_function = quote! {
        #[unsafe(export_name = #register_describer_symbol)]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_http_handler(#handler_name, #internal_ident)
        }
    };

    Ok(quote! {
        #inner_fn

        #vis const #func_name: spacetimedb::http::Handler = spacetimedb::http::Handler::new(#handler_name);

        const _: () = {
            #generated_describe_function
        };

        const _: () = {
            fn _assert_args #lifetime_params () #lifetime_where_clause {
                let _ = <#first_arg_ty as spacetimedb::rt::HttpHandlerContextArg>::_ITEM;
                let _ = <#second_arg_ty as spacetimedb::rt::HttpHandlerRequestArg>::_ITEM;
                let _ = <#ret_ty as spacetimedb::rt::HttpHandlerReturn>::_ITEM;
            }
        };
    })
}

pub(crate) fn router_impl(args: TokenStream, original_function: &ItemFn) -> syn::Result<TokenStream> {
    if !args.is_empty() {
        return Err(syn::Error::new_spanned(
            args,
            "The `router` attribute does not accept arguments",
        ));
    }

    if !original_function.sig.inputs.is_empty() {
        return Err(syn::Error::new_spanned(
            original_function.sig.clone(),
            "HTTP router functions must take no arguments",
        ));
    }

    let func_name = &original_function.sig.ident;
    let register_symbol = "__preinit__30_register_http_router";

    Ok(quote! {
        #original_function

        const _: () = {
            fn _assert_router() {
                // TODO(cleanup): Why two bindings here?
                let _f: fn() -> spacetimedb::http::Router = #func_name;
                let _ = _f;
            }
        };

        const _: () = {
            #[unsafe(export_name = #register_symbol)]
            pub extern "C" fn __register_router() {
                spacetimedb::rt::register_http_router(#func_name)
            }
        };
    })
}
