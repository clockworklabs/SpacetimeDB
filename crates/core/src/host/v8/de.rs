#![allow(dead_code)]

use super::error::{exception_already_thrown, ExceptionThrown};
use super::from_value::{cast, FromValue};
use core::fmt;
use core::iter::{repeat_n, RepeatN};
use core::mem::MaybeUninit;
use derive_more::From;
use spacetimedb_sats::de::{
    self, ArrayVisitor, DeserializeSeed, NoneAccess, ProductVisitor, SliceVisitor, SomeAccess, SumVisitor,
};
use spacetimedb_sats::{i256, u256};
use std::borrow::{Borrow, Cow};
use v8::{Array, Global, HandleScope, Local, Name, Object, Uint8Array, Value};

/// Deserializes from V8 values.
pub(super) struct Deserializer<'a, 's> {
    common: DeserializerCommon<'a, 's>,
    input: Local<'s, Value>,
}

impl<'a, 's> Deserializer<'a, 's> {
    /// Creates a new deserializer from `input` in `scope`.
    pub fn new(scope: &'a mut HandleScope<'s>, input: Local<'_, Value>, key_cache: &'a mut KeyCache) -> Self {
        let input = Local::new(scope, input);
        let common = DeserializerCommon { scope, key_cache };
        Deserializer { input, common }
    }
}

/// Things shared between various [`Deserializer`]s.
///
/// The lifetime `'s` is that of the scope of values deserialized.
struct DeserializerCommon<'a, 's> {
    /// The scope of values to deserialize.
    scope: &'a mut HandleScope<'s>,
    /// A cache for frequently used strings.
    key_cache: &'a mut KeyCache,
}

impl<'s> DeserializerCommon<'_, 's> {
    fn reborrow(&mut self) -> DeserializerCommon<'_, 's> {
        DeserializerCommon {
            scope: self.scope,
            key_cache: self.key_cache,
        }
    }
}

/// The possible errors that [`Deserializer`] can produce.
#[derive(Debug, From)]
pub(super) enum Error<'s> {
    Value(Local<'s, Value>),
    Exception(ExceptionThrown),
    Custom(String),
}

impl de::Error for Error<'_> {
    fn custom(msg: impl fmt::Display) -> Self {
        Self::Custom(msg.to_string())
    }
}

/// Returns a scratch buffer to fill when deserializing strings.
fn scratch_buf<const N: usize>() -> [MaybeUninit<u8>; N] {
    [const { MaybeUninit::uninit() }; N]
}

/// A cache for frequently used strings to avoid re-interning them.
#[derive(Default)]
pub(super) struct KeyCache {
    /// The `tag` property for sum values in JS.
    tag: Option<Global<v8::String>>,
    /// The `value` property for sum values in JS.
    value: Option<Global<v8::String>>,
}

impl KeyCache {
    /// Returns the `tag` property name.
    pub(super) fn tag<'s>(&mut self, scope: &mut HandleScope<'s>) -> Local<'s, v8::String> {
        Self::get_or_create_key(scope, &mut self.tag, "tag")
    }

    /// Returns the `value` property name.
    pub(super) fn value<'s>(&mut self, scope: &mut HandleScope<'s>) -> Local<'s, v8::String> {
        Self::get_or_create_key(scope, &mut self.value, "value")
    }

    /// Returns an interned string corresponding to `string`
    /// and memoizes the creation on the v8 side.
    fn get_or_create_key<'s>(
        scope: &mut HandleScope<'s>,
        slot: &mut Option<Global<v8::String>>,
        string: &str,
    ) -> Local<'s, v8::String> {
        if let Some(s) = &*slot {
            v8::Local::new(scope, s)
        } else {
            let s = v8_interned_string(scope, string);
            *slot = Some(v8::Global::new(scope, s));
            s
        }
    }
}

// Creates an interned [`v8::String`].
pub(super) fn v8_interned_string<'s>(scope: &mut HandleScope<'s>, field: &str) -> Local<'s, v8::String> {
    // Internalized v8 strings are significantly faster than "normal" v8 strings
    // since v8 deduplicates re-used strings minimizing new allocations
    // see: https://github.com/v8/v8/blob/14ac92e02cc3db38131a57e75e2392529f405f2f/include/v8.h#L3165-L3171
    v8::String::new_from_utf8(scope, field.as_ref(), v8::NewStringType::Internalized).unwrap()
}

/// Extracts a reference `&'s T` from an owned V8 [`Local<'s, T>`].
///
/// The lifetime `'s` is that of the [`HandleScope<'s>`].
/// This ensures that the reference to `T` won't outlive the `HandleScope`.
fn deref_local<'s, T>(local: Local<'s, T>) -> &'s T {
    let reference = local.borrow();
    // SAFETY: Lifetime extend `'0` to `'s`.
    // This is safe as the returned reference `&'s T`
    // will not outlive its `HandleScope<'s, _>`,
    // as both are tied to the lifetime `'s`.
    unsafe { core::mem::transmute::<&T, &'s T>(reference) }
}

/// Deserializes a primitive via [`FromValue`].
macro_rules! deserialize_primitive {
    ($dmethod:ident, $t:ty) => {
        fn $dmethod(self) -> Result<$t, Self::Error> {
            FromValue::from_value(self.input, self.common.scope).map_err(Error::Value)
        }
    };
}

impl<'de, 'a, 's: 'de> de::Deserializer<'de> for Deserializer<'a, 's> {
    type Error = Error<'s>;

    // Deserialization of primitive types defers to `FromValue`.
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

    fn deserialize_product<V: ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let object = cast!(
            self.common.scope,
            self.input,
            Object,
            "object for product type `{}`",
            visitor.product_name().unwrap_or("<unknown>")
        )?;

        visitor.visit_named_product(ProductAccess {
            common: self.common,
            object,
            next_value: None,
            index: 0,
        })
    }

    fn deserialize_sum<V: SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let scope = &mut *self.common.scope;
        let sum_name = visitor.sum_name().unwrap_or("<unknown>");

        // We expect a canonical representation of a sum value in JS to be
        // `{ tag: "foo", value: a_value_for_foo }`
        // with special convenience for optionals
        // where we also accept `null`, `undefined` and an object without `tag`.
        let (object, tag_field) = 'treat_as_regular_sum: {
            // Optionals receive some special handling for added convenience in JS.
            if visitor.is_option() {
                // If we don't have an object at all,
                // it's either `null | undefined` which means `none`
                // or it is `some(the_value)`.
                if let Some(object) = self.input.to_object(scope) {
                    // If there is `tag` field, treat this as a normal sum.
                    // Otherwise, we have `some(the_value)`.
                    let tag_field = self.common.key_cache.tag(scope);
                    if object
                        .has_own_property(scope, tag_field.into())
                        .ok_or_else(exception_already_thrown)?
                    {
                        break 'treat_as_regular_sum (object, tag_field);
                    }
                } else if self.input.is_null_or_undefined() {
                    // JS has support for `undefined` and `null` values.
                    // It's reasonable to interpret these as `None`
                    // when we're deserializing to an optional value
                    // rust-side, such as `Option<T>`.
                    return visitor.visit_sum(NoneAccess::new());
                }

                return visitor.visit_sum(SomeAccess::new(self));
            } else {
                let tag_field = self.common.key_cache.tag(scope);
                let val = cast!(scope, self.input, Object, "object for sum type `{}`", sum_name)?;
                (val, tag_field)
            }
        };

        // Extract the `tag` field. It needs to contain a string.
        let tag = object
            .get(scope, tag_field.into())
            .ok_or_else(exception_already_thrown)?;
        let tag = cast!(scope, tag, v8::String, "string for sum tag of `{}`", sum_name)?;

        // Extract the `value` field.
        let value_field = self.common.key_cache.value(scope);
        let value = object
            .get(scope, value_field.into())
            .ok_or_else(exception_already_thrown)?;

        // Stitch it all together.
        visitor.visit_sum(SumAccess {
            common: self.common,
            tag,
            value,
        })
    }

    fn deserialize_str<V: SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let val = cast!(self.common.scope, self.input, v8::String, "`string`")?;
        let mut buf = scratch_buf::<64>();
        match val.to_rust_cow_lossy(self.common.scope, &mut buf) {
            Cow::Borrowed(s) => visitor.visit(s),
            Cow::Owned(string) => visitor.visit_owned(string),
        }
    }

    fn deserialize_bytes<V: SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let arr = cast!(self.common.scope, self.input, Uint8Array, "`Uint8Array` for bytes")?;
        let storage: &'static mut [u8] = &mut [0; v8::TYPED_ARRAY_MAX_SIZE_IN_HEAP];
        let bytes: &'s [u8] = deref_local(arr).get_contents(storage);
        visitor.visit_borrowed(bytes)
    }

    fn deserialize_array_seed<V: ArrayVisitor<'de, T::Output>, T: DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error> {
        let arr = cast!(self.common.scope, self.input, v8::Array, "`Array`")?;
        visitor.visit(ArrayAccess::new(arr, self.common, seed))
    }
}

/// Provides access to the field names and values in a JS object
/// under the assumption that it's a product.
struct ProductAccess<'a, 's> {
    common: DeserializerCommon<'a, 's>,
    /// The input object being deserialized.
    object: Local<'s, Object>,
    /// A field's value, to deserialize next in [`NamedProductAccess::get_field_value_seed`].
    next_value: Option<Local<'s, Value>>,
    /// The index in the product to
    index: usize,
}

/// Normalizes `field` into an interned `v8::String`.
pub(super) fn intern_field_name<'s>(scope: &mut HandleScope<'s>, field: Option<&str>, index: usize) -> Local<'s, Name> {
    let field = match field {
        Some(field) => Cow::Borrowed(field),
        None => Cow::Owned(format!("{index}")),
    };
    v8_interned_string(scope, field).into()
}

impl<'de, 's: 'de> de::NamedProductAccess<'de> for ProductAccess<'_, 's> {
    type Error = Error<'s>;

    fn get_field_ident<V: de::FieldNameVisitor<'de>>(&mut self, visitor: V) -> Result<Option<V::Output>, Self::Error> {
        let scope = &mut *self.common.scope;
        let mut field_names = visitor.field_names();
        while let Some(field) = field_names.nth(self.index) {
            // Get and advance the current index.
            let index = self.index;
            self.index += 1;

            // Normalize the field name.
            // Integer keys are converted to strings,
            // as that is supported on JS objects.
            let key = intern_field_name(scope, field, index);

            // Check that such a field/key exists.
            if !self
                .object
                .has_own_property(scope, key)
                .ok_or_else(exception_already_thrown)?
            {
                continue;
            }

            // Extract the next value, to be processed in `get_field_value_seed`.
            let val = self
                .object
                .get(scope, key.into())
                .ok_or_else(exception_already_thrown)?;
            self.next_value = Some(val);

            drop(field_names);
            return Ok(Some(visitor.visit_seq(index)));
        }

        Ok(None)
    }

    fn get_field_value_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Output, Self::Error> {
        let common = self.common.reborrow();
        // Extract the field's value.
        let input = self
            .next_value
            .take()
            .expect("Call next_key_seed before next_value_seed");
        // Deserialize the field's value.
        seed.deserialize(Deserializer { common, input })
    }
}

/// Used in `Deserializer::deserialize_sum` to translate a `tag` property of a JS object
/// to a variant and to provide a deserializer for its value/payload.
struct SumAccess<'a, 's> {
    common: DeserializerCommon<'a, 's>,
    /// The tag of the sum value.
    tag: Local<'s, v8::String>,
    /// The value of the sum value.
    value: Local<'s, Value>,
}

impl<'de, 'a, 's: 'de> de::SumAccess<'de> for SumAccess<'a, 's> {
    type Error = Error<'s>;
    type Variant = Deserializer<'a, 's>;

    fn variant<V: de::VariantVisitor<'de>>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        // Read the `tag` property in JS.
        // We generously provide 32 bytes for inline reading of the property
        // before resorting to the heap.
        let mut buf = scratch_buf::<32>();
        let name = self.tag.to_rust_cow_lossy(self.common.scope, &mut buf);

        // Select the variant to deserialize.
        let variant = visitor.visit_name::<Self::Error>(&name)?;

        // Prepare the deserialization of the payload.
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

/// Used by an `ArrayVisitor` to deserialize every element of a JS array
/// to a SATS array.
struct ArrayAccess<'a, 's, T> {
    common: DeserializerCommon<'a, 's>,
    arr: Local<'s, Array>,
    seeds: RepeatN<T>,
    index: u32,
}

impl<'de, 'a, 's, T> ArrayAccess<'a, 's, T>
where
    T: DeserializeSeed<'de> + Clone,
{
    fn new(arr: Local<'s, Array>, common: DeserializerCommon<'a, 's>, seed: T) -> Self {
        Self {
            arr,
            common,
            seeds: repeat_n(seed, arr.length() as usize),
            index: 0,
        }
    }
}

impl<'de, 's: 'de, T: DeserializeSeed<'de> + Clone> de::ArrayAccess<'de> for ArrayAccess<'_, 's, T> {
    type Element = T::Output;
    type Error = Error<'s>;

    fn next_element(&mut self) -> Result<Option<Self::Element>, Self::Error> {
        self.seeds
            .next()
            .map(|seed| {
                // Extract the array element.
                let val = self
                    .arr
                    .get_index(self.common.scope, self.index)
                    .ok_or_else(exception_already_thrown)?;

                // Deserialize the element.
                let val = seed.deserialize(Deserializer {
                    common: self.common.reborrow(),
                    input: val,
                })?;

                self.index += 1;
                Ok(val)
            })
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.seeds.len())
    }
}
