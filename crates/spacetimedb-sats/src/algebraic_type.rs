pub mod map_notation;
pub mod satn;
use crate::{
    algebraic_type_ref::AlgebraicTypeRef, algebraic_value::AlgebraicValue, builtin_type::BuiltinType,
    builtin_value::BuiltinValue, product_type::ProductType, sum_type::SumType, sum_type_variant::SumTypeVariant,
    sum_value::SumValue,
};
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};

/// The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
/// which a nominal type system can be constructed.
///
/// The type system unifies the concepts sum types, product types, and built-in
/// primitive types into a single type system.
///
/// Below are some common types you might implement in this type system.
///
/// ```ignore
/// type Unit = () // or (,) or , Product with zero elements
/// type Never = (|) // or | Sum with zero elements
/// type U8 = U8 // Builtin
/// type Foo = (foo: I8) != I8
/// type Bar = (bar: I8)
/// type Color = (a: I8 | b: I8) // Sum with one element
/// type Age = (age: U8) // Product with one element
/// type Option<T> = (some: T | none: ())
/// type Ref = &0
///
/// type AlgebraicType = (sum: SumType | product: ProductType | builtin: BuiltinType | set: AlgebraicType)
/// type Catalog<T> = (name: String, indices: Set<Set<Tag>>, relation: Set<>)
/// type CatalogEntry = { name: string, indexes: {some type}, relation: Relation }
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
/// ```
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum AlgebraicType {
    Sum(SumType),
    Product(ProductType),
    Builtin(BuiltinType),
    Ref(AlgebraicTypeRef),
}

impl AlgebraicType {
    /// This is a static function that constructs the type of AlgebraicType and
    /// returns it as an AlgebraicType. This could alternatively be implemented
    /// as a regular AlgebraicValue or as a static variable.
    pub fn make_meta_type() -> AlgebraicType {
        AlgebraicType::Sum(SumType::new(vec![
            SumTypeVariant::new_named(SumType::make_meta_type(), "sum"),
            SumTypeVariant::new_named(ProductType::make_meta_type(), "product"),
            SumTypeVariant::new_named(BuiltinType::make_meta_type(), "builtin"),
            SumTypeVariant::new_named(AlgebraicTypeRef::make_meta_type(), "ref"),
        ]))
    }

    pub fn make_never_type() -> AlgebraicType {
        AlgebraicType::Sum(SumType { variants: vec![] })
    }

    pub fn make_option_type(some_type: AlgebraicType) -> AlgebraicType {
        let unit = AlgebraicType::Product(ProductType::new(vec![]));
        AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant::new_named(some_type, "some"),
                SumTypeVariant::new_named(unit, "none"),
            ],
        })
    }

    pub fn as_value(&self) -> AlgebraicValue {
        match self {
            AlgebraicType::Sum(e_ty) => AlgebraicValue::Sum(SumValue {
                tag: 0,
                value: Box::new(e_ty.as_value()),
            }),
            AlgebraicType::Product(e_ty) => AlgebraicValue::Sum(SumValue {
                tag: 1,
                value: Box::new(e_ty.as_value()),
            }),
            AlgebraicType::Builtin(e_ty) => AlgebraicValue::Sum(SumValue {
                tag: 2,
                value: Box::new(e_ty.as_value()),
            }),
            AlgebraicType::Ref(r) => AlgebraicValue::Sum(SumValue {
                tag: 3,
                value: Box::new(AlgebraicValue::Builtin(BuiltinValue::U32(r.0))),
            }),
        }
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<AlgebraicType, ()> {
        match value {
            AlgebraicValue::Sum(value) => match value.tag {
                0 => Ok(AlgebraicType::Sum(SumType::from_value(&value.value)?)),
                1 => Ok(AlgebraicType::Product(ProductType::from_value(&value.value)?)),
                2 => Ok(AlgebraicType::Builtin(BuiltinType::from_value(&value.value)?)),
                3 => {
                    let Some(value) = value.value.as_builtin() else {
                            return Err(());
                        };
                    let Some(r) = value.as_u32() else {
                            return Err(());
                        };
                    Ok(AlgebraicType::Ref(AlgebraicTypeRef(*r)))
                }
                _ => Err(()),
            },
            AlgebraicValue::Product(_) => Err(()),
            AlgebraicValue::Builtin(_) => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AlgebraicType;
    use crate::algebraic_type::map_notation;
    use crate::{
        algebraic_type::satn::Formatter, algebraic_type_ref::AlgebraicTypeRef, algebraic_value,
        builtin_type::BuiltinType, product_type::ProductType, product_type_element::ProductTypeElement,
        sum_type::SumType, typespace::Typespace,
    };

    #[test]
    fn never() {
        let never = AlgebraicType::Sum(SumType { variants: vec![] });
        assert_eq!("(|)", Formatter::new(&never).to_string());
    }

    #[test]
    fn never_map() {
        let never = AlgebraicType::Sum(SumType { variants: vec![] });
        assert_eq!("{ ty_: Sum }", map_notation::Formatter::new(&never).to_string());
    }

    #[test]
    fn unit() {
        let unit = AlgebraicType::Product(ProductType { elements: vec![] });
        assert_eq!("()", Formatter::new(&unit).to_string());
    }

    #[test]
    fn unit_map() {
        let unit = AlgebraicType::Product(ProductType { elements: vec![] });
        assert_eq!("{ ty_: Product }", map_notation::Formatter::new(&unit).to_string());
    }

    #[test]
    fn primitive() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        assert_eq!("U8", Formatter::new(&u8).to_string());
    }

    #[test]
    fn primitive_map() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        assert_eq!("{ ty_: Builtin, 0: U8 }", map_notation::Formatter::new(&u8).to_string());
    }

    #[test]
    fn option() {
        let never = AlgebraicType::Sum(SumType { variants: vec![] });
        let option = AlgebraicType::make_option_type(never);
        assert_eq!("(some: (|) | none: ())", Formatter::new(&option).to_string());
    }

    #[test]
    fn option_map() {
        let never = AlgebraicType::Sum(SumType { variants: vec![] });
        let option = AlgebraicType::make_option_type(never);
        assert_eq!(
            "{ ty_: Sum, some: { ty_: Sum }, none: { ty_: Product } }",
            map_notation::Formatter::new(&option).to_string()
        );
    }

    #[test]
    fn algebraic_type() {
        let algebraic_type = AlgebraicType::make_meta_type();
        assert_eq!("(sum: (variants: Array<(name: (some: String | none: ()), algebraic_type: &0)>) | product: (elements: Array<(name: (some: String | none: ()), algebraic_type: &0)>) | builtin: (bool: () | i8: () | u8: () | i16: () | u16: () | i32: () | u32: () | i64: () | u64: () | i128: () | u128: () | f32: () | f64: () | string: () | array: &0 | map: (key_ty: &0, ty: &0)) | ref: U32)", Formatter::new(&algebraic_type).to_string());
    }

    #[test]
    fn algebraic_type_map() {
        let algebraic_type = AlgebraicType::make_meta_type();
        assert_eq!("{ ty_: Sum, sum: { ty_: Product, variants: { ty_: Builtin, 0: Array, 1: { ty_: Product, name: { ty_: Sum, some: { ty_: Builtin, 0: String }, none: { ty_: Product } }, algebraic_type: { ty_: Ref, 0: 0 } } } }, product: { ty_: Product, elements: { ty_: Builtin, 0: Array, 1: { ty_: Product, name: { ty_: Sum, some: { ty_: Builtin, 0: String }, none: { ty_: Product } }, algebraic_type: { ty_: Ref, 0: 0 } } } }, builtin: { ty_: Sum, bool: { ty_: Product }, i8: { ty_: Product }, u8: { ty_: Product }, i16: { ty_: Product }, u16: { ty_: Product }, i32: { ty_: Product }, u32: { ty_: Product }, i64: { ty_: Product }, u64: { ty_: Product }, i128: { ty_: Product }, u128: { ty_: Product }, f32: { ty_: Product }, f64: { ty_: Product }, string: { ty_: Product }, array: { ty_: Ref, 0: 0 }, map: { ty_: Product, key_ty: { ty_: Ref, 0: 0 }, ty: { ty_: Ref, 0: 0 } } }, ref: { ty_: Builtin, 0: U32 } }", map_notation::Formatter::new(&algebraic_type).to_string());
    }

    #[test]
    fn nested_products_and_sums() {
        let never = AlgebraicType::Sum(SumType { variants: vec![] });
        let builtin = AlgebraicType::Builtin(BuiltinType::U8);
        let product = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            name: Some("thing".into()),
            algebraic_type: AlgebraicType::Builtin(BuiltinType::U8),
        }]));
        let next = AlgebraicType::Sum(SumType::new_unnamed(vec![builtin.clone(), builtin.clone(), product]));
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
            "(test: U8, 1: (U8 | U8 | (thing: U8)), 2: U8, never: (|))",
            Formatter::new(&next).to_string()
        );
    }

    #[test]
    fn option_as_value() {
        let never = AlgebraicType::Sum(SumType::new(Vec::new()));
        let option = AlgebraicType::make_option_type(never);
        let algebraic_type = AlgebraicType::make_meta_type();
        let typespace = Typespace::new(vec![algebraic_type]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));
        assert_eq!(
            r#"(sum = (variants = [(name = (some = "some"), algebraic_type = (sum = (variants = []))), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))"#,
            algebraic_value::satn::Formatter::new(&typespace, &at_ref, &option.as_value()).to_string()
        );
    }

    #[test]
    fn builtin_as_value() {
        let array = AlgebraicType::Builtin(BuiltinType::U8);
        let algebraic_type = AlgebraicType::make_meta_type();
        let typespace = Typespace::new(vec![algebraic_type]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));
        assert_eq!(
            "(builtin = (u8 = ()))",
            algebraic_value::satn::Formatter::new(&typespace, &at_ref, &array.as_value()).to_string()
        );
    }

    #[test]
    fn algebraic_type_as_value() {
        let algebraic_type = AlgebraicType::make_meta_type();
        let typespace = Typespace::new(vec![algebraic_type.clone()]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));
        assert_eq!(
            r#"(sum = (variants = [(name = (some = "sum"), algebraic_type = (product = (elements = [(name = (some = "variants"), algebraic_type = (builtin = (array = (product = (elements = [(name = (some = "name"), algebraic_type = (sum = (variants = [(name = (some = "some"), algebraic_type = (builtin = (string = ()))), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))), (name = (some = "algebraic_type"), algebraic_type = (ref = 0))])))))]))), (name = (some = "product"), algebraic_type = (product = (elements = [(name = (some = "elements"), algebraic_type = (builtin = (array = (product = (elements = [(name = (some = "name"), algebraic_type = (sum = (variants = [(name = (some = "some"), algebraic_type = (builtin = (string = ()))), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))), (name = (some = "algebraic_type"), algebraic_type = (ref = 0))])))))]))), (name = (some = "builtin"), algebraic_type = (sum = (variants = [(name = (some = "bool"), algebraic_type = (product = (elements = []))), (name = (some = "i8"), algebraic_type = (product = (elements = []))), (name = (some = "u8"), algebraic_type = (product = (elements = []))), (name = (some = "i16"), algebraic_type = (product = (elements = []))), (name = (some = "u16"), algebraic_type = (product = (elements = []))), (name = (some = "i32"), algebraic_type = (product = (elements = []))), (name = (some = "u32"), algebraic_type = (product = (elements = []))), (name = (some = "i64"), algebraic_type = (product = (elements = []))), (name = (some = "u64"), algebraic_type = (product = (elements = []))), (name = (some = "i128"), algebraic_type = (product = (elements = []))), (name = (some = "u128"), algebraic_type = (product = (elements = []))), (name = (some = "f32"), algebraic_type = (product = (elements = []))), (name = (some = "f64"), algebraic_type = (product = (elements = []))), (name = (some = "string"), algebraic_type = (product = (elements = []))), (name = (some = "array"), algebraic_type = (ref = 0)), (name = (some = "map"), algebraic_type = (product = (elements = [(name = (some = "key_ty"), algebraic_type = (ref = 0)), (name = (some = "ty"), algebraic_type = (ref = 0))])))]))), (name = (some = "ref"), algebraic_type = (builtin = (u32 = ())))]))"#,
            algebraic_value::satn::Formatter::new(&typespace, &at_ref, &algebraic_type.as_value()).to_string()
        );
    }

    #[test]
    fn option_from_value() {
        let never = AlgebraicType::Sum(SumType::new(Vec::new()));
        let option = AlgebraicType::make_option_type(never);
        AlgebraicType::from_value(&option.as_value()).expect("No errors.");
    }

    #[test]
    fn builtin_from_value() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        AlgebraicType::from_value(&u8.as_value()).expect("No errors.");
    }

    #[test]
    fn algebraic_type_from_value() {
        let algebraic_type = AlgebraicType::make_meta_type();
        AlgebraicType::from_value(&algebraic_type.as_value()).expect("No errors.");
    }

    fn _legacy_encoding_comparison() {
        let algebraic_type = AlgebraicType::make_meta_type();

        let mut buf = Vec::new();
        algebraic_type.as_value().encode(&mut buf);
        println!("buf: {:?}", buf);

        let mut buf = Vec::new();
        algebraic_type.encode(&mut buf);
        println!("buf: {:?}", buf);
    }
}
