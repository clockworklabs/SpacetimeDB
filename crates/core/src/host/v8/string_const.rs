use v8::{Local, OneByteConst, PinScope, String};

/// A string known at compile time to be ASCII.
pub(super) struct StringConst(OneByteConst);

impl StringConst {
    /// Returns a new string that is known to be ASCII and static.
    pub(super) const fn new(string: &'static str) -> Self {
        Self(String::create_external_onebyte_const(string.as_bytes()))
    }

    /// Converts the string to a V8 string.
    pub(super) fn string<'scope>(&'static self, scope: &PinScope<'scope, '_>) -> Local<'scope, String> {
        String::new_from_onebyte_const(scope, &self.0)
            .expect("`create_external_onebyte_const` should've asserted `.len() < kMaxLength`")
    }
}

/// Converts an identifier to a compile-time ASCII string.
#[macro_export]
macro_rules! str_from_ident {
    ($ident:ident) => {{
        const STR: &$crate::host::v8::string_const::StringConst =
            &$crate::host::v8::string_const::StringConst::new(stringify!($ident));
        STR
    }};
}
pub(super) use str_from_ident;

/// The `tag` property of a sum object in JS.
pub(super) const TAG: &StringConst = str_from_ident!(tag);
/// The `value` property of a sum object in JS.
pub(super) const VALUE: &StringConst = str_from_ident!(value);
