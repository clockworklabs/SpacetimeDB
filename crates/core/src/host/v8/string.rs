use super::error::StringTooLongError;
use v8::{Local, OneByteConst, PinScope};

type LString<'scope> = Local<'scope, v8::String>;
type StringResult<'scope> = Result<LString<'scope>, StringTooLongError>;

/// Types that can convert into a JS string type.
pub(super) trait IntoJsString {
    /// Converts `self` into a JS string.
    fn into_string<'scope>(self, scope: &PinScope<'scope, '_>) -> StringResult<'scope>;
}

impl IntoJsString for &str {
    fn into_string<'scope>(self, scope: &PinScope<'scope, '_>) -> StringResult<'scope> {
        v8::String::new(scope, self).ok_or_else(|| StringTooLongError::new(self))
    }
}

impl IntoJsString for String {
    fn into_string<'scope>(self, scope: &PinScope<'scope, '_>) -> StringResult<'scope> {
        (&*self).into_string(scope)
    }
}

/// A string known at compile time to be ASCII.
pub(super) struct StringConst(OneByteConst);

impl StringConst {
    /// Returns a new string that is known to be ASCII and static.
    pub(super) const fn new(string: &'static str) -> Self {
        Self(v8::String::create_external_onebyte_const(string.as_bytes()))
    }

    /// Converts the string to a V8 string.
    pub(super) fn string<'scope>(&'static self, scope: &PinScope<'scope, '_>) -> LString<'scope> {
        v8::String::new_from_onebyte_const(scope, &self.0)
            .expect("`create_external_onebyte_const` should've asserted `.len() < kMaxLength`")
    }
}

/// Converts an identifier to a compile-time ASCII string.
#[macro_export]
macro_rules! str_from_ident {
    ($ident:ident) => {{
        const STR: &$crate::host::v8::string::StringConst =
            &$crate::host::v8::string::StringConst::new(stringify!($ident));
        STR
    }};
}
pub(super) use str_from_ident;

/// The `tag` property of a sum object in JS.
pub(super) const TAG: &StringConst = str_from_ident!(tag);
/// The `value` property of a sum object in JS.
pub(super) const VALUE: &StringConst = str_from_ident!(value);
