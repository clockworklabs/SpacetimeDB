use spacetimedb_sats::{i256, ser, u256};

use super::convert::ToValue;
use super::de::{v8_struct_key, Error, KeyCache};
use super::ExceptionOptionExt;

pub(super) struct Serializer<'a, 's> {
    scope: &'a mut v8::HandleScope<'s>,
    key_cache: &'a mut KeyCache,
}

impl<'a, 's> Serializer<'a, 's> {
    fn reborrow(&mut self) -> Serializer<'_, 's> {
        Serializer {
            scope: self.scope,
            key_cache: self.key_cache,
        }
    }
}

impl ser::Error for Error<'_> {
    fn custom<T: core::fmt::Display>(msg: T) -> Self {
        Self::String(msg.to_string())
    }
}

macro_rules! serialize_primitive {
    ($smethod:ident, $t:ty) => {
        fn $smethod(self, v: $t) -> Result<Self::Ok, Self::Error> {
            Ok(ToValue::to_value(&v, self.scope)?)
        }
    };
}

impl<'a, 's> ser::Serializer for Serializer<'a, 's> {
    type Ok = v8::Local<'s, v8::Value>;
    type Error = Error<'s>;

    type SerializeArray = SerializeArray<'a, 's>;
    type SerializeSeqProduct = ser::Impossible<Self::Ok, Self::Error>;
    type SerializeNamedProduct = SerializeNamedProduct<'a, 's>;

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

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        v8::String::new(self.scope, v)
            .map(Into::into)
            .ok_or_else(|| ser::Error::custom("string too large"))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        let store = v8::ArrayBuffer::new_backing_store_from_boxed_slice(v.into()).make_shared();
        let buf = v8::ArrayBuffer::with_backing_store(self.scope, &store);
        Ok(v8::Uint8Array::new(self.scope, buf, 0, v.len()).unwrap().into())
    }

    fn serialize_array(self, len: usize) -> Result<Self::SerializeArray, Self::Error> {
        Ok(SerializeArray {
            arr: v8::Array::new(self.scope, len as _),
            inner: self,
            next: 0,
        })
    }

    fn serialize_seq_product(self, _len: usize) -> Result<Self::SerializeSeqProduct, Self::Error> {
        Err(ser::Error::custom("Can't serialize seqproduct for JS"))
    }

    fn serialize_named_product(self, _len: usize) -> Result<Self::SerializeNamedProduct, Self::Error> {
        // TODO: this can be more efficient if we tell it the names ahead of time
        let obj = v8::Object::new(self.scope);
        Ok(SerializeNamedProduct { inner: self, obj })
    }

    fn serialize_variant<T: ser::Serialize + ?Sized>(
        mut self,
        _tag: u8,
        name: Option<&str>,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        let names = [
            self.key_cache.tag(self.scope).into(),
            self.key_cache.value(self.scope).into(),
        ];

        let values = [
            v8_struct_key(self.scope, name.unwrap()).into(),
            value.serialize(self.reborrow())?,
        ];

        let null = v8::null(self.scope);
        Ok(v8::Object::with_prototype_and_properties(self.scope, null.into(), &names, &values).into())
    }
}

pub(super) struct SerializeArray<'a, 's> {
    inner: Serializer<'a, 's>,
    arr: v8::Local<'s, v8::Array>,
    next: u32,
}

impl<'a, 's> ser::SerializeArray for SerializeArray<'a, 's> {
    type Ok = v8::Local<'s, v8::Value>;
    type Error = Error<'s>;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, element: &T) -> Result<(), Self::Error> {
        let i = self.next;
        let value = element.serialize(self.inner.reborrow())?;
        self.arr.set_index(self.inner.scope, i, value).err()?;
        self.next += 1;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.arr.into())
    }
}

pub(super) struct SerializeNamedProduct<'a, 's> {
    inner: Serializer<'a, 's>,
    obj: v8::Local<'s, v8::Object>,
}

impl<'a, 's> ser::SerializeNamedProduct for SerializeNamedProduct<'a, 's> {
    type Ok = v8::Local<'s, v8::Value>;
    type Error = Error<'s>;

    fn serialize_element<T: ser::Serialize + ?Sized>(
        &mut self,
        name: Option<&str>,
        elem: &T,
    ) -> Result<(), Self::Error> {
        let key = v8_struct_key(self.inner.scope, name.unwrap());
        let value = elem.serialize(self.inner.reborrow())?;
        self.obj.set(self.inner.scope, key.into(), value).err()?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.obj
            .set_integrity_level(self.inner.scope, v8::IntegrityLevel::Sealed)
            .err()?;
        Ok(self.obj.into())
    }
}
