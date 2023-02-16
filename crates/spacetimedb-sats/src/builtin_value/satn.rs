use crate::satn::EntryWrapper;
use crate::{BuiltinValue, MapType, ValueWithType};
use std::fmt;

impl<'a> crate::satn::Satn for ValueWithType<'a, BuiltinValue> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value() {
            BuiltinValue::Bool(val) => write!(f, "{}", val),
            BuiltinValue::I8(val) => write!(f, "{}", val),
            BuiltinValue::U8(val) => write!(f, "{}", val),
            BuiltinValue::I16(val) => write!(f, "{}", val),
            BuiltinValue::U16(val) => write!(f, "{}", val),
            BuiltinValue::I32(val) => write!(f, "{}", val),
            BuiltinValue::U32(val) => write!(f, "{}", val),
            BuiltinValue::I64(val) => write!(f, "{}", val),
            BuiltinValue::U64(val) => write!(f, "{}", val),
            BuiltinValue::I128(val) => write!(f, "{}", val),
            BuiltinValue::U128(val) => write!(f, " {}", val),
            BuiltinValue::F32(val) => write!(f, "{}", val),
            BuiltinValue::F64(val) => write!(f, "{}", val),
            BuiltinValue::String(val) => write!(f, "\"{}\"", val),
            BuiltinValue::Bytes(val) => write!(f, "{:?}", val),
            BuiltinValue::Array { val } => {
                write!(f, "[")?;
                EntryWrapper::<','>::new(f).entries(
                    val.iter()
                        .map(|e| |f: &mut fmt::Formatter| self.with(&**self.ty().as_array().unwrap(), e).fmt(f)),
                )?;
                write!(f, "]")
            }
            BuiltinValue::Map { val } => {
                if val.len() == 0 {
                    return write!(f, "[:]");
                }
                let MapType { key_ty, ty } = self.ty().as_map().unwrap();
                write!(f, "[")?;
                EntryWrapper::<','>::new(f).entries(val.iter().map(|(key, e)| {
                    |f: &mut fmt::Formatter| {
                        self.with(&**key_ty, key).fmt(f)?;
                        f.write_str(": ")?;
                        self.with(&**ty, e).fmt(f)
                    }
                }))?;
                write!(f, "]")
            }
        }
    }
}
