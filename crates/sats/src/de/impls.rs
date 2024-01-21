use std::borrow::Cow;
use std::collections::BTreeMap;
use std::marker::PhantomData;

// use crate::type_value::{ElementValue, EnumValue};
// use crate::{ProductTypeElement, SumType, PrimitiveType, ReducerDef, ProductType, ProductValue, AlgebraicType, AlgebraicValue};

use spacetimedb_primitives::{ColId, ColListBuilder};

use crate::{
    de::{array_visit, ArrayAccess, ArrayVisitor, GrowingVec},
    AlgebraicType, AlgebraicValue, ArrayType, ArrayValue, MapType, MapValue, ProductType, ProductTypeElement,
    ProductValue, SumType, SumValue, WithTypespace, F32, F64,
};

use super::{
    BasicMapVisitor, BasicVecVisitor, Deserialize, DeserializeSeed, Deserializer, Error, FieldNameVisitor, ProductKind,
    ProductVisitor, SeqProductAccess, SliceVisitor, SumAccess, SumVisitor, VariantAccess, VariantVisitor,
};

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
    (bool, deserialize_bool) /*(u8, deserialize_u8)*/ (u16, deserialize_u16)
    (u32, deserialize_u32) (u64, deserialize_u64) (u128, deserialize_u128) (i8, deserialize_i8)
    (i16, deserialize_i16) (i32, deserialize_i32) (i64, deserialize_i64) (i128, deserialize_i128)
    (f32, deserialize_f32) (f64, deserialize_f64)
}

impl_deserialize!([] (), de => de.deserialize_product(UnitVisitor));

/// The `UnitVisitor` looks for a unit product.
/// That is, it consumes nothing from the input.
struct UnitVisitor;
impl<'de> ProductVisitor<'de> for UnitVisitor {
    type Output = ();

    fn product_name(&self) -> Option<&str> {
        None
    }

    fn product_len(&self) -> usize {
        0
    }

    fn visit_seq_product<A: SeqProductAccess<'de>>(self, _prod: A) -> Result<Self::Output, A::Error> {
        Ok(())
    }

    fn visit_named_product<A: super::NamedProductAccess<'de>>(self, _prod: A) -> Result<Self::Output, A::Error> {
        Ok(())
    }
}

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
impl_deserialize!([T: Deserialize<'de>, const N: usize] [T; N], de => T::__deserialize_array(de));
impl_deserialize!([] Box<str>, de => String::deserialize(de).map(|s| s.into_boxed_str()));
impl_deserialize!([T: Deserialize<'de>] Box<[T]>, de => Vec::deserialize(de).map(|s| s.into_boxed_slice()));

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

impl_deserialize!(
    [K: Deserialize<'de> + Ord, V: Deserialize<'de>] BTreeMap<K, V>,
    de => de.deserialize_map(BasicMapVisitor)
);

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

impl<'de, T: Deserialize<'de>> VariantVisitor for OptionVisitor<T> {
    type Output = bool;

    fn variant_names(&self, names: &mut dyn super::ValidNames) {
        names.extend(["some", "none"])
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

impl<'de, T: Deserialize<'de>, U: Deserialize<'de>> VariantVisitor for ResultVisitor<T, U> {
    type Output = ResultVariant;

    fn variant_names(&self, names: &mut dyn super::ValidNames) {
        names.extend(["ok", "err"])
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

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, AlgebraicType> {
    type Output = AlgebraicValue;

    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<Self::Output, D::Error> {
        match self.ty() {
            AlgebraicType::Ref(r) => self.resolve(*r).deserialize(de),
            AlgebraicType::Sum(sum) => self.with(sum).deserialize(de).map(Into::into),
            AlgebraicType::Product(prod) => self.with(prod).deserialize(de).map(Into::into),
            AlgebraicType::Builtin(crate::BuiltinType::Array(ty)) => self.with(ty).deserialize(de).map(Into::into),
            AlgebraicType::Builtin(crate::BuiltinType::Map(ty)) => self.with(&**ty).deserialize(de).map(Into::into),
            &AlgebraicType::Bool => bool::deserialize(de).map(Into::into),
            &AlgebraicType::I8 => i8::deserialize(de).map(Into::into),
            &AlgebraicType::U8 => u8::deserialize(de).map(Into::into),
            &AlgebraicType::I16 => i16::deserialize(de).map(Into::into),
            &AlgebraicType::U16 => u16::deserialize(de).map(Into::into),
            &AlgebraicType::I32 => i32::deserialize(de).map(Into::into),
            &AlgebraicType::U32 => u32::deserialize(de).map(Into::into),
            &AlgebraicType::I64 => i64::deserialize(de).map(Into::into),
            &AlgebraicType::U64 => u64::deserialize(de).map(Into::into),
            &AlgebraicType::I128 => i128::deserialize(de).map(Into::into),
            &AlgebraicType::U128 => u128::deserialize(de).map(Into::into),
            &AlgebraicType::F32 => f32::deserialize(de).map(Into::into),
            &AlgebraicType::F64 => f64::deserialize(de).map(Into::into),
            &AlgebraicType::String => String::deserialize(de).map(Into::into),
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

impl VariantVisitor for WithTypespace<'_, SumType> {
    type Output = u8;

    fn variant_names(&self, names: &mut dyn super::ValidNames) {
        // Provide the names known from the `SumType`.
        names.extend(self.ty().variants.iter().filter_map(|v| v.name()))
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
        deserializer.deserialize_product(self)
    }
}

impl<'de> ProductVisitor<'de> for WithTypespace<'_, ProductType> {
    type Output = ProductValue;

    fn product_name(&self) -> Option<&str> {
        None
    }
    fn product_len(&self) -> usize {
        self.ty().elements.len()
    }

    fn visit_seq_product<A: SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        visit_seq_product(self.map(|ty| &*ty.elements), &self, tup)
    }

    fn visit_named_product<A: super::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        visit_named_product(self.map(|ty| &*ty.elements), &self, tup)
    }
}

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, ArrayType> {
    type Output = ArrayValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        /// Deserialize a vector and `map` it to the appropriate `ArrayValue` variant.
        fn de_array<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
            de: D,
            map: impl FnOnce(Vec<T>) -> ArrayValue,
        ) -> Result<ArrayValue, D::Error> {
            de.deserialize_array(BasicVecVisitor).map(map)
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
                    .map(ArrayValue::Sum),
                AlgebraicType::Product(ty) => deserializer
                    .deserialize_array_seed(BasicVecVisitor, self.with(ty))
                    .map(ArrayValue::Product),
                AlgebraicType::Builtin(crate::BuiltinType::Array(ty)) => deserializer
                    .deserialize_array_seed(BasicVecVisitor, self.with(ty))
                    .map(ArrayValue::Array),
                AlgebraicType::Builtin(crate::BuiltinType::Map(ty)) => deserializer
                    .deserialize_array_seed(BasicVecVisitor, self.with(&**ty))
                    .map(ArrayValue::Map),
                &AlgebraicType::Bool => de_array(deserializer, ArrayValue::Bool),
                &AlgebraicType::I8 => de_array(deserializer, ArrayValue::I8),
                &AlgebraicType::U8 => deserializer.deserialize_bytes(OwnedSliceVisitor).map(ArrayValue::U8),
                &AlgebraicType::I16 => de_array(deserializer, ArrayValue::I16),
                &AlgebraicType::U16 => de_array(deserializer, ArrayValue::U16),
                &AlgebraicType::I32 => de_array(deserializer, ArrayValue::I32),
                &AlgebraicType::U32 => de_array(deserializer, ArrayValue::U32),
                &AlgebraicType::I64 => de_array(deserializer, ArrayValue::I64),
                &AlgebraicType::U64 => de_array(deserializer, ArrayValue::U64),
                &AlgebraicType::I128 => de_array(deserializer, ArrayValue::I128),
                &AlgebraicType::U128 => de_array(deserializer, ArrayValue::U128),
                &AlgebraicType::F32 => de_array(deserializer, ArrayValue::F32),
                &AlgebraicType::F64 => de_array(deserializer, ArrayValue::F64),
                &AlgebraicType::String => de_array(deserializer, ArrayValue::String),
            };
        }
    }
}

impl<'de> DeserializeSeed<'de> for WithTypespace<'_, MapType> {
    type Output = MapValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        let MapType { key_ty, ty } = self.ty();
        deserializer.deserialize_map_seed(BasicMapVisitor, self.with(key_ty), self.with(ty))
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
        // Deserialize a field name, match against the element types, .
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

    fn field_names(&self, names: &mut dyn super::ValidNames) {
        names.extend(self.elems.iter().filter_map(|f| f.name()))
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
}

impl_deserialize!([] spacetimedb_primitives::ColId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::TableId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::IndexId, de => u32::deserialize(de).map(Self));
impl_deserialize!([] spacetimedb_primitives::SequenceId, de => u32::deserialize(de).map(Self));

impl_deserialize!([] spacetimedb_primitives::ColList, de => {
    impl GrowingVec<ColId> for ColListBuilder {
        fn with_capacity(_: usize) -> Self {
            Self::new()
        }
        fn push(&mut self, elem: ColId) {
            self.push(elem);
        }
    }

    struct ColListVisitor;
    impl<'de> ArrayVisitor<'de, ColId> for ColListVisitor {
        type Output = ColListBuilder;

        fn visit<A: ArrayAccess<'de, Element = ColId>>(self, vec: A) -> Result<Self::Output, A::Error> {
            array_visit(vec)
        }
    }
    let col_list = de.deserialize_array(ColListVisitor)?;
    col_list.build().map_err(|_| crate::de::Error::custom("invalid empty ColList".to_string()))
});
