use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::{FnArg, ItemFn};

use crate::sym;
use crate::util::{ident_to_litstr, match_meta};

pub(crate) struct ViewArgs {
    anonymous: bool,
}

impl ViewArgs {
    /// Parse `#[view(public)]` where public is required
    pub(crate) fn parse(input: TokenStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "views must be declared as #[view(public)]; public is required",
            ));
        }
        let mut public = false;
        let mut anonymous = false;
        syn::meta::parser(|meta| {
            match_meta!(match meta {
                sym::public => {
                    public = true;
                }
                sym::anonymous => {
                    anonymous = true;
                }
            });
            Ok(())
        })
        .parse2(input)?;
        if !public {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "views must be declared as #[view(public)]; public is required",
            ));
        }
        Ok(Self { anonymous })
    }
}

fn view_impl_anon(original_function: &ItemFn) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let view_name = ident_to_litstr(func_name);
    let vis = &original_function.vis;

    for param in &original_function.sig.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => return Err(err("type parameters are not allowed on views")),
            syn::GenericParam::Const(_) => return Err(err("const parameters are not allowed on views")),
        }
    }

    // Extract all function parameters, except for `self` ones that aren't allowed.
    let typed_args = original_function
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => Ok(arg),
            FnArg::Receiver(_) => Err(syn::Error::new_spanned(arg, "`self` arguments not allowed in views")),
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

    // Extract the context type
    let ctx_ty = arg_tys.first().ok_or_else(|| {
        syn::Error::new_spanned(
            original_function.sig.fn_token,
            "An anonymous view must have `&AnonymousViewContext` as its first argument",
        )
    })?;

    // Extract the return type
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .ok_or_else(|| {
        syn::Error::new_spanned(
            original_function.sig.fn_token,
            "views must return `Vec<T>` where `T` is a `SpacetimeType`",
        )
    })?;

    // Extract the non-context parameters
    let arg_tys = arg_tys.iter().skip(1);

    let register_describer_symbol = format!("__preinit__20_register_describer_{}", view_name.value());

    let lt_params = &original_function.sig.generics;
    let lt_where_clause = &lt_params.where_clause;

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_anonymous_view::<_, #func_name, _>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = { #generated_describe_function };

        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }

        const _: () = {
            fn _assert_args #lt_params () #lt_where_clause {
                  let _ = <#ctx_ty  as spacetimedb::rt::AnonymousViewContextArg>::_ITEM;
                  let _ = <#ret_ty  as spacetimedb::rt::ViewReturn>::_ITEM;
                #(let _ = <#arg_tys as spacetimedb::rt::ViewArg>::_ITEM;)*
            }
        };

        impl #func_name {
            fn invoke(__ctx: spacetimedb::AnonymousViewContext, __args: &[u8]) -> Vec<u8> {
                spacetimedb::rt::invoke_anonymous_view(#func_name, __ctx, __args)
            }
        }

        #[automatically_derived]
        impl spacetimedb::rt::FnInfo for #func_name {
            /// The type of this function
            type Invoke = spacetimedb::rt::AnonymousFn;

            /// The name of this function
            const NAME: &'static str = #view_name;

            /// The parameter names of this function
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];

            /// The pointer for invoking this function
            const INVOKE: Self::Invoke = #func_name::invoke;

            /// The return type of this function
            fn return_type(
                ts: &mut impl spacetimedb::sats::typespace::TypespaceBuilder
            ) -> Option<spacetimedb::sats::AlgebraicType> {
                Some(<#ret_ty as spacetimedb::SpacetimeType>::make_type(ts))
            }
        }
    })
}

fn view_impl_client(original_function: &ItemFn) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let view_name = ident_to_litstr(func_name);
    let vis = &original_function.vis;

    for param in &original_function.sig.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => return Err(err("type parameters are not allowed on views")),
            syn::GenericParam::Const(_) => return Err(err("const parameters are not allowed on views")),
        }
    }

    // Extract all function parameters, except for `self` ones that aren't allowed.
    let typed_args = original_function
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => Ok(arg),
            FnArg::Receiver(_) => Err(syn::Error::new_spanned(arg, "`self` arguments not allowed in views")),
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

    // Extract the context type
    let ctx_ty = arg_tys.first().ok_or_else(|| {
        syn::Error::new_spanned(
            original_function.sig.fn_token,
            "A view must have `&ViewContext` as its first argument",
        )
    })?;

    // Extract the return type
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .ok_or_else(|| {
        syn::Error::new_spanned(
            original_function.sig.fn_token,
            "views must return `Vec<T>` where `T` is a `SpacetimeType`",
        )
    })?;

    // Extract the non-context parameters
    let arg_tys = arg_tys.iter().skip(1);

    let register_describer_symbol = format!("__preinit__20_register_describer_{}", view_name.value());

    let lt_params = &original_function.sig.generics;
    let lt_where_clause = &lt_params.where_clause;

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_view::<_, #func_name, _>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = { #generated_describe_function };

        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }

        const _: () = {
            fn _assert_args #lt_params () #lt_where_clause {
                  let _ = <#ctx_ty  as spacetimedb::rt::ViewContextArg>::_ITEM;
                  let _ = <#ret_ty  as spacetimedb::rt::ViewReturn>::_ITEM;
                #(let _ = <#arg_tys as spacetimedb::rt::ViewArg>::_ITEM;)*
            }
        };

        impl #func_name {
            fn invoke(__ctx: spacetimedb::ViewContext, __args: &[u8]) -> Vec<u8> {
                spacetimedb::rt::invoke_view(#func_name, __ctx, __args)
            }
        }

        #[automatically_derived]
        impl spacetimedb::rt::FnInfo for #func_name {
            /// The type of this function
            type Invoke = spacetimedb::rt::ViewFn;

            /// The name of this function
            const NAME: &'static str = #view_name;

            /// The parameter names of this function
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];

            /// The pointer for invoking this function
            const INVOKE: Self::Invoke = #func_name::invoke;

            /// The return type of this function
            fn return_type(
                ts: &mut impl spacetimedb::sats::typespace::TypespaceBuilder
            ) -> Option<spacetimedb::sats::AlgebraicType> {
                Some(<#ret_ty as spacetimedb::SpacetimeType>::make_type(ts))
            }
        }
    })
}

pub(crate) fn view_impl(args: ViewArgs, original_function: &ItemFn) -> syn::Result<TokenStream> {
    if args.anonymous {
        view_impl_anon(original_function)
    } else {
        view_impl_client(original_function)
    }
}
