use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::Nothing;
use syn::{FnArg, ItemFn};

pub(crate) fn reducer_impl(_args: Nothing, original_function: ItemFn) -> syn::Result<TokenStream> {
    // Extract reducer name, making sure it's not `__XXX__` as that's the form we reserve for special reducers.
    let reducer_name = original_function.sig.ident.to_string();
    if reducer_name.starts_with("__") && reducer_name.ends_with("__") {
        return Err(syn::Error::new_spanned(
            &original_function.sig.ident,
            "reserved reducer name",
        ));
    }

    gen_reducer(original_function, &reducer_name, ReducerExtra::Schedule)
}

pub(crate) fn special_reducer(reducer_name: &'static str) -> impl Fn(Nothing, ItemFn) -> syn::Result<TokenStream> {
    |Nothing, original_function| gen_reducer(original_function, reducer_name, ReducerExtra::None)
}

enum ReducerExtra {
    None,
    Schedule,
}

fn gen_reducer(original_function: ItemFn, reducer_name: &str, extra: ReducerExtra) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    // let errmsg = "reducer should have at least 2 arguments: (identity: Identity, timestamp: u64, ...)";
    // let ([arg1, arg2], args) = validate_reducer_args(&original_function.sig, errmsg)?;

    // // TODO: better (non-string-based) validation for these
    // if !matches!(
    //     &*arg1.to_token_stream().to_string(),
    //     "spacetimedb::spacetimedb_sats::hash::Hash" | "Hash"
    // ) {
    //     return Err(syn::Error::new_spanned(
    //         &arg1,
    //         "1st parameter of a reducer must be of type \'u64\'.",
    //     ));
    // }
    // if arg2.to_token_stream().to_string() != "u64" {
    //     return Err(syn::Error::new_spanned(
    //         &arg2,
    //         "2nd parameter of a reducer must be of type \'u64\'.",
    //     ));
    // }

    // Extract all function parameters, except for `self` ones that aren't allowed.
    let typed_args = original_function
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => Ok(arg),
            _ => Err(syn::Error::new_spanned(arg, "expected typed argument")),
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

    // Extract the return type.
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let register_describer_symbol = format!("__preinit__20_register_describer_{reducer_name}");

    let mut extra_impls = TokenStream::new();

    if !matches!(extra, ReducerExtra::None) {
        let arg_names = typed_args
            .iter()
            .enumerate()
            .map(|(i, arg)| match &*arg.pat {
                syn::Pat::Ident(pat) => pat.ident.clone(),
                _ => format_ident!("__arg{}", i),
            })
            .collect::<Vec<_>>();

        extra_impls.extend(quote!(impl #func_name {
            pub fn schedule(__time: spacetimedb::Timestamp #(, #arg_names: #arg_tys)*) -> spacetimedb::ScheduleToken<#func_name> {
                spacetimedb::rt::schedule(__time, (#(#arg_names,)*))
            }
        }));
    }

    let generated_function = quote! {
        fn __reducer(
            __sender: spacetimedb::sys::Buffer,
            __caller_address: spacetimedb::sys::Buffer,
            __timestamp: u64,
            __args: &[u8]
        ) -> spacetimedb::sys::Buffer {
            #(spacetimedb::rt::assert_reducer_arg::<#arg_tys>();)*
            #(spacetimedb::rt::assert_reducer_ret::<#ret_ty>();)*
            spacetimedb::rt::invoke_reducer(
                #func_name,
                __sender,
                __caller_address,
                __timestamp,
                __args,
            )
        }
    };

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_reducer::<_, _, #func_name>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        impl spacetimedb::rt::ReducerInfo for #func_name {
            const NAME: &'static str = #reducer_name;
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];
            const INVOKE: spacetimedb::rt::ReducerFn = {
                #generated_function
                __reducer
            };
        }
        #extra_impls
    })
}
