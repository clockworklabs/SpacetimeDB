use super::BuiltinValue;
use crate::{algebraic_value, builtin_type::BuiltinType, typespace::Typespace};
use std::fmt::Display;

pub struct Formatter<'a> {
    typespace: &'a Typespace,
    ty: &'a BuiltinType,
    val: &'a BuiltinValue,
}

impl<'a> Formatter<'a> {
    pub fn new(typespace: &'a Typespace, ty: &'a BuiltinType, val: &'a BuiltinValue) -> Self {
        Self { typespace, ty, val }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.val {
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
                for (i, e) in val.iter().enumerate() {
                    write!(
                        f,
                        "{}",
                        algebraic_value::satn::Formatter::new(self.typespace, self.ty.as_array().unwrap(), e)
                    )?;
                    if i != val.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")
            }
            BuiltinValue::Map { val } => {
                if val.len() == 0 {
                    return write!(f, "[:]");
                }
                let (key_ty, ty) = self.ty.as_map().unwrap();
                write!(f, "[")?;
                for (i, (key, e)) in val.iter().enumerate() {
                    write!(
                        f,
                        "{}: {}",
                        algebraic_value::satn::Formatter::new(self.typespace, key_ty, key),
                        algebraic_value::satn::Formatter::new(self.typespace, ty, e)
                    )?;
                    if i != val.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")
            }
        }
    }
}
