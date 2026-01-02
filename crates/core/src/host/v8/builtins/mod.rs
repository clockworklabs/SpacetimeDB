use v8::{FunctionCallbackArguments, Local, PinScope};

use super::de::scratch_buf;
use super::error::{exception_already_thrown, ExcResult, StringTooLongError, Throwable, TypeError};
use super::string::{str_from_ident, StringConst};
use super::{FnRet, IntoJsString};

pub(super) fn evalute_builtins(scope: &mut PinScope<'_, '_>) -> ExcResult<()> {
    macro_rules! eval_builtin {
        ($file:literal) => {
            eval_builtin(
                scope,
                const { &StringConst::new(concat!("internal:", $file)) },
                const { &StringConst::new(include_str!(concat!("./", $file))) },
            )
        };
    }
    eval_builtin!("text_encoding.js")?;
    Ok(())
}

fn eval_builtin(
    scope: &mut PinScope<'_, '_>,
    resource_name: &'static StringConst,
    code: &'static StringConst,
) -> ExcResult<()> {
    let resource_name = resource_name.string(scope);
    let code = code.string(scope);
    super::eval_module(scope, resource_name.into(), code, resolve_builtins_module)?;
    Ok(())
}

macro_rules! create_synthetic_module {
    ($scope:expr, $module_name:expr $(,  $fun:ident)* $(,)?) => {{
        let export_names = &[$(str_from_ident!($fun).string($scope)),*];
        let eval_steps = |context, module| {
            v8::callback_scope!(unsafe scope, context);
            $(
                register_module_fun(scope, &module, str_from_ident!($fun), $fun);
            )*

            Some(v8::undefined(scope).into())
        };

        v8::Module::create_synthetic_module(
            $scope,
            const { StringConst::new($module_name) }.string($scope),
            export_names,
            eval_steps,
        )
    }}
}

/// Adapts `fun`, which returns a [`Value`] to one that works on [`v8::ReturnValue`].
fn adapt_fun(
    fun: impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> FnRet<'scope>,
) -> impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>, v8::ReturnValue) {
    move |scope, args, mut rv| {
        // Set the result `value` on success.
        if let Ok(value) = fun(scope, args) {
            rv.set(value);
        }
    }
}

/// Registers a function in `module`
/// where the function has `name` and does `body`.
fn register_module_fun(
    scope: &mut v8::PinCallbackScope<'_, '_>,
    module: &Local<'_, v8::Module>,
    name: &'static StringConst,
    body: impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> FnRet<'scope>,
) -> Option<bool> {
    // Convert the name.
    let name = name.string(scope);

    // Convert the function.
    let fun = v8::Function::builder(adapt_fun(body)).constructor_behavior(v8::ConstructorBehavior::Throw);
    let fun = fun.build(scope)?.into();

    // Set the export on the module.
    module.set_synthetic_module_export(scope, name, fun)
}

fn resolve_builtins_module<'scope>(
    context: Local<'scope, v8::Context>,
    spec: Local<'scope, v8::String>,
    _attrs: Local<'scope, v8::FixedArray>,
    _referrer: Local<'scope, v8::Module>,
) -> Option<Local<'scope, v8::Module>> {
    v8::callback_scope!(unsafe scope, context);
    // resolve_sys_module_inner(scope, spec).ok()
    let buf = &mut scratch_buf::<32>();
    if spec.to_rust_cow_lossy(scope, buf) != "spacetime:internal_builtins" {
        TypeError("Unknown module").throw(scope);
        return None;
    }
    Some(internal_builtins_module(scope))
}

/// An internal module providing native functions for certain JS builtins.
///
/// This is not public API, since it's not accessible to user modules - only to
/// the js builtins in this directory.
fn internal_builtins_module<'scope>(scope: &mut PinScope<'scope, '_>) -> Local<'scope, v8::Module> {
    create_synthetic_module!(scope, "spacetime:internal_builtins", utf8_encode, utf8_decode)
}

/// Encode a JS string into UTF-8.
///
/// Implementing this as a host call is much faster than implementing it as userspace JS.
///
/// Signature from ./types.d.ts:
/// ```ts
/// export function utf8_encode(s: string): Uint8Array<ArrayBuffer>;
/// ```
fn utf8_encode<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'scope>) -> FnRet<'scope> {
    let string_val = args.get(0);
    let string = string_val
        .to_string(scope)
        .ok_or_else(exception_already_thrown)?
        .to_rust_string_lossy(scope);
    let byte_length = string.len();
    let buf = v8::ArrayBuffer::new_backing_store_from_bytes(string.into_bytes()).make_shared();
    let buf = v8::ArrayBuffer::with_backing_store(scope, &buf);
    v8::Uint8Array::new(scope, buf, 0, byte_length)
        .map(Into::into)
        .ok_or_else(exception_already_thrown)
}

/// Decode a UTF-8 string from an `ArrayBuffer` into a JS string.
///
/// If `fatal` is true, throw an error if the data is not valid UTF-8.
///
/// Signature fom ./types.d.ts:
/// ```ts
/// export function utf8_decode(s: ArrayBufferView, fatal: boolean): string;
/// ```
fn utf8_decode<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'scope>) -> FnRet<'scope> {
    let buf = args.get(0);
    let fatal = args.get(1).boolean_value(scope);
    if let Ok(buf) = buf.try_cast::<v8::ArrayBufferView>() {
        let buffer = buf.get_contents(&mut []);
        let res = if fatal {
            let s = std::str::from_utf8(buffer).map_err(|e| TypeError(e.to_string()).throw(scope))?;
            s.into_string(scope)
        } else {
            v8::String::new_from_utf8(scope, buffer, v8::NewStringType::Normal)
                .ok_or_else(|| StringTooLongError::new(&String::from_utf8_lossy(buffer)))
        };
        res.map(Into::into).map_err(|e| e.into_range_error().throw(scope))
    } else {
        Err(TypeError("argument is not an `ArrayBuffer` or a view on one").throw(scope))
    }
}
