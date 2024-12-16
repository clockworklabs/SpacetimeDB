use crate::sym;
use crate::util::{check_duplicate_msg, match_meta};
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::parse::Parser as _;
use syn::spanned::Spanned;
use syn::{FnArg, Ident, ItemFn};

#[derive(Default)]
pub(crate) struct ReducerArgs {
    lifecycle: Option<LifecycleReducer>,
}

enum LifecycleReducer {
    Init(Span),
    ClientConnected(Span),
    ClientDisconnected(Span),
    Update(Span),
}
impl LifecycleReducer {
    fn reducer_name(&self) -> &'static str {
        match self {
            Self::Init(_) => "__init__",
            Self::ClientConnected(_) => "__identity_connected__",
            Self::ClientDisconnected(_) => "__identity_disconnected__",
            Self::Update(_) => "__update__",
        }
    }
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
            });
            Ok(())
        })
        .parse2(input)?;
        Ok(args)
    }
}

pub(crate) fn reducer_impl(args: ReducerArgs, original_function: &ItemFn) -> syn::Result<TokenStream> {
    let func_name = &original_function.sig.ident;
    let vis = &original_function.vis;

    // Extract reducer name, making sure it's not `__XXX__` as that's the form we reserve for special reducers.
    let reducer_name;
    let reducer_name = match &args.lifecycle {
        Some(lifecycle) => lifecycle.reducer_name(),
        None => {
            reducer_name = func_name.to_string();
            if reducer_name.starts_with("__") && reducer_name.ends_with("__") {
                return Err(syn::Error::new_spanned(
                    &original_function.sig.ident,
                    "reserved reducer name",
                ));
            }
            &reducer_name
        }
    };

    for param in &original_function.sig.generics.params {
        let err = |msg| syn::Error::new_spanned(param, msg);
        match param {
            syn::GenericParam::Lifetime(_) => {}
            syn::GenericParam::Type(_) => return Err(err("type parameters are not allowed on reducers")),
            syn::GenericParam::Const(_) => return Err(err("const parameters are not allowed on reducers")),
        }
    }

    let lifecycle = args.lifecycle.iter().filter_map(|lc| lc.to_lifecycle_value());

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
    let first_arg_ty = arg_tys.first().into_iter();
    let rest_arg_tys = arg_tys.iter().skip(1);

    // Extract the return type.
    let ret_ty = match &original_function.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, t) => Some(&**t),
    }
    .into_iter();

    let register_describer_symbol = format!("__preinit__20_register_describer_{reducer_name}");

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
        impl spacetimedb::rt::ReducerInfo for #func_name {
            const NAME: &'static str = #reducer_name;
            #(const LIFECYCLE: Option<spacetimedb::rt::LifecycleReducer> = Some(#lifecycle);)*
            const ARG_NAMES: &'static [Option<&'static str>] = &[#(#opt_arg_names),*];
            const INVOKE: spacetimedb::rt::ReducerFn = #func_name::invoke;
        }
    })
}
