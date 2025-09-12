//! Utilities for error handling when dealing with V8.

use core::fmt;
use v8::{Exception, HandleScope, Local, StackFrame, StackTrace, TryCatch, Value};

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
pub(crate) struct ExceptionThrown {
    _priv: (),
}

/// A result where the error indicates that an exception has already been thrown in V8.
pub(crate) type ExcResult<T> = Result<T, ExceptionThrown>;

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
pub(super) struct JsError {
    msg: String,
    trace: JsStackTrace,
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "js error {}", self.msg)?;
        if !f.alternate() {
            writeln!(f, "{}", self.trace)?;
        }
        Ok(())
    }
}

/// A V8 stack trace that is independent of a `'scope`.
#[derive(Debug, Default)]
pub(super) struct JsStackTrace {
    frames: Box<[JsStackTraceFrame]>,
}

impl JsStackTrace {
    /// Converts a V8 [`StackTrace`] into one independent of `'scope`.
    fn from_trace<'scope>(scope: &mut HandleScope<'scope>, trace: Local<'scope, StackTrace>) -> Self {
        let frames = (0..trace.get_frame_count())
            .map(|index| {
                let frame = trace.get_frame(scope, index).unwrap();
                JsStackTraceFrame::from_frame(scope, frame)
            })
            .collect::<Box<[_]>>();
        Self { frames }
    }
}

impl fmt::Display for JsStackTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for frame in self.frames.iter() {
            writeln!(f, "\t{frame}")?;
        }

        Ok(())
    }
}

/// A V8 stack trace frame that is independent of a `'scope`.
#[derive(Debug)]
pub(super) struct JsStackTraceFrame {
    line: usize,
    column: usize,
    script_id: usize,
    script_name: Option<String>,
    fn_name: Option<String>,
    is_eval: bool,
    is_ctor: bool,
    is_wasm: bool,
    is_user_js: bool,
}

impl JsStackTraceFrame {
    /// Converts a V8 [`StackFrame`] into one independent of `'scope`.
    fn from_frame<'scope>(scope: &mut HandleScope<'scope>, frame: Local<'scope, StackFrame>) -> Self {
        let script_name = frame
            .get_script_name_or_source_url(scope)
            .map(|s| s.to_rust_string_lossy(scope));

        let fn_name = frame.get_function_name(scope).map(|s| s.to_rust_string_lossy(scope));

        Self {
            line: frame.get_line_number(),
            column: frame.get_column(),
            script_id: frame.get_script_id(),
            script_name,
            fn_name,
            is_eval: frame.is_eval(),
            is_ctor: frame.is_constructor(),
            is_wasm: frame.is_wasm(),
            is_user_js: frame.is_user_javascript(),
        }
    }
}

impl fmt::Display for JsStackTraceFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fn_name = self.fn_name.as_deref().unwrap_or("<anonymous>");
        let script_name = self.script_name.as_deref().unwrap_or("<unknown location>");

        // This isn't exactly the same format as chrome uses,
        // but it's close enough for now.
        // TODO(v8): make it more like chrome in the future.
        f.write_fmt(format_args!(
            "at {} ({}:{}:{})",
            fn_name, script_name, &self.line, &self.column
        ))?;

        if self.is_ctor {
            f.write_str(" (constructor)")?;
        }

        if self.is_eval {
            f.write_str(" (eval)")?;
        }

        if self.is_wasm {
            f.write_str(" (wasm)")?;
        }

        if !self.is_user_js {
            f.write_str(" (native)")?;
        }

        Ok(())
    }
}

impl JsError {
    /// Turns a caught JS exception in `scope` into a [`JSError`].
    fn from_caught(scope: &mut TryCatch<'_, HandleScope<'_>>) -> Self {
        match scope.message() {
            Some(message) => Self {
                trace: message
                    .get_stack_trace(scope)
                    .map(|trace| JsStackTrace::from_trace(scope, trace))
                    .unwrap_or_default(),
                msg: message.get(scope).to_rust_string_lossy(scope),
            },
            None => Self {
                trace: JsStackTrace::default(),
                msg: "unknown error".to_owned(),
            },
        }
    }
}

pub(super) fn log_traceback(func_type: &str, func: &str, e: &anyhow::Error) {
    log::info!("{func_type} \"{func}\" runtime error: {e:#}");
    if let Some(js_err) = e.downcast_ref::<JsError>() {
        log::info!("js error {}", js_err.msg);
        for (index, frame) in js_err.trace.frames.iter().enumerate() {
            log::info!("  Frame #{index}: {frame}");
        }
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
