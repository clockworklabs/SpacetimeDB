use std::marker::PhantomData;

use syn::parse::{Lookahead1, Parse, ParseStream};
use syn::token::Token;

macro_rules! match_tok {
    (match $input:ident { $($matches:tt)* }) => {{
        use $crate::macros::PeekParse;
        let input: syn::parse::ParseStream = $input;
        let lookahead = input.lookahead1();
        match_tok!(@match (), lookahead, input { $($matches)* })
    }};

    (@match $acc:tt, $lookahead:ident, $input:ident { $binding:tt @ $tok:ty => $body:block $($rest:tt)* }) => {
        match_tok!(@case $acc, $lookahead, $input, $binding, $tok, $body, { $($rest)* })
    };
    (@match $acc:tt, $lookahead:ident, $input:ident { $tok:ty => $body:block $($rest:tt)* }) => {
        match_tok!(@case $acc, $lookahead, $input, _, $tok, $body, { $($rest)* })
    };
    (@match $acc:tt, $lookahead:ident, $input:ident { $binding:tt @ $tok:ty => $body:expr, $($rest:tt)* }) => {
        match_tok!(@case $acc, $lookahead, $input, $binding, $tok, $body, { $($rest)* })
    };
    (@match $acc:tt, $lookahead:ident, $input:ident { $tok:ty => $body:expr, $($rest:tt)* }) => {
        match_tok!(@case $acc, $lookahead, $input, _, $tok, $body, { $($rest)* })
    };

    (@match ($($acc:tt)*), $lookahead:ident, $input:ident {}) => {
        match () {
            $($acc)*
            _ => return Err($lookahead.error()),
        }
    };

    (@case ($($acc:tt)*), $lookahead:ident, $input:ident, $binding:tt, $tok:ty, $body:expr, { $($rest:tt)* }) => {
        match_tok!(@match (
            $($acc)*
            _ if $crate::macros::peekparser::<$tok>().peekparse_peek(&$lookahead, $input) => {
                let $binding = $crate::macros::peekparser::<$tok>().peekparse_parse($input)?;
                $body
            }
        ), $lookahead, $input { $($rest)* })
    };
}

pub fn peekparser<T>() -> &'static &'static PhantomData<T> {
    &&PhantomData
}

pub trait PeekParse {
    type Output;
    fn peekparse_peek(&self, lookahead1: &Lookahead1, input: ParseStream) -> bool;
    fn peekparse_parse(&self, input: ParseStream) -> syn::Result<Self::Output>;
}

impl<T: Token> PeekParse for PhantomData<T> {
    type Output = ();
    fn peekparse_peek(&self, lookahead1: &Lookahead1, _input: ParseStream) -> bool {
        lookahead1.peek(|x| -> T { match x {} })
    }
    fn peekparse_parse(&self, _input: ParseStream) -> syn::Result<Self::Output> {
        Ok(())
    }
}

impl<T: Token + Parse> PeekParse for &PhantomData<T> {
    type Output = T;
    fn peekparse_peek(&self, lookahead1: &Lookahead1, _input: ParseStream) -> bool {
        lookahead1.peek(|x| -> T { match x {} })
    }
    fn peekparse_parse(&self, input: ParseStream) -> syn::Result<Self::Output> {
        input.parse()
    }
}

impl<T1, T2> PeekParse for &&PhantomData<(T1, T2)>
where
    T1: Token + Parse,
    T2: Token + Parse,
{
    type Output = (T1, T2);
    fn peekparse_peek(&self, lookahead1: &Lookahead1, input: ParseStream) -> bool {
        lookahead1.peek(|x| -> T1 { match x {} }) && input.peek2(|x| -> T2 { match x {} })
    }
    fn peekparse_parse(&self, input: ParseStream) -> syn::Result<Self::Output> {
        Ok((input.parse()?, input.parse()?))
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
            format!("expected one of: `{}`", join)
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
            _ => return Err($meta.error($crate::macros::one_of(&[$($comparisons),*]))),
        }
    };

    (@case ($($acc:tt)*), ($($comparisons:expr),*), $meta:ident, $binding:tt, $sym:path, $body:expr, { $($rest:tt)* }) => {
        match_meta!(@match (
            $($acc)*
            _ if $meta.path == $sym => $body,
        ), ($($comparisons,)* $sym), $meta { $($rest)* })
    };
}
