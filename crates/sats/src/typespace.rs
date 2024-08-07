use std::any::TypeId;
use std::ops::{Index, IndexMut};

use crate::algebraic_type::AlgebraicType;
use crate::algebraic_type_ref::AlgebraicTypeRef;
use crate::WithTypespace;
use crate::{de::Deserialize, ser::Serialize};

#[derive(thiserror::Error, Debug)]
pub enum TypeRefError {
    // TODO: ideally this should give some useful type name or path.
    // Figure out if we can provide that even though it's not encoded in SATS.
    #[error("Found recursive type reference {0}")]
    RecursiveTypeRef(AlgebraicTypeRef),

    #[error("Type reference {0} out of bounds")]
    InvalidTypeRef(AlgebraicTypeRef),
}

/// A `Typespace` represents the typing context in SATS.
///
/// That is, this is the `Δ` or `Γ` you'll see in type theory litterature.
///
/// We use (sort of) [deBrujin indices](https://en.wikipedia.org/wiki/De_Bruijn_index)
/// to represent our type variables.
/// Notably however, these are given for the entire module
/// and there are no universal quantifiers (i.e., `Δ, α ⊢ τ | Δ ⊢ ∀ α. τ`)
/// nor are there type lambdas (i.e., `Λτ. v`).
/// See [System F], the second-order lambda calculus, for more on `∀` and `Λ`.
///
/// There are however recursive types in SATs,
/// e.g., `&0 = { Cons({ v: U8, t: &0 }), Nil }` represents a basic cons list
/// where `&0` is the type reference at index `0`.
///
/// [System F]: https://en.wikipedia.org/wiki/System_F
#[derive(Debug, Clone, Deserialize, Serialize)]
#[sats(crate = crate)]
pub struct Typespace {
    /// The types in our typing context that can be referred to with [`AlgebraicTypeRef`]s.
    pub types: Vec<AlgebraicType>,
}

impl Default for Typespace {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Index<AlgebraicTypeRef> for Typespace {
    type Output = AlgebraicType;

    fn index(&self, index: AlgebraicTypeRef) -> &Self::Output {
        &self.types[index.idx()]
    }
}
impl IndexMut<AlgebraicTypeRef> for Typespace {
    fn index_mut(&mut self, index: AlgebraicTypeRef) -> &mut Self::Output {
        &mut self.types[index.idx()]
    }
}

impl Typespace {
    pub const EMPTY: &'static Typespace = &Self::new(Vec::new());

    /// Returns a context ([`Typespace`]) with the given `types`.
    pub const fn new(types: Vec<AlgebraicType>) -> Self {
        Self { types }
    }

    /// Returns the [`AlgebraicType`] referred to by `r` within this context.
    pub fn get(&self, r: AlgebraicTypeRef) -> Option<&AlgebraicType> {
        self.types.get(r.idx())
    }

    /// Returns a mutable reference to the [`AlgebraicType`] referred to by `r` within this context.
    pub fn get_mut(&mut self, r: AlgebraicTypeRef) -> Option<&mut AlgebraicType> {
        self.types.get_mut(r.idx())
    }

    /// Inserts an `AlgebraicType` into the typespace
    /// and returns an `AlgebraicTypeRef` that refers to the inserted `AlgebraicType`.
    ///
    /// This allows for self referential,
    /// recursive or other complex types to be declared in the typespace.
    ///
    /// You can also use this to later change the meaning of the returned `AlgebraicTypeRef`
    /// when you cannot provide the full definition of the type yet.
    ///
    /// Panics if the number of type references exceeds an `u32`.
    pub fn add(&mut self, ty: AlgebraicType) -> AlgebraicTypeRef {
        let index = self
            .types
            .len()
            .try_into()
            .expect("ran out of space for `AlgebraicTypeRef`s");

        self.types.push(ty);
        AlgebraicTypeRef(index)
    }

    /// Returns `ty` combined with the context `self`.
    pub const fn with_type<'a, T: ?Sized>(&'a self, ty: &'a T) -> WithTypespace<'a, T> {
        WithTypespace::new(self, ty)
    }

    /// Returns the `AlgebraicType` that `r` resolves to in the context of the `Typespace`.
    ///
    /// Panics if `r` is not known by the `Typespace`.
    pub fn resolve(&self, r: AlgebraicTypeRef) -> WithTypespace<'_, AlgebraicType> {
        self.with_type(&self[r])
    }

    /// Inlines all type references in `ty` recursively using the current typeset.
    pub fn inline_typerefs_in_type(&mut self, ty: &mut AlgebraicType) -> Result<(), TypeRefError> {
        match ty {
            AlgebraicType::Sum(sum_ty) => {
                for variant in &mut *sum_ty.variants {
                    self.inline_typerefs_in_type(&mut variant.algebraic_type)?;
                }
            }
            AlgebraicType::Product(product_ty) => {
                for element in &mut *product_ty.elements {
                    self.inline_typerefs_in_type(&mut element.algebraic_type)?;
                }
            }
            AlgebraicType::Array(array_ty) => {
                self.inline_typerefs_in_type(&mut array_ty.elem_ty)?;
            }
            AlgebraicType::Map(map_type) => {
                self.inline_typerefs_in_type(&mut map_type.key_ty)?;
                self.inline_typerefs_in_type(&mut map_type.ty)?;
            }
            AlgebraicType::Ref(r) => {
                // Lazily resolve any nested references first.
                let resolved_ty = self.inline_typerefs_in_ref(*r)?;
                // Now we can clone the fully-resolved type.
                *ty = resolved_ty.clone();
            }
            _ => {}
        }
        Ok(())
    }

    /// Inlines all nested references behind the current [`AlgebraicTypeRef`] recursively using the current typeset.
    ///
    /// Returns the fully-resolved type or an error if the type reference is invalid or self-referential.
    fn inline_typerefs_in_ref(&mut self, r: AlgebraicTypeRef) -> Result<&AlgebraicType, TypeRefError> {
        let resolved_ty = match self.get_mut(r) {
            None => return Err(TypeRefError::InvalidTypeRef(r)),
            // If we encountered a type reference, that means one of the parent calls
            // to `inline_typerefs_in_ref(r)` swapped its definition out,
            // i.e. the type referred to by `r` is recursive.
            // Note that it doesn't necessarily need to be the current call,
            // e.g. A -> B -> A dependency also forms a recursive cycle.
            // Our database can't handle recursive types, so return an error.
            // TODO: support recursive types in the future.
            Some(AlgebraicType::Ref(_)) => return Err(TypeRefError::RecursiveTypeRef(r)),
            Some(resolved_ty) => resolved_ty,
        };
        // First, swap the type with a reference.
        // This allows us to:
        // 1. Recurse into each type mutably while holding a mutable
        //    reference to the typespace as well, without cloning.
        // 2. Easily detect self-references at arbitrary depth without
        //    having to keep a separate `seen: HashSet<_>` or something.
        let mut resolved_ty = std::mem::replace(resolved_ty, AlgebraicType::Ref(r));
        // Next, recurse into the type and inline any nested type references.
        self.inline_typerefs_in_type(&mut resolved_ty)?;
        // Resolve the place again, since we couldn't hold the mutable reference across the call above.
        let place = &mut self[r];
        // Now we can put the fully-resolved type back and return that place.
        *place = resolved_ty;
        Ok(place)
    }

    /// Inlines all type references in the typespace recursively.
    ///
    /// Errors out if any type reference is invalid or self-referential.
    pub fn inline_all_typerefs(&mut self) -> Result<(), TypeRefError> {
        // We need to use indices here to allow mutable reference on each iteration.
        for r in 0..self.types.len() as u32 {
            self.inline_typerefs_in_ref(AlgebraicTypeRef(r))?;
        }
        Ok(())
    }

    /// Check that the entire typespace is in nominal normal form.
    ///
    /// Types directly contained in `self.types` are allowed to be sums or products.
    /// The *fields* of these must be in nominal form, as determined by
    /// (AlgebraicType::is_nominal_normal_form).
    ///
    /// Any type in `self.types` that is not a sum or products must also be in nominal form.
    /// (TODO(1.0): should we forbid these types entirely?)
    pub fn is_nominal_normal_form(&self) -> bool {
        self.types.iter().all(|ty| match ty {
            AlgebraicType::Sum(sum_ty) => sum_ty
                .variants
                .iter()
                .all(|variant| variant.algebraic_type.is_nominal_normal_form()),

            AlgebraicType::Product(product_ty) => product_ty
                .elements
                .iter()
                .all(|element| element.algebraic_type.is_nominal_normal_form()),

            other => other.is_nominal_normal_form(),
        })
    }
}

impl FromIterator<AlgebraicType> for Typespace {
    fn from_iter<T: IntoIterator<Item = AlgebraicType>>(iter: T) -> Self {
        Self {
            types: iter.into_iter().collect(),
        }
    }
}

/// A trait for types that can be represented as an `AlgebraicType`
/// provided a typing context `typespace`.
pub trait SpacetimeType {
    /// Returns an `AlgebraicType` representing the type for `Self` in SATS
    /// and in the typing context in `typespace`.
    fn make_type<S: TypespaceBuilder>(typespace: &mut S) -> AlgebraicType;
}

pub use spacetimedb_bindings_macro::SpacetimeType;

/// A trait for types that can build a [`Typespace`].
pub trait TypespaceBuilder {
    /// Returns and adds a representation of type `T: 'static` as an `AlgebraicType`
    /// with an optional `name` to the typing context in `self`.
    fn add(
        &mut self,
        typeid: TypeId,
        name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType;

    fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType
    where
        Self: Sized,
    {
        T::make_type(self)
    }
}

/// Implements [`SpacetimeType`] for a type in a simplified manner.
///
/// An example:
/// ```ignore
/// struct Foo<'a, T>(&'a T, u8);
/// impl_st!(
/// //     Type parameters      Impl type
/// //            v                 v
/// //   --------------------  ----------
///     ['a, T: SpacetimeType] Foo<'a, T>,
/// //  The `make_type` implementation where `ts: impl TypespaceBuilder`
/// //  and the expression right of `=>` is an `AlgebraicType`.
///     ts => AlgebraicType::product([T::make_type(ts), AlgebraicType::U8])
/// );
/// ```
#[macro_export]
macro_rules! impl_st {
    ([ $($rgenerics:tt)* ] $rty:ty, $ts:ident => $stty:expr) => {
        impl<$($rgenerics)*> $crate::SpacetimeType for $rty {
            fn make_type<S: $crate::typespace::TypespaceBuilder>($ts: &mut S) -> $crate::AlgebraicType {
                $stty
            }
        }
    };
}

macro_rules! impl_primitives {
    ($($t:ty => $x:ident,)*) => {
        $(impl_st!([] $t, _ts => AlgebraicType::$x);)*
    };
}

impl_primitives! {
    bool => Bool,
    u8 => U8,
    i8 => I8,
    u16 => U16,
    i16 => I16,
    u32 => U32,
    i32 => I32,
    u64 => U64,
    i64 => I64,
    u128 => U128,
    i128 => I128,
    f32 => F32,
    f64 => F64,
    String => String,
}

impl_st!([] (), _ts => AlgebraicType::unit());
impl_st!([] &str, _ts => AlgebraicType::String);
impl_st!([T: SpacetimeType] Vec<T>, ts => AlgebraicType::array(T::make_type(ts)));
impl_st!([T: SpacetimeType] Option<T>, ts => AlgebraicType::option(T::make_type(ts)));

impl_st!([] spacetimedb_primitives::ColId, _ts => AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::TableId, _ts => AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::IndexId, _ts => AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::SequenceId, _ts => AlgebraicType::U32);

impl_st!([] bytes::Bytes, _ts => AlgebraicType::bytes());

#[cfg(feature = "bytestring")]
impl_st!([] bytestring::ByteString, _ts => AlgebraicType::String);

#[cfg(test)]
mod tests {
    use crate::proptest::generate_nominal_typespace;
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]
        #[test]
        fn is_nominal(typespace in generate_nominal_typespace(5)) {
            prop_assert!(typespace.is_nominal_normal_form());
        }
    }

    #[test]
    fn is_not_nominal() {
        let bad_inner_1 = AlgebraicType::sum([("red", AlgebraicType::U8), ("green", AlgebraicType::U8)]);
        let bad_inner_2 = AlgebraicType::product([("red", AlgebraicType::U8), ("green", AlgebraicType::U8)]);

        fn assert_not_nominal(ty: AlgebraicType) {
            let typespace = Typespace::new(vec![ty.clone()]);
            assert!(!typespace.is_nominal_normal_form(), "{:?}", ty);
        }
        assert_not_nominal(AlgebraicType::product([AlgebraicType::U8, bad_inner_1.clone()]));
        assert_not_nominal(AlgebraicType::product([AlgebraicType::U8, bad_inner_2.clone()]));

        assert_not_nominal(AlgebraicType::sum([AlgebraicType::U8, bad_inner_1.clone()]));
        assert_not_nominal(AlgebraicType::sum([AlgebraicType::U8, bad_inner_2.clone()]));

        assert_not_nominal(AlgebraicType::array(bad_inner_1.clone()));
        assert_not_nominal(AlgebraicType::array(bad_inner_2.clone()));

        assert_not_nominal(AlgebraicType::option(bad_inner_1.clone()));
        assert_not_nominal(AlgebraicType::option(bad_inner_2.clone()));

        assert_not_nominal(AlgebraicType::map(AlgebraicType::U8, bad_inner_1.clone()));
        assert_not_nominal(AlgebraicType::map(AlgebraicType::U8, bad_inner_2.clone()));

        assert_not_nominal(AlgebraicType::map(bad_inner_1.clone(), AlgebraicType::U8));
        assert_not_nominal(AlgebraicType::map(bad_inner_2.clone(), AlgebraicType::U8));

        assert_not_nominal(AlgebraicType::option(AlgebraicType::array(AlgebraicType::option(
            bad_inner_1.clone(),
        ))));
    }
}
