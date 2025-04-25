use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::TokenStream;
use syn::parse::Parse;

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
