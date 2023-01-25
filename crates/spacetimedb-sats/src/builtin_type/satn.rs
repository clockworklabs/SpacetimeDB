use super::BuiltinType;
use crate::algebraic_type;
use std::fmt::Display;

pub struct Formatter<'a> {
    ty: &'a BuiltinType,
}

impl<'a> Formatter<'a> {
    pub fn new(ty: &'a BuiltinType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            BuiltinType::Bool => write!(f, "Bool"),
            BuiltinType::I8 => write!(f, "I8"),
            BuiltinType::U8 => write!(f, "U8"),
            BuiltinType::I16 => write!(f, "I16"),
            BuiltinType::U16 => write!(f, "U16"),
            BuiltinType::I32 => write!(f, "I32"),
            BuiltinType::U32 => write!(f, "U32"),
            BuiltinType::I64 => write!(f, "I64"),
            BuiltinType::U64 => write!(f, "U64"),
            BuiltinType::I128 => write!(f, "I128"),
            BuiltinType::U128 => write!(f, "U128"),
            BuiltinType::F32 => write!(f, "F32"),
            BuiltinType::F64 => write!(f, "F64"),
            BuiltinType::String => write!(f, "String"),
            BuiltinType::Array { ty } => write!(f, "Array<{}>", algebraic_type::satn::Formatter::new(ty)),
            BuiltinType::Map { key_ty, ty } => write!(
                f,
                "Map<{}, {}>",
                algebraic_type::satn::Formatter::new(key_ty),
                algebraic_type::satn::Formatter::new(ty)
            ),
        }
    }
}
