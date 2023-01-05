use std::fmt::Display;

use crate::{
    builtin_type::{self, BuiltinType},
    product_type::{self, ProductType},
    sum_type::{self, SumType},
};
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};

pub const TAG_PRODUCT: u8 = 0x0;
pub const TAG_SUM: u8 = 0x1;
pub const TAG_BOOL: u8 = 0x02;
pub const TAG_I8: u8 = 0x03;
pub const TAG_U8: u8 = 0x04;
pub const TAG_I16: u8 = 0x05;
pub const TAG_U16: u8 = 0x06;
pub const TAG_I32: u8 = 0x07;
pub const TAG_U32: u8 = 0x08;
pub const TAG_I64: u8 = 0x09;
pub const TAG_U64: u8 = 0x0a;
pub const TAG_I128: u8 = 0x0b;
pub const TAG_U128: u8 = 0x0c;
pub const TAG_F32: u8 = 0x0d;
pub const TAG_F64: u8 = 0x0e;
pub const TAG_STRING: u8 = 0x0f;
pub const TAG_ARRAY: u8 = 0x10;

/// The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
/// which a nominal type system can be constructed.
///
/// The type system unifies the concepts sum types, product types, sets, and relations
/// into a single type system.
///
/// Below are some common types implemented in this type system.
///
/// Unit = (,) or () or , // Product with zero elements
/// Never = (|) or | // Sum with zero elements
/// U8 = U8 // Builtin
/// Foo = (foo: I8) != I8
/// Bar = (bar: I8)
/// Color = ((a: I8) | (b: I8)) // Sum with one element
/// Age = (age: U8) // Product with one element
/// Option<T> = (some: (|) | none: ())
/// SetType<T> = {T}
/// Tag = ??
/// ElementType = (tag: Tag, type: AlgebraicType)
/// ProductType = {ElementType}
/// AlgebraicType = (sum: {AlgebraicType} | product: ProductType | builtin: BuiltinType | set: AlgebraicType)
/// Catalog<T> = (name: String, indices: Set<Set<Tag>>, relation: Set<>)
/// define CatalogEntry = { name: string, indexes: {some type}, relation: Relation }
/// type ElementValue = (tag: Tag, value: AlgebraicValue)
/// type AlgebraicValue = (sum: ElementValue | product: {ElementValue} | builtin: BuiltinValue | set: {AlgebraicValue})
/// type Any = (value: Bytes, type: AlgebraicType)
///
/// type Table<Row: ProductType> = (
///     rows: Array<Row>
/// )
///
/// type HashSet<T> = (
///     array: Array<T>
/// )
///
/// type BTreeSet<T> = (
///     array: Array<T>
/// )
///
/// type TableType<Row: ProductType> = (
///     relation: Table<Row>,
///     indexes: Array<(index_type: String)>,
/// )
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum AlgebraicType {
    Sum(SumType),
    Product(ProductType),
    Builtin(BuiltinType),
}

pub struct SATNFormatter<'a> {
    ty: &'a AlgebraicType,
}

impl<'a> SATNFormatter<'a> {
    pub fn new(ty: &'a AlgebraicType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for SATNFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            AlgebraicType::Sum(ty) => {
                write!(f, "{}", sum_type::SATNFormatter::new(ty))
            }
            AlgebraicType::Product(ty) => {
                write!(f, "{}", product_type::SATNFormatter::new(ty))
            }
            AlgebraicType::Builtin(p) => {
                write!(f, "{}", builtin_type::SATNFormatter::new(p))
            }
        }
    }
}

pub struct MapFormatter<'a> {
    ty: &'a AlgebraicType,
}

impl<'a> MapFormatter<'a> {
    pub fn new(ty: &'a AlgebraicType) -> Self {
        Self { ty }
    }
}

impl<'a> Display for MapFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.ty {
            AlgebraicType::Sum(ty) => {
                write!(f, "{{ ty_: Sum",)?;
                if ty.types.len() != 0 {
                    write!(f, ", ")?;
                }
                for (i, e_ty) in ty.types.iter().enumerate() {
                    write!(f, "{}: {}", i, MapFormatter::new(e_ty))?;
                    if i < ty.types.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, " }}",)
            }
            AlgebraicType::Product(ty) => {
                write!(f, "{{ ty_: Product",)?;
                if ty.elements.len() != 0 {
                    write!(f, ", ")?;
                }
                for (i, e_ty) in ty.elements.iter().enumerate() {
                    if let Some(name) = &e_ty.name {
                        write!(f, "{}: {}", name, MapFormatter::new(&e_ty.algebraic_type))?;
                    } else {
                        write!(f, "{}: {}", i, MapFormatter::new(&e_ty.algebraic_type))?;
                    }
                    if i < ty.elements.len() - 1 {
                        write!(f, ", ")?;
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
                    BuiltinType::Array { ty } => write!(f, ", 0: Array, 1: {}", MapFormatter::new(ty))?,
                }
                write!(f, " }}",)
            }
        }
    }
}

impl AlgebraicType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return Err("Byte array length is invalid.".to_string());
        }
        match bytes[0] {
            TAG_PRODUCT => {
                let (ty, bytes_read) = ProductType::decode(&bytes[1..])?;
                Ok((AlgebraicType::Product(ty), bytes_read + 1))
            }
            TAG_SUM => {
                let (ty, bytes_read) = SumType::decode(&bytes[1..])?;
                Ok((AlgebraicType::Sum(ty), bytes_read + 1))
            }
            _ => {
                let (ty, bytes_read) = BuiltinType::decode(&bytes[0..])?;
                Ok((AlgebraicType::Builtin(ty), bytes_read))
            }
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            AlgebraicType::Product(ty) => {
                bytes.push(TAG_PRODUCT);
                ty.encode(bytes);
            }
            AlgebraicType::Sum(ty) => {
                bytes.push(TAG_SUM);
                ty.encode(bytes);
            }
            AlgebraicType::Builtin(ty) => {
                ty.encode(bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AlgebraicType;
    use crate::{
        algebraic_type::{MapFormatter, SATNFormatter},
        builtin_type::BuiltinType,
        product_type::ProductType,
        product_type_element::ProductTypeElement,
        sum_type::SumType,
    };

    #[test]
    fn never() {
        let never = AlgebraicType::Sum(SumType { types: vec![] });
        assert_eq!("|", SATNFormatter::new(&never).to_string());
    }

    #[test]
    fn never_map() {
        let never = AlgebraicType::Sum(SumType { types: vec![] });
        assert_eq!("{ ty_: Sum }", MapFormatter::new(&never).to_string());
    }

    #[test]
    fn unit() {
        let unit = AlgebraicType::Product(ProductType { elements: vec![] });
        assert_eq!("()", SATNFormatter::new(&unit).to_string());
    }

    #[test]
    fn unit_map() {
        let unit = AlgebraicType::Product(ProductType { elements: vec![] });
        assert_eq!("{ ty_: Product }", MapFormatter::new(&unit).to_string());
    }

    #[test]
    fn primitive() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        assert_eq!("U8", SATNFormatter::new(&u8).to_string());
    }

    #[test]
    fn primitive_map() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        assert_eq!("{ ty_: Builtin, 0: U8 }", MapFormatter::new(&u8).to_string());
    }

    fn make_option_type() -> AlgebraicType {
        let never = AlgebraicType::Sum(SumType { types: vec![] });
        let unit = AlgebraicType::Product(ProductType::new(vec![]));
        let some_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: never.clone(),
            name: Some("some".into()),
        }]));
        let none_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: unit.clone(),
            name: Some("none".into()),
        }]));
        AlgebraicType::Sum(SumType {
            types: vec![some_type, none_type],
        })
    }

    #[test]
    fn option() {
        let option = make_option_type();
        assert_eq!("((some: |) | (none: ()))", SATNFormatter::new(&option).to_string());
    }

    #[test]
    fn option_map() {
        let option = make_option_type();
        assert_eq!(
            "{ ty_: Sum, 0: { ty_: Product, some: { ty_: Sum } }, 1: { ty_: Product, none: { ty_: Product } } }",
            MapFormatter::new(&option).to_string()
        );
    }

    // TODO: recursive types
    // TODO: parameterized types
    fn make_algebraic_type_type() -> AlgebraicType {
        let never = AlgebraicType::Sum(SumType { types: vec![] });
        let string = AlgebraicType::Builtin(BuiltinType::String);
        let array = AlgebraicType::Builtin(BuiltinType::Array {
            ty: Box::new(never.clone()),
        });
        let never = AlgebraicType::Sum(SumType { types: vec![] });
        let unit = AlgebraicType::Product(ProductType::new(vec![]));
        let some_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: string.clone(),
            name: Some("some".into()),
        }]));
        let none_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: unit.clone(),
            name: Some("none".into()),
        }]));
        let option = AlgebraicType::Sum(SumType {
            types: vec![some_type, none_type],
        });
        let sum_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: array.clone(),
            name: Some("types".into()),
        }]));
        let element_type = AlgebraicType::Product(ProductType::new(vec![
            ProductTypeElement {
                algebraic_type: option,
                name: Some("name".into()),
            },
            ProductTypeElement {
                algebraic_type: never.clone(),
                name: Some("algebraic_type".into()),
            },
        ]));
        let product_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: AlgebraicType::Builtin(BuiltinType::Array {
                ty: Box::new(element_type),
            }),
            name: Some("elements".into()),
        }]));
        let builtin_type = AlgebraicType::Sum(SumType::new(vec![
            AlgebraicType::Builtin(BuiltinType::Bool),
            AlgebraicType::Builtin(BuiltinType::I8),
            AlgebraicType::Builtin(BuiltinType::U8),
            AlgebraicType::Builtin(BuiltinType::I16),
            AlgebraicType::Builtin(BuiltinType::U16),
            AlgebraicType::Builtin(BuiltinType::I32),
            AlgebraicType::Builtin(BuiltinType::U32),
            AlgebraicType::Builtin(BuiltinType::I64),
            AlgebraicType::Builtin(BuiltinType::U64),
            AlgebraicType::Builtin(BuiltinType::I128),
            AlgebraicType::Builtin(BuiltinType::U128),
            AlgebraicType::Builtin(BuiltinType::F32),
            AlgebraicType::Builtin(BuiltinType::F64),
            AlgebraicType::Builtin(BuiltinType::String),
            AlgebraicType::Builtin(BuiltinType::Array {
                ty: Box::new(never.clone()),
            }),
        ]));
        AlgebraicType::Sum(SumType::new(vec![sum_type, product_type, builtin_type]))
    }

    #[test]
    fn algebraic_type() {
        let algebraic_type = make_algebraic_type_type();
        assert_eq!("((types: Array<|>) | (elements: Array<(name: ((some: String) | (none: ())), algebraic_type: |)>) | (Bool | I8 | U8 | I16 | U16 | I32 | U32 | I64 | U64 | I128 | U128 | F32 | F64 | String | Array<|>))", SATNFormatter::new(&algebraic_type).to_string());
    }

    #[test]
    fn algebraic_type_map() {
        let algebraic_type = make_algebraic_type_type();
        assert_eq!("{ ty_: Sum, 0: { ty_: Product, types: { ty_: Builtin, 0: Array, 1: { ty_: Sum } } }, 1: { ty_: Product, elements: { ty_: Builtin, 0: Array, 1: { ty_: Product, name: { ty_: Sum, 0: { ty_: Product, some: { ty_: Builtin, 0: String } }, 1: { ty_: Product, none: { ty_: Product } } }, algebraic_type: { ty_: Sum } } } }, 2: { ty_: Sum, 0: { ty_: Builtin, 0: Bool }, 1: { ty_: Builtin, 0: I8 }, 2: { ty_: Builtin, 0: U8 }, 3: { ty_: Builtin, 0: I16 }, 4: { ty_: Builtin, 0: U16 }, 5: { ty_: Builtin, 0: I32 }, 6: { ty_: Builtin, 0: U32 }, 7: { ty_: Builtin, 0: I64 }, 8: { ty_: Builtin, 0: U64 }, 9: { ty_: Builtin, 0: I128 }, 10: { ty_: Builtin, 0: U128 }, 11: { ty_: Builtin, 0: F32 }, 12: { ty_: Builtin, 0: F64 }, 13: { ty_: Builtin, 0: String }, 14: { ty_: Builtin, 0: Array, 1: { ty_: Sum } } } }", MapFormatter::new(&algebraic_type).to_string());
    }

    #[test]
    fn it_works() {
        let never = AlgebraicType::Sum(SumType { types: vec![] });
        let builtin = AlgebraicType::Builtin(BuiltinType::U8);
        let product = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            name: Some("thing".into()),
            algebraic_type: AlgebraicType::Builtin(BuiltinType::U8),
        }]));
        let next = AlgebraicType::Sum(SumType::new(vec![builtin.clone(), builtin.clone(), product]));
        let next = AlgebraicType::Product(ProductType::new(vec![
            ProductTypeElement {
                algebraic_type: builtin.clone(),
                name: Some("test".into()),
            },
            ProductTypeElement {
                algebraic_type: next,
                name: None, //Some("foo".into()),
            },
            ProductTypeElement {
                algebraic_type: builtin,
                name: None,
            },
            ProductTypeElement {
                algebraic_type: never,
                name: Some("never".into()),
            },
        ]));
        assert_eq!(
            "(test: U8, 1: (U8 | U8 | (thing: U8)), 2: U8, never: |)",
            SATNFormatter::new(&next).to_string()
        );
    }
}
