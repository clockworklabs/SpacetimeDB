use super::error::{exception_already_thrown, ExcResult, ExceptionThrown, ExceptionValue, Throwable, TypeError};
use super::from_value::{cast, FromValue};
use super::string::{TAG, VALUE};
use super::FnRet;
use core::fmt;
use core::iter::{repeat_n, RepeatN};
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use derive_more::From;
use spacetimedb_sats::de::{self, ArrayVisitor, DeserializeSeed, ProductVisitor, SliceVisitor, SumVisitor};
use spacetimedb_sats::{i256, u256};
use std::borrow::{Borrow, Cow};
use v8::{Array, Local, Name, Object, PinScope, Uint8Array, Value};

/// Deserializes a `T` from `val` in `scope`, using `seed` for any context needed.
pub(super) fn deserialize_js_seed<'de, T: DeserializeSeed<'de>>(
    scope: &mut PinScope<'de, '_>,
    val: Local<'_, Value>,
    seed: T,
) -> ExcResult<T::Output> {
    let de = Deserializer::new(scope, val);
    seed.deserialize(de).map_err(|e| e.throw(scope))
}

/// Deserializes a `T` from `val` in `scope`.
pub(super) fn deserialize_js<'de, T: de::Deserialize<'de>>(
    scope: &mut PinScope<'de, '_>,
    val: Local<'_, Value>,
) -> ExcResult<T> {
    deserialize_js_seed(scope, val, PhantomData)
}

/// Deserializes from V8 values.
struct Deserializer<'this, 'scope, 'isolate> {
    common: DeserializerCommon<'this, 'scope, 'isolate>,
    input: Local<'scope, Value>,
}

impl<'this, 'scope, 'isolate> Deserializer<'this, 'scope, 'isolate> {
    /// Creates a new deserializer from `input` in `scope`.
    fn new(scope: &'this mut PinScope<'scope, 'isolate>, input: Local<'_, Value>) -> Self {
        let input = Local::new(scope, input);
        let common = DeserializerCommon { scope };
        Deserializer { input, common }
    }
}

/// Things shared between various [`Deserializer`]s.
///
/// The lifetime `'scope` is that of the scope of values deserialized.
struct DeserializerCommon<'this, 'scope, 'isolate> {
    /// The scope of values to deserialize.
    scope: &'this mut PinScope<'scope, 'isolate>,
}

impl<'scope, 'isolate> DeserializerCommon<'_, 'scope, 'isolate> {
    fn reborrow(&mut self) -> DeserializerCommon<'_, 'scope, 'isolate> {
        DeserializerCommon { scope: self.scope }
    }
}

/// The possible errors that [`Deserializer`] can produce.
#[derive(Debug, From)]
enum Error<'scope> {
    Unthrown(ExceptionValue<'scope>),
    Thrown(ExceptionThrown),
    Custom(String),
}

impl<'scope> Throwable<'scope> for Error<'scope> {
    fn throw(self, scope: &PinScope<'scope, '_>) -> ExceptionThrown {
        match self {
            Self::Unthrown(exception) => exception.throw(scope),
            Self::Thrown(thrown) => thrown,
            Self::Custom(msg) => TypeError(msg).throw(scope),
        }
    }
}

impl de::Error for Error<'_> {
    fn custom(msg: impl fmt::Display) -> Self {
        Self::Custom(msg.to_string())
    }
}

/// Returns a scratch buffer to fill when deserializing strings.
pub(crate) fn scratch_buf<const N: usize>() -> [MaybeUninit<u8>; N] {
    [const { MaybeUninit::uninit() }; N]
}

/// Extracts a reference `&'scope T` from an owned V8 [`Local<'scope, T>`].
///
/// The lifetime `'scope` is that of the [`HandleScope<'scope>`].
/// This ensures that the reference to `T` won't outlive the `HandleScope`.
fn deref_local<'scope, T>(local: Local<'scope, T>) -> &'scope T {
    let reference = local.borrow();
    // SAFETY: Lifetime extend `'0` to `'scope`.
    // This is safe as the returned reference `&'scope T`
    // will not outlive its `HandleScope<'scope, _>`,
    // as both are tied to the lifetime `'scope`.
    unsafe { core::mem::transmute::<&T, &'scope T>(reference) }
}

/// Deserializes a primitive via [`FromValue`].
macro_rules! deserialize_primitive {
    ($dmethod:ident, $t:ty) => {
        fn $dmethod(self) -> Result<$t, Self::Error> {
            FromValue::from_value(self.input, self.common.scope).map_err(Error::Unthrown)
        }
    };
}

impl<'de, 'this, 'scope: 'de> de::Deserializer<'de> for Deserializer<'this, 'scope, '_> {
    type Error = Error<'scope>;

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
        // In `ProductType.serializeValue()` in the TS SDK, null/undefined is accepted for the unit type.
        if visitor.product_len() == 0 && self.input.is_null_or_undefined() {
            return visitor.visit_seq_product(de::UnitAccess::new());
        }

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
        let scope = &*self.common.scope;

        // In `SumType.serializeValue()` in the TS SDK, option is treated specially -
        // null/undefined marks none, any other value `x` is `some(x)`.
        if visitor.is_option() {
            return if self.input.is_null_or_undefined() {
                visitor.visit_sum(de::NoneAccess::new())
            } else {
                visitor.visit_sum(de::SomeAccess::new(self))
            };
        }

        let sum_name = visitor.sum_name().unwrap_or("<unknown>");

        // We expect a canonical representation of a sum value in JS to be
        // `{ tag: "foo", value: a_value_for_foo }`.
        let tag_field = TAG.string(scope);
        let object = cast!(scope, self.input, Object, "object for sum type `{}`", sum_name)?;

        // Extract the `tag` field. It needs to contain a string.
        let tag = property(scope, object, tag_field)?;
        let tag = cast!(scope, tag, v8::String, "string for sum tag of `{}`", sum_name)?;

        // Extract the `value` field.
        let value_field = VALUE.string(scope);
        let value = property(scope, object, value_field)?;

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
        match val.to_rust_cow_lossy(&mut *self.common.scope, &mut buf) {
            Cow::Borrowed(s) => visitor.visit(s),
            Cow::Owned(string) => visitor.visit_owned(string),
        }
    }

    fn deserialize_bytes<V: SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        let arr = cast!(self.common.scope, self.input, Uint8Array, "`Uint8Array` for bytes")?;
        let storage: &'static mut [u8] = &mut [0; v8::TYPED_ARRAY_MAX_SIZE_IN_HEAP];
        let bytes: &'scope [u8] = deref_local(arr).get_contents(storage);
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
struct ProductAccess<'this, 'scope, 'isolate> {
    common: DeserializerCommon<'this, 'scope, 'isolate>,
    /// The input object being deserialized.
    object: Local<'scope, Object>,
    /// A field's value, to deserialize next in [`NamedProductAccess::get_field_value_seed`].
    next_value: Option<Local<'scope, Value>>,
    /// The index in the product to
    index: usize,
}

// Creates an interned [`v8::String`].
pub(super) fn v8_interned_string<'scope>(scope: &PinScope<'scope, '_>, field: &str) -> Local<'scope, v8::String> {
    // Internalized v8 strings are significantly faster than "normal" v8 strings
    // since v8 deduplicates re-used strings minimizing new allocations
    // see: https://github.com/v8/v8/blob/14ac92e02cc3db38131a57e75e2392529f405f2f/include/v8.h#L3165-L3171
    v8::String::new_from_utf8(scope, field.as_ref(), v8::NewStringType::Internalized).unwrap()
}

/// Normalizes `field` into an interned `v8::String`.
pub(super) fn intern_field_name<'scope>(
    scope: &PinScope<'scope, '_>,
    field: Option<&str>,
    index: usize,
) -> Local<'scope, Name> {
    let field = match field {
        Some(field) => Cow::Borrowed(field),
        None => Cow::Owned(format!("{index}")),
    };
    v8_interned_string(scope, &field).into()
}

/// Returns the property for `key` on `object`.
pub(super) fn property<'scope>(
    scope: &PinScope<'scope, '_>,
    object: Local<'_, Object>,
    key: impl Into<Local<'scope, Value>>,
) -> FnRet<'scope> {
    object.get(scope, key.into()).ok_or_else(exception_already_thrown)
}

impl<'de, 'scope: 'de> de::NamedProductAccess<'de> for ProductAccess<'_, 'scope, '_> {
    type Error = Error<'scope>;

    fn get_field_ident<V: de::FieldNameVisitor<'de>>(&mut self, visitor: V) -> Result<Option<V::Output>, Self::Error> {
        let scope = &*self.common.scope;
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
            let val = property(scope, self.object, key)?;
            self.next_value = Some(val);

            drop(field_names);
            return Ok(Some(visitor.visit_seq(index)));
        }

        Ok(None)
    }

    fn get_field_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Output, Self::Error> {
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
struct SumAccess<'this, 'scope, 'isolate> {
    common: DeserializerCommon<'this, 'scope, 'isolate>,
    /// The tag of the sum value.
    tag: Local<'scope, v8::String>,
    /// The value of the sum value.
    value: Local<'scope, Value>,
}

impl<'de, 'this, 'scope: 'de, 'isolate> de::SumAccess<'de> for SumAccess<'this, 'scope, 'isolate> {
    type Error = Error<'scope>;
    type Variant = Deserializer<'this, 'scope, 'isolate>;

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

impl<'de, 'this, 'scope: 'de> de::VariantAccess<'de> for Deserializer<'this, 'scope, '_> {
    type Error = Error<'scope>;

    fn deserialize_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        seed.deserialize(self)
    }
}

/// Used by an `ArrayVisitor` to deserialize every element of a JS array
/// to a SATS array.
struct ArrayAccess<'this, 'scope, 'isolate, T> {
    common: DeserializerCommon<'this, 'scope, 'isolate>,
    arr: Local<'scope, Array>,
    seeds: RepeatN<T>,
    index: u32,
}

impl<'de, 'this, 'scope, 'isolate, T> ArrayAccess<'this, 'scope, 'isolate, T>
where
    T: DeserializeSeed<'de> + Clone,
{
    fn new(arr: Local<'scope, Array>, common: DeserializerCommon<'this, 'scope, 'isolate>, seed: T) -> Self {
        Self {
            arr,
            common,
            seeds: repeat_n(seed, arr.length() as usize),
            index: 0,
        }
    }
}

impl<'de, 'scope: 'de, T: DeserializeSeed<'de> + Clone> de::ArrayAccess<'de> for ArrayAccess<'_, 'scope, '_, T> {
    type Element = T::Output;
    type Error = Error<'scope>;

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
