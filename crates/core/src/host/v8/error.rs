//! Utilities for error handling when dealing with V8.

use v8::{Exception, HandleScope, Local, TryCatch, Value};

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

/// Either an error outside V8 JS execution, or an exception inside.
#[derive(Debug)]
pub(super) enum ErrorOrException<Exc> {
    Err(anyhow::Error),
    Exception(Exc),
}

impl<E> From<anyhow::Error> for ErrorOrException<E> {
    fn from(e: anyhow::Error) -> Self {
        Self::Err(e)
    }
}

impl From<ExceptionThrown> for ErrorOrException<ExceptionThrown> {
    fn from(e: ExceptionThrown) -> Self {
        Self::Exception(e)
    }
}

impl From<ErrorOrException<JsError>> for anyhow::Error {
    fn from(err: ErrorOrException<JsError>) -> Self {
        match err {
            ErrorOrException::Err(e) => e,
            ErrorOrException::Exception(e) => e.into(),
        }
    }
}

/// A JS exception turned into an error.
#[derive(thiserror::Error, Debug)]
#[error("js error: {msg:?}")]
pub(super) struct JsError {
    msg: String,
}

impl JsError {
    /// Turns a caught JS exception in `scope` into a [`JSError`].
    fn from_caught(scope: &mut TryCatch<'_, HandleScope<'_>>) -> Self {
        let msg = match scope.message() {
            Some(msg) => msg.get(scope).to_rust_string_lossy(scope),
            None => "unknown error".to_owned(),
        };
        Self { msg }
    }
}

/// Run `body` within a try-catch context and capture any JS exception thrown as a [`JsError`].
pub(super) fn catch_exception<'scope, T>(
    scope: &mut HandleScope<'scope>,
    body: impl FnOnce(&mut HandleScope<'scope>) -> Result<T, ErrorOrException<ExceptionThrown>>,
) -> Result<T, ErrorOrException<JsError>> {
    let scope = &mut TryCatch::new(scope);
    body(scope).map_err(|e| match e {
        ErrorOrException::Err(e) => ErrorOrException::Err(e),
        ErrorOrException::Exception(_) => ErrorOrException::Exception(JsError::from_caught(scope)),
    })
}
