//! Utilities for error handling when dealing with V8.

use v8::{Exception, HandleScope, Local, Value};

/// The result of trying to convert a [`Value`] in scope `'scope` to some type `T`.
pub(super) type ValueResult<'scope, T> = Result<T, Local<'scope, Value>>;

/// Types that can convert into a JS string type.
pub(super) trait IntoJsString {
    /// Converts `self` into a JS string.
    fn into_string<'scope>(self, scope: &mut HandleScope<'scope>) -> Local<'scope, v8::String>;
}

impl IntoJsString for String {
    fn into_string<'scope>(self, scope: &mut HandleScope<'scope>) -> Local<'scope, v8::String> {
        v8::String::new(scope, &self).unwrap()
    }
}

/// Error types that can convert into JS exception values.
pub(super) trait IntoException {
    /// Converts `self` into a JS exception value.
    fn into_exception<'scope>(self, scope: &mut HandleScope<'scope>) -> Local<'scope, Value>;
}

/// A type converting into a JS `TypeError` exception.
#[derive(Copy, Clone)]
pub struct TypeError<M>(pub M);

impl<M: IntoJsString> IntoException for TypeError<M> {
    fn into_exception<'scope>(self, scope: &mut HandleScope<'scope>) -> Local<'scope, Value> {
        let msg = self.0.into_string(scope);
        Exception::type_error(scope, msg)
    }
}

#[derive(Debug)]
pub(super) struct ExceptionThrown {
    _priv: (),
}

/// Indicates that the JS side had thrown an exception.
pub(super) fn exception_already_thrown() -> ExceptionThrown {
    ExceptionThrown { _priv: () }
}
