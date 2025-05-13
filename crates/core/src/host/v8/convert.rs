use super::util::{IntoException, ThrowExceptionResultExt, TypeError, ValueResult};
use num_traits::ToPrimitive;
use spacetimedb_sats::{i256, u256};

pub(super) trait FromValue: Sized {
    fn from_value<'s>(scope: &mut v8::HandleScope<'s>, val: v8::Local<'_, v8::Value>) -> ValueResult<'s, Self>;
}

impl FromValue for bool {
    fn from_value<'s>(scope: &mut v8::HandleScope<'s>, val: v8::Local<'_, v8::Value>) -> ValueResult<'s, Self> {
        let b = cast!(val, v8::Boolean, "boolean").map_err(|e| e.into_exception(scope))?;
        Ok(b.is_true())
    }
}

macro_rules! cast {
    ($val:expr, $t:ty, $expected:literal $(, $args:expr)* $(,)?) => {{
        let val = $val;
        val.try_cast::<$t>()
            .map_err(|_| $crate::host::v8::util::TypeError(format!(
                concat!("Expected ", $expected, ", got {__got}"),
                $($args,)*
                __got = val.type_repr()
            )))
    }};
}
pub(super) use cast;

macro_rules! num_from_value {
    ($($t:ident: $to:ident),*) => {
        $(impl FromValue for $t {
            fn from_value<'s>(scope: &mut v8::HandleScope<'s>, val: v8::Local<'_, v8::Value>) -> ValueResult<'s, Self> {
                let num = cast!(val, v8::Number, "number for {}", stringify!($t)).map_err_exc(scope)?;
                num.value()
                    .$to()
                    .ok_or_else(|| TypeError(format!("Value overflowed {}", stringify!($t))))
                    .map_err_exc(scope)
            }
        })*
    };
    (64bit $($t:ident: $value_method:ident),*) => {
        $(impl FromValue for $t {
            fn from_value<'s>(scope: &mut v8::HandleScope<'s>, val: v8::Local<'_, v8::Value>) -> ValueResult<'s, Self> {
                let int = cast!(val, v8::BigInt, "bigint for {}", stringify!($t)).map_err_exc(scope)?;
                let (val, ok) = int.$value_method();
                ok.then_some(val)
                    .ok_or_else(|| TypeError(format!("Value overflowed {}", stringify!($t))))
                    .map_err_exc(scope)
            }
        })*
    };
    (float $($t:ident),*) => {
        $(impl FromValue for $t {
            fn from_value<'s>(scope: &mut v8::HandleScope<'s>, val: v8::Local<'_, v8::Value>) -> ValueResult<'s, Self> {
                let num = cast!(val, v8::Number, "number for {}", stringify!($t)).map_err_exc(scope)?;
                Ok(num.value() as _)
            }
        })*
    };
    (large $($t:ident: $value64:ident),*)  => {
        $(impl FromValue for $t {
            fn from_value<'s>(scope: &mut v8::HandleScope<'s>, val: v8::Local<'_, v8::Value>) -> ValueResult<'s, Self> {
                let int = cast!(val, v8::BigInt, "bigint for {}", stringify!($t)).map_err_exc(scope)?;
                if let (val, true) = int.u64_value() {
                    return Ok(val.into());
                }
                const WORDS: usize = size_of::<$t>() / size_of::<u64>();
                let mut err = || TypeError(format!("Value overflowed {}", stringify!($t))).into_exception(scope);
                if int.word_count() > WORDS {
                    #[allow(unused_comparisons)]
                    if $t::MIN < 0 && int.word_count() == WORDS + 1 {
                        let mut words = [0u64; WORDS + 1];
                        let (sign, _) = int.to_words_array(&mut words);
                        let [prev @ .., last] = words;
                        if sign && prev == [0; WORDS] && last == (1 << 63) {
                            return Ok($t::MIN)
                        }
                    }
                    return Err(err());
                }
                let mut words = [0u64; WORDS];
                let (sign, _) = int.to_words_array(&mut words);
                let bytes = bytemuck::must_cast(words.map(|w| w.to_le_bytes()));
                let x = Self::from_le_bytes(bytes);
                if sign {
                    x.checked_neg().ok_or_else(err)
                } else {
                    Ok(x)
                }
            }
        })*
    };
}

num_from_value!(u8: to_u8, i8: to_i8, u16: to_u16, i16: to_i16, u32: to_u32, i32: to_i32);

num_from_value!(64bit u64: u64_value, i64: i64_value);

num_from_value!(float f32, f64);

num_from_value!(large u128: u64_value, i128: i64_value, u256: u64_value, i256: i64_value);

pub(super) trait ToValue {
    fn to_value<'s>(&self, scope: &mut v8::HandleScope<'s>) -> ValueResult<'s, v8::Local<'s, v8::Value>>;
}

impl ToValue for bool {
    fn to_value<'s>(&self, scope: &mut v8::HandleScope<'s>) -> ValueResult<'s, v8::Local<'s, v8::Value>> {
        Ok(v8::Boolean::new(scope, *self).into())
    }
}

macro_rules! num_to_value {
    ($($t:ident),*) => {
        $(impl ToValue for $t {
            fn to_value<'s>(&self, scope: &mut v8::HandleScope<'s>) -> ValueResult<'s, v8::Local<'s, v8::Value>> {
                Ok(v8::Number::new(scope, *self as f64).into())
            }
        })*
    };
    (64bit $($t:ident: $new_from:ident),*) => {
        $(impl ToValue for $t {
            fn to_value<'s>(&self, scope: &mut v8::HandleScope<'s>) -> ValueResult<'s, v8::Local<'s, v8::Value>> {
                Ok(v8::BigInt::$new_from(scope, *self).into())
            }
        })*
    };
    (large $($t:ident),*) => {
        $(impl ToValue for $t {
            fn to_value<'s>(&self, scope: &mut v8::HandleScope<'s>) -> ValueResult<'s, v8::Local<'s, v8::Value>> {
                const WORDS: usize = size_of::<$t>() / size_of::<u64>();
                #[allow(unused_comparisons)]
                let sign = *self < 0;
                let Some(magnitude) = (if sign { self.checked_neg() } else { Some(*self) }) else {
                    let mut words = [0u64; WORDS + 1];
                    let [.., last] = &mut words;
                    *last = 1 << 63;
                    return Ok(v8::BigInt::new_from_words(scope, true, &words).unwrap().into());
                };
                let bytes = magnitude.to_le_bytes();
                let words = bytemuck::must_cast::<_, [u64; WORDS]>(bytes).map(u64::from_le);
                Ok(v8::BigInt::new_from_words(scope, sign, &words).unwrap().into())
            }
        })*
    };
}

num_to_value!(u8, i8, u16, i16, u32, i32, f32, f64);

num_to_value!(64bit u64: new_from_u64, i64: new_from_i64);

num_to_value!(large u128, i128, u256, i256);
