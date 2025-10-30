//! Utilities for error handling when dealing with V8.

use super::de::scratch_buf;
use super::serialize_to_js;
use super::string::IntoJsString;
use crate::{
    database_logger::{BacktraceFrame, BacktraceProvider, LogLevel, ModuleBacktrace, Record},
    host::instance_env::InstanceEnv,
    replica_context::ReplicaContext,
};
use core::fmt;
use spacetimedb_sats::Serialize;
use v8::{tc_scope, Exception, HandleScope, Local, PinScope, PinnedRef, StackFrame, StackTrace, TryCatch, Value};

/// The result of trying to convert a [`Value`] in scope `'scope` to some type `T`.
pub(super) type ValueResult<'scope, T> = Result<T, ExceptionValue<'scope>>;

/// A JS exception value.
///
/// Newtyped for additional type safety and to track JS exceptions in the type system.
#[derive(Debug)]
pub(super) struct ExceptionValue<'scope>(pub(super) Local<'scope, Value>);

/// Error types that can convert into JS exception values.
pub(super) trait IntoException<'scope> {
    /// Converts `self` into a JS exception value.
    fn into_exception(self, scope: &PinScope<'scope, '_>) -> ExceptionValue<'scope>;
}

impl<'scope> IntoException<'scope> for ExceptionValue<'scope> {
    fn into_exception(self, _: &PinScope<'scope, '_>) -> ExceptionValue<'scope> {
        self
    }
}

/// A type converting into a JS `TypeError` exception.
#[derive(Copy, Clone)]
pub struct TypeError<M>(pub M);

impl<'scope, M: IntoJsString> IntoException<'scope> for TypeError<M> {
    fn into_exception(self, scope: &PinScope<'scope, '_>) -> ExceptionValue<'scope> {
        match self.0.into_string(scope) {
            Ok(msg) => ExceptionValue(Exception::type_error(scope, msg)),
            Err(err) => err.into_range_error().into_exception(scope),
        }
    }
}

/// Returns a "module not found" exception to be thrown.
pub fn module_exception(scope: &mut PinScope<'_, '_>, spec: Local<'_, v8::String>) -> TypeError<String> {
    let mut buf = scratch_buf::<32>();
    let spec = spec.to_rust_cow_lossy(scope, &mut buf);
    TypeError(format!("Could not find module {spec:?}"))
}

/// A type converting into a JS `RangeError` exception.
#[derive(Copy, Clone)]
pub struct RangeError<M>(pub M);

impl<'scope, M: IntoJsString> IntoException<'scope> for RangeError<M> {
    fn into_exception(self, scope: &PinScope<'scope, '_>) -> ExceptionValue<'scope> {
        match self.0.into_string(scope) {
            Ok(msg) => ExceptionValue(Exception::range_error(scope, msg)),
            // This is not an infinite recursion.
            // The `r: RangeError<String>` that `StringTooLongError` produces
            // will always enter the branch above, as `r.0` is shorter than the maximum allowed.
            Err(err) => err.into_range_error().into_exception(scope),
        }
    }
}

/// A non-JS string couldn't convert to JS as it was to long.
#[derive(Debug)]
pub(super) struct StringTooLongError {
    /// The length of the string that was too long (`len >` [`v8::String::MAX_LENGTH`]).
    pub(super) len: usize,
    /// A prefix of the string for the purpose of rendering an exception that aids the module dev.
    pub(super) prefix: String,
}

impl StringTooLongError {
    /// Returns a new error that keeps a prefix of `string` and records its length.
    pub(super) fn new(string: &str) -> Self {
        let len = string.len();
        let prefix = string[0..16.max(len)].to_owned();
        Self { len, prefix }
    }

    /// Converts the error to a [`RangeError<String>`].
    pub(super) fn into_range_error(self) -> RangeError<String> {
        let Self { len, prefix } = self;
        RangeError(format!(
            r#"The string "`{prefix}..`" of `{len}` bytes is too long for JS"#
        ))
    }
}

/// A non-JS array couldn't convert to JS as it was to long.
#[derive(Debug)]
pub(super) struct ArrayTooLongError {
    /// The length of the array that was too long (`len >` [`i32::MAX`]).
    pub(super) len: usize,
}

impl ArrayTooLongError {
    /// Converts the error to a [`RangeError<String>`].
    pub(super) fn into_range_error(self) -> RangeError<String> {
        let Self { len } = self;
        RangeError(format!("`{len}` elements are too many for a JS array"))
    }
}

/// A catchable termination error thrown in callbacks to indicate a host error.
#[derive(Serialize)]
pub(super) struct TerminationError {
    __terminated__: String,
}

impl TerminationError {
    /// Converts [`anyhow::Error`] to a termination error.
    pub(super) fn from_error<'scope>(
        scope: &PinScope<'scope, '_>,
        error: &anyhow::Error,
    ) -> ExcResult<ExceptionValue<'scope>> {
        let __terminated__ = format!("{error}");
        let error = Self { __terminated__ };
        serialize_to_js(scope, &error).map(ExceptionValue)
    }
}

/// A catchable error code thrown in callbacks
/// to indicate bad arguments to a syscall.
#[derive(Serialize)]
pub(super) struct CodeError {
    __code_error__: u16,
}

impl CodeError {
    /// Create a code error from a code.
    pub(super) fn from_code<'scope>(
        scope: &PinScope<'scope, '_>,
        __code_error__: u16,
    ) -> ExcResult<ExceptionValue<'scope>> {
        let error = Self { __code_error__ };
        serialize_to_js(scope, &error).map(ExceptionValue)
    }
}

/// A catchable error code thrown in callbacks
/// to indicate that a buffer was too small and the minimum size required.
#[derive(Serialize)]
pub(super) struct BufferTooSmall {
    __buffer_too_small__: u32,
}

impl BufferTooSmall {
    /// Create a code error from a code.
    pub(super) fn from_requirement<'scope>(
        scope: &PinScope<'scope, '_>,
        __buffer_too_small__: u32,
    ) -> ExcResult<ExceptionValue<'scope>> {
        let error = Self { __buffer_too_small__ };
        serialize_to_js(scope, &error).map(ExceptionValue)
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
    fn throw(self, scope: &PinScope<'scope, '_>) -> ExceptionThrown;
}

impl<'scope, T: IntoException<'scope>> Throwable<'scope> for T {
    fn throw(self, scope: &PinScope<'scope, '_>) -> ExceptionThrown {
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
        writeln!(f, "{}", self.trace)?;
        Ok(())
    }
}

/// A V8 stack trace that is independent of a `'scope`.
#[derive(Debug, Default, Clone)]
pub(super) struct JsStackTrace {
    frames: Box<[JsStackTraceFrame]>,
}

impl JsStackTrace {
    /// Converts a V8 [`StackTrace`] into one independent of `'scope`.
    pub(super) fn from_trace<'scope>(scope: &PinScope<'scope, '_>, trace: Local<'scope, StackTrace>) -> Self {
        let frames = (0..trace.get_frame_count())
            .map(|index| {
                let frame = trace.get_frame(scope, index).unwrap();
                JsStackTraceFrame::from_frame(scope, frame)
            })
            .collect::<Box<[_]>>();
        Self { frames }
    }

    /// Construct a backtrace from `scope`.
    pub(super) fn from_current_stack_trace(scope: &PinScope<'_, '_>) -> ExcResult<Self> {
        let trace = StackTrace::current_stack_trace(scope, 1024).ok_or_else(exception_already_thrown)?;
        Ok(Self::from_trace(scope, trace))
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

impl BacktraceProvider for JsStackTrace {
    fn capture(&self) -> Box<dyn ModuleBacktrace> {
        let trace = self
            .frames
            .iter()
            .map(|f| {
                (
                    format!("{}:{}:{}", f.script_name(), f.line, f.column),
                    f.fn_name().to_owned(),
                )
            })
            .collect();
        Box::new(JsBacktrace { trace })
    }
}

/// A rendered backtrace for a JS exception.
struct JsBacktrace {
    trace: Vec<(String, String)>,
}

impl ModuleBacktrace for JsBacktrace {
    fn frames(&self) -> Vec<BacktraceFrame<'_>> {
        self.trace
            .iter()
            .map(|(module_name, func_name)| BacktraceFrame {
                module_name: Some(module_name),
                func_name: Some(func_name),
            })
            .collect()
    }
}

/// A V8 stack trace frame that is independent of a `'scope`.
#[derive(Debug, Clone)]
pub(super) struct JsStackTraceFrame {
    line: usize,
    column: usize,
    #[allow(dead_code)]
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
    fn from_frame<'scope>(scope: &PinScope<'scope, '_>, frame: Local<'scope, StackFrame>) -> Self {
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

    /// Returns the name of the function that was called.
    fn fn_name(&self) -> &str {
        self.fn_name.as_deref().unwrap_or("<anonymous>")
    }

    /// Returns the name of the script where the function resides.
    fn script_name(&self) -> &str {
        self.script_name.as_deref().unwrap_or("<unknown location>")
    }
}

impl fmt::Display for JsStackTraceFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fn_name = self.fn_name();
        let script_name = self.script_name();

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
    fn from_caught(scope: &PinnedRef<'_, TryCatch<'_, '_, HandleScope<'_>>>) -> Self {
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

pub(super) fn log_traceback(replica_ctx: &ReplicaContext, func_type: &str, func: &str, e: &anyhow::Error) {
    log::info!("{func_type} \"{func}\" runtime error: {e:}");
    if let Some(js_err) = e.downcast_ref::<JsError>() {
        log::info!("js error {}", js_err.msg);
        for (index, frame) in js_err.trace.frames.iter().enumerate() {
            log::info!("  Frame #{index}: {frame}");
        }

        // Also log to module logs.
        let first_frame = js_err.trace.frames.first();
        let filename = first_frame.map(|f| f.script_name());
        let line_number = first_frame.map(|f| f.line as u32);
        let message = &js_err.msg;
        let record = Record {
            ts: InstanceEnv::now_for_logging(),
            target: None,
            filename,
            line_number,
            function: Some(func),
            message,
        };
        replica_ctx.logger.write(LogLevel::Panic, &record, &js_err.trace);
    }
}

/// Run `body` within a try-catch context and capture any JS exception thrown as a [`JsError`].
pub(super) fn catch_exception<'scope, T>(
    scope: &mut PinScope<'scope, '_>,
    body: impl FnOnce(&mut PinScope<'scope, '_>) -> Result<T, ErrorOrException<ExceptionThrown>>,
) -> Result<T, (ErrorOrException<JsError>, CanContinue)> {
    tc_scope!(scope, scope);
    body(scope).map_err(|e| match e {
        ErrorOrException::Err(e) => (ErrorOrException::Err(e), CanContinue::Yes),
        ErrorOrException::Exception(_) => {
            let error = ErrorOrException::Exception(JsError::from_caught(scope));

            let can_continue = if scope.can_continue() {
                // We can continue.
                CanContinue::Yes
            } else if scope.has_terminated() {
                // We can continue if we do `Isolate::cancel_terminate_execution`.
                CanContinue::YesCancelTermination
            } else {
                // We cannot.
                CanContinue::No
            };

            (error, can_continue)
        }
    })
}

/// Encodes whether it is safe to continue using the [`Isolate`]
/// for further execution after [`catch_exception`] has happened.
pub(super) enum CanContinue {
    Yes,
    YesCancelTermination,
    No,
}
