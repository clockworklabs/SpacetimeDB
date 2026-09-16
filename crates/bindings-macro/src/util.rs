use std::fmt;

use proc_macro::TokenStream as StdTokenStream;
use proc_macro2::{Literal, Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
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

/// Parses `item`, passing it and `args` to `f`,
/// which will mutate item and return extra tokens to emit.
pub(crate) fn cvt_attr_mut<Item: Parse + quote::ToTokens>(
    args: StdTokenStream,
    item: StdTokenStream,
    f: impl FnOnce(TokenStream, &mut Item) -> syn::Result<TokenStream>,
) -> StdTokenStream {
    let item: TokenStream = item.into();
    let mut parsed_item = match syn::parse2::<Item>(item.clone()) {
        Ok(i) => i,
        Err(e) => return TokenStream::from_iter([item, e.into_compile_error()]).into(),
    };
    let generated = f(args.into(), &mut parsed_item).unwrap_or_else(syn::Error::into_compile_error);
    TokenStream::from_iter([parsed_item.into_token_stream(), generated]).into()
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

pub(crate) fn preinit(prio: u8, kind: &str, name: impl fmt::Display, body: TokenStream) -> TokenStream {
    let symbol_name = format!("__preinit__{prio:02}_{kind}_{name}");
    let ident = format_ident!("__{}", kind);
    quote! {
        const _: () = {
            #[unsafe(export_name = #symbol_name)]
            extern "C" fn #ident() {
                #body
            }
        };
    }
}

pub(crate) fn superize_path(mut path: syn::Path) -> syn::Path {
    let first = path.segments.first_mut().unwrap();
    if path.leading_colon.is_some() || first.ident == "crate" {
        // the path is absolute
    } else if first.ident == "self" {
        first.ident = Ident::new("super", first.ident.span());
    } else {
        let super_ = Ident::new("super", first.ident.span());
        path.segments.insert(0, super_.into());
    }
    path
}

pub(crate) struct HexLiteral<const BYTES: usize> {
    inner: Literal,
    hex: String,
    data: [u8; BYTES],
}

impl<const BYTES: usize> Parse for HexLiteral<BYTES> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lit = input.parse::<Literal>()?;
        let err = |msg| syn::Error::new(lit.span(), msg);
        let suffix_err = || err("hex literal cannot have suffix");

        let is_hexstring = |s: &str| s.bytes().all(|c| c.is_ascii_hexdigit());

        // parse a hexadecimal int literal manually, because `syn::LitInt` doesn't have a radix
        let hex = match lit.to_string().strip_prefix("0x") {
            Some(hex) if is_hexstring(hex) => hex.to_owned(),
            Some(_) => return Err(suffix_err()),
            // allow string literals because syn seems to normalize integer literals into base-10
            None => {
                let syn::Lit::Str(s) = syn::Lit::new(lit.clone()) else {
                    return Err(syn::Error::new(lit.span(), "expected hexadecimal literal"));
                };
                let hex = s.value();
                if !is_hexstring(&hex) {
                    return Err(err("invalid hex literal"));
                };
                if !s.suffix().is_empty() {
                    return Err(suffix_err());
                }
                hex
            }
        };

        let (chunks, extra) = hex.as_bytes().as_chunks::<2>();
        let chunks = chunks
            .as_array::<BYTES>()
            .filter(|_| extra.is_empty())
            .ok_or_else(|| syn::Error::new(lit.span(), format_args!("hex literal must be {BYTES} bytes")))?;
        let data = chunks
            .map(|[hi, lo]| (char::from(hi).to_digit(16).unwrap() << 8 | char::from(lo).to_digit(16).unwrap()) as u8);
        Ok(Self { inner: lit, hex, data })
    }
}

impl<const BYTES: usize> HexLiteral<BYTES> {
    pub(crate) fn as_hex(&self) -> &str {
        &self.hex
    }

    pub(crate) fn to_array(&self) -> TokenStream {
        let hex = self.data.iter().copied().map(Literal::u8_suffixed);
        quote_spanned!(self.inner.span() => [#(#hex),*])
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
