use crate::reducer::{assert_only_lifetime_generics, extract_typed_args};
use crate::sym;
use crate::util::{check_duplicate, ident_to_litstr, match_meta};
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser as _;
use syn::{ItemFn, LitStr};

#[derive(Default)]
pub(crate) struct ProcedureArgs {
    /// For consistency with reducers: allow specifying a different export name than the Rust function name.
    name: Option<LitStr>,
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
            });
            Ok(())
        })
        .parse2(input)?;
        Ok(args)
    }
}

pub(crate) fn procedure_impl(args: ProcedureArgs, original_function: &ItemFn) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    let procedure_name = args.name.unwrap_or_else(|| ident_to_litstr(func_name));

    assert_only_lifetime_generics(original_function, "procedures")?;

    let typed_args = extract_typed_args(original_function)?;

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

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_procedure::<_, _, #func_name>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        const _: () = {
            fn _assert_args #lifetime_params () #lifetime_where_clause {
                #(let _ = <#first_arg_ty as spacetimedb::rt::ProcedureContextArg>::_ITEM;)*
                #(let _ = <#rest_arg_tys as spacetimedb::rt::ProcedureArg>::_ITEM;)*
                #(let _ = <#ret_ty_for_assert as spacetimedb::rt::IntoProcedureResult>::to_result;)*
            }
        };
        impl #func_name {
            fn invoke(__ctx: spacetimedb::ProcedureContext, __args: &[u8]) -> spacetimedb::ProcedureResult {
                spacetimedb::rt::invoke_procedure(#func_name, __ctx, __args)
            }
        }
        #[automatically_derived]
        impl spacetimedb::rt::FnInfo for #func_name {
            /// The type of this function.
            type Invoke = spacetimedb::rt::ProcedureFn;

            /// The function kind, which will cause scheduled tables to accept procedures.
            type FnKind = spacetimedb::rt::FnKindProcedure<#ret_ty_for_info>;

            /// The name of this function
            const NAME: &'static str = #procedure_name;

            /// The parameter names of this function
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];

            /// The pointer for invoking this function
            const INVOKE: spacetimedb::rt::ProcedureFn = #func_name::invoke;

            /// The return type of this function
            fn return_type(ts: &mut impl spacetimedb::sats::typespace::TypespaceBuilder) -> Option<spacetimedb::sats::AlgebraicType> {
                Some(<#ret_ty_for_info as spacetimedb::SpacetimeType>::make_type(ts))
            }
        }
    })
}
