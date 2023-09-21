pub mod fmt;
pub mod map_notation;

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::meta_type::MetaType;
use crate::{de::Deserialize, ser::Serialize, MapType};
use crate::{
    AlgebraicTypeRef, AlgebraicValue, ArrayType, BuiltinType, ProductType, ProductTypeElement, SumType, SumTypeVariant,
};
use enum_as_inner::EnumAsInner;

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
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub enum AlgebraicType {
    /// A structural sum type.
    ///
    /// Unlike most languages, sums in SATs are *[structural]* and not nominal.
    /// When checking whether two nominal types are the same,
    /// their names and/or declaration sites (e.g., module / namespace) are considered.
    /// Meanwhile, a structural type system would only check the structure of the type itself,
    /// e.g., the names of its variants and their inner data types in the case of a sum.
    ///
    /// This is also known as a discriminated union (implementation) or disjoint union.
    /// Another name is [coproduct (category theory)](https://ncatlab.org/nlab/show/coproduct).
    ///
    /// These structures are known as sum types because the number of possible values a sum
    /// ```ignore
    /// { N_0(T_0), N_1(T_1), ..., N_n(T_n) }
    /// ```
    /// is:
    /// ```ignore
    /// Σ (i ∈ 0..n). values(T_i)
    /// ```
    /// so for example, `values({ A(U64), B(Bool) }) = values(U64) + values(Bool)`.
    ///
    /// See also: https://ncatlab.org/nlab/show/sum+type.
    ///
    /// [structural]: https://en.wikipedia.org/wiki/Structural_type_system
    Sum(SumType),
    /// A structural product type.
    ///
    /// This is also known as `struct` and `tuple` in many languages,
    /// but note that unlike most languages, sums in SATs are *[structural]* and not nominal.
    /// When checking whether two nominal types are the same,
    /// their names and/or declaration sites (e.g., module / namespace) are considered.
    /// Meanwhile, a structural type system would only check the structure of the type itself,
    /// e.g., the names of its fields and their types in the case of a record.
    /// The name "product" comes from category theory.
    ///
    /// See also: https://ncatlab.org/nlab/show/product+type.
    ///
    /// These structures are known as product types because the number of possible values in product
    /// ```ignore
    /// { N_0: T_0, N_1: T_1, ..., N_n: T_n }
    /// ```
    /// is:
    /// ```ignore
    /// Π (i ∈ 0..n). values(T_i)
    /// ```
    /// so for example, `values({ A: U64, B: Bool }) = values(U64) * values(Bool)`.
    ///
    /// [structural]: https://en.wikipedia.org/wiki/Structural_type_system
    Product(ProductType),
    /// A bulltin type, e.g., `bool`.
    Builtin(BuiltinType),
    /// A type where the definition is given by the typing context (`Typespace`).
    /// In other words, this is defined by a pointer to another `AlgebraicType`.
    ///
    /// This should not be conflated with reference and pointer types in languages like Rust,
    /// In other words, this is not `&T` or `*const T`.
    Ref(AlgebraicTypeRef),
}

#[allow(non_upper_case_globals)]
impl AlgebraicType {
    /// The built-in Bool type.
    pub const Bool: Self = Self::Builtin(BuiltinType::Bool);

    /// The built-in signed 8-bit integer type.
    pub const I8: Self = Self::Builtin(BuiltinType::I8);

    /// The built-in unsigned 8-bit integer type.
    pub const U8: Self = Self::Builtin(BuiltinType::U8);

    /// The built-in signed 16-bit integer type.
    pub const I16: Self = Self::Builtin(BuiltinType::I16);

    /// The built-in unsigned 16-bit integer type.
    pub const U16: Self = Self::Builtin(BuiltinType::U16);

    /// The built-in signed 32-bit integer type.
    pub const I32: Self = Self::Builtin(BuiltinType::I32);

    /// The built-in unsigned 32-bit integer type.
    pub const U32: Self = Self::Builtin(BuiltinType::U32);

    /// The built-in signed 64-bit integer type.
    pub const I64: Self = Self::Builtin(BuiltinType::I64);

    /// The built-in unsigned 64-bit integer type.
    pub const U64: Self = Self::Builtin(BuiltinType::U64);

    /// The built-in signed 128-bit integer type.
    pub const I128: Self = Self::Builtin(BuiltinType::I128);

    /// The built-in unsigned 128-bit integer type.
    pub const U128: Self = Self::Builtin(BuiltinType::U128);

    /// The built-in 32-bit floating point type.
    pub const F32: Self = Self::Builtin(BuiltinType::F32);

    /// The built-in 64-bit floating point type.
    pub const F64: Self = Self::Builtin(BuiltinType::F64);

    /// The built-in string type.
    pub const String: Self = Self::Builtin(BuiltinType::String);

    /// The canonical 0-element unit type.
    pub const UNIT_TYPE: Self = Self::product(Vec::new());

    /// The canonical 0-variant "never" / "absurd" / "void" type.
    pub const NEVER_TYPE: Self = Self::sum(Vec::new());
}

impl MetaType for AlgebraicType {
    /// This is a static function that constructs the type of `AlgebraicType`
    /// and returns it as an `AlgebraicType`.
    ///
    /// This could alternatively be implemented
    /// as a regular AlgebraicValue or as a static variable.
    fn meta_type() -> Self {
        AlgebraicType::sum(vec![
            SumTypeVariant::new_named(SumType::meta_type(), "sum"),
            SumTypeVariant::new_named(ProductType::meta_type(), "product"),
            SumTypeVariant::new_named(BuiltinType::meta_type(), "builtin"),
            SumTypeVariant::new_named(AlgebraicTypeRef::meta_type(), "ref"),
        ])
    }
}

impl AlgebraicType {
    /// A type representing an array of `U8`s.
    pub fn bytes() -> Self {
        Self::array(Self::U8)
    }

    /// Returns whether this type is `AlgebraicType::bytes()`.
    pub fn is_bytes(&self) -> bool {
        matches!(self, AlgebraicType::Builtin(BuiltinType::Array(ArrayType { elem_ty }))
            if **elem_ty == AlgebraicType::U8
        )
    }

    /// Returns a sum type with the given `variants`.
    pub const fn sum(variants: Vec<SumTypeVariant>) -> Self {
        AlgebraicType::Sum(SumType { variants })
    }

    /// Returns a product type with the given `factors`.
    pub const fn product(factors: Vec<ProductTypeElement>) -> Self {
        AlgebraicType::Product(ProductType::new(factors))
    }

    /// Returns a structural option type where `some_type` is the type for the `some` variant.
    pub fn option(some_type: Self) -> Self {
        Self::sum(vec![
            SumTypeVariant::new_named(some_type, "some"),
            SumTypeVariant::unit("none"),
        ])
    }

    /// Returns an unsized array type where the element type is `ty`.
    pub fn array(ty: Self) -> Self {
        AlgebraicType::Builtin(BuiltinType::Array(ArrayType { elem_ty: Box::new(ty) }))
    }

    /// Returns a map type from the type `key` to the type `value`.
    pub fn map(key: Self, value: Self) -> Self {
        let value = MapType::new(key, value);
        AlgebraicType::Builtin(BuiltinType::Map(value))
    }

    /// Returns a sum type of unit variants with names taken from `var_names`.
    pub fn simple_enum<'a>(var_names: impl Iterator<Item = &'a str>) -> Self {
        Self::sum(var_names.into_iter().map(SumTypeVariant::unit).collect())
    }

    pub fn as_value(&self) -> AlgebraicValue {
        self.serialize(ValueSerializer).unwrap_or_else(|x| match x {})
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<Self, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}

#[cfg(test)]
mod tests {
    use super::AlgebraicType;
    use crate::meta_type::MetaType;
    use crate::satn::Satn;
    use crate::{
        algebraic_type::fmt::fmt_algebraic_type, algebraic_type::map_notation::fmt_algebraic_type as fmt_map,
        algebraic_type_ref::AlgebraicTypeRef, product_type_element::ProductTypeElement, typespace::Typespace,
    };
    use crate::{ValueWithType, WithTypespace};

    #[test]
    fn never() {
        assert_eq!("(|)", fmt_algebraic_type(&AlgebraicType::NEVER_TYPE).to_string());
    }

    #[test]
    fn never_map() {
        assert_eq!("{ ty_: Sum }", fmt_map(&AlgebraicType::NEVER_TYPE).to_string());
    }

    #[test]
    fn unit() {
        assert_eq!("()", fmt_algebraic_type(&AlgebraicType::UNIT_TYPE).to_string());
    }

    #[test]
    fn unit_map() {
        assert_eq!("{ ty_: Product }", fmt_map(&AlgebraicType::UNIT_TYPE).to_string());
    }

    #[test]
    fn primitive() {
        assert_eq!("U8", fmt_algebraic_type(&AlgebraicType::U8).to_string());
    }

    #[test]
    fn primitive_map() {
        assert_eq!("{ ty_: Builtin, 0: U8 }", fmt_map(&AlgebraicType::U8).to_string());
    }

    #[test]
    fn option() {
        let option = AlgebraicType::option(AlgebraicType::NEVER_TYPE);
        assert_eq!("(some: (|) | none: ())", fmt_algebraic_type(&option).to_string());
    }

    #[test]
    fn option_map() {
        let option = AlgebraicType::option(AlgebraicType::NEVER_TYPE);
        assert_eq!(
            "{ ty_: Sum, some: { ty_: Sum }, none: { ty_: Product } }",
            fmt_map(&option).to_string()
        );
    }

    #[test]
    fn algebraic_type() {
        let algebraic_type = AlgebraicType::meta_type();
        assert_eq!(
            "(sum: (variants: Array<(name: (some: String | none: ()), algebraic_type: &0)>) | product: (elements: Array<(name: (some: String | none: ()), algebraic_type: &0)>) | builtin: (bool: () | i8: () | u8: () | i16: () | u16: () | i32: () | u32: () | i64: () | u64: () | i128: () | u128: () | f32: () | f64: () | string: () | array: &0 | map: (key_ty: &0, ty: &0)) | ref: U32)",
            fmt_algebraic_type(&algebraic_type).to_string()
        );
    }

    #[test]
    fn algebraic_type_map() {
        let algebraic_type = AlgebraicType::meta_type();
        assert_eq!(
            "{ ty_: Sum, sum: { ty_: Product, variants: { ty_: Builtin, 0: Array, 1: { ty_: Product, name: { ty_: Sum, some: { ty_: Builtin, 0: String }, none: { ty_: Product } }, algebraic_type: { ty_: Ref, 0: 0 } } } }, product: { ty_: Product, elements: { ty_: Builtin, 0: Array, 1: { ty_: Product, name: { ty_: Sum, some: { ty_: Builtin, 0: String }, none: { ty_: Product } }, algebraic_type: { ty_: Ref, 0: 0 } } } }, builtin: { ty_: Sum, bool: { ty_: Product }, i8: { ty_: Product }, u8: { ty_: Product }, i16: { ty_: Product }, u16: { ty_: Product }, i32: { ty_: Product }, u32: { ty_: Product }, i64: { ty_: Product }, u64: { ty_: Product }, i128: { ty_: Product }, u128: { ty_: Product }, f32: { ty_: Product }, f64: { ty_: Product }, string: { ty_: Product }, array: { ty_: Ref, 0: 0 }, map: { ty_: Product, key_ty: { ty_: Ref, 0: 0 }, ty: { ty_: Ref, 0: 0 } } }, ref: { ty_: Builtin, 0: U32 } }",
            fmt_map(&algebraic_type).to_string()
        );
    }

    #[test]
    fn nested_products_and_sums() {
        let builtin = AlgebraicType::U8;
        let product = AlgebraicType::product(vec![ProductTypeElement {
            name: Some("thing".into()),
            algebraic_type: AlgebraicType::U8,
        }]);
        let next = AlgebraicType::sum(vec![builtin.clone().into(), builtin.clone().into(), product.into()]);
        let next = AlgebraicType::product(vec![
            ProductTypeElement {
                algebraic_type: builtin.clone(),
                name: Some("test".into()),
            },
            next.into(),
            builtin.into(),
            ProductTypeElement {
                algebraic_type: AlgebraicType::NEVER_TYPE,
                name: Some("never".into()),
            },
        ]);
        assert_eq!(
            "(test: U8, 1: (U8 | U8 | (thing: U8)), 2: U8, never: (|))",
            fmt_algebraic_type(&next).to_string()
        );
    }

    fn in_space<'a, T: crate::Value>(ts: &'a Typespace, ty: &'a T::Type, val: &'a T) -> ValueWithType<'a, T> {
        WithTypespace::new(ts, ty).with_value(val)
    }

    #[test]
    fn option_as_value() {
        let option = AlgebraicType::option(AlgebraicType::NEVER_TYPE);
        let algebraic_type = AlgebraicType::meta_type();
        let typespace = Typespace::new(vec![algebraic_type]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));
        assert_eq!(
            r#"(sum = (variants = [(name = (some = "some"), algebraic_type = (sum = (variants = []))), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))"#,
            in_space(&typespace, &at_ref, &option.as_value()).to_satn()
        );
    }

    #[test]
    fn builtin_as_value() {
        let array = AlgebraicType::U8;
        let algebraic_type = AlgebraicType::meta_type();
        let typespace = Typespace::new(vec![algebraic_type]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));
        assert_eq!(
            "(builtin = (u8 = ()))",
            in_space(&typespace, &at_ref, &array.as_value()).to_satn()
        );
    }

    #[test]
    fn algebraic_type_as_value() {
        let algebraic_type = AlgebraicType::meta_type();
        let typespace = Typespace::new(vec![algebraic_type.clone()]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));
        assert_eq!(
            r#"(sum = (variants = [(name = (some = "sum"), algebraic_type = (product = (elements = [(name = (some = "variants"), algebraic_type = (builtin = (array = (product = (elements = [(name = (some = "name"), algebraic_type = (sum = (variants = [(name = (some = "some"), algebraic_type = (builtin = (string = ()))), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))), (name = (some = "algebraic_type"), algebraic_type = (ref = 0))])))))]))), (name = (some = "product"), algebraic_type = (product = (elements = [(name = (some = "elements"), algebraic_type = (builtin = (array = (product = (elements = [(name = (some = "name"), algebraic_type = (sum = (variants = [(name = (some = "some"), algebraic_type = (builtin = (string = ()))), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))), (name = (some = "algebraic_type"), algebraic_type = (ref = 0))])))))]))), (name = (some = "builtin"), algebraic_type = (sum = (variants = [(name = (some = "bool"), algebraic_type = (product = (elements = []))), (name = (some = "i8"), algebraic_type = (product = (elements = []))), (name = (some = "u8"), algebraic_type = (product = (elements = []))), (name = (some = "i16"), algebraic_type = (product = (elements = []))), (name = (some = "u16"), algebraic_type = (product = (elements = []))), (name = (some = "i32"), algebraic_type = (product = (elements = []))), (name = (some = "u32"), algebraic_type = (product = (elements = []))), (name = (some = "i64"), algebraic_type = (product = (elements = []))), (name = (some = "u64"), algebraic_type = (product = (elements = []))), (name = (some = "i128"), algebraic_type = (product = (elements = []))), (name = (some = "u128"), algebraic_type = (product = (elements = []))), (name = (some = "f32"), algebraic_type = (product = (elements = []))), (name = (some = "f64"), algebraic_type = (product = (elements = []))), (name = (some = "string"), algebraic_type = (product = (elements = []))), (name = (some = "array"), algebraic_type = (ref = 0)), (name = (some = "map"), algebraic_type = (product = (elements = [(name = (some = "key_ty"), algebraic_type = (ref = 0)), (name = (some = "ty"), algebraic_type = (ref = 0))])))]))), (name = (some = "ref"), algebraic_type = (builtin = (u32 = ())))]))"#,
            in_space(&typespace, &at_ref, &algebraic_type.as_value()).to_satn()
        );
    }

    #[test]
    fn option_from_value() {
        let option = AlgebraicType::option(AlgebraicType::NEVER_TYPE);
        AlgebraicType::from_value(&option.as_value()).expect("No errors.");
    }

    #[test]
    fn builtin_from_value() {
        let u8 = AlgebraicType::U8;
        AlgebraicType::from_value(&u8.as_value()).expect("No errors.");
    }

    #[test]
    fn algebraic_type_from_value() {
        let algebraic_type = AlgebraicType::meta_type();
        AlgebraicType::from_value(&algebraic_type.as_value()).expect("No errors.");
    }
}
