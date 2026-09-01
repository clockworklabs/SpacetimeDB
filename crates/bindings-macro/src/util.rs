use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::{Span, TokenStream};
use syn::parse::Parse;
use syn::Ident;

/// Parses `item`, passing it and `args` to `f`,
/// which should return only whats newly added, excluding the `item`.
/// Returns the full token stream `extra_attr item newly_added`.
pub(crate) fn cvt_attr<Item: Parse + quote::ToTokens>(
    args: StdTokenStream,
    item: StdTokenStream,
    extra_attr: TokenStream,
    f: impl FnOnce(TokenStream, &Item) -> syn::Result<TokenStream>,
) -> StdTokenStream {
    let item: TokenStream = item.into();
    let parsed_item = match syn::parse2::<Item>(item.clone()) {
        Ok(i) => i,
        Err(e) => return TokenStream::from_iter([item, e.into_compile_error()]).into(),
    };
    let generated = f(args.into(), &parsed_item).unwrap_or_else(syn::Error::into_compile_error);
    TokenStream::from_iter([extra_attr, item, generated]).into()
}

/// Run `f`, converting `Err` returns into a compile error.
///
/// This helper allows code within the closure `f` to use `?` for early return.
pub(crate) fn ok_or_compile_error<Res: Into<StdTokenStream>>(f: impl FnOnce() -> syn::Result<Res>) -> StdTokenStream {
    match f() {
        Ok(ok) => ok.into(),
        Err(e) => e.into_compile_error().into(),
    }
}
/// Extracts the argument names for a reducer/procedure/view's typed parameters,
/// as `Some(name)` for each argument, for embedding into `ARG_NAMES`.
///
/// Rejects arguments whose pattern is not a plain identifier (e.g. destructuring
/// patterns like `SomeProduct { a, b }: SomeProduct`), since such a pattern can't
/// be represented as a single, meaningful argument name in the schema sent to
/// the SDK/client codegen. Callers should bind such arguments to a name and
/// destructure in the function body instead.
pub(crate) fn extract_arg_names(typed_args: &[&syn::PatType]) -> syn::Result<Vec<TokenStream>> {
    typed_args
        .iter()
        .map(|arg| match &*arg.pat {
            syn::Pat::Ident(i) => {
                let name = i.ident.to_string();
                Ok(quote::quote!(Some(#name)))
            }
            // A bare `_` intentionally means "no name for this argument" —
            // that's fine, unlike a destructuring pattern which silently
            // drops multiple named sub-fields into a single unnamed schema entry.
            syn::Pat::Wild(_) => Ok(quote::quote!(None)),
            other => Err(syn::Error::new_spanned(
                other,
                "arguments must be simple identifiers (e.g. `x: MyType`) or `_`, not \
                 patterns like destructuring; bind to a name and destructure in the \
                 function body instead",
            )),
        })
        .collect()
}
pub(crate) fn ident_to_litstr(ident: &Ident) -> syn::LitStr {
    syn::LitStr::new(&ident.to_string(), ident.span())
}

pub(crate) trait ErrorSource {
    fn error(self, msg: impl std::fmt::Display) -> syn::Error;
}
impl ErrorSource for Span {
    fn error(self, msg: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, msg)
    }
}
impl ErrorSource for &syn::meta::ParseNestedMeta<'_> {
    fn error(self, msg: impl std::fmt::Display) -> syn::Error {
        self.error(msg)
    }
}

/// Ensures that `x` is `None` or returns an error.
pub(crate) fn check_duplicate<T>(x: &Option<T>, src: impl ErrorSource) -> syn::Result<()> {
    check_duplicate_msg(x, src, "duplicate attribute")
}
pub(crate) fn check_duplicate_msg<T>(
    x: &Option<T>,
    src: impl ErrorSource,
    msg: impl std::fmt::Display,
) -> syn::Result<()> {
    if x.is_none() {
        Ok(())
    } else {
        Err(src.error(msg))
    }
}

pub(crate) fn one_of(options: &[crate::sym::Symbol]) -> String {
    match options {
        [] => "unexpected attribute".to_owned(),
        [a] => {
            format!("expected `{a}`")
        }
        [a, b] => {
            format!("expected `{a}` or `{b}`")
        }
        _ => {
            let join = options.join("`, `");
            format!("expected one of: `{join}`")
        }
    }
}

macro_rules! match_meta {
    (match $meta:ident { $($matches:tt)* }) => {{
        let meta: &syn::meta::ParseNestedMeta = &$meta;
        match_meta!(@match (), (), meta { $($matches)* })
    }};

    (@match $acc:tt, $comparisons:tt, $meta:ident { $sym:path => $body:block $($rest:tt)* }) => {
        match_meta!(@case $acc, $comparisons, $meta, _, $sym, $body, { $($rest)* })
    };
    (@match $acc:tt, $comparisons:tt, $meta:ident { $sym:path => $body:expr, $($rest:tt)* }) => {
        match_meta!(@case $acc, $comparisons, $meta, _, $sym, $body, { $($rest)* })
    };

    (@match ($($acc:tt)*), ($($comparisons:expr),*), $meta:ident {}) => {
        match () {
            $($acc)*
            _ => return Err($meta.error($crate::util::one_of(&[$($comparisons),*]))),
        }
    };

    (@case ($($acc:tt)*), ($($comparisons:expr),*), $meta:ident, $binding:tt, $sym:path, $body:expr, { $($rest:tt)* }) => {
        match_meta!(@match (
            $($acc)*
            _ if $meta.path == $sym => $body,
        ), ($($comparisons,)* $sym), $meta { $($rest)* })
    };
}
pub(crate) use match_meta;
