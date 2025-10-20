use std::any::TypeId;
use std::ops::{Index, IndexMut};
use std::rc::Rc;
use std::sync::Arc;

use crate::algebraic_type::AlgebraicType;
use crate::algebraic_type_ref::AlgebraicTypeRef;
use crate::WithTypespace;

/// An error that occurs when attempting to resolve a type.
#[derive(thiserror::Error, Debug, PartialOrd, Ord, PartialEq, Eq)]
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
/// That is, this is the `Δ` or `Γ` you'll see in type theory literature.
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
#[derive(Clone, SpacetimeType)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[sats(crate = crate)]
pub struct Typespace {
    /// The types in our typing context that can be referred to with [`AlgebraicTypeRef`]s.
    pub types: Vec<AlgebraicType>,
}

impl std::fmt::Debug for Typespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Typespace ")?;
        f.debug_list().entries(&self.types).finish()
    }
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

    /// Iterate over types in the typespace with their references.
    pub fn refs_with_types(&self) -> impl Iterator<Item = (AlgebraicTypeRef, &AlgebraicType)> {
        self.types
            .iter()
            .enumerate()
            .map(|(idx, ty)| (AlgebraicTypeRef(idx as _), ty))
    }

    /// Check that the entire typespace is valid for generating a `SpacetimeDB` client module.
    /// See also the `spacetimedb_schema` crate, which layers additional validation on top
    /// of these checks.
    ///
    /// All types in the typespace must either satisfy
    /// [`is_valid_for_client_type_definition`](AlgebraicType::is_valid_for_client_type_definition) or
    /// [`is_valid_for_client_type_use`](AlgebraicType::is_valid_for_client_type_use).
    /// (Only the types that are `valid_for_client_type_definition` will have types generated in
    /// the client, but the other types are allowed for the convenience of module binding codegen.)
    pub fn is_valid_for_client_code_generation(&self) -> bool {
        self.types
            .iter()
            .all(|ty| ty.is_valid_for_client_type_definition() || ty.is_valid_for_client_type_use())
    }
}

impl FromIterator<AlgebraicType> for Typespace {
    fn from_iter<T: IntoIterator<Item = AlgebraicType>>(iter: T) -> Self {
        Self {
            types: iter.into_iter().collect(),
        }
    }
}

/// A trait for Rust types that can be represented as an [`AlgebraicType`]
/// with an empty typing context.
///
/// The returned `AlgebraicType` must have no free variables,
/// that is, no `AlgebraicTypeRef`s in its tree at all.
pub trait GroundSpacetimeType {
    /// Returns the `AlgebraicType` representation of `Self`.
    fn get_type() -> AlgebraicType;
}

/// This trait makes types self-describing, allowing them to automatically register their structure
/// with SpacetimeDB. This is used to tell SpacetimeDB about the structure of a module's tables and
/// reducers.
///
/// Deriving this trait also derives [`Serialize`](crate::ser::Serialize), [`Deserialize`](crate::de::Deserialize),
/// and [`Debug`](std::fmt::Debug). (There are currently no trait bounds on `SpacetimeType` documenting this fact.)
/// `Serialize` and `Deserialize` are used to convert Rust data structures to other formats, suitable for storing on disk or passing over the network. `Debug` is simply for debugging convenience.
///
/// Any Rust type implementing `SpacetimeType` can be used as a table column or reducer argument. A derive macro is provided, and can be used on both structs and enums:
///
/// ```rust
/// # use spacetimedb_sats::SpacetimeType;
///
/// #[derive(SpacetimeType)]
/// # #[sats(crate = spacetimedb_sats)]
/// struct Location {
///     x: u64,
///     y: u64
/// }
///
/// #[derive(SpacetimeType)]
/// # #[sats(crate = spacetimedb_sats)]
/// struct PlasticCrate {
///     count: u32,
/// }
///
/// #[derive(SpacetimeType)]
/// # #[sats(crate = spacetimedb_sats)]
/// struct AppleCrate {
///     variety: String,
///     count: u32,
///     freshness: u32,
/// }
///
/// #[derive(SpacetimeType)]
/// # #[sats(crate = spacetimedb_sats)]
/// enum FruitCrate {
///     Apples(AppleCrate),
///     Plastic(PlasticCrate),
/// }
/// ```
///
/// The fields of the struct/enum must also implement `SpacetimeType`.
///
/// Any type annotated with `#[table(..)]` automatically derives `SpacetimeType`.
///
/// SpacetimeType is implemented for many of the primitive types in the standard library:
///
/// - `bool`
/// - `u8`, `u16`, `u32`, `u64`, `u128`
/// - `i8`, `i16`, `i32`, `i64`, `i128`
/// - `f32`, `f64`
///
/// And common data structures:
///
/// - `String` and `&str`, utf-8 string data
/// - `()`, the unit type
/// - `Option<T> where T: SpacetimeType`
/// - `Vec<T> where T: SpacetimeType`
///
/// (Storing collections in rows of a database table is a form of [denormalization](https://en.wikipedia.org/wiki/Denormalization).)
///
/// Do not manually implement this trait unless you are VERY sure you know what you're doing.
/// Implementations must be consistent with `Deerialize<'de> for T`, `Serialize for T` and `Serialize, Deserialize for AlgebraicValue`.
/// Implementations that are inconsistent across these traits may result in data loss.
///
/// N.B.: It's `SpacetimeType`, not `SpaceTimeType`.
// TODO: we might want to have a note about what to do if you're trying to use a type from another crate in your table.
// keep this note in sync with the ones on spacetimedb::rt::{ReducerArg, TableColumn}
#[diagnostic::on_unimplemented(note = "if you own the type, try adding `#[derive(SpacetimeType)]` to its definition")]
pub trait SpacetimeType {
    /// Returns an `AlgebraicType` representing the type for `Self` in SATS
    /// and in the typing context in `typespace`. This is used by the
    /// automatic type registration system in Rust modules.
    ///
    /// The resulting `AlgebraicType` may contain `Ref`s that only make sense
    /// within the context of this particular `typespace`.
    fn make_type<S: TypespaceBuilder>(typespace: &mut S) -> AlgebraicType;
}

use ethnum::{i256, u256};
use smallvec::SmallVec;
pub use spacetimedb_bindings_macro::SpacetimeType;

/// A trait for types that can build a [`Typespace`].
pub trait TypespaceBuilder {
    /// Returns and adds a representation of type `T: 'static` as an [`AlgebraicType`]
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
    ([ $($generic_wrapped:ident $($other_generics:tt)*)? ] $rty:ty, $stty:expr) => {
        impl<$($generic_wrapped $($other_generics)*)?> $crate::GroundSpacetimeType for $rty
            $(where $generic_wrapped: $crate::GroundSpacetimeType)?
        {
            fn get_type() -> $crate::AlgebraicType {
                $stty
            }
        }

        impl_st!([ $($generic $($other_generics)*)? ] $rty, _ts => $stty);
    };
    ([ $($generic_wrapped:ident $($other_generics:tt)*)? ] $rty:ty, $ts:ident => $stty:expr) => {
        impl<$($generic_wrapped $($other_generics)*)?> $crate::SpacetimeType for $rty
            $(where $generic_wrapped: $crate::SpacetimeType)?
        {
            fn make_type<S: $crate::typespace::TypespaceBuilder>($ts: &mut S) -> $crate::AlgebraicType {
                $stty
            }
        }
    };
}

macro_rules! impl_primitives {
    ($($t:ty => $x:ident,)*) => {
        $(impl_st!([] $t, AlgebraicType::$x);)*
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
    u256 => U256,
    i256 => I256,
    f32 => F32,
    f64 => F64,
    String => String,
}

impl_st!([](), AlgebraicType::unit());
impl_st!([] str, AlgebraicType::String);
impl_st!([T] [T], ts => AlgebraicType::array(T::make_type(ts)));
impl_st!([T: ?Sized] &T, ts => T::make_type(ts));
impl_st!([T: ?Sized] Box<T>, ts => T::make_type(ts));
impl_st!([T: ?Sized] Rc<T>, ts => T::make_type(ts));
impl_st!([T: ?Sized] Arc<T>, ts => T::make_type(ts));
impl_st!([T] Vec<T>, ts => <[T]>::make_type(ts));
impl_st!([T, const N: usize] SmallVec<[T; N]>, ts => <[T]>::make_type(ts));
impl_st!([T] Option<T>, ts => AlgebraicType::option(T::make_type(ts)));

impl_st!([] spacetimedb_primitives::ColId, AlgebraicType::U16);
impl_st!([] spacetimedb_primitives::TableId, AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::ViewId, AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::IndexId, AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::SequenceId, AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::ConstraintId, AlgebraicType::U32);
impl_st!([] spacetimedb_primitives::ScheduleId, AlgebraicType::U32);

impl_st!([] spacetimedb_primitives::ColList, ts => AlgebraicType::array(spacetimedb_primitives::ColId::make_type(ts)));
impl_st!([] spacetimedb_primitives::ColSet, ts => AlgebraicType::array(spacetimedb_primitives::ColId::make_type(ts)));

impl_st!([] bytes::Bytes, AlgebraicType::bytes());

#[cfg(feature = "bytestring")]
impl_st!([] bytestring::ByteString, AlgebraicType::String);

#[cfg(test)]
mod tests {
    use crate::proptest::generate_typespace_valid_for_codegen;
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]
        #[test]
        fn is_valid_for_client_code_generation(typespace in generate_typespace_valid_for_codegen(5)) {
            prop_assert!(typespace.is_valid_for_client_code_generation());
        }
    }

    #[test]
    fn is_not_valid_for_client_code_generation() {
        let bad_inner_1 = AlgebraicType::sum([("red", AlgebraicType::U8), ("green", AlgebraicType::U8)]);
        let bad_inner_2 = AlgebraicType::product([("red", AlgebraicType::U8), ("green", AlgebraicType::U8)]);

        fn assert_not_valid(ty: AlgebraicType) {
            let typespace = Typespace::new(vec![ty.clone()]);
            assert!(!typespace.is_valid_for_client_code_generation(), "{ty:?}");
        }
        assert_not_valid(AlgebraicType::product([AlgebraicType::U8, bad_inner_1.clone()]));
        assert_not_valid(AlgebraicType::product([AlgebraicType::U8, bad_inner_2.clone()]));

        assert_not_valid(AlgebraicType::sum([AlgebraicType::U8, bad_inner_1.clone()]));
        assert_not_valid(AlgebraicType::sum([AlgebraicType::U8, bad_inner_2.clone()]));

        assert_not_valid(AlgebraicType::array(bad_inner_1.clone()));
        assert_not_valid(AlgebraicType::array(bad_inner_2.clone()));

        assert_not_valid(AlgebraicType::option(bad_inner_1.clone()));
        assert_not_valid(AlgebraicType::option(bad_inner_2.clone()));

        assert_not_valid(AlgebraicType::option(AlgebraicType::array(AlgebraicType::option(
            bad_inner_1.clone(),
        ))));
    }
}
