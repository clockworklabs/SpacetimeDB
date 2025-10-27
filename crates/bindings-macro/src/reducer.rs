use crate::sym;
use crate::util::{check_duplicate, check_duplicate_msg, ident_to_litstr, match_meta};
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::parse::Parser as _;
use syn::spanned::Spanned;
use syn::{FnArg, Ident, ItemFn, LitStr, PatType};

#[derive(Default)]
pub(crate) struct ReducerArgs {
    name: Option<LitStr>,
    lifecycle: Option<LifecycleReducer>,
}

enum LifecycleReducer {
    Init(Span),
    ClientConnected(Span),
    ClientDisconnected(Span),
    Update(Span),
}
impl LifecycleReducer {
    fn to_lifecycle_value(&self) -> Option<TokenStream> {
        let (Self::Init(span) | Self::ClientConnected(span) | Self::ClientDisconnected(span) | Self::Update(span)) =
            *self;
        let name = match self {
            Self::Init(_) => "Init",
            Self::ClientConnected(_) => "OnConnect",
            Self::ClientDisconnected(_) => "OnDisconnect",
            Self::Update(_) => return None,
        };
        let ident = Ident::new(name, span);
        Some(quote_spanned!(span => spacetimedb::rt::LifecycleReducer::#ident))
    }
}

impl ReducerArgs {
    pub(crate) fn parse(input: TokenStream) -> syn::Result<Self> {
        let mut args = Self::default();
        syn::meta::parser(|meta| {
            let mut set_lifecycle = |kind: fn(Span) -> _| -> syn::Result<()> {
                check_duplicate_msg(&args.lifecycle, &meta, "already specified a lifecycle reducer kind")?;
                args.lifecycle = Some(kind(meta.path.span()));
                Ok(())
            };
            match_meta!(match meta {
                sym::init => set_lifecycle(LifecycleReducer::Init)?,
                sym::client_connected => set_lifecycle(LifecycleReducer::ClientConnected)?,
                sym::client_disconnected => set_lifecycle(LifecycleReducer::ClientDisconnected)?,
                sym::update => set_lifecycle(LifecycleReducer::Update)?,
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

pub(crate) fn assert_only_lifetime_generics(original_function: &ItemFn, function_kind_plural: &str) -> syn::Result<()> {
    for param in &original_function.sig.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => {
                return Err(err(format!(
                    "type parameters are not allowed on {function_kind_plural}"
                )))
            }
            syn::GenericParam::Const(_) => {
                return Err(err(format!(
                    "const parameters are not allowed on {function_kind_plural}"
                )))
            }
        }
    }
    Ok(())
}

/// Extract all function parameters, except for `self` ones that aren't allowed.
pub(crate) fn extract_typed_args(original_function: &ItemFn) -> syn::Result<Vec<&PatType>> {
    original_function
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(arg) => Ok(arg),
            _ => Err(syn::Error::new_spanned(arg, "expected typed argument")),
        })
        .collect()
}

pub(crate) fn reducer_impl(args: ReducerArgs, original_function: &ItemFn) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    let reducer_name = args.name.unwrap_or_else(|| ident_to_litstr(func_name));

    assert_only_lifetime_generics(original_function, "reducers")?;

    let lifecycle = args.lifecycle.iter().filter_map(|lc| lc.to_lifecycle_value());

    let typed_args = extract_typed_args(original_function)?;

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
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let register_describer_symbol = format!("__preinit__20_register_describer_{}", reducer_name.value());

    let lt_params = &original_function.sig.generics;
    let lt_where_clause = &lt_params.where_clause;

    let generated_describe_function = quote! {
        #[export_name = #register_describer_symbol]
        pub extern "C" fn __register_describer() {
            spacetimedb::rt::register_reducer::<_, #func_name>(#func_name)
        }
    };

    Ok(quote! {
        const _: () = {
            #generated_describe_function
        };
        #[allow(non_camel_case_types)]
        #vis struct #func_name { _never: ::core::convert::Infallible }
        const _: () = {
            fn _assert_args #lt_params () #lt_where_clause {
                #(let _ = <#first_arg_ty as spacetimedb::rt::ReducerContextArg>::_ITEM;)*
                #(let _ = <#rest_arg_tys as spacetimedb::rt::ReducerArg>::_ITEM;)*
                #(let _ = <#ret_ty as spacetimedb::rt::IntoReducerResult>::into_result;)*
            }
        };
        impl #func_name {
            fn invoke(__ctx: spacetimedb::ReducerContext, __args: &[u8]) -> spacetimedb::ReducerResult {
                spacetimedb::rt::invoke_reducer(#func_name, __ctx, __args)
            }
        }
        #[automatically_derived]
        impl spacetimedb::rt::FnInfo for #func_name {
            type Invoke = spacetimedb::rt::ReducerFn;
            /// The function kind, which will cause scheduled tables to accept reducers.
            type FnKind = spacetimedb::rt::FnKindReducer;
            const NAME: &'static str = #reducer_name;
            #(const LIFECYCLE: Option<spacetimedb::rt::LifecycleReducer> = Some(#lifecycle);)*
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];
            const INVOKE: Self::Invoke = #func_name::invoke;
        }
    })
}
