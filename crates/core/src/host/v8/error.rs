//! Utilities for error handling when dealing with V8.

use v8::{Exception, HandleScope, Local, Value};

/// The result of trying to convert a [`Value`] in scope `'scope` to some type `T`.
pub(super) type ValueResult<'scope, T> = Result<T, ExceptionValue<'scope>>;

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

/// A JS exception value.
///
/// Newtyped for additional type safety and to track JS exceptions in the type system.
#[derive(Debug)]
pub(super) struct ExceptionValue<'scope>(Local<'scope, Value>);

/// Error types that can convert into JS exception values.
pub(super) trait IntoException<'scope> {
    /// Converts `self` into a JS exception value.
    fn into_exception(self, scope: &mut HandleScope<'scope>) -> ExceptionValue<'scope>;
}

impl<'scope> IntoException<'scope> for ExceptionValue<'scope> {
    fn into_exception(self, _: &mut HandleScope<'scope>) -> ExceptionValue<'scope> {
        self
    }
}

/// A type converting into a JS `TypeError` exception.
#[derive(Copy, Clone)]
pub struct TypeError<M>(pub M);

impl<'scope, M: IntoJsString> IntoException<'scope> for TypeError<M> {
    fn into_exception(self, scope: &mut HandleScope<'scope>) -> ExceptionValue<'scope> {
        let msg = self.0.into_string(scope);
        ExceptionValue(Exception::type_error(scope, msg))
    }
}

/// A type converting into a JS `RangeError` exception.
#[derive(Copy, Clone)]
pub struct RangeError<M>(pub M);

impl<'scope, M: IntoJsString> IntoException<'scope> for RangeError<M> {
    fn into_exception(self, scope: &mut HandleScope<'scope>) -> ExceptionValue<'scope> {
        let msg = self.0.into_string(scope);
        ExceptionValue(Exception::range_error(scope, msg))
    }
}

#[derive(Debug)]
pub(super) struct ExceptionThrown {
    _priv: (),
}

/// A result where the error indicates that an exception has already been thrown in V8.
pub(super) type ExcResult<T> = Result<T, ExceptionThrown>;

/// Indicates that the JS side had thrown an exception.
pub(super) fn exception_already_thrown() -> ExceptionThrown {
    ExceptionThrown { _priv: () }
}

/// Types that can be thrown as a V8 exception.
pub(super) trait Throwable<'scope> {
    /// Throw `self` into the V8 engine as an exception.
    ///
    /// If an exception has already been thrown,
    /// [`ExceptionThrown`] can be returned directly.
    fn throw(self, scope: &mut HandleScope<'scope>) -> ExceptionThrown;
}

impl<'scope, T: IntoException<'scope>> Throwable<'scope> for T {
    fn throw(self, scope: &mut HandleScope<'scope>) -> ExceptionThrown {
        let ExceptionValue(exception) = self.into_exception(scope);
        scope.throw_exception(exception);
        exception_already_thrown()
    }
}
