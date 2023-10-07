use std::any::TypeId;
use std::ops::{Index, IndexMut};

use crate::algebraic_type::AlgebraicType;
use crate::algebraic_type_ref::AlgebraicTypeRef;
use crate::{de::Deserialize, ser::Serialize};
use crate::{SatsStr, WithTypespace};

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
        &self.types[index.0 as usize]
    }
}
impl IndexMut<AlgebraicTypeRef> for Typespace {
    fn index_mut(&mut self, index: AlgebraicTypeRef) -> &mut Self::Output {
        &mut self.types[index.0 as usize]
    }
}

impl Typespace {
    /// Returns a context ([`Typespace`]) with the given `types`.
    pub const fn new(types: Vec<AlgebraicType>) -> Self {
        Self { types }
    }

    /// Returns the [`AlgebraicType`] referred to by `r` within this context.
    pub fn get(&self, r: AlgebraicTypeRef) -> Option<&AlgebraicType> {
        self.types.get(r.idx())
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
        name: Option<&'static SatsStr<'static>>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType;
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
///     ts => AlgebraicType::product(vec![T::make_type(ts).into(), AlgebraicType::U8.into()])
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
