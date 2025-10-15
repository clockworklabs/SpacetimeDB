use super::de::intern_field_name;
use super::error::{
    exception_already_thrown, ArrayTooLongError, ExcResult, ExceptionThrown, StringTooLongError, Throwable, TypeError,
};
use super::string::{IntoJsString, TAG, VALUE};
use super::syscall::FnRet;
use super::to_value::ToValue;
use derive_more::From;
use spacetimedb_sats::{
    i256,
    ser::{self, Serialize},
    u256,
};
use v8::{Array, ArrayBuffer, IntegrityLevel, Local, Object, PinScope, Uint8Array, Value};

/// Serializes `value` into a V8 into `scope`.
pub(super) fn serialize_to_js<'scope>(scope: &PinScope<'scope, '_>, value: &impl Serialize) -> FnRet<'scope> {
    value.serialize(Serializer::new(scope)).map_err(|e| e.throw(scope))
}

/// Deserializes to V8 values.
#[derive(Copy, Clone)]
struct Serializer<'this, 'scope, 'isolate> {
    /// The scope to serialize values into.
    scope: &'this PinScope<'scope, 'isolate>,
}

impl<'this, 'scope, 'isolate> Serializer<'this, 'scope, 'isolate> {
    /// Creates a new serializer into `scope`.
    pub fn new(scope: &'this PinScope<'scope, 'isolate>) -> Self {
        Self { scope }
    }
}

/// The possible errors that [`Serializer`] can produce.
#[derive(Debug, From)]
enum Error {
    Custom(String),
    Thrown(ExceptionThrown),
    StringTooLong(StringTooLongError),
    ArrayLengthTooLong(ArrayTooLongError),
}

impl<'scope> Throwable<'scope> for Error {
    fn throw(self, scope: &PinScope<'scope, '_>) -> ExceptionThrown {
        match self {
            Self::StringTooLong(err) => err.into_range_error().throw(scope),
            Self::ArrayLengthTooLong(err) => err.into_range_error().throw(scope),
            Self::Thrown(thrown) => thrown,
            Self::Custom(msg) => TypeError(msg).throw(scope),
        }
    }
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
fn seal_object(scope: &PinScope<'_, '_>, object: &Object) -> ExcResult<()> {
    let _ = object
        .set_integrity_level(scope, IntegrityLevel::Sealed)
        .ok_or_else(exception_already_thrown)?;
    Ok(())
}

impl<'this, 'scope, 'isolate> ser::Serializer for Serializer<'this, 'scope, 'isolate> {
    type Ok = Local<'scope, Value>;
    type Error = Error;

    type SerializeArray = SerializeArray<'this, 'scope, 'isolate>;
    type SerializeSeqProduct = Self::SerializeNamedProduct;
    type SerializeNamedProduct = SerializeNamedProduct<'this, 'scope, 'isolate>;

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
        string
            .into_string(self.scope)
            .map(Into::into)
            .map_err(Error::StringTooLong)
    }

    fn serialize_bytes(self, bytes: &[u8]) -> Result<Self::Ok, Self::Error> {
        let store = ArrayBuffer::new_backing_store_from_boxed_slice(bytes.into()).make_shared();
        let buf = ArrayBuffer::with_backing_store(self.scope, &store);
        Ok(Uint8Array::new(self.scope, buf, 0, bytes.len()).unwrap().into())
    }

    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error> {
        let len = len.try_into().map_err(|_| ArrayTooLongError { len })?;
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
        // TODO(v8, noa): this can be more efficient if we tell it the names ahead of time
        let object = Object::new(self.scope);
        Ok(SerializeNamedProduct {
            inner: self,
            object,
            next_index: 0,
        })
    }

    fn serialize_variant<T: Serialize + ?Sized>(
        self,
        tag: u8,
        var_name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        // Serialize the payload.
        let value_value: Local<'scope, Value> = value.serialize(self)?;
        // Figure out the tag.
        let tag_value: Local<'scope, Value> = intern_field_name(self.scope, var_name, tag as usize).into();
        let values = [tag_value, value_value];

        // The property keys are always `"tag"` an `"value"`.
        let names = [TAG.string(self.scope).into(), VALUE.string(self.scope).into()];

        // Stitch together the object.
        let prototype = v8::null(self.scope).into();
        let object = Object::with_prototype_and_properties(self.scope, prototype, &names, &values);
        seal_object(self.scope, &object)?;
        Ok(object.into())
    }
}

/// Serializes array elements and finalizes the JS array.
struct SerializeArray<'this, 'scope, 'isolate> {
    inner: Serializer<'this, 'scope, 'isolate>,
    array: Local<'scope, Array>,
    next_index: u32,
}

impl<'scope> ser::SerializeArray for SerializeArray<'_, 'scope, '_> {
    type Ok = Local<'scope, Value>;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        // Serialize the current `elem`ent.
        let value = elem.serialize(self.inner)?;

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
struct SerializeNamedProduct<'this, 'scope, 'isolate> {
    inner: Serializer<'this, 'scope, 'isolate>,
    object: Local<'scope, Object>,
    next_index: usize,
}

impl<'scope> ser::SerializeSeqProduct for SerializeNamedProduct<'_, 'scope, '_> {
    type Ok = Local<'scope, Value>;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, elem: &T) -> Result<(), Self::Error> {
        ser::SerializeNamedProduct::serialize_element(self, None, elem)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeNamedProduct::end(self)
    }
}

impl<'scope> ser::SerializeNamedProduct for SerializeNamedProduct<'_, 'scope, '_> {
    type Ok = Local<'scope, Value>;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(
        &mut self,
        field_name: Option<&str>,
        elem: &T,
    ) -> Result<(), Self::Error> {
        // Serialize the field value.
        let value = elem.serialize(self.inner)?;

        // Figure out the object property to use.
        let scope = self.inner.scope;
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
    use crate::host::v8::de::deserialize_js_seed;

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
            // Convert to JS...
            let js_val = serialize_to_js(scope, &rust_val).unwrap();

            // ...and then back to Rust.
            let rust_val_prime = deserialize_js_seed(scope, js_val, seed).unwrap();

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
