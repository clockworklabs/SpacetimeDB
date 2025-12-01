use super::error::{ExceptionValue, IntoException as _, TypeError, ValueResult};
use bytemuck::{AnyBitPattern, NoUninit};
use spacetimedb_sats::{i256, u256};
use v8::{BigInt, Boolean, Int32, Local, Number, PinScope, Uint32, Value};

/// Types that a v8 [`Value`] can be converted into.
pub(super) trait FromValue: Sized {
    /// Converts `val` in `scope` to `Self` if possible.
    fn from_value<'scope>(val: Local<'_, Value>, scope: &PinScope<'scope, '_>) -> ValueResult<'scope, Self>;
}

/// Provides a [`FromValue`] implementation.
macro_rules! impl_from_value {
    ($ty:ty, ($val:ident, $scope:ident) => $logic:expr) => {
        impl FromValue for $ty {
            fn from_value<'scope>($val: Local<'_, Value>, $scope: &PinScope<'scope, '_>) -> ValueResult<'scope, Self> {
                $logic
            }
        }
    };
}

/// Tries to cast `Value` into `T` or raises a JS exception as a returned `Err` value.
pub(super) fn try_cast<'scope_a, 'scope_b, T>(
    scope: &PinScope<'scope_a, '_>,
    val: Local<'scope_b, Value>,
    on_err: impl FnOnce(&str) -> String,
) -> ValueResult<'scope_a, Local<'scope_b, T>>
where
    Local<'scope_b, T>: TryFrom<Local<'scope_b, Value>>,
{
    val.try_cast::<T>()
        .map_err(|_| TypeError(on_err(val.type_repr())).into_exception(scope))
}

/// Tries to cast `Value` into `T` or raises a JS exception as a returned `Err` value.
macro_rules! cast {
    ($scope:expr, $val:expr, $js_ty:ty, $expected:literal $(, $args:expr)* $(,)?) => {{
        $crate::host::v8::from_value::try_cast::<$js_ty>($scope, $val, |got| format!(concat!("Expected ", $expected, ", got {__got}"), $($args,)* __got = got))
    }};
}
pub(super) use cast;

/// Returns a JS exception value indicating that a value overflowed
/// when converting to the type `rust_ty`.
fn value_overflowed<'scope>(rust_ty: &str, scope: &PinScope<'scope, '_>) -> ExceptionValue<'scope> {
    TypeError(format!("Value overflowed `{rust_ty}`")).into_exception(scope)
}

/// Returns a JS exception value indicating that a value underflowed
/// when converting to the type `rust_ty`.
fn value_underflowed<'scope>(rust_ty: &str, scope: &PinScope<'scope, '_>) -> ExceptionValue<'scope> {
    TypeError(format!("Value underflowed `{rust_ty}`")).into_exception(scope)
}

// `FromValue for bool`.
impl_from_value!(bool, (val, scope) => cast!(scope, val, Boolean, "boolean").map(|b| b.is_true()));

// `FromValue for u8, u16, u32, i8, i16, i32`.
macro_rules! int32_from_value {
    ($js_ty:ty, $rust_ty:ty) => {
        impl_from_value!($rust_ty, (val, scope) => {
            let num = cast!(scope, val, $js_ty, "number for `{}`", stringify!($rust_ty))?;
            num.value().try_into().map_err(|_| value_overflowed(stringify!($rust_ty), scope))
        });
    }
}
int32_from_value!(Uint32, u8);
int32_from_value!(Uint32, u16);
int32_from_value!(Uint32, u32);
int32_from_value!(Int32, i8);
int32_from_value!(Int32, i16);
int32_from_value!(Int32, i32);

// `FromValue for f32, f64`.
//
// Note that, as per the rust-reference,
// - "Casting from an f64 to an f32 will produce the closest possible f32"
// https://doc.rust-lang.org/reference/expressions/operator-expr.html#r-expr.as.numeric.float-narrowing
macro_rules! float_from_value {
    ($rust_ty:ty) => {
        impl_from_value!($rust_ty, (val, scope) => {
            cast!(scope, val, Number, "number for `{}`", stringify!($rust_ty)).map(|n| n.value() as _)
        });
    }
}
float_from_value!(f32);
float_from_value!(f64);

// `FromValue for u64, i64`.
macro_rules! int64_from_value {
    ($rust_ty:ty, $conv_method: ident) => {
        impl_from_value!($rust_ty, (val, scope) => {
            let rust_ty = stringify!($rust_ty);
            let bigint = cast!(scope, val, BigInt, "bigint for `{}`", rust_ty)?;
            let (val, ok) = bigint.$conv_method();
            ok.then_some(val).ok_or_else(|| value_overflowed(rust_ty, scope))
        });
    }
}
int64_from_value!(u64, u64_value);
int64_from_value!(i64, i64_value);

/// Converts `bigint` into its signnedness and its list of bytes in little-endian,
/// or errors on overflow or unwanted signedness.
///
/// Parameters:
/// - `N` are the number of bytes to accept at most.
/// - `W = N / 8` are the number of words to accept at most.
/// - `UNSIGNED` is `true` if only unsigned integers are accepted.
/// - `rust_ty` is the target type as a string, for errors.
/// - `scope` for any JS exceptions that need to be raised.
/// - `bigint` is the integer to convert.
fn bigint_to_bytes<'scope, const N: usize, const W: usize, const UNSIGNED: bool>(
    rust_ty: &str,
    scope: &PinScope<'scope, '_>,
    bigint: &BigInt,
) -> ValueResult<'scope, (bool, [u8; N])>
where
    [[u8; 8]; W]: NoUninit,
    [u8; N]: AnyBitPattern,
{
    // Read the words.
    let mut words = [0u64; W];
    let (sign, _) = bigint.to_words_array(&mut words);

    if bigint.word_count() > W {
        // There's an under-/over-flow if the caller cannot handle that many words.
        return Err(if sign {
            value_underflowed(rust_ty, scope)
        } else {
            value_overflowed(rust_ty, scope)
        });
    }

    if sign && UNSIGNED {
        // There's an overflow if the caller cannot accept negative numbers.
        return Err(value_overflowed(rust_ty, scope));
    }

    // convert the words to little-endian bytes.
    let bytes = bytemuck::must_cast(words.map(|w| w.to_le_bytes()));
    Ok((sign, bytes))
}

// `FromValue for u128, u256`.
macro_rules! unsigned_bigint_from_value {
    ($rust_ty:ty, $bytes:literal, $words:literal) => {
        impl_from_value!($rust_ty, (val, scope) => {
            let rust_ty = stringify!($rust_ty);
            let bigint = cast!(scope, val, v8::BigInt, "bigint for `{}`", rust_ty)?;
            if let (val, true) = bigint.u64_value() {
                // Fast path.
                return Ok(val.into());
            }
            let (_, bytes) = bigint_to_bytes::<$bytes, $words, true>(rust_ty, scope, &bigint)?;
            Ok(Self::from_le_bytes(bytes))
        });
    };
}
unsigned_bigint_from_value!(u128, 16, 2);
unsigned_bigint_from_value!(u256, 32, 4);

// `FromValue for i128, i256`.
macro_rules! signed_bigint_from_value {
    ($rust_ty:ty, $bytes:literal, $words:literal) => {
        impl_from_value!($rust_ty, (val, scope) => {
            let rust_ty = stringify!($rust_ty);
            let bigint = cast!(scope, val, v8::BigInt, "bigint for `{}`", rust_ty)?;
            if let (val, true) = bigint.i64_value() {
                // Fast path.
                return Ok(val.into());
            }
            let (sign, bytes) = bigint_to_bytes::<$bytes, $words, false>(rust_ty, scope, &bigint)?;
            let x = Self::from_le_bytes(bytes);
            Ok(if sign {
                // A negative number, but we have a positive number `x`, so we want `-x`.
                // If that's not possible, and as we know there's no underflow, we have `MIN`.
                x.checked_neg().unwrap_or(Self::MIN)
            } else {
                x
            })
        });
    };
}
signed_bigint_from_value!(i128, 16, 2);
signed_bigint_from_value!(i256, 32, 4);
