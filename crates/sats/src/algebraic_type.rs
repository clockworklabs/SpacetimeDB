pub mod fmt;
pub mod map_notation;

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::value_serialize;
use crate::de::Deserialize;
use crate::meta_type::MetaType;
use crate::product_type::{CONNECTION_ID_TAG, IDENTITY_TAG, TIMESTAMP_TAG, TIME_DURATION_TAG};
use crate::sum_type::{OPTION_NONE_TAG, OPTION_SOME_TAG};
use crate::typespace::Typespace;
use crate::{i256, u256};
use crate::{AlgebraicTypeRef, AlgebraicValue, ArrayType, ProductType, SpacetimeType, SumType, SumTypeVariant};
use derive_more::From;
use enum_as_inner::EnumAsInner;

/// The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
/// which a nominal type system can be constructed.
///
/// The type system unifies the concepts sum types, product types, scalar value types,
/// and convenience types strings, arrays, and maps,
/// into a single type system.
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, SpacetimeType, From)]
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
    /// ```text
    /// { N_0(T_0), N_1(T_1), ..., N_n(T_n) }
    /// ```
    /// is:
    /// ```text
    /// Σ (i ∈ 0..n). values(T_i)
    /// ```
    /// so for example, `values({ A(U64), B(Bool) }) = values(U64) + values(Bool)`.
    ///
    /// See also:
    /// - <https://en.wikipedia.org/wiki/Tagged_union>
    /// - <https://ncatlab.org/nlab/show/sum+type>
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
    /// See also:
    /// - <https://en.wikipedia.org/wiki/Record_(computer_science)>
    /// - <https://ncatlab.org/nlab/show/product+type>
    ///
    /// These structures are known as product types because the number of possible values in product
    /// ```text
    /// { N_0: T_0, N_1: T_1, ..., N_n: T_n }
    /// ```
    /// is:
    /// ```text
    /// Π (i ∈ 0..n). values(T_i)
    /// ```
    /// so for example, `values({ A: U64, B: Bool }) = values(U64) * values(Bool)`.
    ///
    /// [structural]: https://en.wikipedia.org/wiki/Structural_type_system
    Product(ProductType),
    /// The type of array values where elements are of a base type `elem_ty`.
    /// Values [`AlgebraicValue::Array(array)`](crate::AlgebraicValue::Array) will have this type.
    Array(ArrayType),
    /// The UTF-8 encoded `String` type.
    /// Values [`AlgebraicValue::String(s)`](crate::AlgebraicValue::String) will have this type.
    ///
    /// This type exists for convenience and because it is easy to just use Rust's `String` (UTF-8)
    /// as opposed to rolling your own equivalent byte-array based UTF-8 encoding.
    String,
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
    /// The `I256` type. Values [`AlgebraicValue::I256(v)`](crate::AlgebraicValue::I256) will have this type.
    I256,
    /// The `U256` type. Values [`AlgebraicValue::U256(v)`](crate::AlgebraicValue::U256) will have this type.
    U256,
    /// The `F32` type. Values [`AlgebraicValue::F32(v)`](crate::AlgebraicValue::F32) will have this type.
    F32,
    /// The `F64` type. Values [`AlgebraicValue::F64(v)`](crate::AlgebraicValue::F64) will have this type.
    F64,
}

impl MetaType for AlgebraicType {
    /// This is a static function that constructs the type of `AlgebraicType`
    /// and returns it as an `AlgebraicType`.
    ///
    /// This could alternatively be implemented
    /// as a regular AlgebraicValue or as a static variable.
    fn meta_type() -> Self {
        AlgebraicType::sum([
            ("ref", AlgebraicTypeRef::meta_type()),
            ("sum", SumType::meta_type()),
            ("product", ProductType::meta_type()),
            ("array", ArrayType::meta_type()),
            ("string", AlgebraicType::unit()),
            ("bool", AlgebraicType::unit()),
            ("i8", AlgebraicType::unit()),
            ("u8", AlgebraicType::unit()),
            ("i16", AlgebraicType::unit()),
            ("u16", AlgebraicType::unit()),
            ("i32", AlgebraicType::unit()),
            ("u32", AlgebraicType::unit()),
            ("i64", AlgebraicType::unit()),
            ("u64", AlgebraicType::unit()),
            ("i128", AlgebraicType::unit()),
            ("u128", AlgebraicType::unit()),
            ("i256", AlgebraicType::unit()),
            ("u256", AlgebraicType::unit()),
            ("f32", AlgebraicType::unit()),
            ("f64", AlgebraicType::unit()),
        ])
    }
}

/// Provided to enable `mem::take`.
impl Default for AlgebraicType {
    fn default() -> Self {
        Self::ZERO_REF
    }
}

impl AlgebraicType {
    /// The first type in the typespace.
    pub const ZERO_REF: Self = Self::Ref(AlgebraicTypeRef(0));

    /// Returns whether this type is the `ConnectionId` type.
    ///
    /// Construct an instance of this type with [`Self::connection_id`]
    pub fn is_connection_id(&self) -> bool {
        matches!(self, Self::Product(p) if p.is_connection_id())
    }

    /// Returns whether this type is the conventional identity type.
    pub fn is_identity(&self) -> bool {
        matches!(self, Self::Product(p) if p.is_identity())
    }

    /// Returns whether this type is the conventional point-in-time `Timestamp` type.
    pub fn is_timestamp(&self) -> bool {
        matches!(self, Self::Product(p) if p.is_timestamp())
    }

    /// Returns whether this type is the conventional time-delta `TimeDuration` type.
    pub fn is_time_duration(&self) -> bool {
        matches!(self, Self::Product(p) if p.is_time_duration())
    }

    /// Returns whether this type is the conventional `ScheduleAt` type.
    pub fn is_schedule_at(&self) -> bool {
        matches!(self, Self::Sum(p) if p.is_schedule_at())
    }

    /// Returns whether this type is a unit type.
    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Product(p) if p.is_unit())
    }

    /// Returns whether this type is a never type.
    pub fn is_never(&self) -> bool {
        matches!(self, Self::Sum(p) if p.is_empty())
    }

    /// Returns whether this type is an option type.
    pub fn is_option(&self) -> bool {
        matches!(self, Self::Sum(p) if p.is_option())
    }

    /// If this type is the standard option type, returns the type of the `some` variant.
    /// Otherwise, returns `None`.
    pub fn as_option(&self) -> Option<&AlgebraicType> {
        self.as_sum()?.as_option()
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
        self.is_bool() || self.is_integer() || self.is_float()
    }

    /// Returns whether the type is a signed integer type.
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::I256
        )
    }

    /// Returns whether the type is an unsigned integer type.
    pub fn is_unsigned(&self) -> bool {
        matches!(
            self,
            Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::U128 | Self::U256
        )
    }

    /// Returns whether this type is one of the integer types, e.g., `U64` and `I32`.
    pub fn is_integer(&self) -> bool {
        self.is_signed() || self.is_unsigned()
    }

    /// Returns whether the type is a float type.
    pub fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    /// The canonical 0-element unit type.
    pub fn unit() -> Self {
        let fs: [AlgebraicType; 0] = [];
        Self::product(fs)
    }

    /// The canonical 0-variant "never" / "absurd" / "void" type.
    pub fn never() -> Self {
        let vs: [SumTypeVariant; 0] = [];
        Self::sum(vs)
    }

    /// A type representing an array of `U8`s.
    pub fn bytes() -> Self {
        Self::array(Self::U8)
    }

    /// Returns whether this type is `AlgebraicType::bytes()`.
    pub fn is_bytes(&self) -> bool {
        self.as_array().is_some_and(|ty| ty.elem_ty.is_u8())
    }

    /// Whether this type, or the types it references, contain any `AlgebraicTypeRef`s.
    pub fn contains_refs(&self) -> bool {
        match self {
            AlgebraicType::Ref(_) => true,
            AlgebraicType::Product(ProductType { elements }) => {
                elements.iter().any(|elem| elem.algebraic_type.contains_refs())
            }
            AlgebraicType::Sum(SumType { variants }) => {
                variants.iter().any(|variant| variant.algebraic_type.contains_refs())
            }
            AlgebraicType::Array(array) => array.elem_ty.contains_refs(),
            _ => false,
        }
    }

    /// Returns a sum type with the given `sum`.
    pub fn sum<S: Into<SumType>>(sum: S) -> Self {
        AlgebraicType::Sum(sum.into())
    }

    /// Returns a product type with the given `prod`.
    pub fn product<P: Into<ProductType>>(prod: P) -> Self {
        AlgebraicType::Product(prod.into())
    }

    /// Returns a structural option type where `some_type` is the type for the `some` variant.
    pub fn option(some_type: Self) -> Self {
        Self::sum([(OPTION_SOME_TAG, some_type), (OPTION_NONE_TAG, AlgebraicType::unit())])
    }

    /// Returns an unsized array type where the element type is `ty`.
    pub fn array(ty: Self) -> Self {
        ArrayType { elem_ty: Box::new(ty) }.into()
    }

    /// Construct a copy of the `Identity` type.
    pub fn identity() -> Self {
        AlgebraicType::product([(IDENTITY_TAG, AlgebraicType::U256)])
    }

    /// Construct a copy of the `ConnectionId` type.
    pub fn connection_id() -> Self {
        AlgebraicType::product([(CONNECTION_ID_TAG, AlgebraicType::U128)])
    }

    /// Construct a copy of the point-in-time `Timestamp` type.
    pub fn timestamp() -> Self {
        AlgebraicType::product([(TIMESTAMP_TAG, AlgebraicType::I64)])
    }

    /// Construct a copy of the time-delta `TimeDuration` type.
    pub fn time_duration() -> Self {
        AlgebraicType::product([(TIME_DURATION_TAG, AlgebraicType::I64)])
    }

    /// Returns a sum type of unit variants with names taken from `var_names`.
    pub fn simple_enum<'a>(var_names: impl Iterator<Item = &'a str>) -> Self {
        Self::sum(var_names.into_iter().map(SumTypeVariant::unit).collect::<Box<[_]>>())
    }

    pub fn as_value(&self) -> AlgebraicValue {
        value_serialize(self)
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<Self, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }

    #[inline]
    /// Given an AlgebraicType, returns the min value for that type.
    pub fn min_value(&self) -> Option<AlgebraicValue> {
        match *self {
            Self::I8 => Some(i8::MIN.into()),
            Self::U8 => Some(u8::MIN.into()),
            Self::I16 => Some(i16::MIN.into()),
            Self::U16 => Some(u16::MIN.into()),
            Self::I32 => Some(i32::MIN.into()),
            Self::U32 => Some(u32::MIN.into()),
            Self::I64 => Some(i64::MIN.into()),
            Self::U64 => Some(u64::MIN.into()),
            Self::I128 => Some(i128::MIN.into()),
            Self::U128 => Some(u128::MIN.into()),
            Self::I256 => Some(i256::MIN.into()),
            Self::U256 => Some(u256::MIN.into()),
            Self::F32 => Some(f32::MIN.into()),
            Self::F64 => Some(f64::MIN.into()),
            _ => None,
        }
    }

    #[inline]
    /// Given an AlgebraicType, returns the max value for that type.
    pub fn max_value(&self) -> Option<AlgebraicValue> {
        match *self {
            Self::I8 => Some(i8::MAX.into()),
            Self::U8 => Some(u8::MAX.into()),
            Self::I16 => Some(i16::MAX.into()),
            Self::U16 => Some(u16::MAX.into()),
            Self::I32 => Some(i32::MAX.into()),
            Self::U32 => Some(u32::MAX.into()),
            Self::I64 => Some(i64::MAX.into()),
            Self::U64 => Some(u64::MAX.into()),
            Self::I128 => Some(i128::MAX.into()),
            Self::U128 => Some(u128::MAX.into()),
            Self::I256 => Some(i256::MAX.into()),
            Self::U256 => Some(u256::MAX.into()),
            Self::F32 => Some(f32::MAX.into()),
            Self::F64 => Some(f64::MAX.into()),
            _ => None,
        }
    }

    /// Check if the type is one of a small number of special, known types
    /// with specific layouts.
    /// See also [`ProductType::is_special`] and [`SumType::is_special`].
    pub fn is_special(&self) -> bool {
        match self {
            AlgebraicType::Product(product) => product.is_special(),
            AlgebraicType::Sum(sum) => sum.is_special(),
            _ => false,
        }
    }

    /// Validates that the type can be used to generate a type definition
    /// in a `SpacetimeDB` client module.
    ///
    /// Such a type must be a non-special sum or product type.
    /// All of the elements of the type must satisfy [`AlgebraicType::is_valid_for_client_type_use`].
    ///
    /// This method does not actually follow `Ref`s to check the types they point to,
    /// it only checks the structure of this type.
    pub fn is_valid_for_client_type_definition(&self) -> bool {
        // Special types should not be used to generate type definitions.
        if self.is_special() {
            return false;
        }
        match self {
            AlgebraicType::Sum(sum) => sum
                .variants
                .iter()
                .all(|variant| variant.algebraic_type.is_valid_for_client_type_use()),
            AlgebraicType::Product(product) => product
                .elements
                .iter()
                .all(|elem| elem.algebraic_type.is_valid_for_client_type_use()),
            _ => false,
        }
    }

    /// Validates that the type can be used to generate a *use* of a type in a `SpacetimeDB` client module.
    /// (As opposed to a *definition* of a type.)
    ///
    /// This means that the type is either:
    /// - a reference
    /// - a special, known type
    /// - a non-compound type like `U8`, `I32`, `F64`, etc.
    /// - or a map, array, or option built from types that satisfy [`AlgebraicType::is_valid_for_client_type_use`]
    ///
    /// This method does not actually follow `Ref`s to check the types they point to,
    /// it only checks the structure of the type.
    pub fn is_valid_for_client_type_use(&self) -> bool {
        match self {
            AlgebraicType::Sum(sum) => {
                if let Some(wrapped) = sum.as_option() {
                    wrapped.is_valid_for_client_type_use()
                } else {
                    sum.is_special() || sum.is_empty()
                }
            }
            AlgebraicType::Product(product) => product.is_special() || product.is_unit(),
            AlgebraicType::Array(array) => array.elem_ty.is_valid_for_client_type_use(),
            AlgebraicType::Ref(_) => true,
            _ => true,
        }
    }

    pub fn type_check(&self, value: &AlgebraicValue, typespace: &Typespace) -> bool {
        match (self, value) {
            (_, AlgebraicValue::Min | AlgebraicValue::Max) => true,
            (AlgebraicType::Ref(r), _) => {
                if let Some(resolved_ty) = typespace.get(*r) {
                    resolved_ty.type_check(value, typespace)
                } else {
                    false
                }
            }
            (AlgebraicType::Sum(sum_ty), AlgebraicValue::Sum(sv)) => sum_ty.type_check(sv, typespace),
            (AlgebraicType::Product(product_ty), AlgebraicValue::Product(pv)) => product_ty.type_check(pv, typespace),
            (AlgebraicType::Array(array_ty), AlgebraicValue::Array(arr)) => array_ty.type_check(arr, typespace),

            (AlgebraicType::String, AlgebraicValue::String(_))
            | (AlgebraicType::Bool, AlgebraicValue::Bool(_))
            | (AlgebraicType::I8, AlgebraicValue::I8(_))
            | (AlgebraicType::U8, AlgebraicValue::U8(_))
            | (AlgebraicType::I16, AlgebraicValue::I16(_))
            | (AlgebraicType::U16, AlgebraicValue::U16(_))
            | (AlgebraicType::I32, AlgebraicValue::I32(_))
            | (AlgebraicType::U32, AlgebraicValue::U32(_))
            | (AlgebraicType::I64, AlgebraicValue::I64(_))
            | (AlgebraicType::U64, AlgebraicValue::U64(_))
            | (AlgebraicType::I128, AlgebraicValue::I128(_))
            | (AlgebraicType::U128, AlgebraicValue::U128(_))
            | (AlgebraicType::I256, AlgebraicValue::I256(_))
            | (AlgebraicType::U256, AlgebraicValue::U256(_))
            | (AlgebraicType::F32, AlgebraicValue::F32(_))
            | (AlgebraicType::F64, AlgebraicValue::F64(_)) => true,
            _ => false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::AlgebraicType;
    use crate::meta_type::MetaType;
    use crate::satn::Satn;
    use crate::{
        algebraic_type::fmt::fmt_algebraic_type, algebraic_type::map_notation::fmt_algebraic_type as fmt_map,
        algebraic_type_ref::AlgebraicTypeRef, typespace::Typespace,
    };
    use crate::{product, AlgebraicValue, ValueWithType, WithTypespace};

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
            "(\
                ref: U32 \
                | sum: (variants: Array<(\
                    name: (some: String | none: ()), \
                    algebraic_type: &0\
                )>) \
                | product: (elements: Array<(\
                    name: (some: String | none: ()), \
                    algebraic_type: &0\
                )>) \
                | array: &0 \
                | string: () \
                | bool: () \
                | i8: () | u8: () \
                | i16: () | u16: () \
                | i32: () | u32: () \
                | i64: () | u64: () \
                | i128: () | u128: () \
                | i256: () | u256: () \
                | f32: () | f64: ()\
            )",
            fmt_algebraic_type(&algebraic_type).to_string()
        );
    }

    #[test]
    fn algebraic_type_map() {
        let algebraic_type = AlgebraicType::meta_type();
        assert_eq!(
            "{ \
                ty_: Sum, \
                ref: { ty_: U32 }, \
                sum: { \
                    ty_: Product, \
                    variants: { \
                        ty_: Array, \
                        0: { \
                            ty_: Product, \
                            name: { ty_: Sum, some: { ty_: String }, none: { ty_: Product } }, \
                            algebraic_type: { ty_: Ref, 0: 0 } \
                        } \
                    } \
                }, \
                product: { \
                    ty_: Product, \
                    elements: { \
                        ty_: Array, \
                        0: { \
                            ty_: Product, \
                            name: { ty_: Sum, some: { ty_: String }, none: { ty_: Product } }, \
                            algebraic_type: { ty_: Ref, 0: 0 } \
                        } \
                    } \
                }, \
                array: { ty_: Ref, 0: 0 }, \
                string: { ty_: Product }, \
                bool: { ty_: Product }, \
                i8: { ty_: Product }, u8: { ty_: Product }, \
                i16: { ty_: Product }, u16: { ty_: Product }, \
                i32: { ty_: Product }, u32: { ty_: Product }, \
                i64: { ty_: Product }, u64: { ty_: Product }, \
                i128: { ty_: Product }, u128: { ty_: Product }, \
                i256: { ty_: Product }, u256: { ty_: Product }, \
                f32: { ty_: Product }, f64: { ty_: Product } \
            }",
            fmt_map(&algebraic_type).to_string()
        );
    }

    #[test]
    fn nested_products_and_sums() {
        let builtin = AlgebraicType::U8;
        let product = AlgebraicType::product([("thing", AlgebraicType::U8)]);
        let sum = AlgebraicType::sum([builtin.clone(), builtin.clone(), product]);
        let next = AlgebraicType::product([
            (Some("test"), builtin.clone()),
            (None, sum),
            (None, builtin),
            (Some("never"), AlgebraicType::never()),
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
    fn algebraic_type_as_value() {
        let algebraic_type = AlgebraicType::meta_type();
        let typespace = Typespace::new(vec![algebraic_type.clone()]);
        let at_ref = AlgebraicType::Ref(AlgebraicTypeRef(0));

        let ref0 = "algebraic_type = (ref = 0)";
        let unit = "algebraic_type = (product = (elements = []))";
        let aggr_elems_ty = format!(
            "algebraic_type = (array = (product = (elements = [\
                (\
                    name = (some = \"name\"), \
                    algebraic_type = (sum = (variants = [\
                        (name = (some = \"some\"), algebraic_type = (string = ())), \
                        (name = (some = \"none\"), {unit})\
                    ]))\
                ), \
                (name = (some = \"algebraic_type\"), {ref0})\
            ])))"
        );

        assert_eq!(
            format!(
                "(\
                sum = (\
                    variants = [\
                        (name = (some = \"ref\"), algebraic_type = (u32 = ())), \
                        (\
                            name = (some = \"sum\"), \
                            algebraic_type = (product = (elements = [\
                                (name = (some = \"variants\"), {aggr_elems_ty})\
                            ]))\
                        ), \
                        (\
                            name = (some = \"product\"), \
                            algebraic_type = (product = (elements = [\
                                (name = (some = \"elements\"), {aggr_elems_ty})\
                            ]))\
                        ), \
                        (name = (some = \"array\"), {ref0}), \
                        (name = (some = \"string\"), {unit}), \
                        (name = (some = \"bool\"), {unit}), \
                        (name = (some = \"i8\"), {unit}), \
                        (name = (some = \"u8\"), {unit}), \
                        (name = (some = \"i16\"), {unit}), \
                        (name = (some = \"u16\"), {unit}), \
                        (name = (some = \"i32\"), {unit}), \
                        (name = (some = \"u32\"), {unit}), \
                        (name = (some = \"i64\"), {unit}), \
                        (name = (some = \"u64\"), {unit}), \
                        (name = (some = \"i128\"), {unit}), \
                        (name = (some = \"u128\"), {unit}), \
                        (name = (some = \"i256\"), {unit}), \
                        (name = (some = \"u256\"), {unit}), \
                        (name = (some = \"f32\"), {unit}), \
                        (name = (some = \"f64\"), {unit})\
                    ]\
                )\
            )"
            ),
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

    #[test]
    fn special_types_are_special() {
        assert!(AlgebraicType::identity().is_identity());
        assert!(AlgebraicType::identity().is_special());
        assert!(AlgebraicType::connection_id().is_connection_id());
        assert!(AlgebraicType::connection_id().is_special());
        assert!(AlgebraicType::timestamp().is_timestamp());
        assert!(AlgebraicType::timestamp().is_special());
        assert!(AlgebraicType::time_duration().is_special());
        assert!(AlgebraicType::time_duration().is_time_duration());
    }

    #[test]
    fn type_check() {
        let av = AlgebraicValue::sum(1, AlgebraicValue::from(product![0u16, 1u32]));
        let at = AlgebraicType::sum([
            ("a", AlgebraicType::U8),
            ("b", AlgebraicType::product([AlgebraicType::U16, AlgebraicType::U32])),
        ]);

        at.type_check(&av, Typespace::EMPTY);
    }
}
