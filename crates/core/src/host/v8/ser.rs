#![allow(dead_code)]

use super::de::{intern_field_name, KeyCache};
use super::error::{exception_already_thrown, ExceptionThrown};
use super::to_value::ToValue;
use core::num::TryFromIntError;
use derive_more::From;
use spacetimedb_sats::{
    i256,
    ser::{self, Serialize},
    u256,
};
use v8::{Array, ArrayBuffer, HandleScope, IntegrityLevel, Local, Object, Uint8Array, Value};

/// Deserializes to V8 values.
pub(super) struct Serializer<'a, 's> {
    /// The scope to serialize values into.
    scope: &'a mut HandleScope<'s>,
    /// A cache for frequently used strings.
    key_cache: &'a mut KeyCache,
}

impl<'a, 's> Serializer<'a, 's> {
    /// Creates a new serializer into `scope`.
    pub fn new(scope: &'a mut HandleScope<'s>, key_cache: &'a mut KeyCache) -> Self {
        Self { scope, key_cache }
    }

    fn reborrow(&mut self) -> Serializer<'_, 's> {
        Serializer {
            scope: self.scope,
            key_cache: self.key_cache,
        }
    }
}

/// The possible errors that [`Serializer`] can produce.
#[derive(Debug, From)]
pub(super) enum Error {
    Custom(String),
    Exception(ExceptionThrown),
    StringTooLarge(usize),
    ArrayLengthTooLarge(TryFromIntError),
}

impl ser::Error for Error {
    fn custom<T: core::fmt::Display>(msg: T) -> Self {
        Self::Custom(msg.to_string())
    }
}

/// Serializes a primitive via [`ToValue`].
macro_rules! serialize_primitive {
    ($method:ident, $ty:ty) => {
        fn $method(self, val: $ty) -> Result<Self::Ok, Self::Error> {
            Ok(ToValue::to_value(&val, self.scope))
        }
    };
}

/// Seal the object, so that e.g., new properties cannot be added.
///
/// However, the values of existing properties may be modified,
/// which can be useful if the module wants to modify a property
/// and then send the object back.
fn seal_object(scope: &mut HandleScope, object: &Object) -> Result<(), ExceptionThrown> {
    let _ = object
        .set_integrity_level(scope, IntegrityLevel::Sealed)
        .ok_or_else(exception_already_thrown)?;
    Ok(())
}

impl<'a, 's> ser::Serializer for Serializer<'a, 's> {
    type Ok = Local<'s, Value>;
    type Error = Error;

    type SerializeArray = SerializeArray<'a, 's>;
    type SerializeSeqProduct = Self::SerializeNamedProduct;
    type SerializeNamedProduct = SerializeNamedProduct<'a, 's>;

    // Serialization of primitive types defers to `ToValue`.
    serialize_primitive!(serialize_bool, bool);
    serialize_primitive!(serialize_u8, u8);
    serialize_primitive!(serialize_u16, u16);
    serialize_primitive!(serialize_u32, u32);
    serialize_primitive!(serialize_u64, u64);
    serialize_primitive!(serialize_u128, u128);
    serialize_primitive!(serialize_u256, u256);
    serialize_primitive!(serialize_i8, i8);
    serialize_primitive!(serialize_i16, i16);
    serialize_primitive!(serialize_i32, i32);
    serialize_primitive!(serialize_i64, i64);
    serialize_primitive!(serialize_i128, i128);
    serialize_primitive!(serialize_i256, i256);
    serialize_primitive!(serialize_f64, f64);
    serialize_primitive!(serialize_f32, f32);

    fn serialize_str(self, string: &str) -> Result<Self::Ok, Self::Error> {
        v8::String::new(self.scope, string)
            .map(Into::into)
            .ok_or(Error::StringTooLarge(string.len()))
    }

    fn serialize_bytes(self, bytes: &[u8]) -> Result<Self::Ok, Self::Error> {
        let store = ArrayBuffer::new_backing_store_from_boxed_slice(bytes.into()).make_shared();
        let buf = ArrayBuffer::with_backing_store(self.scope, &store);
        Ok(Uint8Array::new(self.scope, buf, 0, bytes.len()).unwrap().into())
    }

    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error> {
        let len = len.try_into()?;
        Ok(SerializeArray {
            array: Array::new(self.scope, len),
            inner: self,
            next_index: 0,
        })
    }

    fn serialize_seq_product(self, len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        self.serialize_named_product(len)
    }

    fn serialize_named_product(self, _len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        // TODO(noa): this can be more efficient if we tell it the names ahead of time
        let object = Object::new(self.scope);
        Ok(SerializeNamedProduct {
            inner: self,
            object,
            next_index: 0,
        })
    }

    fn serialize_variant<T: Serialize + ?Sized>(
        mut self,
        tag: u8,
        var_name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        // Serialize the payload.
        let value_value: Local<'s, Value> = value.serialize(self.reborrow())?;
        // Figure out the tag.
        let tag_value: Local<'s, Value> = intern_field_name(self.scope, var_name, tag as usize).into();
        let values = [tag_value, value_value];

        // The property keys are always `"tag"` an `"value"`.
        let names = [
            self.key_cache.tag(self.scope).into(),
            self.key_cache.value(self.scope).into(),
        ];

        // Stitch together the object.
        let prototype = v8::null(self.scope).into();
        let object = Object::with_prototype_and_properties(self.scope, prototype, &names, &values);
        seal_object(self.scope, &object)?;
        Ok(object.into())
    }
}

/// Serializes array elements and finalizes the JS array.
pub(super) struct SerializeArray<'a, 's> {
    inner: Serializer<'a, 's>,
    array: Local<'s, Array>,
    next_index: u32,
}

impl<'s> ser::SerializeArray for SerializeArray<'_, 's> {
    type Ok = Local<'s, Value>;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        // Serialize the current `elem`ent.
        let value = elem.serialize(self.inner.reborrow())?;

        // Set the value to the array slot at `index`.
        let index = self.next_index;
        self.next_index += 1;
        self.array
            .set_index(self.inner.scope, index, value)
            .ok_or_else(exception_already_thrown)?;

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.array.into())
    }
}

/// Serializes into JS objects where field names are turned into property names.
pub(super) struct SerializeNamedProduct<'a, 's> {
    inner: Serializer<'a, 's>,
    object: Local<'s, Object>,
    next_index: usize,
}

impl<'s> ser::SerializeSeqProduct for SerializeNamedProduct<'_, 's> {
    type Ok = Local<'s, Value>;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        ser::SerializeNamedProduct::serialize_element(self, None, elem)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeNamedProduct::end(self)
    }
}

impl<'s> ser::SerializeNamedProduct for SerializeNamedProduct<'_, 's> {
    type Ok = Local<'s, Value>;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(
        &mut self,
        field_name: Option<&str>,
        elem: &T,
    ) -> Result<(), Self::Error> {
        // Serialize the field value.
        let value = elem.serialize(self.inner.reborrow())?;

        // Figure out the object property to use.
        let scope = &mut *self.inner.scope;
        let index = self.next_index;
        self.next_index += 1;
        let key = intern_field_name(scope, field_name, index).into();

        // Set the value to the property.
        self.object
            .set(scope, key, value)
            .ok_or_else(exception_already_thrown)?;

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        seal_object(self.inner.scope, &self.object)?;
        Ok(self.object.into())
    }
}

#[cfg(test)]
mod test {
    use super::super::de::Deserializer;
    use super::super::to_value::test::with_scope;
    use super::*;
    use core::fmt::Debug;
    use proptest::prelude::*;
    use spacetimedb_lib::{AlgebraicType, AlgebraicValue};
    use spacetimedb_sats::de::DeserializeSeed;
    use spacetimedb_sats::proptest::generate_typed_value;
    use spacetimedb_sats::{product, SumValue, ValueWithType, WithTypespace};
    use AlgebraicType::Bool;

    /// Roundtrips `rust_val` via [`Serialize`] to the V8 representation
    /// and then back via [`DeserializeSeed`],
    /// asserting that it's the same as the passed value.
    fn assert_roundtrips<B: Debug>(
        rust_val: impl Serialize + PartialEq<B> + Debug,
        seed: impl for<'de> DeserializeSeed<'de, Output = B>,
    ) {
        with_scope(|scope| {
            let key_cache = &mut KeyCache::default();

            // Convert to JS...
            let ser = Serializer::new(scope, key_cache);
            let js_val = rust_val.serialize(ser).unwrap();

            // ...and then back to Rust.
            let de = Deserializer::new(scope, js_val, key_cache);
            let rust_val_prime = seed.deserialize(de).unwrap();

            // We should end up where we started.
            assert_eq!(rust_val, rust_val_prime);
        })
    }

    fn assert_roundtrips_with_ty(ty: AlgebraicType, val: AlgebraicValue) {
        let ctx = WithTypespace::empty(&ty);
        let value = ValueWithType::new(ctx, &val);
        let seed = value.ty_s();
        assert_roundtrips(value, seed);
    }

    proptest! {
        #[test]
        fn test_random_typed_value_roundtrips((ty, val) in generate_typed_value()) {
            assert_roundtrips_with_ty(ty, val);
        }
    }

    #[test]
    fn anonymized_product_works() {
        let ty = AlgebraicType::product([Bool]);
        let val = product![false].into();
        assert_roundtrips_with_ty(ty, val);
    }

    /// This test demonstrates that serialization misbehaves without using [`ValueWithType`].
    #[test]
    fn regression_test_product_serialization_needs_value_with_type() {
        let ty = AlgebraicType::product([("field_0", Bool)]);
        let val = product![false].into();
        assert_roundtrips_with_ty(ty, val);
    }

    #[test]
    fn regression_test_variant() {
        let ty = AlgebraicType::sum([("variant_0", Bool)]);
        let val = SumValue::new(0, false).into();
        assert_roundtrips_with_ty(ty, val);
    }
}
