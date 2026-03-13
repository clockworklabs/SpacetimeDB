use crate::reducer::{assert_only_lifetime_generics, extract_typed_args, generate_explicit_names_impl};
use crate::sym;
use crate::util::{check_duplicate, ident_to_litstr, match_meta};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::Parser as _;
use syn::{Expr, ExprCall, ExprLit, ExprPath, ItemFn, Lit, LitStr};

#[derive(Default)]
pub(crate) struct ProcedureArgs {
    /// For consistency with reducers: allow specifying a different export name than the Rust function name.
    name: Option<LitStr>,
    route: Option<RouteAttr>,
}

#[derive(Clone)]
pub(crate) struct RouteAttr {
    method: syn::Ident,
    path: LitStr,
}

impl ProcedureArgs {
    pub(crate) fn parse(input: TokenStream) -> syn::Result<Self> {
        let mut args = Self::default();
        syn::meta::parser(|meta| {
            match_meta!(match meta {
                sym::name => {
                    check_duplicate(&args.name, &meta)?;
                    args.name = Some(meta.value()?.parse()?);
                }
                sym::route => {
                    check_duplicate(&args.route, &meta)?;
                    let expr: Expr = meta.value()?.parse()?;
                    args.route = Some(parse_route_expr(expr)?);
                }
            });
            Ok(())
        })
        .parse2(input)?;
        Ok(args)
    }
}

fn parse_route_expr(expr: Expr) -> syn::Result<RouteAttr> {
    let Expr::Call(ExprCall { func, args, .. }) = expr else {
        return Err(syn::Error::new_spanned(expr, "expected `route = method(\"/path\")`"));
    };

    let Expr::Path(ExprPath { path, .. }) = *func else {
        return Err(syn::Error::new_spanned(func, "expected `route = method(\"/path\")`"));
    };

    let method = path
        .get_ident()
        .cloned()
        .ok_or_else(|| syn::Error::new_spanned(path, "expected method identifier like `get` or `post`"))?;

    if args.len() != 1 {
        return Err(syn::Error::new_spanned(args, "expected a single path argument"));
    }

    let Expr::Lit(ExprLit {
        lit: Lit::Str(path), ..
    }) = args.first().unwrap()
    else {
        return Err(syn::Error::new_spanned(args, "expected a string literal path"));
    };

    Ok(RouteAttr {
        method,
        path: path.clone(),
    })
}

pub(crate) fn procedure_impl(_args: ProcedureArgs, original_function: &ItemFn) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;
    let explicit_name = _args.name.as_ref();
    let route = _args.route.as_ref();

    let procedure_name = ident_to_litstr(func_name);

    assert_only_lifetime_generics(original_function, "procedures")?;

    let typed_args = extract_typed_args(original_function)?;
    let is_http_route = route.is_some();

    if is_http_route && typed_args.len() != 2 {
        return Err(syn::Error::new_spanned(
            original_function.sig.clone(),
            "HTTP route procedures must take `(&mut ProcedureContext, Request)`",
        ));
    }

    // TODO: Require that procedures be `async` functions syntactically,
    // and use `futures_util::FutureExt::now_or_never` to poll them.
    // if !&original_function.sig.asyncness.is_some() {
    //     return Err(syn::Error::new_spanned(
    //         original_function.sig.clone(),
    //         "procedures must be `async`",
    //     ));
    // };

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
    let ret_ty_for_assert = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let ret_ty_for_info = match &original_function.sig.output {
        syn::ReturnType::Default => quote!(()),
        syn::ReturnType::Type(_, t) => quote!(#t),
    };

    let register_describer_symbol = format!("__preinit__20_register_describer_{}", procedure_name.value());

    let lifetime_params = &original_function.sig.generics;
    let lifetime_where_clause = &lifetime_params.where_clause;

    let (generated_describe_function, wrapper_fn, invoke_target, fn_kind_ty, return_type_ty) = if let Some(route) =
        route
    {
        let RouteAttr { method, path } = route.clone();
        let method_str = method.to_string();
        let method_expr = match method_str.as_str() {
            "get" => quote!(spacetimedb::spacetimedb_lib::http::Method::Get),
            "post" => quote!(spacetimedb::spacetimedb_lib::http::Method::Post),
            "put" => quote!(spacetimedb::spacetimedb_lib::http::Method::Put),
            "delete" => quote!(spacetimedb::spacetimedb_lib::http::Method::Delete),
            "patch" => quote!(spacetimedb::spacetimedb_lib::http::Method::Patch),
            _ => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "unsupported HTTP method; expected get, post, put, delete, or patch",
                ));
            }
        };

        let path_value = path.value();
        let valid_path = path_value.starts_with('/') && !path_value[1..].is_empty() && !path_value[1..].contains('/');
        if !valid_path {
            return Err(syn::Error::new_spanned(
                path,
                "route path must be a single segment starting with `/`",
            ));
        }

        let wrapper_name = format_ident!("__spacetimedb_http_route_wrapper_{}", func_name);
        let wrapper_fn = quote! {
            fn #wrapper_name(
                __ctx: &mut spacetimedb::ProcedureContext,
                __request: spacetimedb::spacetimedb_lib::http::RequestAndBody,
            ) -> spacetimedb::spacetimedb_lib::http::ResponseAndBody {
                let __request = match spacetimedb::http::request_and_body_to_http(__request) {
                    Ok(req) => req,
                    Err(_) => {
                        let response = spacetimedb::http::Response::builder()
                            .status(400)
                            .body(spacetimedb::http::Body::empty())
                            .expect("Failed to build error response");
                        return spacetimedb::http::response_to_response_and_body(response);
                    }
                };
                let __response = #func_name(__ctx, __request);
                spacetimedb::http::response_to_response_and_body(__response)
            }
        };

        let describe = quote! {
            #[unsafe(export_name = #register_describer_symbol)]
            pub extern "C" fn __register_describer() {
                spacetimedb::rt::register_http_route_procedure::<#func_name>(#method_expr, #path)
            }
        };

        (
            describe,
            wrapper_fn,
            quote!(#wrapper_name),
            quote!(spacetimedb::rt::FnKindProcedure<spacetimedb::spacetimedb_lib::http::ResponseAndBody>),
            quote!(spacetimedb::spacetimedb_lib::http::ResponseAndBody),
        )
    } else {
        let describe = quote! {
            #[unsafe(export_name = #register_describer_symbol)]
            pub extern "C" fn __register_describer() {
                spacetimedb::rt::register_procedure::<_, _, #func_name>(#func_name)
            }
        };
        (
            describe,
            quote!(),
            quote!(#func_name),
            quote!(spacetimedb::rt::FnKindProcedure<#ret_ty_for_info>),
            ret_ty_for_info.clone(),
        )
    };

    let generate_explicit_names = generate_explicit_names_impl(&procedure_name.value(), func_name, explicit_name);

    let assert_args_block = if is_http_route {
        quote!()
    } else {
        quote! {
            const _: () = {
                fn _assert_args #lifetime_params () #lifetime_where_clause {
                    #(let _ = <#first_arg_ty as spacetimedb::rt::ProcedureContextArg>::_ITEM;)*
                    #(let _ = <#rest_arg_tys as spacetimedb::rt::ProcedureArg>::_ITEM;)*
                    #(let _ = <#ret_ty_for_assert as spacetimedb::rt::IntoProcedureResult>::to_result;)*
                }
            };
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
        };
        #wrapper_fn
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        #assert_args_block
        impl #func_name {
            fn invoke(__ctx: &mut spacetimedb::ProcedureContext, __args: &[u8]) -> spacetimedb::ProcedureResult {
                spacetimedb::rt::invoke_procedure(#invoke_target, __ctx, __args)
            }
        }
        #[automatically_derived]
        impl spacetimedb::rt::FnInfo for #func_name {
            /// The type of this function.
            type Invoke = spacetimedb::rt::ProcedureFn;

            /// The function kind, which will cause scheduled tables to accept procedures.
            type FnKind = #fn_kind_ty;

            /// The name of this function
            const NAME: &'static str = #procedure_name;

            /// The parameter names of this function
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];

            /// The pointer for invoking this function
            const INVOKE: spacetimedb::rt::ProcedureFn = #func_name::invoke;

            /// The return type of this function
            fn return_type(ts: &mut impl spacetimedb::sats::typespace::TypespaceBuilder) -> Option<spacetimedb::sats::AlgebraicType> {
                Some(<#return_type_ty as spacetimedb::SpacetimeType>::make_type(ts))
            }
        }

        #generate_explicit_names
    })
}
