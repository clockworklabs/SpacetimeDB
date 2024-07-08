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
