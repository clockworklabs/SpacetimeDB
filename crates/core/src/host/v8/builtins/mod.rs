use v8::{FunctionCallbackArguments, Local, PinScope};

use super::de::scratch_buf;
use super::error::{exception_already_thrown, ExcResult, RangeError, Throwable, TypeError};
use super::string::{str_from_ident, StringConst};
use super::{FnRet, IntoJsString};

pub(super) fn evaluate_builtins(scope: &mut PinScope<'_, '_>) -> ExcResult<()> {
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
    eval_builtin!("delete_math_random.js")?;
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
    create_synthetic_module!(
        scope,
        "spacetime:internal_builtins",
        utf8_encode,
        utf8_decode,
        normalize_label,
        generic_decode
    )
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
/// export function utf8_decode(buf: AllowSharedBufferSource, ignoreBOM: boolean): string;
/// ```
fn utf8_decode<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'scope>) -> FnRet<'scope> {
    let buf = args.get(0);
    let ignore_bom = args.get(1).boolean_value(scope);
    let buf = cast_buffer(scope, buf)?;
    let mut buffer = buf.get_contents(&mut []);
    const BOM_UTF8: &[u8] = &[0xEF, 0xBB, 0xBF];
    if !ignore_bom {
        buffer = buffer.strip_prefix(BOM_UTF8).unwrap_or(buffer)
    }
    let string = v8::String::new_from_utf8(scope, buffer, v8::NewStringType::Normal)
        .ok_or_else(|| RangeError("Value too large to decode").throw(scope))?;
    Ok(string.into())
}

fn cast_buffer<'scope, 'a: 'scope>(
    scope: &mut PinScope<'scope, '_>,
    buf: Local<'a, v8::Value>,
) -> ExcResult<v8::Local<'scope, v8::ArrayBufferView>> {
    buf.try_cast::<v8::ArrayBufferView>()
        .or_else(|_| {
            buf.try_cast::<v8::ArrayBuffer>()
                .map(|buf| v8::Uint8Array::new(scope, buf, 0, buf.byte_length()).unwrap().into())
        })
        .or_else(|_| {
            buf.try_cast::<v8::SharedArrayBuffer>().map(|buf| {
                // pretend that the SAB is a regular AB - rusty_v8 doesn't implement the overload
                let arr = unsafe { Local::<v8::ArrayBuffer>::cast_unchecked(Local::<v8::Object>::from(buf)) };
                v8::Uint8Array::new(scope, arr, 0, buf.byte_length()).unwrap().into()
            })
        })
        .map_err(|_| TypeError("argument is not an `ArrayBuffer` or a view on one").throw(scope))
}

/// Map a label for an encoding to its canonical form.
///
/// Signature fom ./types.d.ts:
/// ```ts
/// export function normalize_label(label: string): string | null;
/// ```
fn normalize_label<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'scope>) -> FnRet<'scope> {
    let label = args.get(0);
    let label = label
        .to_string(scope)
        .ok_or_else(exception_already_thrown)?
        .to_rust_string_lossy(scope);
    match encoding_rs::Encoding::for_label_no_replacement(label.as_bytes()) {
        Some(encoding) => Ok(encoding.name().to_ascii_lowercase().into_string(scope).unwrap().into()),
        None => Ok(v8::null(scope).into()),
    }
}

/// Decode a UTF-8 string from an `ArrayBuffer` into a JS string.
///
/// If `fatal` is true, throw an error if the data is not valid UTF-8.
///
/// Signature fom ./types.d.ts:
/// ```ts
/// export function generic_decode(encoding: string, buf: AllowSharedBufferSource, fatal: boolean, ignoreBOM: boolean): string;
/// ```
fn generic_decode<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'scope>) -> FnRet<'scope> {
    let mut scratch = scratch_buf::<32>();
    // unsafe cast: this will only ever be called by this privileged code in text_encoding.js
    let encoding = args.get(0).cast::<v8::String>().to_rust_cow_lossy(scope, &mut scratch);
    let encoding = encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap();

    let buf = args
        .get(1)
        .try_cast::<v8::ArrayBufferView>()
        .map_err(|_| TypeError("argument is not an `ArrayBuffer` or a view on one").throw(scope))?;
    let fatal = args.get(2).boolean_value(scope);
    let ignore_bom = args.get(3).boolean_value(scope);

    let mut buffer = buf.get_contents(&mut []);

    let mut decoder = if ignore_bom {
        encoding.new_decoder_without_bom_handling()
    } else {
        encoding.new_decoder_with_bom_removal()
    };

    let len = decoder
        .max_utf16_buffer_length(buffer.len())
        .ok_or_else(|| RangeError("Value too large to decode").throw(scope))?;
    let mut output = vec![0u16; len];

    let mut total_written = 0;
    let mut out = &mut output[..];
    if fatal {
        loop {
            let (result, read, written) = decoder.decode_to_utf16_without_replacement(buffer, out, true);
            total_written += written;
            match result {
                encoding_rs::DecoderResult::InputEmpty => break,
                encoding_rs::DecoderResult::OutputFull => {
                    buffer = &buffer[read..];
                    output.reserve(1);
                    out = &mut output[total_written..];
                }
                encoding_rs::DecoderResult::Malformed(_, _) => {
                    return Err(TypeError("The encoded data is not valid").throw(scope))
                }
            }
        }
    } else {
        loop {
            let (result, read, written, _) = decoder.decode_to_utf16(buffer, out, true);
            total_written += written;
            match result {
                encoding_rs::CoderResult::InputEmpty => break,
                encoding_rs::CoderResult::OutputFull => {
                    buffer = &buffer[read..];
                    output.reserve(1);
                    out = &mut output[total_written..];
                }
            }
        }
    }

    output.truncate(total_written);

    v8::String::new_from_two_byte(scope, &output, v8::NewStringType::Normal)
        .map(Into::into)
        .ok_or_else(|| RangeError("Value too large to decode").throw(scope))
}
