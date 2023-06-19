use super::AlgebraicType;
use crate::builtin_type::BuiltinType;
use crate::{ArrayType, MapType};
use std::fmt::Display;

pub struct Formatter<'a> {
    ty: &'a AlgebraicType,
}

impl<'a> Formatter<'a> {
    pub fn new(ty: &'a AlgebraicType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for Formatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.ty {
            AlgebraicType::Sum(ty) => {
                write!(f, "{{ ty_: Sum",)?;
                for (i, e_ty) in ty.variants.iter().enumerate() {
                    write!(f, ", ")?;
                    if let Some(name) = &e_ty.name {
                        write!(f, "{}: {}", name, Formatter::new(&e_ty.algebraic_type))?;
                    } else {
                        write!(f, "{}: {}", i, Formatter::new(&e_ty.algebraic_type))?;
                    }
                }
                write!(f, " }}",)
            }
            AlgebraicType::Product(ty) => {
                write!(f, "{{ ty_: Product",)?;
                for (i, e_ty) in ty.elements.iter().enumerate() {
                    write!(f, ", ")?;
                    if let Some(name) = &e_ty.name {
                        write!(f, "{}: {}", name, Formatter::new(&e_ty.algebraic_type))?;
                    } else {
                        write!(f, "{}: {}", i, Formatter::new(&e_ty.algebraic_type))?;
                    }
                }
                write!(f, " }}",)
            }
            AlgebraicType::Builtin(ty) => {
                write!(f, "{{ ty_: Builtin")?;
                match &ty {
                    BuiltinType::Bool => write!(f, ", 0: Bool")?,
                    BuiltinType::I8 => write!(f, ", 0: I8")?,
                    BuiltinType::U8 => write!(f, ", 0: U8")?,
                    BuiltinType::I16 => write!(f, ", 0: I16")?,
                    BuiltinType::U16 => write!(f, ", 0: U16")?,
                    BuiltinType::I32 => write!(f, ", 0: I32")?,
                    BuiltinType::U32 => write!(f, ", 0: U32")?,
                    BuiltinType::I64 => write!(f, ", 0: I64")?,
                    BuiltinType::U64 => write!(f, ", 0: U64")?,
                    BuiltinType::I128 => write!(f, ", 0: I128")?,
                    BuiltinType::U128 => write!(f, ", 0: U128")?,
                    BuiltinType::F32 => write!(f, ", 0: F32")?,
                    BuiltinType::F64 => write!(f, ", 0: F64")?,
                    BuiltinType::String => write!(f, ", 0: String")?,
                    BuiltinType::Array(ArrayType { elem_ty }) => {
                        write!(f, ", 0: Array, 1: {}", Formatter::new(elem_ty))?
                    }
                    BuiltinType::Map(MapType { key_ty, ty }) => {
                        write!(f, "0: Map, 1: {}, 2: {}", Formatter::new(key_ty), Formatter::new(ty))?
                    }
                }
                write!(f, " }}",)
            }
            AlgebraicType::Ref(r) => {
                write!(f, "{{ ty_: Ref, 0: ")?;
                write!(f, "{}", r.0)?;
                write!(f, " }}",)
            }
        }
    }
}
