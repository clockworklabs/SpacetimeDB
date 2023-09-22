pub mod fmt;
pub mod map_notation;

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::map_type::MapType;
use crate::meta_type::MetaType;
use crate::slim_slice::SlimSliceBoxCollected;
use crate::{de::Deserialize, ser::Serialize};
use crate::{
    static_assert_size, AlgebraicTypeRef, AlgebraicValue, ArrayType, ProductType, ProductTypeElement, SatsVec, SumType,
    SumTypeVariant,
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
/// type Catalog<T> = (name: String, indices: Set<Set<Tag>>, relation: Set<>)
/// type CatalogEntry = { name: string, indexes: {some type}, relation: Relation }
/// type ElementValue = (tag: Tag, value: AlgebraicValue)
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
    /// A type where the definition is given by the typing context (`Typespace`).
    /// In other words, this is defined by a pointer to another `AlgebraicType`.
    ///
    /// This should not be conflated with reference and pointer types in languages like Rust,
    /// In other words, this is not `&T` or `*const T`.
    Ref(AlgebraicTypeRef),
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
    /// The type of array values where elements are of a base type `elem_ty`.
    /// Values [`AlgebraicValue::Array(array)`](crate::AlgebraicValue::Array) will have this type.
    Array(ArrayType),
    /// The type of map values consisting of a key type `key_ty` and value `ty`.
    /// Values [`AlgebraicValue::Map(map)`](crate::AlgebraicValue::Map) will have this type.
    /// The order of entries in a map value is observable.
    Map(Box<MapType>),
    /// The bool type. Values [`AlgebraicValue::Bool(b)`](crate::AlgebraicValue::Bool) will have this type.
    Bool,
    /// The `I8` type. Values [`AlgebraicValue::I8(v)`](crate::AlgebraicValue::I8) will have this type.
    I8,
    /// The `U8` type. Values [`AlgebraicValue::U8(v)`](crate::AlgebraicValue::U8) will have this type.
    U8,
    /// The `I16` type. Values [`AlgebraicValue::I16(v)`](crate::AlgebraicValue::I16) will have this type.
    I16,
    /// The `U16` type. Values [`AlgebraicValue::U16(v)`](crate::AlgebraicValue::U16) will have this type.
    U16,
    /// The `I32` type. Values [`AlgebraicValue::I32(v)`](crate::AlgebraicValue::I32) will have this type.
    I32,
    /// The `U32` type. Values [`AlgebraicValue::U32(v)`](crate::AlgebraicValue::U32) will have this type.
    U32,
    /// The `I64` type. Values [`AlgebraicValue::I64(v)`](crate::AlgebraicValue::I64) will have this type.
    I64,
    /// The `U64` type. Values [`AlgebraicValue::U64(v)`](crate::AlgebraicValue::U64) will have this type.
    U64,
    /// The `I128` type. Values [`AlgebraicValue::I128(v)`](crate::AlgebraicValue::I128) will have this type.
    I128,
    /// The `U128` type. Values [`AlgebraicValue::U128(v)`](crate::AlgebraicValue::U128) will have this type.
    U128,
    /// The `F32` type. Values [`AlgebraicValue::F32(v)`](crate::AlgebraicValue::F32) will have this type.
    F32,
    /// The `F64` type. Values [`AlgebraicValue::F64(v)`](crate::AlgebraicValue::F64) will have this type.
    F64,
    /// The UTF-8 encoded `String` type.
    /// Values [`AlgebraicValue::String(s)`](crate::AlgebraicValue::String) will have this type.
    ///
    /// This type exists for convenience and because it is easy to just use Rust's `String` (UTF-8)
    /// as opposed to rolling your own equivalent byte-array based UTF-8 encoding.
    String,
}

#[cfg(target_arch = "wasm32")]
static_assert_size!(AlgebraicType, 12);
#[cfg(not(target_arch = "wasm32"))]
static_assert_size!(AlgebraicType, 16);

impl MetaType for AlgebraicType {
    /// This is a static function that constructs the type of `AlgebraicType`
    /// and returns it as an `AlgebraicType`.
    ///
    /// This could alternatively be implemented
    /// as a regular AlgebraicValue or as a static variable.
    fn meta_type() -> Self {
        let map_fs = [
            ProductTypeElement::new_named(Self::ZERO_REF, "key_ty"),
            ProductTypeElement::new_named(Self::ZERO_REF, "ty"),
        ];
        let vs = [
            SumTypeVariant::new_named(AlgebraicTypeRef::meta_type(), "ref"),
            SumTypeVariant::new_named(SumType::meta_type(), "sum"),
            SumTypeVariant::new_named(ProductType::meta_type(), "product"),
            SumTypeVariant::new_named(AlgebraicType::ZERO_REF, "array"),
            SumTypeVariant::new_named(AlgebraicType::product(map_fs.into()), "map"),
            SumTypeVariant::unit("bool"),
            SumTypeVariant::unit("i8"),
            SumTypeVariant::unit("u8"),
            SumTypeVariant::unit("i16"),
            SumTypeVariant::unit("u16"),
            SumTypeVariant::unit("i32"),
            SumTypeVariant::unit("u32"),
            SumTypeVariant::unit("i64"),
            SumTypeVariant::unit("u64"),
            SumTypeVariant::unit("i128"),
            SumTypeVariant::unit("u128"),
            SumTypeVariant::unit("f32"),
            SumTypeVariant::unit("f64"),
            SumTypeVariant::unit("string"),
        ];
        AlgebraicType::sum(vs.into())
    }
}

impl AlgebraicType {
    pub const ZERO_REF: Self = Self::Ref(AlgebraicTypeRef(0));

    /// Returns whether this type is the conventional identity type.
    pub fn is_identity(&self) -> bool {
        matches!(self, Self::Product(p) if p.is_identity())
    }

    /// Returns whether this type is scalar or a string type.
    pub fn is_scalar_or_string(&self) -> bool {
        self.is_scalar() || self.is_string()
    }

    /// Returns whether this type is one which holds a scalar value.
    ///
    /// A scalar value is one not made up of other values, i.e., not composite.
    /// These are all integer and float values,
    /// i.e., integer and float types are scalar.
    /// References to other types, i.e., [`AlgebraicType::Ref`]s are not scalar.
    pub fn is_scalar(&self) -> bool {
        self.is_bool() || self.is_int() || self.is_float()
    }

    /// Returns whether the type is a signed integer type.
    pub fn is_signed(&self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128)
    }

    /// Returns whether the type is an unsigned integer type.
    pub fn is_unsigned(&self) -> bool {
        matches!(self, Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::U128)
    }

    /// Returns whether the type is an integer type.
    pub fn is_int(&self) -> bool {
        self.is_signed() || self.is_unsigned()
    }

    /// Returns whether the type is a float type.
    pub fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    /// The canonical 0-element unit type.
    pub fn unit() -> Self {
        Self::product([].into())
    }

    /// The canonical 0-variant "never" / "absurd" / "void" type.
    pub fn never() -> Self {
        Self::sum([].into())
    }

    /// A type representing an array of `U8`s.
    pub fn bytes() -> Self {
        Self::array(Self::U8)
    }

    /// Returns whether this type is `AlgebraicType::bytes()`.
    pub fn is_bytes(&self) -> bool {
        matches!(self, AlgebraicType::Array(ArrayType { elem_ty })
            if **elem_ty == AlgebraicType::U8
        )
    }

    /// Returns a sum type with the given `variants`.
    pub const fn sum(variants: SatsVec<SumTypeVariant>) -> Self {
        AlgebraicType::Sum(SumType { variants })
    }

    /// Returns a product type with the given `factors`.
    pub const fn product(factors: SatsVec<ProductTypeElement>) -> Self {
        AlgebraicType::Product(ProductType::new(factors))
    }

    /// Returns a structural option type where `some_type` is the type for the `some` variant.
    pub fn option(some_type: Self) -> Self {
        Self::sum(
            [
                SumTypeVariant::new_named(some_type, "some"),
                SumTypeVariant::unit("none"),
            ]
            .into(),
        )
    }

    /// Returns an unsized array type where the element type is `ty`.
    pub fn array(ty: Self) -> Self {
        AlgebraicType::Array(ArrayType { elem_ty: Box::new(ty) })
    }

    /// Returns a map type from the type `key` to the type `value`.
    pub fn map(key: Self, value: Self) -> Self {
        AlgebraicType::Map(Box::new(MapType::new(key, value)))
    }

    /// Returns a sum type of unit variants with names taken from `var_names`.
    pub fn simple_enum<'a>(var_names: impl Iterator<Item = &'a str>) -> Self {
        let vars = var_names
            .into_iter()
            .map(SumTypeVariant::unit)
            .collect::<SlimSliceBoxCollected<_>>()
            .unwrap();
        Self::sum(vars)
    }

    pub fn as_value(&self) -> AlgebraicValue {
        self.serialize(ValueSerializer).expect("unexpected `len >= u32::MAX`")
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
        assert_eq!("(|)", fmt_algebraic_type(&AlgebraicType::never()).to_string());
    }

    #[test]
    fn never_map() {
        assert_eq!("{ ty_: Sum }", fmt_map(&AlgebraicType::never()).to_string());
    }

    #[test]
    fn unit() {
        assert_eq!("()", fmt_algebraic_type(&AlgebraicType::unit()).to_string());
    }

    #[test]
    fn unit_map() {
        assert_eq!("{ ty_: Product }", fmt_map(&AlgebraicType::unit()).to_string());
    }

    #[test]
    fn primitive() {
        assert_eq!("U8", fmt_algebraic_type(&AlgebraicType::U8).to_string());
    }

    #[test]
    fn primitive_map() {
        assert_eq!("{ ty_: U8 }", fmt_map(&AlgebraicType::U8).to_string());
    }

    #[test]
    fn option() {
        let option = AlgebraicType::option(AlgebraicType::never());
        assert_eq!("(some: (|) | none: ())", fmt_algebraic_type(&option).to_string());
    }

    #[test]
    fn option_map() {
        let option = AlgebraicType::option(AlgebraicType::never());
        assert_eq!(
            "{ ty_: Sum, some: { ty_: Sum }, none: { ty_: Product } }",
            fmt_map(&option).to_string()
        );
    }

    #[test]
    fn algebraic_type() {
        let algebraic_type = AlgebraicType::meta_type();
        assert_eq!(
            "(ref: U32 | sum: (variants: Array<(name: (some: String | none: ()), algebraic_type: &0)>) | product: (elements: Array<(name: (some: String | none: ()), algebraic_type: &0)>) | array: &0 | map: (key_ty: &0, ty: &0) | bool: () | i8: () | u8: () | i16: () | u16: () | i32: () | u32: () | i64: () | u64: () | i128: () | u128: () | f32: () | f64: () | string: ())",
            fmt_algebraic_type(&algebraic_type).to_string()
        );
    }

    #[test]
    fn algebraic_type_map() {
        let algebraic_type = AlgebraicType::meta_type();
        assert_eq!(
            "{ ty_: Sum, ref: { ty_: U32 }, sum: { ty_: Product, variants: { ty_: Array, 0: { ty_: Product, name: { ty_: Sum, some: { ty_: String }, none: { ty_: Product } }, algebraic_type: { ty_: Ref, 0: 0 } } } }, product: { ty_: Product, elements: { ty_: Array, 0: { ty_: Product, name: { ty_: Sum, some: { ty_: String }, none: { ty_: Product } }, algebraic_type: { ty_: Ref, 0: 0 } } } }, array: { ty_: Ref, 0: 0 }, map: { ty_: Product, key_ty: { ty_: Ref, 0: 0 }, ty: { ty_: Ref, 0: 0 } }, bool: { ty_: Product }, i8: { ty_: Product }, u8: { ty_: Product }, i16: { ty_: Product }, u16: { ty_: Product }, i32: { ty_: Product }, u32: { ty_: Product }, i64: { ty_: Product }, u64: { ty_: Product }, i128: { ty_: Product }, u128: { ty_: Product }, f32: { ty_: Product }, f64: { ty_: Product }, string: { ty_: Product } }",
            fmt_map(&algebraic_type).to_string()
        );
    }

    #[test]
    fn nested_products_and_sums() {
        let builtin = AlgebraicType::U8;
        let product = AlgebraicType::product([ProductTypeElement::new_named(AlgebraicType::U8, "thing")].into());
        let next = AlgebraicType::sum([builtin.clone().into(), builtin.clone().into(), product.into()].into());
        let next = AlgebraicType::product(
            [
                ProductTypeElement::new_named(builtin.clone(), "test"),
                next.into(),
                builtin.into(),
                ProductTypeElement::new_named(AlgebraicType::never(), "never"),
            ]
            .into(),
        );
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
        let option = AlgebraicType::option(AlgebraicType::never());
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
        assert_eq!("(u8 = ())", in_space(&typespace, &at_ref, &array.as_value()).to_satn());
    }

    #[test]
    fn algebraic_type_as_value() {
        let algebraic_type = AlgebraicType::meta_type();
        let typespace = Typespace::new(vec![algebraic_type.clone()]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));
        assert_eq!(
            r#"(sum = (variants = [(name = (some = "ref"), algebraic_type = (u32 = ())), (name = (some = "sum"), algebraic_type = (product = (elements = [(name = (some = "variants"), algebraic_type = (array = (product = (elements = [(name = (some = "name"), algebraic_type = (sum = (variants = [(name = (some = "some"), algebraic_type = (string = ())), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))), (name = (some = "algebraic_type"), algebraic_type = (ref = 0))]))))]))), (name = (some = "product"), algebraic_type = (product = (elements = [(name = (some = "elements"), algebraic_type = (array = (product = (elements = [(name = (some = "name"), algebraic_type = (sum = (variants = [(name = (some = "some"), algebraic_type = (string = ())), (name = (some = "none"), algebraic_type = (product = (elements = [])))]))), (name = (some = "algebraic_type"), algebraic_type = (ref = 0))]))))]))), (name = (some = "array"), algebraic_type = (ref = 0)), (name = (some = "map"), algebraic_type = (product = (elements = [(name = (some = "key_ty"), algebraic_type = (ref = 0)), (name = (some = "ty"), algebraic_type = (ref = 0))]))), (name = (some = "bool"), algebraic_type = (product = (elements = []))), (name = (some = "i8"), algebraic_type = (product = (elements = []))), (name = (some = "u8"), algebraic_type = (product = (elements = []))), (name = (some = "i16"), algebraic_type = (product = (elements = []))), (name = (some = "u16"), algebraic_type = (product = (elements = []))), (name = (some = "i32"), algebraic_type = (product = (elements = []))), (name = (some = "u32"), algebraic_type = (product = (elements = []))), (name = (some = "i64"), algebraic_type = (product = (elements = []))), (name = (some = "u64"), algebraic_type = (product = (elements = []))), (name = (some = "i128"), algebraic_type = (product = (elements = []))), (name = (some = "u128"), algebraic_type = (product = (elements = []))), (name = (some = "f32"), algebraic_type = (product = (elements = []))), (name = (some = "f64"), algebraic_type = (product = (elements = []))), (name = (some = "string"), algebraic_type = (product = (elements = [])))]))"#,
            in_space(&typespace, &at_ref, &algebraic_type.as_value()).to_satn()
        );
    }

    #[test]
    fn option_from_value() {
        let option = AlgebraicType::option(AlgebraicType::never());
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
