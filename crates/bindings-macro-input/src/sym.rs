/// A symbol known at compile-time against
/// which identifiers and paths may be matched.
pub struct Symbol(pub &'static str);

#[macro_export]
macro_rules! symbol {
    ($ident:ident) => {
        symbol!($ident, $ident);
    };
    ($const:ident, $ident:ident) => {
        #[allow(non_upper_case_globals)]
        #[doc = concat!("Matches `", stringify!($ident), "`.")]
        pub const $const: Symbol = Symbol(stringify!($ident));
    };
}
pub use symbol;

symbol!(at);
symbol!(auto_inc);
symbol!(btree);
symbol!(column);
symbol!(columns);
symbol!(crate_, crate);
symbol!(direct);
symbol!(index);
symbol!(name);
symbol!(primary_key);
symbol!(private);
symbol!(public);
symbol!(repr);
symbol!(sats);
symbol!(scheduled);
symbol!(unique);

impl PartialEq<Symbol> for syn::Ident {
    fn eq(&self, sym: &Symbol) -> bool {
        self == sym.0
    }
}
impl PartialEq<Symbol> for &syn::Ident {
    fn eq(&self, sym: &Symbol) -> bool {
        *self == sym.0
    }
}
impl PartialEq<Symbol> for syn::Path {
    fn eq(&self, sym: &Symbol) -> bool {
        self.is_ident(sym)
    }
}
impl PartialEq<Symbol> for &syn::Path {
    fn eq(&self, sym: &Symbol) -> bool {
        self.is_ident(sym)
    }
}
impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::borrow::Borrow<str> for Symbol {
    fn borrow(&self) -> &str {
        self.0
    }
}
