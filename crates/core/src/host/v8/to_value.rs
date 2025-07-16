#![allow(dead_code)]

use bytemuck::{NoUninit, Pod};
use spacetimedb_sats::{i256, u256};
use v8::{BigInt, Boolean, HandleScope, Integer, Local, Number, Value};

/// Types that can be converted to a v8-stack-allocated [`Value`].
/// The conversion can be done without the possibility for error.
pub(super) trait ToValue {
    /// Convert `self` within `scope` (a sort of stack management in V8) to a [`Value`].
    fn to_value<'s>(&self, scope: &mut HandleScope<'s>) -> Local<'s, Value>;
}

/// Provides a [`ToValue`] implementation.
macro_rules! impl_to_value {
    ($ty:ty, ($val:ident, $scope:ident) => $logic:expr) => {
        impl ToValue for $ty {
            fn to_value<'s>(&self, $scope: &mut HandleScope<'s>) -> Local<'s, Value> {
                let $val = *self;
                $logic.into()
            }
        }
    };
}

// Floats are the most direct conversion.
impl_to_value!(f32, (val, scope) => (val as f64).to_value(scope));
impl_to_value!(f64, (val, scope) => Number::new(scope, val));

// Booleans have dedicated conversions.
impl_to_value!(bool, (val, scope) => Boolean::new(scope, val));

// Sub-32-bit integers get widened to 32-bit first.
impl_to_value!(i8, (val, scope) => (val as i32).to_value(scope));
impl_to_value!(u8, (val, scope) => (val as u32).to_value(scope));
impl_to_value!(i16, (val, scope) => (val as i32).to_value(scope));
impl_to_value!(u16, (val, scope) => (val as u32).to_value(scope));

// 32-bit integers have dedicated conversions.
impl_to_value!(i32, (val, scope) => Integer::new(scope, val));
impl_to_value!(u32, (val, scope) => Integer::new_from_unsigned(scope, val));

// 64-bit integers have dedicated conversions.
impl_to_value!(i64, (val, scope) => BigInt::new_from_i64(scope, val));
impl_to_value!(u64, (val, scope) => BigInt::new_from_u64(scope, val));

/// Converts the little-endian bytes of a number to a V8 [`BigInt`].
///
/// The `sign` is passed along to the `BigInt`.
fn le_bytes_to_bigint<'s, const N: usize, const W: usize>(
    scope: &mut HandleScope<'s>,
    sign: bool,
    le_bytes: [u8; N],
) -> Local<'s, BigInt>
where
    [u8; N]: NoUninit,
    [u64; W]: Pod,
{
    let words = bytemuck::must_cast::<_, [u64; W]>(le_bytes).map(u64::from_le);
    BigInt::new_from_words(scope, sign, &words).unwrap()
}

// Unsigned 128-bit and 256-bit integers have dedicated conversions.
// They are convered to a list of words before becoming `BigInt`s.
impl_to_value!(u128, (val, scope) => le_bytes_to_bigint::<16, 2>(scope, false, val.to_le_bytes()));
impl_to_value!(u256, (val, scope) => le_bytes_to_bigint::<32, 4>(scope, false, val.to_le_bytes()));

/// Returns `iN::MIN` for `N = 8 * WORDS` as a V8 [`BigInt`].
///
/// Examples:
/// `i64::MIN` becomes `-1 * WORD_MIN * (2^64)^0 = -1 * WORD_MIN`
/// `i128::MIN` becomes `-1 * (0 * (2^64)^0 + WORD_MIN * (2^64)^1) = -1 * WORD_MIN * 2^64`
/// `i256::MIN` becomes `-1 * (0 * (2^64)^0 + 0 * (2^64)^1 + WORD_MIN * (2^64)^2) = -1 * WORD_MIN * (2^128)`
fn signed_min_bigint<'s, const WORDS: usize>(scope: &mut HandleScope<'s>) -> Local<'s, BigInt> {
    const WORD_MIN: u64 = i64::MIN as u64;
    let words = &mut [0u64; WORDS];
    if let [.., last] = words.as_mut_slice() {
        *last = WORD_MIN;
    }
    v8::BigInt::new_from_words(scope, true, words).unwrap()
}

// Signed 128-bit and 256-bit integers have dedicated conversions.
//
// For the negative number case, the magnitude is computed and the sign is passed along.
// A special case is the minimum number.
impl_to_value!(i128, (val, scope) => {
    let sign = val.is_negative();
    let magnitude = if sign { val.checked_neg() } else { Some(val) };
    match magnitude {
        Some(magnitude) => le_bytes_to_bigint::<16, 2>(scope, sign, magnitude.to_le_bytes()),
        None => signed_min_bigint::<2>(scope),
    }
});
impl_to_value!(i256, (val, scope) => {
    let sign = val.is_negative();
    let magnitude = if sign { val.checked_neg() } else { Some(val) };
    match magnitude {
        Some(magnitude) => le_bytes_to_bigint::<32, 4>(scope, sign, magnitude.to_le_bytes()),
        None => signed_min_bigint::<4>(scope),
    }
});
