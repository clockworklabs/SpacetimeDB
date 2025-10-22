use super::{
    BasicSmallVecVisitor, BasicVecVisitor, Deserialize, DeserializeSeed, Deserializer, Error, FieldNameVisitor,
    ProductKind, ProductVisitor, SeqProductAccess, SliceVisitor, SumAccess, SumVisitor, VariantAccess, VariantVisitor,
};
use crate::{
    de::{array_visit, ArrayAccess, ArrayVisitor, GrowingVec},
    AlgebraicType, AlgebraicValue, ArrayType, ArrayValue, ProductType, ProductTypeElement, ProductValue, SumType,
    SumValue, WithTypespace, F32, F64,
};
use crate::{i256, u256};
use core::{iter, marker::PhantomData, ops::Bound};
use smallvec::SmallVec;
use spacetimedb_primitives::{ColId, ColList};
use std::{borrow::Cow, rc::Rc, sync::Arc};

/// Implements [`Deserialize`] for a type in a simplified manner.
///
/// An example:
/// ```ignore
/// impl_deserialize!(
/// //     Type parameters  Optional where  Impl type
/// //            v               v             v
/// //   ----------------  --------------- ----------
///     [T: Deserialize<'de>] where [T: Copy] std::rc::Rc<T>,
/// //  The `deserialize` implementation where `de` is the `Deserializer<'de>`
/// //  and the expression right of `=>` is the body of `deserialize`.
///     de => T::deserialize(de).map(std::rc::Rc::new)
/// );
/// ```
#[macro_export]
macro_rules! impl_deserialize {
    ([$($generics:tt)*] $(where [$($wc:tt)*])? $typ:ty, $de:ident => $body:expr) => {
        impl<'de, $($generics)*> $crate::de::Deserialize<'de> for $typ {
            fn deserialize<D: $crate::de::Deserializer<'de>>($de: D) -> Result<Self, D::Error> { $body }
        }
    };
}

/// Implements [`Deserialize`] for a primitive type.
///
/// The `$method` is a parameterless method on `deserializer` to call.
macro_rules! impl_prim {
    ($(($prim:ty, $method:ident))*) => {
        $(impl_deserialize!([] $prim, de => de.$method());)*
    };
}

impl_prim! {
    (bool, deserialize_bool)
    /*(u8, deserialize_u8)*/ (u16, deserialize_u16) (u32, deserialize_u32) (u64, deserialize_u64) (u128, deserialize_u128) (u256, deserialize_u256)
    (i8, deserialize_i8)     (i16, deserialize_i16) (i32, deserialize_i32) (i64, deserialize_i64) (i128, deserialize_i128) (i256, deserialize_i256)
    (f32, deserialize_f32) (f64, deserialize_f64)
}

struct TupleVisitor<A>(PhantomData<A>);
#[derive(Copy, Clone)]
struct TupleNameVisitorMax(usize);

impl FieldNameVisitor<'_> for TupleNameVisitorMax {
    // The index of the field name.
    type Output = usize;

    fn field_names(&self) -> impl '_ + Iterator<Item = Option<&str>> {
        iter::repeat_n(None, self.0)
    }

    fn kind(&self) -> ProductKind {
        ProductKind::Normal
    }

    fn visit<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        let err = || Error::unknown_field_name(name, &self);
        // Convert `name` to an index.
        let Ok(index) = name.parse() else {
            return Err(err());
        };
        // Confirm that the index exists or error.
        if index < self.0 {
            Ok(index)
        } else {
            Err(err())
        }
    }

    fn visit_seq(self, index: usize) -> Self::Output {
        // Assert that the index exists.
        assert!(index < self.0);
        index
    }
}

macro_rules! impl_deserialize_tuple {
    ($($ty_name:ident => $const_val:literal),*) => {
        impl<'de, $($ty_name: Deserialize<'de>),*> ProductVisitor<'de> for TupleVisitor<($($ty_name,)*)> {
            type Output = ($($ty_name,)*);
            fn product_name(&self) -> Option<&str> { None }
            fn product_len(&self) -> usize { crate::count!($($ty_name)*) }
            fn visit_seq_product<A: SeqProductAccess<'de>>(self, mut _prod: A) -> Result<Self::Output, A::Error> {
                $(
                    #[allow(non_snake_case)]
                    let $ty_name = _prod
                        .next_element()?
                        .ok_or_else(|| Error::invalid_product_length($const_val, &self))?;
                )*

                Ok(($($ty_name,)*))
            }
            fn visit_named_product<A: super::NamedProductAccess<'de>>(self, mut prod: A) -> Result<Self::Output, A::Error> {
                $(
                    #[allow(non_snake_case)]
                    let mut $ty_name = None;
                )*

                let visit = TupleNameVisitorMax(self.product_len());
                while let Some(index) = prod.get_field_ident(visit)? {
                    match index {
                        $($const_val => {
                            if $ty_name.is_some() {
                                return Err(A::Error::duplicate_field($const_val, None, &self))
                            }
                            $ty_name = Some(prod.get_field_value()?);
                        })*
                        index => return Err(Error::invalid_product_length(index, &self)),
                    }
                }
                Ok(($(
                    $ty_name.ok_or_else(|| A::Error::missing_field($const_val, None, &self))?,
                )*))
            }
        }

        impl_deserialize!([$($ty_name: Deserialize<'de>),*] ($($ty_name,)*), de => {
            de.deserialize_product(TupleVisitor::<($($ty_name,)*)>(PhantomData))
        });
    };
}

impl_deserialize_tuple!();
impl_deserialize_tuple!(T0 => 0);
impl_deserialize_tuple!(T0 => 0, T1 => 1);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5, T6 => 6);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5, T6 => 6, T7 => 7);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5, T6 => 6, T7 => 7, T8 => 8);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5, T6 => 6, T7 => 7, T8 => 8, T9 => 9);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5, T6 => 6, T7 => 7, T8 => 8, T9 => 9, T10 => 10);
impl_deserialize_tuple!(T0 => 0, T1 => 1, T2 => 2, T3 => 3, T4 => 4, T5 => 5, T6 => 6, T7 => 7, T8 => 8, T9 => 9, T10 => 10, T11 => 11);

impl<'de> Deserialize<'de> for u8 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_u8()
    }

    // Specialize `Vec<u8>` deserialization.
    // This is more likely to compile down to a `memcpy`.
    fn __deserialize_vec<D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Self>, D::Error> {
        deserializer.deserialize_bytes(OwnedSliceVisitor)
    }

    fn __deserialize_array<D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[Self; N], D::Error> {
        deserializer.deserialize_bytes(ByteArrayVisitor)
    }
}

impl_deserialize!([] F32, de => f32::deserialize(de).map(Into::into));
impl_deserialize!([] F64, de => f64::deserialize(de).map(Into::into));
impl_deserialize!([] String, de => de.deserialize_str(OwnedSliceVisitor));
impl_deserialize!([T: Deserialize<'de>] Vec<T>, de => T::__deserialize_vec(de));
impl_deserialize!([T: Deserialize<'de>, const N: usize] SmallVec<[T; N]>, de => {
    de.deserialize_array(BasicSmallVecVisitor)
});
impl_deserialize!([T: Deserialize<'de>, const N: usize] [T; N], de => T::__deserialize_array(de));
impl_deserialize!([] Box<str>, de => String::deserialize(de).map(|s| s.into_boxed_str()));
impl_deserialize!([T: Deserialize<'de>] Box<[T]>, de => Vec::deserialize(de).map(|s| s.into_boxed_slice()));
impl_deserialize!([T: Deserialize<'de>] Rc<[T]>, de => Vec::deserialize(de).map(|s| s.into()));
impl_deserialize!([T: Deserialize<'de>] Arc<[T]>, de => Vec::deserialize(de).map(|s| s.into()));

/// The visitor converts the slice to its owned version.
struct OwnedSliceVisitor;

impl<T: ToOwned + ?Sized> SliceVisitor<'_, T> for OwnedSliceVisitor {
    type Output = T::Owned;

    fn visit<E: Error>(self, slice: &T) -> Result<Self::Output, E> {
        Ok(slice.to_owned())
    }

    fn visit_owned<E: Error>(self, buf: T::Owned) -> Result<Self::Output, E> {
        Ok(buf)
    }
}

/// The visitor will convert the byte slice to `[u8; N]`.
///
/// When `slice.len() != N` an error will be raised.
struct ByteArrayVisitor<const N: usize>;

impl<const N: usize> SliceVisitor<'_, [u8]> for ByteArrayVisitor<N> {
    type Output = [u8; N];

    fn visit<E: Error>(self, slice: &[u8]) -> Result<Self::Output, E> {
        slice.try_into().map_err(|_| {
            Error::custom(if slice.len() > N {
                "too many elements for array"
            } else {
                "too few elements for array"
            })
        })
    }
}

impl_deserialize!([] &'de str, de => de.deserialize_str(BorrowedSliceVisitor));
impl_deserialize!([] &'de [u8], de => de.deserialize_bytes(BorrowedSliceVisitor));

/// The visitor returns the slice as-is and borrowed.
pub(crate) struct BorrowedSliceVisitor;

impl<'de, T: ToOwned + ?Sized + 'de> SliceVisitor<'de, T> for BorrowedSliceVisitor {
    type Output = &'de T;

    fn visit<E: Error>(self, _: &T) -> Result<Self::Output, E> {
        Err(E::custom("expected *borrowed* slice"))
    }

    fn visit_borrowed<E: Error>(self, borrowed_slice: &'de T) -> Result<Self::Output, E> {
        Ok(borrowed_slice)
    }
}

impl_deserialize!([] Cow<'de, str>, de => de.deserialize_str(CowSliceVisitor));
impl_deserialize!([] Cow<'de, [u8]>, de => de.deserialize_bytes(CowSliceVisitor));

/// The visitor works with either owned or borrowed versions to produce `Cow<'de, T>`.
struct CowSliceVisitor;

impl<'de, T: ToOwned + ?Sized + 'de> SliceVisitor<'de, T> for CowSliceVisitor {
    type Output = Cow<'de, T>;

    fn visit<E: Error>(self, slice: &T) -> Result<Self::Output, E> {
        self.visit_owned(slice.to_owned())
    }

    fn visit_owned<E: Error>(self, buf: <T as ToOwned>::Owned) -> Result<Self::Output, E> {
        Ok(Cow::Owned(buf))
    }

    fn visit_borrowed<E: Error>(self, borrowed_slice: &'de T) -> Result<Self::Output, E> {
        Ok(Cow::Borrowed(borrowed_slice))
    }
}

impl_deserialize!([T: Deserialize<'de>] Box<T>, de => T::deserialize(de).map(Box::new));
impl_deserialize!([T: Deserialize<'de>] Option<T>, de => de.deserialize_sum(OptionVisitor(PhantomData)));

/// The visitor deserializes an `Option<T>`.
struct OptionVisitor<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de>> SumVisitor<'de> for OptionVisitor<T> {
    type Output = Option<T>;

    fn sum_name(&self) -> Option<&str> {
        Some("option")
    }

    fn is_option(&self) -> bool {
        true
    }

    fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error> {
        // Determine the variant.
        let (some, data) = data.variant(self)?;

        // Deserialize contents for it.
        Ok(if some {
            Some(data.deserialize()?)
        } else {
            data.deserialize::<()>()?;
            None
        })
    }
}

impl<'de, T: Deserialize<'de>> VariantVisitor<'de> for OptionVisitor<T> {
    type Output = bool;

    fn variant_names(&self) -> impl '_ + Iterator<Item = &str> {
        ["some", "none"].into_iter()
    }

    fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E> {
        match tag {
            0 => Ok(true),
            1 => Ok(false),
            _ => Err(E::unknown_variant_tag(tag, &self)),
        }
    }

    fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        match name {
            "some" => Ok(true),
            "none" => Ok(false),
            _ => Err(E::unknown_variant_name(name, &self)),
        }
    }
}

impl_deserialize!([T: Deserialize<'de>, E: Deserialize<'de>] Result<T, E>, de =>
    de.deserialize_sum(ResultVisitor(PhantomData))
);

/// Visitor to deserialize a `Result<T, E>`.
struct ResultVisitor<T, E>(PhantomData<(T, E)>);

/// Variant determined by the [`VariantVisitor`] for `Result<T, E>`.
enum ResultVariant {
    Ok,
    Err,
}

impl<'de, T: Deserialize<'de>, E: Deserialize<'de>> SumVisitor<'de> for ResultVisitor<T, E> {
    type Output = Result<T, E>;

    fn sum_name(&self) -> Option<&str> {
        Some("result")
    }

    fn is_option(&self) -> bool {
        false
    }

    fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error> {
        let (variant, data) = data.variant(self)?;
        Ok(match variant {
            ResultVariant::Ok => Ok(data.deserialize()?),
            ResultVariant::Err => Err(data.deserialize()?),
        })
    }
}

impl<'de, T: Deserialize<'de>, U: Deserialize<'de>> VariantVisitor<'de> for ResultVisitor<T, U> {
    type Output = ResultVariant;

    fn variant_names(&self) -> impl '_ + Iterator<Item = &str> {
        ["ok", "err"].into_iter()
    }

    fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E> {
        match tag {
            0 => Ok(ResultVariant::Ok),
            1 => Ok(ResultVariant::Err),
            _ => Err(E::unknown_variant_tag(tag, &self)),
        }
    }

    fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        match name {
            "ok" => Ok(ResultVariant::Ok),
            "err" => Ok(ResultVariant::Err),
            _ => Err(E::unknown_variant_name(name, &self)),
        }
    }
}

/// The visitor deserializes a `Bound<T>`.
#[derive(Clone, Copy)]
pub struct WithBound<S>(pub S);

impl<'de, S: Copy + DeserializeSeed<'de>> DeserializeSeed<'de> for WithBound<S> {
    type Output = Bound<S::Output>;

    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<Self::Output, D::Error> {
        de.deserialize_sum(BoundVisitor(self.0))
    }
}

/// The visitor deserializes a `Bound<T>`.
struct BoundVisitor<S>(S);

/// Variant determined by the [`BoundVisitor`] for `Bound<T>`.
enum BoundVariant {
    Included,
    Excluded,
    Unbounded,
}

impl<'de, S: Copy + DeserializeSeed<'de>> SumVisitor<'de> for BoundVisitor<S> {
    type Output = Bound<S::Output>;

    fn sum_name(&self) -> Option<&str> {
        Some("bound")
    }

    fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error> {
        // Determine the variant.
        let this = self.0;
        let (variant, data) = data.variant(self)?;

        // Deserialize contents for it.
        match variant {
            BoundVariant::Included => data.deserialize_seed(this).map(Bound::Included),
            BoundVariant::Excluded => data.deserialize_seed(this).map(Bound::Excluded),
            BoundVariant::Unbounded => data.deserialize::<()>().map(|_| Bound::Unbounded),
        }
    }
}

impl<'de, T: Copy + DeserializeSeed<'de>> VariantVisitor<'de> for BoundVisitor<T> {
    type Output = BoundVariant;

    fn variant_names(&self) -> impl '_ + Iterator<Item = &str> {
        ["included", "excluded", "unbounded"].into_iter()
    }

    fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E> {
        match tag {
            0 => Ok(BoundVariant::Included),
            1 => Ok(BoundVariant::Excluded),
            // if this ever changes, edit crates/bindings/src/table.rs
            2 => Ok(BoundVariant::Unbounded),
            _ => Err(E::unknown_variant_tag(tag, &self)),
        }
    }

    fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        match name {
            "included" => Ok(BoundVariant::Included),
            "excluded" => Ok(BoundVariant::Excluded),
            "unbounded" => Ok(BoundVariant::Unbounded),
            _ => Err(E::unknown_variant_name(name, &self)),
        }
    }
}

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, AlgebraicType> {
    type Output = AlgebraicValue;

    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<Self::Output, D::Error> {
        match self.ty() {
            AlgebraicType::Ref(r) => self.resolve(*r).deserialize(de),
            AlgebraicType::Sum(sum) => self.with(sum).deserialize(de).map(Into::into),
            AlgebraicType::Product(prod) => self.with(prod).deserialize(de).map(Into::into),
            AlgebraicType::Array(ty) => self.with(ty).deserialize(de).map(Into::into),
            AlgebraicType::Bool => bool::deserialize(de).map(Into::into),
            AlgebraicType::I8 => i8::deserialize(de).map(Into::into),
            AlgebraicType::U8 => u8::deserialize(de).map(Into::into),
            AlgebraicType::I16 => i16::deserialize(de).map(Into::into),
            AlgebraicType::U16 => u16::deserialize(de).map(Into::into),
            AlgebraicType::I32 => i32::deserialize(de).map(Into::into),
            AlgebraicType::U32 => u32::deserialize(de).map(Into::into),
            AlgebraicType::I64 => i64::deserialize(de).map(Into::into),
            AlgebraicType::U64 => u64::deserialize(de).map(Into::into),
            AlgebraicType::I128 => i128::deserialize(de).map(Into::into),
            AlgebraicType::U128 => u128::deserialize(de).map(Into::into),
            AlgebraicType::I256 => i256::deserialize(de).map(Into::into),
            AlgebraicType::U256 => u256::deserialize(de).map(Into::into),
            AlgebraicType::F32 => f32::deserialize(de).map(Into::into),
            AlgebraicType::F64 => f64::deserialize(de).map(Into::into),
            AlgebraicType::String => <Box<str>>::deserialize(de).map(Into::into),
        }
    }
}

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, SumType> {
    type Output = SumValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_sum(self)
    }
}

impl<'de> SumVisitor<'de> for WithTypespace<'_, SumType> {
    type Output = SumValue;

    fn sum_name(&self) -> Option<&str> {
        None
    }

    fn is_option(&self) -> bool {
        self.ty().as_option().is_some()
    }

    fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error> {
        let (tag, data) = data.variant(self)?;
        // Find the variant type by `tag`.
        let variant_ty = self.map(|ty| &ty.variants[tag as usize].algebraic_type);

        let value = Box::new(data.deserialize_seed(variant_ty)?);
        Ok(SumValue { tag, value })
    }
}

impl VariantVisitor<'_> for WithTypespace<'_, SumType> {
    type Output = u8;

    fn variant_names(&self) -> impl '_ + Iterator<Item = &str> {
        // Provide the names known from the `SumType`.
        self.ty().variants.iter().filter_map(|v| v.name())
    }

    fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E> {
        // Verify that tag identifies a valid variant in `SumType`.
        self.ty()
            .variants
            .get(tag as usize)
            .ok_or_else(|| E::unknown_variant_tag(tag, &self))?;

        Ok(tag)
    }

    fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        // Translate the variant `name` to its tag.
        self.ty()
            .variants
            .iter()
            .position(|var| var.has_name(name))
            .map(|pos| pos as u8)
            .ok_or_else(|| E::unknown_variant_name(name, &self))
    }
}

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, ProductType> {
    type Output = ProductValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self.map(|pt| &*pt.elements))
    }
}

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, [ProductTypeElement]> {
    type Output = ProductValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self)
    }
}

impl<'de> ProductVisitor<'de> for WithTypespace<'_, [ProductTypeElement]> {
    type Output = ProductValue;

    fn product_name(&self) -> Option<&str> {
        None
    }
    fn product_len(&self) -> usize {
        self.ty().len()
    }

    fn visit_seq_product<A: SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        visit_seq_product(self, &self, tup)
    }

    fn visit_named_product<A: super::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        visit_named_product(self, &self, tup)
    }
}

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, ArrayType> {
    type Output = ArrayValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        /// Deserialize a vector and `map` it to the appropriate `ArrayValue` variant.
        fn de_array<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
            de: D,
            map: impl FnOnce(Box<[T]>) -> ArrayValue,
        ) -> Result<ArrayValue, D::Error> {
            de.deserialize_array(BasicVecVisitor).map(<Box<[_]>>::from).map(map)
        }

        let mut ty = &*self.ty().elem_ty;

        // Loop, resolving `Ref`s, until we reach a non-`Ref` type.
        loop {
            break match ty {
                AlgebraicType::Ref(r) => {
                    // The only arm that will loop.
                    ty = self.resolve(*r).ty();
                    continue;
                }
                AlgebraicType::Sum(ty) => deserializer
                    .deserialize_array_seed(BasicVecVisitor, self.with(ty))
                    .map(<Box<[_]>>::from)
                    .map(ArrayValue::Sum),
                AlgebraicType::Product(ty) => deserializer
                    .deserialize_array_seed(BasicVecVisitor, self.with(ty))
                    .map(<Box<[_]>>::from)
                    .map(ArrayValue::Product),
                AlgebraicType::Array(ty) => deserializer
                    .deserialize_array_seed(BasicVecVisitor, self.with(ty))
                    .map(<Box<[_]>>::from)
                    .map(ArrayValue::Array),
                &AlgebraicType::Bool => de_array(deserializer, ArrayValue::Bool),
                &AlgebraicType::I8 => de_array(deserializer, ArrayValue::I8),
                &AlgebraicType::U8 => deserializer
                    .deserialize_bytes(OwnedSliceVisitor)
                    .map(<Box<[_]>>::from)
                    .map(ArrayValue::U8),
                &AlgebraicType::I16 => de_array(deserializer, ArrayValue::I16),
                &AlgebraicType::U16 => de_array(deserializer, ArrayValue::U16),
                &AlgebraicType::I32 => de_array(deserializer, ArrayValue::I32),
                &AlgebraicType::U32 => de_array(deserializer, ArrayValue::U32),
                &AlgebraicType::I64 => de_array(deserializer, ArrayValue::I64),
                &AlgebraicType::U64 => de_array(deserializer, ArrayValue::U64),
                &AlgebraicType::I128 => de_array(deserializer, ArrayValue::I128),
                &AlgebraicType::U128 => de_array(deserializer, ArrayValue::U128),
                &AlgebraicType::I256 => de_array(deserializer, ArrayValue::I256),
                &AlgebraicType::U256 => de_array(deserializer, ArrayValue::U256),
                &AlgebraicType::F32 => de_array(deserializer, ArrayValue::F32),
                &AlgebraicType::F64 => de_array(deserializer, ArrayValue::F64),
                &AlgebraicType::String => de_array(deserializer, ArrayValue::String),
            };
        }
    }
}

// impl<'de> DeserializeSeed<'de> for &ReducerDef {
//     type Output = ProductValue;

//     fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
//         deserializer.deserialize_product(self)
//     }
// }

// impl<'de> ProductVisitor<'de> for &ReducerDef {
//     type Output = ProductValue;

//     fn product_name(&self) -> Option<&str> {
//         self.name.as_deref()
//     }
//     fn product_len(&self) -> usize {
//         self.args.len()
//     }
//     fn product_kind(&self) -> ProductKind {
//         ProductKind::ReducerArgs
//     }

//     fn visit_seq_product<A: super::SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
//         visit_seq_product(&self.args, &self, tup)
//     }

//     fn visit_named_product<A: super::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
//         visit_named_product(&self.args, &self, tup)
//     }
// }

/// Deserialize, provided the fields' types, a product value with unnamed fields.
pub fn visit_seq_product<'de, A: SeqProductAccess<'de>>(
    elems: WithTypespace<[ProductTypeElement]>,
    visitor: &impl ProductVisitor<'de>,
    mut tup: A,
) -> Result<ProductValue, A::Error> {
    let elements = elems.ty().iter().enumerate().map(|(i, el)| {
        tup.next_element_seed(elems.with(&el.algebraic_type))?
            .ok_or_else(|| Error::invalid_product_length(i, visitor))
    });
    let elements = elements.collect::<Result<_, _>>()?;
    Ok(ProductValue { elements })
}

/// Deserialize, provided the fields' types, a product value with named fields.
pub fn visit_named_product<'de, A: super::NamedProductAccess<'de>>(
    elems_tys: WithTypespace<[ProductTypeElement]>,
    visitor: &impl ProductVisitor<'de>,
    mut tup: A,
) -> Result<ProductValue, A::Error> {
    let elems = elems_tys.ty();
    let mut elements = vec![None; elems.len()];
    let kind = visitor.product_kind();

    // Deserialize a product value corresponding to each product type field.
    // This is worst case quadratic in complexity
    // as fields can be specified out of order (value side) compared to `elems` (type side).
    for _ in 0..elems.len() {
        // Deserialize a field name, match against the element types.
        let index = tup.get_field_ident(TupleNameVisitor { elems, kind })?.ok_or_else(|| {
            // Couldn't deserialize a field name.
            // Find the first field name we haven't filled an element for.
            let missing = elements.iter().position(|field| field.is_none()).unwrap();
            let field_name = elems[missing].name();
            Error::missing_field(missing, field_name, visitor)
        })?;

        let element = &elems[index];

        // By index we can select which element to deserialize a value for.
        let slot = &mut elements[index];
        if slot.is_some() {
            return Err(Error::duplicate_field(index, element.name(), visitor));
        }

        // Deserialize the value for this field's type.
        *slot = Some(tup.get_field_value_seed(elems_tys.with(&element.algebraic_type))?);
    }

    // Get rid of the `Option<_>` layer.
    let elements = elements
        .into_iter()
        // We reached here, so we know nothing was missing, i.e., `None`.
        .map(|x| x.unwrap_or_else(|| unreachable!("visit_named_product")))
        .collect();

    Ok(ProductValue { elements })
}

/// A visitor for extracting indices of field names in the elements of a [`ProductType`].
struct TupleNameVisitor<'a> {
    /// The elements of a product type, in order.
    elems: &'a [ProductTypeElement],
    /// The kind of product this is.
    kind: ProductKind,
}

impl FieldNameVisitor<'_> for TupleNameVisitor<'_> {
    // The index of the field name.
    type Output = usize;

    fn field_names(&self) -> impl '_ + Iterator<Item = Option<&str>> {
        self.elems.iter().map(|f| f.name())
    }

    fn kind(&self) -> ProductKind {
        self.kind
    }

    fn visit<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        // Finds the index of a field with `name`.
        self.elems
            .iter()
            .position(|f| f.has_name(name))
            .ok_or_else(|| Error::unknown_field_name(name, &self))
    }

    fn visit_seq(self, index: usize) -> Self::Output {
        // Confirm that the index exists.
        self.elems
            .get(index)
            .expect("`index` should exist when `visit_seq` is called");

        index
    }
}

impl_deserialize!([] spacetimedb_primitives::TableId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::ViewId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::SequenceId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::IndexId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::ConstraintId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::ColId, de => u16::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::ScheduleId, de => u32::deserialize(de).map(Self));

impl GrowingVec<ColId> for ColList {
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity(cap as u16)
    }
    fn push(&mut self, elem: ColId) {
        self.push(elem);
    }
}
impl_deserialize!([] spacetimedb_primitives::ColList, de => {
    struct ColListVisitor;
    impl<'de> ArrayVisitor<'de, ColId> for ColListVisitor {
        type Output = ColList;

        fn visit<A: ArrayAccess<'de, Element = ColId>>(self, vec: A) -> Result<Self::Output, A::Error> {
            array_visit(vec)
        }
    }
    de.deserialize_array(ColListVisitor)
});
impl_deserialize!([] spacetimedb_primitives::ColSet, de => ColList::deserialize(de).map(Into::into));

#[cfg(feature = "blake3")]
impl_deserialize!([] blake3::Hash, de => <[u8; blake3::OUT_LEN]>::deserialize(de).map(blake3::Hash::from_bytes));

// TODO(perf): integrate Bytes with Deserializer to reduce copying
impl_deserialize!([] bytes::Bytes, de => <Vec<u8>>::deserialize(de).map(Into::into));

#[cfg(feature = "bytestring")]
impl_deserialize!([] bytestring::ByteString, de => <String>::deserialize(de).map(Into::into));

#[cfg(test)]
mod test {
    use crate::{
        algebraic_value::{de::ValueDeserializer, ser::value_serialize},
        bsatn,
        serde::SerdeWrapper,
        Deserialize, Serialize,
    };
    use core::fmt::Debug;

    #[test]
    fn roundtrip_tuples_in_different_data_formats() {
        fn test<T: Serialize + for<'de> Deserialize<'de> + Eq + Debug>(x: T) {
            let bsatn = bsatn::to_vec(&x).unwrap();
            let y: T = bsatn::from_slice(&bsatn).unwrap();
            assert_eq!(x, y);

            let val = value_serialize(&x);
            let y = T::deserialize(ValueDeserializer::new(val)).unwrap();
            assert_eq!(x, y);

            let json = serde_json::to_string(SerdeWrapper::from_ref(&x)).unwrap();
            let SerdeWrapper(y) = serde_json::from_str::<SerdeWrapper<T>>(&json).unwrap();
            assert_eq!(x, y);
        }

        test(());
        test((true,));
        test((1337u64, false));
        test(((7331u64, false), 42u32, 24u8));
    }
}
