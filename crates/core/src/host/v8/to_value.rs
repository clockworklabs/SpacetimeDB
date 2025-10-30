use bytemuck::{NoUninit, Pod};
use spacetimedb_sats::{i256, u256};
use v8::{BigInt, Boolean, Integer, Local, Number, PinScope, Value};

/// Types that can be converted to a v8-stack-allocated [`Value`].
/// The conversion can be done without the possibility for error.
pub(super) trait ToValue {
    /// Converts `self` within `scope` (a sort of stack management in V8) to a [`Value`].
    fn to_value<'scope>(&self, scope: &PinScope<'scope, '_>) -> Local<'scope, Value>;
}

/// Provides a [`ToValue`] implementation.
macro_rules! impl_to_value {
    ($ty:ty, ($val:ident, $scope:ident) => $logic:expr) => {
        impl ToValue for $ty {
            fn to_value<'scope>(&self, $scope: &PinScope<'scope, '_>) -> Local<'scope, Value> {
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
fn le_bytes_to_bigint<'scope, const N: usize, const W: usize>(
    scope: &PinScope<'scope, '_>,
    sign: bool,
    le_bytes: [u8; N],
) -> Local<'scope, BigInt>
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

pub(super) const WORD_MIN: u64 = i64::MIN as u64;

/// Returns `iN::MIN` for `N = 8 * WORDS` as a V8 [`BigInt`].
///
/// Examples:
/// `i64::MIN` becomes `-1 * WORD_MIN * (2^64)^0 = -1 * WORD_MIN`
/// `i128::MIN` becomes `-1 * (0 * (2^64)^0 + WORD_MIN * (2^64)^1) = -1 * WORD_MIN * 2^64`
/// `i256::MIN` becomes `-1 * (0 * (2^64)^0 + 0 * (2^64)^1 + WORD_MIN * (2^64)^2) = -1 * WORD_MIN * (2^128)`
fn signed_min_bigint<'scope, const WORDS: usize>(scope: &PinScope<'scope, '_>) -> Local<'scope, BigInt> {
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

#[cfg(test)]
pub(in super::super) mod test {
    use super::super::{from_value::FromValue, new_isolate, V8Runtime};
    use super::*;
    use core::fmt::Debug;
    use proptest::prelude::*;
    use spacetimedb_sats::proptest::{any_i256, any_u256};
    use v8::{scope_with_context, Context};

    /// Sets up V8 and runs `logic` with a [`PinScope`].
    pub(in super::super) fn with_scope<R>(logic: impl FnOnce(&mut PinScope<'_, '_>) -> R) -> R {
        V8Runtime::init_for_test();

        let mut isolate = new_isolate();
        scope_with_context!(let scope, &mut isolate, Context::new(scope, Default::default()));

        logic(scope)
    }

    /// Roundtrips `rust_val` via `ToValue` to the V8 representation
    /// and then back via `FromValue`,
    /// asserting that it's the same as the passed value.
    fn assert_roundtrips<T: ToValue + FromValue + PartialEq + Debug>(rust_val: T) {
        with_scope(|scope| {
            // Convert to JS and then back.
            let js_val = rust_val.to_value(scope);
            let rust_val_prime = T::from_value(js_val, scope).unwrap();

            // We should end up where we started.
            assert_eq!(rust_val, rust_val_prime);
        })
    }

    proptest! {
        #[test] fn test_bool(x: bool) { assert_roundtrips(x); }

        #[test] fn test_f32(x: f32) { assert_roundtrips(x); }
        #[test] fn test_f64(x: f64) { assert_roundtrips(x); }

        #[test] fn test_u8(x: u8) { assert_roundtrips(x); }
        #[test] fn test_u16(x: u16) { assert_roundtrips(x); }
        #[test] fn test_u32(x: u32) { assert_roundtrips(x); }
        #[test] fn test_u64(x: u64) { assert_roundtrips(x); }
        #[test] fn test_u128(x: u128) { assert_roundtrips(x); }
        #[test] fn test_u256(x in any_u256()) { assert_roundtrips(x); }

        #[test] fn test_i8(x: i8) { assert_roundtrips(x); }
        #[test] fn test_i16(x: i16) { assert_roundtrips(x); }
        #[test] fn test_i32(x: i32) { assert_roundtrips(x); }
        #[test] fn test_i64(x: i64) { assert_roundtrips(x); }
        #[test] fn test_i128(x: i128) { assert_roundtrips(x); }
        #[test] fn test_i256(x in any_i256()) { assert_roundtrips(x); }
    }

    #[test]
    fn test_signed_mins() {
        assert_roundtrips(i8::MIN);
        assert_roundtrips(i16::MIN);
        assert_roundtrips(i32::MIN);
        assert_roundtrips(i64::MIN);
        assert_roundtrips(i128::MIN);
        assert_roundtrips(i256::MIN);
    }
}
