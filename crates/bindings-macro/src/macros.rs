use std::marker::PhantomData;

use syn::parse::{Lookahead1, Parse, ParseStream};
use syn::token::Token;

macro_rules! match_tok {
    (match $input:ident { $($matches:tt)* }) => {{
        use $crate::macros::PeekParse;
        let input: syn::parse::ParseStream = $input;
        let lookahead = input.lookahead1();
        match_tok!(@match lookahead, input { $($matches)* })
    }};

    (@match $lookahead:ident, $input:ident { $binding:tt @ $tok:ty => $body:block $($rest:tt)* }) => {
        match_tok!(@case $lookahead, $input, $binding, $tok, $body, { $($rest)* })
    };
    (@match $lookahead:ident, $input:ident { $tok:ty => $body:block $($rest:tt)* }) => {
        match_tok!(@case $lookahead, $input, _, $tok, $body, { $($rest)* })
    };
    (@match $lookahead:ident, $input:ident { $binding:tt @ $tok:ty => $body:expr, $($rest:tt)* }) => {
        match_tok!(@case $lookahead, $input, $binding, $tok, $body, { $($rest)* })
    };
    (@match $lookahead:ident, $input:ident { $tok:ty => $body:expr, $($rest:tt)* }) => {
        match_tok!(@case $lookahead, $input, _, $tok, $body, { $($rest)* })
    };

    (@match $lookahead:ident, $input:ident {}) => {
        return Err($lookahead.error())
    };

    (@case $lookahead:ident, $input:ident, $binding:tt, $tok:ty, $body:expr, { $($rest:tt)* }) => {
        if $crate::macros::peekparser::<$tok>().peekparse_peek(&$lookahead, $input) {
            let $binding = $crate::macros::peekparser::<$tok>().peekparse_parse($input)?;
            $body
        } else {
            match_tok!(@match $lookahead, $input { $($rest)* })
        }
    };
}

pub fn peekparser<T>() -> &'static PhantomData<T> {
    &PhantomData
}

pub trait PeekParse {
    type Output;
    fn peekparse_peek(&self, lookahead1: &Lookahead1, input: ParseStream) -> bool;
    fn peekparse_parse(&self, input: ParseStream) -> syn::Result<Self::Output>;
}

impl<T: Token + Parse> PeekParse for PhantomData<T> {
    type Output = T;
    fn peekparse_peek(&self, lookahead1: &Lookahead1, _input: ParseStream) -> bool {
        lookahead1.peek(|x| -> T { match x {} })
    }
    fn peekparse_parse(&self, input: ParseStream) -> syn::Result<Self::Output> {
        input.parse()
    }
}

impl<T1, T2> PeekParse for &PhantomData<(T1, T2)>
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
