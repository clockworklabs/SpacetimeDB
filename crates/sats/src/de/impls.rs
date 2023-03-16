use std::borrow::Cow;
use std::collections::BTreeMap;
use std::marker::PhantomData;

// use crate::type_value::{ElementValue, EnumValue};
// use crate::{ElementDef, EnumDef, PrimitiveType, ReducerDef, TupleDef, ProductValue, TypeDef, TypeValue};

use crate::{
    AlgebraicType, AlgebraicValue, BuiltinType, BuiltinValue, MapType, ProductType, ProductTypeElement, ProductValue,
    SumType, SumValue, TypeInSpace,
};

use super::{
    BasicMapVisitor, BasicVecVisitor, Deserialize, DeserializeSeed, Deserializer, Error, FieldNameVisitor, ProductKind,
    ProductVisitor, SeqProductAccess, SliceVisitor, SumAccess, SumVisitor, VariantAccess, VariantVisitor,
};

macro_rules! impl_prim {
    ($(($prim:ty, $method:ident))*) => {
        $(impl<'de> Deserialize<'de> for $prim {
            fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                de.$method()
            }
        })*
    };
}

impl<'de> Deserialize<'de> for () {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_product(UnitVisitor)
    }
}
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

impl_prim! {
    (bool, deserialize_bool) /*(u8, deserialize_u8)*/ (u16, deserialize_u16)
    (u32, deserialize_u32) (u64, deserialize_u64) (u128, deserialize_u128) (i8, deserialize_i8)
    (i16, deserialize_i16) (i32, deserialize_i32) (i64, deserialize_i64) (i128, deserialize_i128)
    (f32, deserialize_f32) (f64, deserialize_f64)
}

impl<'de> Deserialize<'de> for u8 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_u8()
    }
    // specialize Vec<u8> deserialization
    fn __deserialize_vec<D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Self>, D::Error> {
        deserializer.deserialize_bytes(OwnedSliceVisitor)
    }
}

impl<'de> Deserialize<'de> for String {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(OwnedSliceVisitor)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Vec<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::__deserialize_vec(deserializer)
    }
}

impl<'de> Deserialize<'de> for Box<str> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|s| s.into_boxed_str())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Box<[T]> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::deserialize(deserializer).map(|s| s.into_boxed_slice())
    }
}

struct OwnedSliceVisitor;
impl<'de, T: ToOwned + ?Sized> SliceVisitor<'de, T> for OwnedSliceVisitor {
    type Output = T::Owned;

    fn visit<E: Error>(self, slice: &T) -> Result<Self::Output, E> {
        Ok(slice.to_owned())
    }

    fn visit_owned<E: Error>(self, buf: T::Owned) -> Result<Self::Output, E> {
        Ok(buf)
    }
}

impl<'de> Deserialize<'de> for &'de str {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(BorrowedSliceVisitor)
    }
}

impl<'de> Deserialize<'de> for &'de [u8] {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(BorrowedSliceVisitor)
    }
}

struct BorrowedSliceVisitor;
impl<'de, T: ToOwned + ?Sized + 'de> SliceVisitor<'de, T> for BorrowedSliceVisitor {
    type Output = &'de T;

    fn visit<E: Error>(self, _: &T) -> Result<Self::Output, E> {
        Err(E::custom("expected *borrowed* slice"))
    }

    fn visit_borrowed<E: Error>(self, borrowed_slice: &'de T) -> Result<Self::Output, E> {
        Ok(borrowed_slice)
    }
}

impl<'de> Deserialize<'de> for Cow<'de, str> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(CowSliceVisitor)
    }
}

impl<'de> Deserialize<'de> for Cow<'de, [u8]> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(CowSliceVisitor)
    }
}

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

impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> Deserialize<'de> for BTreeMap<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(BasicMapVisitor)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Box<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Box::new)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Option<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_sum(OptionVisitor(PhantomData))
    }
}

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
        let (some, data) = data.variant(self)?;
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

impl<'de> DeserializeSeed<'de> for TypeInSpace<'_, AlgebraicType> {
    type Output = AlgebraicValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        match self.ty() {
            AlgebraicType::Sum(sum) => self.with(sum).deserialize(deserializer).map(AlgebraicValue::Sum),
            AlgebraicType::Product(prod) => self.with(prod).deserialize(deserializer).map(AlgebraicValue::Product),
            AlgebraicType::Builtin(b) => self.with(b).deserialize(deserializer).map(AlgebraicValue::Builtin),
            AlgebraicType::Ref(r) => self.resolve(*r).deserialize(deserializer),
        }
    }
}

impl<'de> DeserializeSeed<'de> for TypeInSpace<'_, BuiltinType> {
    type Output = BuiltinValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        Ok(match self.ty() {
            BuiltinType::Bool => BuiltinValue::Bool(bool::deserialize(deserializer)?),
            BuiltinType::I8 => BuiltinValue::I8(i8::deserialize(deserializer)?),
            BuiltinType::U8 => BuiltinValue::U8(u8::deserialize(deserializer)?),
            BuiltinType::I16 => BuiltinValue::I16(i16::deserialize(deserializer)?),
            BuiltinType::U16 => BuiltinValue::U16(u16::deserialize(deserializer)?),
            BuiltinType::I32 => BuiltinValue::I32(i32::deserialize(deserializer)?),
            BuiltinType::U32 => BuiltinValue::U32(u32::deserialize(deserializer)?),
            BuiltinType::I64 => BuiltinValue::I64(i64::deserialize(deserializer)?),
            BuiltinType::U64 => BuiltinValue::U64(u64::deserialize(deserializer)?),
            BuiltinType::I128 => BuiltinValue::I128(i128::deserialize(deserializer)?),
            BuiltinType::U128 => BuiltinValue::U128(u128::deserialize(deserializer)?),
            BuiltinType::F32 => BuiltinValue::F32(f32::deserialize(deserializer)?.into()),
            BuiltinType::F64 => BuiltinValue::F64(f64::deserialize(deserializer)?.into()),
            BuiltinType::String => BuiltinValue::String(String::deserialize(deserializer)?),
            BuiltinType::Array { ty } if **ty == AlgebraicType::Builtin(BuiltinType::U8) => {
                BuiltinValue::Bytes(Vec::<u8>::deserialize(deserializer)?)
            }
            BuiltinType::Array { ty } => {
                let val = deserializer.deserialize_array_seed(BasicVecVisitor, self.with(&**ty))?;
                BuiltinValue::Array { val }
            }
            BuiltinType::Map(MapType { key_ty, ty }) => {
                let val = deserializer.deserialize_map_seed(BasicMapVisitor, self.with(&**key_ty), self.with(&**ty))?;
                BuiltinValue::Map { val }
            }
        })
    }
}

impl<'de> DeserializeSeed<'de> for TypeInSpace<'_, SumType> {
    type Output = SumValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_sum(self)
    }
}

impl<'de> SumVisitor<'de> for TypeInSpace<'_, SumType> {
    type Output = SumValue;

    fn sum_name(&self) -> Option<&str> {
        None
    }
    fn is_option(&self) -> bool {
        self.ty().looks_like_option().is_some()
    }

    fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error> {
        let (tag, data) = data.variant(self)?;
        let variant_ty = self.map(|ty| &ty.variants[tag as usize].algebraic_type);
        let value = Box::new(data.deserialize_seed(variant_ty)?);
        Ok(SumValue { tag, value })
    }
}
impl VariantVisitor for TypeInSpace<'_, SumType> {
    type Output = u8;

    fn variant_names(&self, names: &mut dyn super::ValidNames) {
        names.extend(self.ty().variants.iter().filter_map(|v| v.name.as_deref()))
    }

    fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E> {
        self.ty()
            .variants
            .get(tag as usize)
            .ok_or_else(|| E::unknown_variant_tag(tag, &self))?;
        Ok(tag)
    }

    fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        self.ty()
            .variants
            .iter()
            .position(|var| var.name.as_deref() == Some(name))
            .map(|pos| pos as u8)
            .ok_or_else(|| E::unknown_variant_name(name, &self))
    }
}

impl<'de> DeserializeSeed<'de> for TypeInSpace<'_, ProductType> {
    type Output = ProductValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self)
    }
}

impl<'de> ProductVisitor<'de> for TypeInSpace<'_, ProductType> {
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

pub fn visit_seq_product<'de, A: SeqProductAccess<'de>>(
    elems: TypeInSpace<[ProductTypeElement]>,
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

pub fn visit_named_product<'de, A: super::NamedProductAccess<'de>>(
    elems_tys: TypeInSpace<[ProductTypeElement]>,
    visitor: &impl ProductVisitor<'de>,
    mut tup: A,
) -> Result<ProductValue, A::Error> {
    let elems = elems_tys.ty();
    let mut elements = vec![None; elems.len()];
    let mut n = 0;
    let kind = visitor.product_kind();
    // under a certain threshold, just do linear searches
    while n < elems.len() {
        let tag = tup.get_field_ident(TupleNameVisitor { elems, kind })?.ok_or_else(|| {
            let missing = elements.iter().position(|field| field.is_none()).unwrap();
            let field_name = elems[missing].name.as_deref();
            Error::missing_field(missing, field_name, visitor)
        })?;
        let element = &elems[tag];
        let slot = &mut elements[tag];
        if slot.is_some() {
            return Err(Error::duplicate_field(tag, element.name.as_deref(), visitor));
        }
        *slot = Some(tup.get_field_value_seed(elems_tys.with(&element.algebraic_type))?);
        n += 1;
    }
    let elements = elements
        .into_iter()
        .map(|x| x.unwrap_or_else(|| unreachable!()))
        .collect();
    Ok(ProductValue { elements })
}

struct TupleNameVisitor<'a> {
    elems: &'a [ProductTypeElement],
    kind: ProductKind,
}
impl<'de> FieldNameVisitor<'de> for TupleNameVisitor<'_> {
    type Output = usize;

    fn field_names(&self, names: &mut dyn super::ValidNames) {
        names.extend(self.elems.iter().filter_map(|f| f.name.as_deref()))
    }
    fn kind(&self) -> ProductKind {
        self.kind
    }

    fn visit<E: Error>(self, name: &str) -> Result<Self::Output, E> {
        self.elems
            .iter()
            .position(|f| f.name.as_deref() == Some(name))
            .ok_or_else(|| Error::unknown_field_name(name, &self))
    }
}
