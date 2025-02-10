pub mod algebraic_type;
mod algebraic_type_ref;
pub mod algebraic_value;
mod algebraic_value_hash;
pub mod array_type;
pub mod array_value;
pub mod bsatn;
pub mod buffer;
pub mod convert;
pub mod de;
pub mod hash;
pub mod hex;
pub mod meta_type;
pub mod primitives;
pub mod product_type;
pub mod product_type_element;
pub mod product_value;
mod resolve_refs;
pub mod satn;
pub mod ser;
pub mod size_of;
pub mod sum_type;
pub mod sum_type_variant;
pub mod sum_value;
pub mod time_duration;
pub mod timestamp;
pub mod typespace;

#[cfg(any(test, feature = "proptest"))]
pub mod proptest;

#[cfg(feature = "serde")]
pub mod serde {
    pub use crate::de::serde::{deserialize_from as deserialize, SerdeDeserializer};
    pub use crate::ser::serde::{serialize_to as serialize, SerdeSerializer};

    /// A wrapper around a `serde` error which occured while translating SATS <-> serde.
    #[repr(transparent)]
    pub struct SerdeError<E>(pub E);

    /// A wrapper type that implements `serde` traits when `T` implements SATS traits.
    ///
    /// Specifically:
    /// - <code>T: [sats::Serialize][crate::ser::Serialize] => `SerializeWrapper<T>`: [serde::Serialize]</code>
    /// - <code>T: [sats::Deserialize<'de>][crate::de::Deserialize] => `SerializeWrapper<T>`: [serde::Deserialize<'de>]</code>
    /// - <code>T: [sats::DeserializeSeed<'de>][crate::de::DeserializeSeed] => `SerializeWrapper<T>`: [serde::DeserializeSeed<'de>]</code>
    #[repr(transparent)]
    pub struct SerdeWrapper<T: ?Sized>(pub T);

    impl<T: ?Sized> SerdeWrapper<T> {
        /// Wraps a value in `SerdeWrapper`.
        pub fn new(t: T) -> Self
        where
            T: Sized,
        {
            Self(t)
        }

        /// Converts `&T` to `&SerializeWrapper<T>`.
        pub fn from_ref(t: &T) -> &Self {
            // SAFETY: OK because of `repr(transparent)`.
            unsafe { &*(t as *const T as *const SerdeWrapper<T>) }
        }
    }
}

/// Allows the macros in [`spacetimedb_bindings_macro`] to accept `crate = spacetimedb_sats`,
/// which will then emit `$krate::sats`.
#[doc(hidden)]
pub use crate as sats;

pub use algebraic_type::AlgebraicType;
pub use algebraic_type_ref::AlgebraicTypeRef;
pub use algebraic_value::{i256, u256, AlgebraicValue, F32, F64};
pub use algebraic_value_hash::hash_bsatn;
pub use array_type::ArrayType;
pub use array_value::ArrayValue;
pub use product_type::ProductType;
pub use product_type_element::ProductTypeElement;
pub use product_value::ProductValue;
pub use sum_type::SumType;
pub use sum_type_variant::SumTypeVariant;
pub use sum_value::SumValue;
pub use typespace::{GroundSpacetimeType, SpacetimeType, Typespace};

pub use de::Deserialize;
pub use ser::Serialize;

/// The `Value` trait provides an abstract notion of a value.
///
/// All we know about values abstractly is that they have a `Type`.
pub trait Value {
    /// The type of this value.
    type Type;
}

impl<T: Value> Value for Box<[T]> {
    // TODO(centril/phoebe): This looks weird; shouldn't it be ArrayType?
    type Type = T::Type;
}

/// A borrowed value combined with its type and typing context (`Typespace`).
pub struct ValueWithType<'a, T: Value> {
    /// The type combined with the context of this `val`ue.
    ty: WithTypespace<'a, T::Type>,
    /// The borrowed value.
    val: &'a T,
}

impl<T: Value> Copy for ValueWithType<'_, T> {}
impl<T: Value> Clone for ValueWithType<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: Value> ValueWithType<'a, T> {
    /// Wraps the borrowed value `val` with its type combined with context.
    pub fn new(ty: WithTypespace<'a, T::Type>, val: &'a T) -> Self {
        Self { ty, val }
    }

    /// Returns the borrowed value.
    pub fn value(&self) -> &'a T {
        self.val
    }

    /// Returns the type of the value.
    pub fn ty(&self) -> &'a T::Type {
        self.ty.ty
    }

    pub fn ty_s(&self) -> WithTypespace<'a, T::Type> {
        self.ty
    }

    /// Returns the typing context (`Typespace`).
    pub fn typespace(&self) -> &'a Typespace {
        self.ty.typespace
    }

    /// Reuses the typespace we already have and returns `val` and `ty` wrapped with it.
    pub fn with<'b, U: Value>(&self, ty: &'b U::Type, val: &'b U) -> ValueWithType<'b, U>
    where
        'a: 'b,
    {
        ValueWithType {
            ty: self.ty.with(ty),
            val,
        }
    }
}

impl<'a, T: Value> ValueWithType<'a, Box<[T]>> {
    pub fn iter(&self) -> impl Iterator<Item = ValueWithType<'a, T>> + use<'_, 'a, T> {
        self.value().iter().map(|val| ValueWithType { ty: self.ty, val })
    }
}

/// Adds a `Typespace` context atop of a borrowed type.
#[derive(Debug)]
pub struct WithTypespace<'a, T: ?Sized> {
    /// The typespace context that has been added to `ty`.
    typespace: &'a Typespace,
    /// What we've added the context to.
    ty: &'a T,
}

impl<T: ?Sized> Copy for WithTypespace<'_, T> {}
impl<T: ?Sized> Clone for WithTypespace<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: ?Sized> WithTypespace<'a, T> {
    /// Wraps `ty` in a context combined with the `typespace`.
    pub const fn new(typespace: &'a Typespace, ty: &'a T) -> Self {
        Self { typespace, ty }
    }

    /// Wraps `ty` in an empty context.
    pub const fn empty(ty: &'a T) -> Self {
        Self::new(Typespace::EMPTY, ty)
    }

    /// Returns the object that the context was created with.
    pub const fn ty(&self) -> &'a T {
        self.ty
    }

    /// Returns the typespace context.
    pub const fn typespace(&self) -> &'a Typespace {
        self.typespace
    }

    /// Reuses the typespace we already have and returns `ty: U` wrapped with it.
    pub fn with<'b, U>(&self, ty: &'b U) -> WithTypespace<'b, U>
    where
        'a: 'b,
    {
        WithTypespace {
            typespace: self.typespace,
            ty,
        }
    }

    pub(crate) fn iter_with<U: 'a, I: IntoIterator<Item = &'a U>>(&self, tys: I) -> IterWithTypespace<'a, I::IntoIter> {
        IterWithTypespace {
            typespace: self.typespace,
            iter: tys.into_iter(),
        }
    }

    /// Wraps `val` with the type and typespace context in `self`.
    pub fn with_value<'b, V: Value<Type = T>>(&self, val: &'b V) -> ValueWithType<'b, V>
    where
        'a: 'b,
    {
        ValueWithType::new(*self, val)
    }

    /// Returns the `AlgebraicType` that `r` resolves to in the context of our `Typespace`.
    ///
    /// Panics if `r` is not known by our `Typespace`.
    pub fn resolve(&self, r: AlgebraicTypeRef) -> WithTypespace<'a, AlgebraicType> {
        WithTypespace {
            typespace: self.typespace,
            ty: &self.typespace[r],
        }
    }

    /// Maps the object we've wrapped from `&T -> &U` in our context.
    ///
    /// This can be used to e.g., project fields and through a structure.
    /// This provides an implementation of functor mapping for `WithTypespace`.
    pub fn map<U: ?Sized>(&self, f: impl FnOnce(&'a T) -> &'a U) -> WithTypespace<'a, U> {
        WithTypespace {
            typespace: self.typespace,
            ty: f(self.ty),
        }
    }
}

pub struct IterWithTypespace<'a, I> {
    typespace: &'a Typespace,
    iter: I,
}

impl<'a, I, T: 'a> Iterator for IterWithTypespace<'a, I>
where
    I: Iterator<Item = &'a T>,
{
    type Item = WithTypespace<'a, T>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|ty| self.typespace.with_type(ty))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a, I, T: 'a> ExactSizeIterator for IterWithTypespace<'a, I>
where
    I: ExactSizeIterator<Item = &'a T>,
{
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Required for derive(SpacetimeType) to work outside of a module
#[macro_export]
#[doc(hidden)]
macro_rules! __make_register_reftype {
    ($ty:ty, $name:literal) => {};
}

/// A helper for prettier Debug implementation, without extra indirection around Some("name").
fn dbg_aggregate_name(opt: &Option<Box<str>>) -> &dyn std::fmt::Debug {
    opt.as_ref().map_or(opt, |s| s)
}
