use proc_macro2::Span;
use syn::Ident;

pub trait ErrorSource {
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
pub fn check_duplicate<T>(x: &Option<T>, src: impl ErrorSource) -> syn::Result<()> {
    check_duplicate_msg(x, src, "duplicate attribute")
}
pub fn check_duplicate_msg<T>(
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

pub fn one_of(options: &[super::sym::Symbol]) -> String {
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
            format!("expected one of: `{}`", join)
        }
    }
}

#[macro_export]
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
            _ => return Err($meta.error($crate::input::util::one_of(&[$($comparisons),*]))),
        }
    };

    (@case ($($acc:tt)*), ($($comparisons:expr),*), $meta:ident, $binding:tt, $sym:path, $body:expr, { $($rest:tt)* }) => {
        match_meta!(@match (
            $($acc)*
            _ if $meta.path == $sym => $body,
        ), ($($comparisons,)* $sym), $meta { $($rest)* })
    };
}
pub use match_meta;

pub fn ident_to_litstr(ident: &Ident) -> syn::LitStr {
    syn::LitStr::new(&ident.to_string(), ident.span())
}
