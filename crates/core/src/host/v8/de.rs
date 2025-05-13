use std::borrow::Cow;

use spacetimedb_sats::{de, i256, u256};

use super::convert::{cast, FromValue};
use super::{scratch_buf, ExceptionOptionExt, ExceptionThrown, ThrowExceptionResultExt};

pub(super) struct Deserializer<'a, 's> {
    common: DeserializerCommon<'a, 's>,
    input: v8::Local<'s, v8::Value>,
}

impl<'a, 's> Deserializer<'a, 's> {
    pub fn new(
        scope: &'a mut v8::HandleScope<'s>,
        input: v8::Local<'_, v8::Value>,
        key_cache: &'a mut KeyCache,
    ) -> Self {
        let input = v8::Local::new(scope, input);
        let common = DeserializerCommon { scope, key_cache };
        Deserializer { input, common }
    }
}

struct DeserializerCommon<'a, 's> {
    scope: &'a mut v8::HandleScope<'s>,
    key_cache: &'a mut KeyCache,
}

impl<'a, 's> DeserializerCommon<'a, 's> {
    fn reborrow(&mut self) -> DeserializerCommon<'_, 's> {
        DeserializerCommon {
            scope: self.scope,
            key_cache: self.key_cache,
        }
    }
}

macro_rules! def_key_cache {
    ($($key:ident$(: $string:expr)?),* $(,)?) => {
        #[derive(Default)]
        pub(super) struct KeyCache {
            $($key: Option<v8::Global<v8::String>>,)*
        }
        impl KeyCache {
            $(pub(super) fn $key<'s>(&mut self, scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::String> {
                get_or_create_key(scope, &mut self.$key, ($($string,)? stringify!($key),).0)
            })*
        }
    };
}

fn get_or_create_key<'s>(
    scope: &mut v8::HandleScope<'s>,
    key: &mut Option<v8::Global<v8::String>>,
    string: &str,
) -> v8::Local<'s, v8::String> {
    if let Some(s) = &*key {
        v8::Local::new(scope, s)
    } else {
        let s = v8_struct_key(scope, string);
        *key = Some(v8::Global::new(scope, s));
        s
    }
}

def_key_cache!(tag, value, some);

// creates an optimized v8::String for a struct field
pub fn v8_struct_key<'s>(scope: &mut v8::HandleScope<'s>, field: &str) -> v8::Local<'s, v8::String> {
    // Internalized v8 strings are significantly faster than "normal" v8 strings
    // since v8 deduplicates re-used strings minimizing new allocations
    // see: https://github.com/v8/v8/blob/14ac92e02cc3db38131a57e75e2392529f405f2f/include/v8.h#L3165-L3171
    v8::String::new_from_utf8(scope, field.as_ref(), v8::NewStringType::Internalized).unwrap()
}

pub(super) enum Error<'s> {
    Value(v8::Local<'s, v8::Value>),
    Exception(ExceptionThrown),
    String(String),
}

impl<'s> From<ExceptionThrown> for Error<'s> {
    fn from(v: ExceptionThrown) -> Self {
        Self::Exception(v)
    }
}

impl<'s> From<v8::Local<'s, v8::Value>> for Error<'s> {
    fn from(v: v8::Local<'s, v8::Value>) -> Self {
        Self::Value(v)
    }
}

impl de::Error for Error<'_> {
    fn custom(msg: impl core::fmt::Display) -> Self {
        Self::String(msg.to_string())
    }
}

fn extend_local<'s, T>(local: v8::Local<'s, T>) -> &'s T {
    unsafe { std::mem::transmute::<&T, &'s T>(&local) }
}

macro_rules! deserialize_primitive {
    ($dmethod:ident, $t:ty) => {
        fn $dmethod(self) -> Result<$t, Self::Error> {
            FromValue::from_value(self.common.scope, self.input).map_err(Error::Value)
        }
    };
}

impl<'de, 'a, 's: 'de, 'x> de::Deserializer<'de> for Deserializer<'a, 's> {
    type Error = Error<'s>;

    fn deserialize_product<V: de::ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let obj = cast!(
            self.input,
            v8::Object,
            "object for product type {}",
            visitor.product_name().unwrap_or("")
        )
        .map_err_exc(self.common.scope)?;

        visitor.visit_named_product(ProductAccess {
            common: self.common,
            obj,
            next_value: None,
            n: 0,
        })
    }

    fn deserialize_sum<V: de::SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let scope = &mut *self.common.scope;
        let val = if visitor.is_option() {
            if self.input.is_null_or_undefined() {
                return visitor.visit_sum(de::NoneAccess::new());
            }
            let val = cast!(self.input, v8::Object, "nullish or {{some:_}} for option").map_err_exc(scope)?;
            let some_field = self.common.key_cache.some(scope);
            if val.has_own_property(scope, some_field.into()).err()? {
                let value = val.get(scope, some_field.into()).err()?;
                return visitor.visit_sum(SumAccess {
                    common: self.common,
                    tag: some_field,
                    value,
                });
            }
            val
        } else {
            cast!(
                self.input,
                v8::Object,
                "object for sum type {}",
                visitor.sum_name().unwrap_or("")
            )
            .map_err_exc(scope)?
        };

        let tag_field = self.common.key_cache.tag(scope);
        let tag = val.get(scope, tag_field.into()).err()?;
        let tag = cast!(
            tag,
            v8::String,
            "string for sum tag of {}",
            visitor.sum_name().unwrap_or("")
        )
        .map_err_exc(scope)?;

        let value_field = self.common.key_cache.value(scope);
        let value = val.get(scope, value_field.into()).err()?;

        visitor.visit_sum(SumAccess {
            common: self.common,
            tag,
            value,
        })
    }

    deserialize_primitive!(deserialize_bool, bool);

    deserialize_primitive!(deserialize_u8, u8);
    deserialize_primitive!(deserialize_u16, u16);
    deserialize_primitive!(deserialize_u32, u32);
    deserialize_primitive!(deserialize_u64, u64);
    deserialize_primitive!(deserialize_u128, u128);
    deserialize_primitive!(deserialize_u256, u256);

    deserialize_primitive!(deserialize_i8, i8);
    deserialize_primitive!(deserialize_i16, i16);
    deserialize_primitive!(deserialize_i32, i32);
    deserialize_primitive!(deserialize_i64, i64);
    deserialize_primitive!(deserialize_i128, i128);
    deserialize_primitive!(deserialize_i256, i256);

    deserialize_primitive!(deserialize_f64, f64);
    deserialize_primitive!(deserialize_f32, f32);

    fn deserialize_str<V: de::SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let scope = self.common.scope;
        let val = cast!(self.input, v8::String, "string").map_err_exc(scope)?;
        let mut buf = scratch_buf::<64>();
        match val.to_rust_cow_lossy(scope, &mut buf) {
            Cow::Borrowed(s) => visitor.visit(s),
            Cow::Owned(string) => visitor.visit_owned(string),
        }
    }

    fn deserialize_bytes<V: de::SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let scope = self.common.scope;
        let arr = cast!(self.input, v8::Uint8Array, "Uint8Array for bytes").map_err_exc(scope)?;
        let storage: &'static mut [u8] = &mut [0; v8::TYPED_ARRAY_MAX_SIZE_IN_HEAP];
        let bytes = extend_local(arr).get_contents(storage);
        visitor.visit_borrowed(bytes)
    }

    fn deserialize_array_seed<V: de::ArrayVisitor<'de, T::Output>, T: de::DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        let arr = cast!(self.input, v8::Array, "Array").map_err_exc(self.common.scope)?;
        visitor.visit(ArrayAccess::new(arr, self.common, seed))
    }
}

struct ProductAccess<'a, 's> {
    common: DeserializerCommon<'a, 's>,
    obj: v8::Local<'s, v8::Object>,
    next_value: Option<v8::Local<'s, v8::Value>>,
    n: usize,
}

impl<'de, 's: 'de> de::NamedProductAccess<'de> for ProductAccess<'_, 's> {
    type Error = Error<'s>;

    fn get_field_ident<V: de::FieldNameVisitor<'de>>(&mut self, visitor: V) -> Result<Option<V::Output>, Self::Error> {
        let scope = &mut *self.common.scope;
        while let Some(field) = visitor.nth_name(self.n) {
            let i = self.n;
            self.n += 1;
            let key = v8_struct_key(scope, field);
            if !self.obj.has_own_property(scope, key.into()).err()? {
                continue;
            }
            let val = self.obj.get(scope, key.into()).err()?;
            self.next_value = Some(val);
            return visitor.visit_seq(i).map(Some);
        }
        Ok(None)
    }

    fn get_field_value_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Output, Self::Error> {
        let val = self
            .next_value
            .take()
            .expect("Call next_key_seed before next_value_seed");
        seed.deserialize(Deserializer {
            common: self.common.reborrow(),
            input: val,
        })
    }
}

struct SumAccess<'a, 's> {
    common: DeserializerCommon<'a, 's>,
    tag: v8::Local<'s, v8::String>,
    value: v8::Local<'s, v8::Value>,
    // p1: std::marker::PhantomData<&'x ()>,
}

impl<'de, 'a, 's: 'de> de::SumAccess<'de> for SumAccess<'a, 's> {
    type Error = Error<'s>;
    type Variant = Deserializer<'a, 's>;

    fn variant<V: de::VariantVisitor>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        let mut buf = scratch_buf::<32>();
        let name = self.tag.to_rust_cow_lossy(self.common.scope, &mut buf);
        let variant = visitor.visit_name::<Self::Error>(&name)?;
        let dpayload = Deserializer {
            common: self.common,
            input: self.value,
        };

        Ok((variant, dpayload))
    }
}

impl<'de, 'a, 's: 'de> de::VariantAccess<'de> for Deserializer<'a, 's> {
    type Error = Error<'s>;

    fn deserialize_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        seed.deserialize(self)
    }
}

struct ArrayAccess<'a, 's, T> {
    common: DeserializerCommon<'a, 's>,
    arr: v8::Local<'s, v8::Array>,
    seeds: std::iter::RepeatN<T>,
    len: u32,
}

impl<'de, 'a, 's, T> ArrayAccess<'a, 's, T>
where
    T: de::DeserializeSeed<'de> + Clone,
{
    fn new(arr: v8::Local<'s, v8::Array>, common: DeserializerCommon<'a, 's>, seed: T) -> Self {
        let len = arr.length();
        Self {
            arr,
            common,
            seeds: std::iter::repeat_n(seed, len as usize),
            len,
        }
    }
}

impl<'de, 's: 'de, T> de::ArrayAccess<'de> for ArrayAccess<'_, 's, T>
where
    T: de::DeserializeSeed<'de> + Clone,
{
    type Element = T::Output;
    type Error = Error<'s>;

    fn next_element(&mut self) -> Result<Option<Self::Element>, Self::Error> {
        let i = self.len - self.seeds.len() as u32;
        if let Some(seed) = self.seeds.next() {
            let val = self.arr.get_index(self.common.scope, i).err()?;
            let val = seed.deserialize(Deserializer {
                common: self.common.reborrow(),
                input: val,
            })?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.seeds.len())
    }
}
