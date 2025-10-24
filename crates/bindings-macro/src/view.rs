use heck::ToSnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::ext::IdentExt;
use syn::parse::Parser;
use syn::{FnArg, ItemFn};

use crate::sym;
use crate::util::{check_duplicate_msg, match_meta};

pub(crate) struct ViewArgs {
    name: Ident,
    #[allow(unused)]
    public: bool,
}

impl ViewArgs {
    /// Parse `#[view(name = ..., public)]` where both `name` and `public` are required.
    pub(crate) fn parse(input: TokenStream, func_ident: &Ident) -> syn::Result<Self> {
        let mut name = None;
        let mut public = None;
        syn::meta::parser(|meta| {
            match_meta!(match meta {
                sym::name => {
                    check_duplicate_msg(&name, &meta, "`name` already specified")?;
                    name = Some(meta.value()?.parse()?);
                }
                sym::public => {
                    check_duplicate_msg(&public, &meta, "`public` already specified")?;
                    public = Some(());
                }
            });
            Ok(())
        })
        .parse2(input)?;
        let name = name.ok_or_else(|| {
            let view = func_ident.to_string().to_snake_case();
            syn::Error::new(
                Span::call_site(),
                format_args!("must specify view name, e.g. `#[spacetimedb::view(name = {view})]"),
            )
        })?;
        let () = public
            .ok_or_else(|| syn::Error::new(Span::call_site(), "views must be `public`, e.g. `#[view(public)]`"))?;
        Ok(Self { name, public: true })
    }
}

pub(crate) fn view_impl(args: ViewArgs, original_function: &ItemFn) -> syn::Result<TokenStream> {
    let vis = &original_function.vis;
    let func_name = &original_function.sig.ident;
    let view_ident = args.name;
    let view_name = view_ident.unraw().to_string();

    for param in &original_function.sig.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => return Err(err("type parameters are not allowed on views")),
            syn::GenericParam::Const(_) => return Err(err("const parameters are not allowed on views")),
        }
    }

    // Extract parameters
    let typed_args = original_function
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => Ok(arg),
            FnArg::Receiver(_) => Err(syn::Error::new_spanned(
                arg,
                "The `self` parameter is not allowed in views",
            )),
        })
        .collect::<syn::Result<Vec<_>>>()?;

    // Extract parameter names
    let opt_arg_names = typed_args.iter().map(|arg| {
        if let syn::Pat::Ident(i) = &*arg.pat {
            let name = i.ident.to_string();
            quote!(Some(#name))
        } else {
            quote!(None)
        }
    });

    let arg_tys = typed_args.iter().map(|arg| arg.ty.as_ref()).collect::<Vec<_>>();

    // Extract the context type and the rest of the parameter types
    let [ctx_ty, arg_tys @ ..] = &arg_tys[..] else {
        return Err(syn::Error::new_spanned(
            &original_function.sig,
            "Views must always have a context parameter: `&ViewContext` or `&AnonymousViewContext`",
        ));
    };

    // Extract the context type
    let ctx_ty = match ctx_ty {
        syn::Type::Reference(ctx_ty) => ctx_ty.elem.as_ref(),
        _ => {
            return Err(syn::Error::new_spanned(
                ctx_ty,
                "The first parameter of a view must be a context parameter: `&ViewContext` or `&AnonymousViewContext`; passed by reference",
            ));
        }
    };

    // Views must return a result
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Type(_, t) => t.as_ref(),
        syn::ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &original_function.sig,
                "views must return `Vec<T>` or `Option<T>` where `T` is a `SpacetimeType`",
            ));
        }
    };

    let register_describer_symbol = format!("__preinit__20_register_describer_{}", view_name);

    let lt_params = &original_function.sig.generics;
    let lt_where_clause = &lt_params.where_clause;

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::ViewRegistrar::<#ctx_ty>::register::<_, #func_name, _, _>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = { #generated_describe_function };

        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }

        const _: () = {
            fn _assert_args #lt_params () #lt_where_clause {
                let _ = <#ctx_ty as spacetimedb::rt::ViewContextArg>::_ITEM;
                let _ = <#ret_ty as spacetimedb::rt::ViewReturn>::_ITEM;
            }
        };

        const _: () = {
            fn _assert_args #lt_params () #lt_where_clause {
                #(let _ = <#arg_tys as spacetimedb::rt::ViewArg>::_ITEM;)*
            }
        };

        impl #func_name {
            fn invoke(__ctx: #ctx_ty, __args: &[u8]) -> Vec<u8> {
                spacetimedb::rt::ViewDispatcher::<#ctx_ty>::invoke::<_, _, _>(#func_name, __ctx, __args)
            }
        }

        #[automatically_derived]
        impl spacetimedb::rt::FnInfo for #func_name {
            /// The type of this function
            type Invoke = <spacetimedb::rt::ViewKind<#ctx_ty> as spacetimedb::rt::ViewKindTrait>::InvokeFn;

            /// The function kind, which will cause scheduled tables to reject views.
            type FnKind = spacetimedb::rt::FnKindView;

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
