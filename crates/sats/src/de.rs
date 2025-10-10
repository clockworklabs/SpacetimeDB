// Some parts copyright Serde developers under the MIT / Apache-2.0 licenses at your option.
// See `serde` version `v1.0.169` for the parts where MIT / Apache-2.0 applies.

mod impls;
#[cfg(any(test, feature = "serde"))]
pub mod serde;

#[doc(hidden)]
pub use impls::{visit_named_product, visit_seq_product, WithBound};

use crate::buffer::BufReader;
use crate::{bsatn, i256, u256};
use core::fmt;
use core::marker::PhantomData;
use smallvec::SmallVec;
use std::borrow::Borrow;

/// A data format that can deserialize any data structure supported by SATS.
///
/// The `Deserializer` trait in SATS performs the same function as `serde::Deserializer` in [`serde`].
/// See the documentation of `serde::Deserializer` for more information of the data model.
///
/// Implementations of `Deserialize` map themselves into this data model
/// by passing to the `Deserializer` a visitor that can receive the necessary types.
/// The kind of visitor depends on the `deserialize_*` method.
/// Unlike in Serde, there isn't a single monolithic `Visitor` trait,
/// but rather, this functionality is split up into more targeted traits such as `SumVisitor<'de>`.
///
/// The lifetime `'de` allows us to deserialize lifetime-generic types in a zero-copy fashion.
///
/// [`serde`]: https://crates.io/crates/serde
pub trait Deserializer<'de>: Sized {
    /// The error type that can be returned if some error occurs during deserialization.
    type Error: Error;

    /// Deserializes a product value from the input.
    fn deserialize_product<V: ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    /// Deserializes a sum value from the input.
    ///
    /// The entire process of deserializing a sum, starting from `deserialize(args...)`, is roughly:
    ///
    /// - [`deserialize`][Deserialize::deserialize] calls this method,
    ///   [`deserialize_sum(sum_visitor)`](Deserializer::deserialize_sum),
    ///   providing us with a [`sum_visitor`](SumVisitor).
    ///
    /// - This method calls [`sum_visitor.visit_sum(sum_access)`](SumVisitor::visit_sum),
    ///   where [`sum_access`](SumAccess) deals with extracting the tag and the variant data,
    ///   with the latter provided as [`VariantAccess`]).
    ///   The `SumVisitor` will then assemble these into the representation of a sum value
    ///   that the [`Deserialize`] implementation wants.
    ///
    /// - [`visit_sum`](SumVisitor::visit_sum) then calls
    ///   [`sum_access.variant(variant_visitor)`](SumAccess::variant),
    ///   and uses the provided `variant_visitor` to translate extracted variant names / tags
    ///   into something that is meaningful for `visit_sum`, e.g., an index.
    ///
    ///   The call to `variant` will also return [`variant_access`](VariantAccess)
    ///   that can deserialize the contents of the variant.
    ///
    /// - Finally, after `variant` returns,
    ///   `visit_sum` deserializes the variant data using
    ///   [`variant_access.deserialize_seed(seed)`](VariantAccess::deserialize_seed)
    ///   or [`variant_access.deserialize()`](VariantAccess::deserialize).
    ///   This part may require some conditional logic depending on the identified variant.
    ///
    ///
    /// The data format will also return an object ([`VariantAccess`])
    /// that can deserialize the contents of the variant.
    fn deserialize_sum<V: SumVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    /// Deserializes a `bool` value from the input.
    fn deserialize_bool(self) -> Result<bool, Self::Error>;

    /// Deserializes a `u8` value from the input.
    fn deserialize_u8(self) -> Result<u8, Self::Error>;

    /// Deserializes a `u16` value from the input.
    fn deserialize_u16(self) -> Result<u16, Self::Error>;

    /// Deserializes a `u32` value from the input.
    fn deserialize_u32(self) -> Result<u32, Self::Error>;

    /// Deserializes a `u64` value from the input.
    fn deserialize_u64(self) -> Result<u64, Self::Error>;

    /// Deserializes a `u128` value from the input.
    fn deserialize_u128(self) -> Result<u128, Self::Error>;

    /// Deserializes a `u256` value from the input.
    fn deserialize_u256(self) -> Result<u256, Self::Error>;

    /// Deserializes an `i8` value from the input.
    fn deserialize_i8(self) -> Result<i8, Self::Error>;

    /// Deserializes an `i16` value from the input.
    fn deserialize_i16(self) -> Result<i16, Self::Error>;

    /// Deserializes an `i32` value from the input.
    fn deserialize_i32(self) -> Result<i32, Self::Error>;

    /// Deserializes an `i64` value from the input.
    fn deserialize_i64(self) -> Result<i64, Self::Error>;

    /// Deserializes an `i128` value from the input.
    fn deserialize_i128(self) -> Result<i128, Self::Error>;

    /// Deserializes an `i256` value from the input.
    fn deserialize_i256(self) -> Result<i256, Self::Error>;

    /// Deserializes an `f32` value from the input.
    fn deserialize_f32(self) -> Result<f32, Self::Error>;

    /// Deserializes an `f64` value from the input.
    fn deserialize_f64(self) -> Result<f64, Self::Error>;

    /// Deserializes a string-like object the input.
    fn deserialize_str<V: SliceVisitor<'de, str>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    /// Deserializes an `&str` string value.
    fn deserialize_str_slice(self) -> Result<&'de str, Self::Error> {
        self.deserialize_str(BorrowedSliceVisitor)
    }

    /// Deserializes a byte slice-like value.
    fn deserialize_bytes<V: SliceVisitor<'de, [u8]>>(self, visitor: V) -> Result<V::Output, Self::Error>;

    /// Deserializes an array value.
    ///
    /// This is typically the same as [`deserialize_array_seed`](Deserializer::deserialize_array_seed)
    /// with an uninteresting `seed` value.
    fn deserialize_array<V: ArrayVisitor<'de, T>, T: Deserialize<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Output, Self::Error> {
        self.deserialize_array_seed(visitor, PhantomData)
    }

    /// Deserializes an array value.
    ///
    /// The deserialization is provided with a `seed` value.
    fn deserialize_array_seed<V: ArrayVisitor<'de, T::Output>, T: DeserializeSeed<'de> + Clone>(
        self,
        visitor: V,
        seed: T,
    ) -> Result<V::Output, Self::Error>;
}

/// The `Error` trait allows [`Deserialize`] implementations to create descriptive error messages
/// belonging to the [`Deserializer`] against which they are currently running.
///
/// Every [`Deserializer`] declares an [`Error`] type
/// that encompasses both general-purpose deserialization errors
/// as well as errors specific to the particular deserialization format.
///
/// Most deserializers should only need to provide the [`Error::custom`] method
/// and inherit the default behavior for the other methods.
pub trait Error: Sized {
    /// Raised when there is general error when deserializing a type.
    fn custom(msg: impl fmt::Display) -> Self;

    /// Deserializing named products are not supported for this visitor.
    fn named_products_not_supported() -> Self {
        Self::custom("named products not supported")
    }

    /// The product length was not as promised.
    fn invalid_product_length<'de, T: ProductVisitor<'de>>(len: usize, expected: &T) -> Self {
        Self::custom(format_args!(
            "invalid length {}, expected {}",
            len,
            fmt_invalid_len(expected)
        ))
    }

    /// There was a missing field at `index`.
    fn missing_field<'de, T: ProductVisitor<'de>>(index: usize, field_name: Option<&str>, prod: &T) -> Self {
        Self::custom(error_on_field("missing ", index, field_name, prod))
    }

    /// A field with `index` was specified more than once.
    fn duplicate_field<'de, T: ProductVisitor<'de>>(index: usize, field_name: Option<&str>, prod: &T) -> Self {
        Self::custom(error_on_field("duplicate ", index, field_name, prod))
    }

    /// A field with name `field_name` does not exist.
    fn unknown_field_name<'de, T: FieldNameVisitor<'de>>(field_name: &str, expected: &T) -> Self {
        let el_ty = match expected.kind() {
            ProductKind::Normal => "field",
            ProductKind::ReducerArgs => "reducer argument",
        };
        if let Some(one_of) = one_of_names(|| expected.field_names()) {
            Self::custom(format_args!("unknown {el_ty} `{field_name}`, expected {one_of}"))
        } else {
            Self::custom(format_args!("unknown {el_ty} `{field_name}`, there are no {el_ty}s"))
        }
    }

    /// The `tag` does not specify a variant of the sum type.
    fn unknown_variant_tag<'de, T: SumVisitor<'de>>(tag: u8, expected: &T) -> Self {
        Self::custom(format_args!(
            "unknown tag {tag:#x} for sum type {}",
            expected.sum_name().unwrap_or("<unknown>"),
        ))
    }

    /// The `name` is not that of a variant of the sum type.
    fn unknown_variant_name<'de, T: VariantVisitor<'de>>(name: &str, expected: &T) -> Self {
        if let Some(one_of) = one_of_names(|| expected.variant_names().map(Some)) {
            Self::custom(format_args!("unknown variant `{name}`, expected {one_of}",))
        } else {
            Self::custom(format_args!("unknown variant `{name}`, there are no variants"))
        }
    }
}

/// Turns a closure `impl Fn(&mut Formatter) -> Result` into a `Display`able object.
pub struct FDisplay<F>(F);

impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Display for FDisplay<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.0)(f)
    }
}

/// Turns a closure `F: Fn(&mut Formatter) -> Result` into a `Display`able object.
pub fn fmt_fn<F: Fn(&mut fmt::Formatter) -> fmt::Result>(f: F) -> FDisplay<F> {
    FDisplay(f)
}

/// Returns an error message for a `problem` with field at `index` and an optional `name`.
fn error_on_field<'a, 'de>(
    problem: &'static str,
    index: usize,
    name: Option<&'a str>,
    prod: &impl ProductVisitor<'de>,
) -> impl fmt::Display + 'a {
    let field_kind = match prod.product_kind() {
        ProductKind::Normal => "field",
        ProductKind::ReducerArgs => "reducer argument",
    };
    fmt_fn(move |f| {
        // e.g. "missing field `foo`"
        f.write_str(problem)?;
        f.write_str(field_kind)?;
        if let Some(name) = name {
            write!(f, " `{name}`")
        } else {
            write!(f, " (index {index})")
        }
    })
}

/// Returns an error message for invalid product type lengths.
fn fmt_invalid_len<'de>(
    expected: &impl ProductVisitor<'de>,
) -> FDisplay<impl '_ + Fn(&mut fmt::Formatter) -> fmt::Result> {
    fmt_fn(|f| {
        let ty = match expected.product_kind() {
            ProductKind::Normal => "product type",
            ProductKind::ReducerArgs => "reducer args for",
        };
        let name = expected.product_name().unwrap_or("<product>");
        let len = expected.product_len();

        write!(f, "{ty} {name} with {len} elements")
    })
}

/// A visitor walking through a [`Deserializer`] for products.
pub trait ProductVisitor<'de> {
    /// The resulting product.
    type Output;

    /// Returns the name of the product, if any.
    fn product_name(&self) -> Option<&str>;

    /// Returns the length of the product.
    fn product_len(&self) -> usize;

    /// Returns the kind of the product.
    fn product_kind(&self) -> ProductKind {
        ProductKind::Normal
    }

    /// The input contains an unnamed product.
    fn visit_seq_product<A: SeqProductAccess<'de>>(self, prod: A) -> Result<Self::Output, A::Error>;

    /// The input contains a named product.
    fn visit_named_product<A: NamedProductAccess<'de>>(self, prod: A) -> Result<Self::Output, A::Error>;
}

/// What kind of product is this?
#[derive(Clone, Copy)]
pub enum ProductKind {
    // A normal product.
    Normal,
    /// A product in the context of reducer arguments.
    ReducerArgs,
}

/// Provides a [`ProductVisitor`] with access to each element of the unnamed product in the input.
///
/// This is a trait that a [`Deserializer`] passes to a [`ProductVisitor`] implementation.
pub trait SeqProductAccess<'de> {
    /// The error type that can be returned if some error occurs during deserialization.
    type Error: Error;

    /// Deserializes an `T` from the input.
    ///
    /// Returns `Ok(Some(value))` for the next element in the product,
    /// or `Ok(None)` if there are no more remaining items.
    ///
    /// This method exists as a convenience for [`Deserialize`] implementations.
    /// [`SeqProductAccess`] implementations should not override the default behavior.
    fn next_element<T: Deserialize<'de>>(&mut self) -> Result<Option<T>, Self::Error> {
        self.next_element_seed(PhantomData)
    }

    /// Statefully deserializes `T::Output` from the input provided a `seed` value.
    ///
    /// Returns `Ok(Some(value))` for the next element in the unnamed product,
    /// or `Ok(None)` if there are no more remaining items.
    ///
    /// [`Deserialize`] implementations should typically use
    /// [`next_element`](SeqProductAccess::next_element) instead.
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Output>, Self::Error>;
}

/// Provides a [`ProductVisitor`] with access to each element of the named product in the input.
///
/// This is a trait that a [`Deserializer`] passes to a [`ProductVisitor`] implementation.
pub trait NamedProductAccess<'de> {
    /// The error type that can be returned if some error occurs during deserialization.
    type Error: Error;

    /// Deserializes field name of type `V::Output`
    /// from the input using a visitor provided by the deserializer.
    fn get_field_ident<V: FieldNameVisitor<'de>>(&mut self, visitor: V) -> Result<Option<V::Output>, Self::Error>;

    /// Deserializes field value of type `T` from the input.
    ///
    /// This method exists as a convenience for [`Deserialize`] implementations.
    /// [`NamedProductAccess`] implementations should not override the default behavior.
    fn get_field_value<T: Deserialize<'de>>(&mut self) -> Result<T, Self::Error> {
        self.get_field_value_seed(PhantomData)
    }

    /// Statefully deserializes the field value `T::Output` from the input provided a `seed` value.
    ///
    /// [`Deserialize`] implementations should typically use
    /// [`next_element`](NamedProductAccess::get_field_value) instead.
    fn get_field_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Output, Self::Error>;
}

/// Visitor used to deserialize the name of a field.
pub trait FieldNameVisitor<'de> {
    /// The resulting field name.
    type Output;

    /// The sort of product deserialized.
    fn kind(&self) -> ProductKind {
        ProductKind::Normal
    }

    /// Provides a list of valid field names.
    ///
    /// Where `None` is yielded, this indicates a nameless field.
    fn field_names(&self) -> impl '_ + Iterator<Item = Option<&str>>;

    /// Deserializes the name of a field using `name`.
    fn visit<E: Error>(self, name: &str) -> Result<Self::Output, E>;

    /// Deserializes the name of a field using `index`.
    ///
    /// Should only be called when `index` is already known to exist
    /// and is expected to panic otherwise.
    fn visit_seq(self, index: usize) -> Self::Output;
}

/// A visitor walking through a [`Deserializer`] for sums.
///
/// This side is provided by a [`Deserialize`] implementation
/// when calling [`Deserializer::deserialize_sum`].
pub trait SumVisitor<'de> {
    /// The resulting sum.
    type Output;

    /// Returns the name of the sum, if any.
    fn sum_name(&self) -> Option<&str>;

    /// Returns whether an option is expected.
    ///
    /// The provided implementation does not.
    fn is_option(&self) -> bool {
        false
    }

    /// Drives the deserialization of a sum value.
    ///
    /// This method will ask the data format ([`A: SumAccess`][SumAccess])
    /// which variant of the sum to select in terms of a variant name / tag.
    /// `A` will use a [`VariantVisitor`], that `SumVisitor` has provided,
    /// to translate into something that is meaningful for `visit_sum`, e.g., an index.
    ///
    /// The data format will also return an object ([`VariantAccess`])
    /// that can deserialize the contents of the variant.
    fn visit_sum<A: SumAccess<'de>>(self, data: A) -> Result<Self::Output, A::Error>;
}

/// Provides a [`SumVisitor`] access to the data of a sum in the input.
///
/// An `A: SumAccess` object is created by the [`D: Deserializer`]
/// which passes `A` to a [`V: SumVisitor`] that `D` in turn was passed.
/// `A` is then used by `V` to split tag and value input apart.
pub trait SumAccess<'de> {
    /// The error type that can be returned if some error occurs during deserialization.
    type Error: Error;

    /// The visitor used to deserialize the content of the sum variant.
    type Variant: VariantAccess<'de, Error = Self::Error>;

    /// Called to identify which variant to deserialize.
    /// Returns a tuple with the result of identification (`V::Output`)
    /// and the input to variant data deserialization.
    ///
    /// The `visitor` is provided by the [`Deserializer`].
    /// This method is typically called from [`SumVisitor::visit_sum`]
    /// which will provide the [`V: VariantVisitor`](VariantVisitor).
    fn variant<V: VariantVisitor<'de>>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error>;
}

/// A visitor passed from [`SumVisitor`] to [`SumAccess::variant`]
/// which the latter uses to decide what variant to deserialize.
pub trait VariantVisitor<'de> {
    /// The result of identifying a variant, e.g., some index type.
    type Output;

    /// Provides a list of variant names.
    fn variant_names(&self) -> impl '_ + Iterator<Item = &str>;

    /// Identify the variant based on `tag`.
    fn visit_tag<E: Error>(self, tag: u8) -> Result<Self::Output, E>;

    /// Identify the variant based on `name`.
    fn visit_name<E: Error>(self, name: &str) -> Result<Self::Output, E>;
}

/// A visitor passed from [`SumAccess`] to [`SumVisitor::visit_sum`]
/// which the latter uses to deserialize the data of a selected variant.
pub trait VariantAccess<'de>: Sized {
    type Error: Error;

    /// Called when deserializing the contents of a sum variant.
    ///
    /// This method exists as a convenience for [`Deserialize`] implementations.
    fn deserialize<T: Deserialize<'de>>(self) -> Result<T, Self::Error> {
        self.deserialize_seed(PhantomData)
    }

    /// Called when deserializing the contents of a sum variant, and provided with a `seed` value.
    fn deserialize_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error>;
}

/// A `SliceVisitor` is provided a slice `T` of some elements by a [`Deserializer`]
/// and is tasked with translating this slice to the `Output` type.
pub trait SliceVisitor<'de, T: ToOwned + ?Sized>: Sized {
    /// The output produced by this visitor.
    type Output;

    /// The input contains a slice.
    ///
    /// The lifetime of the slice is ephemeral
    /// and it may be destroyed after this method returns.
    fn visit<E: Error>(self, slice: &T) -> Result<Self::Output, E>;

    /// The input contains a slice and ownership of the slice is being given to the [`SliceVisitor`].
    fn visit_owned<E: Error>(self, buf: T::Owned) -> Result<Self::Output, E> {
        self.visit(buf.borrow())
    }

    /// The input contains a slice that lives at least as long (`'de`) as the [`Deserializer`].
    fn visit_borrowed<E: Error>(self, borrowed_slice: &'de T) -> Result<Self::Output, E> {
        self.visit(borrowed_slice)
    }
}

/// A visitor walking through a [`Deserializer`] for arrays.
pub trait ArrayVisitor<'de, T> {
    /// The output produced by this visitor.
    type Output;

    /// The input contains an array.
    fn visit<A: ArrayAccess<'de, Element = T>>(self, vec: A) -> Result<Self::Output, A::Error>;
}

/// Provides an [`ArrayVisitor`] with access to each element of the array in the input.
///
/// This is a trait that a [`Deserializer`] passes to an [`ArrayVisitor`] implementation.
pub trait ArrayAccess<'de> {
    /// The element / base type of the array.
    type Element;

    /// The error type that can be returned if some error occurs during deserialization.
    type Error: Error;

    /// This returns `Ok(Some(value))` for the next element in the array,
    /// or `Ok(None)` if there are no more remaining elements.
    fn next_element(&mut self) -> Result<Option<Self::Element>, Self::Error>;

    /// Returns the number of elements remaining in the array, if known.
    fn size_hint(&self) -> Option<usize> {
        None
    }
}

/// `DeserializeSeed` is the stateful form of the [`Deserialize`] trait.
pub trait DeserializeSeed<'de> {
    /// The type produced by using this seed.
    type Output;

    /// Equivalent to the more common [`Deserialize::deserialize`] associated function,
    /// except with some initial piece of data (the seed `self`) passed in.
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error>;
}

use crate::de::impls::BorrowedSliceVisitor;
pub use spacetimedb_bindings_macro::Deserialize;

/// A data structure that can be deserialized from any data format supported by the SpacetimeDB Algebraic Type System.
///
/// In most cases, implementations of `Deserialize` may be `#[derive(Deserialize)]`d.
///
/// The `Deserialize` trait in SATS performs the same function as `serde::Deserialize` in [`serde`].
/// See the documentation of `serde::Deserialize` for more information of the data model.
///
/// The lifetime `'de` allows us to deserialize lifetime-generic types in a zero-copy fashion.
///
/// Do not manually implement this trait unless you know what you are doing.
/// Implementations must be consistent with `Serialize for T`, `SpacetimeType for T` and `Serialize, Deserialize for AlgebraicValue`.
/// Implementations that are inconsistent across these traits may result in data loss.
///
/// [`serde`]: https://crates.io/crates/serde
pub trait Deserialize<'de>: Sized {
    /// Deserialize this value from the given `deserializer`.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;

    #[doc(hidden)]
    /// Deserialize this value from the given the BSATN `deserializer`.
    fn deserialize_from_bsatn<R: BufReader<'de>>(
        deserializer: bsatn::Deserializer<'de, R>,
    ) -> Result<Self, bsatn::DecodeError> {
        Self::deserialize(deserializer)
    }

    /// used in the Deserialize for Vec<T> impl to allow specializing deserializing Vec<T> as bytes
    #[doc(hidden)]
    #[inline(always)]
    fn __deserialize_vec<D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Self>, D::Error> {
        deserializer.deserialize_array(BasicVecVisitor)
    }

    #[doc(hidden)]
    #[inline(always)]
    fn __deserialize_array<D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[Self; N], D::Error> {
        deserializer.deserialize_array(BasicArrayVisitor)
    }
}

/// A data structure that can be deserialized in SATS
/// without borrowing any data from the deserializer.
pub trait DeserializeOwned: for<'de> Deserialize<'de> {}
impl<T: for<'de> Deserialize<'de>> DeserializeOwned for T {}

impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for PhantomData<T> {
    type Output = T;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        T::deserialize(deserializer)
    }
}

/// A vector with two operations: `with_capacity` and `push`.
pub trait GrowingVec<T> {
    /// Create the collection with the given capacity.
    fn with_capacity(cap: usize) -> Self;

    /// Push to the vector the `elem`.
    fn push(&mut self, elem: T);
}

impl<T> GrowingVec<T> for Vec<T> {
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity(cap)
    }
    fn push(&mut self, elem: T) {
        self.push(elem)
    }
}

impl<T, const N: usize> GrowingVec<T> for SmallVec<[T; N]> {
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity(cap)
    }
    fn push(&mut self, elem: T) {
        self.push(elem)
    }
}

/// A basic implementation of `ArrayVisitor::visit` using the provided size hint.
pub fn array_visit<'de, A: ArrayAccess<'de>, V: GrowingVec<A::Element>>(mut access: A) -> Result<V, A::Error> {
    let mut v = V::with_capacity(access.size_hint().unwrap_or(0));
    while let Some(x) = access.next_element()? {
        v.push(x)
    }
    Ok(v)
}

/// An implementation of [`ArrayVisitor<'de, T>`] where the output is a `Vec<T>`.
pub struct BasicVecVisitor;

impl<'de, T> ArrayVisitor<'de, T> for BasicVecVisitor {
    type Output = Vec<T>;

    fn visit<A: ArrayAccess<'de, Element = T>>(self, vec: A) -> Result<Self::Output, A::Error> {
        array_visit(vec)
    }
}

/// An implementation of [`ArrayVisitor<'de, T>`] where the output is a `SmallVec<[T; N]>`.
pub struct BasicSmallVecVisitor<const N: usize>;

impl<'de, T, const N: usize> ArrayVisitor<'de, T> for BasicSmallVecVisitor<N> {
    type Output = SmallVec<[T; N]>;

    fn visit<A: ArrayAccess<'de, Element = T>>(self, vec: A) -> Result<Self::Output, A::Error> {
        array_visit(vec)
    }
}

/// An implementation of [`ArrayVisitor<'de, T>`] where the output is a `[T; N]`.
struct BasicArrayVisitor<const N: usize>;

impl<'de, T, const N: usize> ArrayVisitor<'de, T> for BasicArrayVisitor<N> {
    type Output = [T; N];

    fn visit<A: ArrayAccess<'de, Element = T>>(self, mut vec: A) -> Result<Self::Output, A::Error> {
        let mut v = arrayvec::ArrayVec::<T, N>::new();
        while let Some(el) = vec.next_element()? {
            v.try_push(el)
                .map_err(|_| Error::custom("too many elements for array"))?
        }
        v.into_inner().map_err(|_| Error::custom("too few elements for array"))
    }
}

/// Provided a list of names,
/// returns a human readable list of all the names,
/// or `None` in the case of an empty list of names.
fn one_of_names<'a, I: Iterator<Item = Option<&'a str>>>(names: impl Fn() -> I) -> Option<impl fmt::Display> {
    // Count how many names there are.
    let count = names().count();

    // There was at least one name; render those names.
    (count != 0).then(move || {
        fmt_fn(move |f| {
            let mut anon_name = 0;
            // An example of what happens for names "foo", "bar", and "baz":
            //
            // count = 1 -> "`foo`"
            //       = 2 -> "`foo` or `bar`"
            //       > 2 -> "one of `foo`, `bar`, or `baz`"
            for (index, mut name) in names().enumerate() {
                let mut name_buf: String = String::new();
                let name = name.get_or_insert_with(|| {
                    name_buf = format!("{anon_name}");
                    anon_name += 1;
                    &name_buf
                });
                match (count, index) {
                    (1, _) => write!(f, "`{name}`"),
                    (2, 1) => write!(f, "`{name}`"),
                    (2, 2) => write!(f, "`or `{name}`"),
                    (_, 1) => write!(f, "one of `{name}`"),
                    (c, i) if i < c => write!(f, ", `{name}`"),
                    (_, _) => write!(f, ", `, or {name}`"),
                }?;
            }

            Ok(())
        })
    })
}

/// Deserializes `none` variant of an optional value.
pub struct NoneAccess<E>(PhantomData<E>);

impl<E: Error> NoneAccess<E> {
    /// Returns a new [`NoneAccess`].
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<E: Error> Default for NoneAccess<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de, E: Error> SumAccess<'de> for NoneAccess<E> {
    type Error = E;
    type Variant = Self;

    fn variant<V: VariantVisitor<'de>>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        visitor.visit_name("none").map(|var| (var, self))
    }
}
impl<'de, E: Error> VariantAccess<'de> for NoneAccess<E> {
    type Error = E;
    fn deserialize_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        seed.deserialize(UnitAccess::new())
    }
}

/// Deserializes `some` variant of an optional value.
pub struct SomeAccess<D>(D);

impl<D> SomeAccess<D> {
    /// Returns a new [`SomeAccess`] with a given deserializer for the `some` variant.
    pub fn new(de: D) -> Self {
        Self(de)
    }
}

impl<'de, D: Deserializer<'de>> SumAccess<'de> for SomeAccess<D> {
    type Error = D::Error;
    type Variant = Self;

    fn variant<V: VariantVisitor<'de>>(self, visitor: V) -> Result<(V::Output, Self::Variant), Self::Error> {
        visitor.visit_name("some").map(|var| (var, self))
    }
}

impl<'de, D: Deserializer<'de>> VariantAccess<'de> for SomeAccess<D> {
    type Error = D::Error;
    fn deserialize_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Output, Self::Error> {
        seed.deserialize(self.0)
    }
}

/// A `Deserializer` that represents a unit value.
// used in the implementation of `VariantAccess for NoneAccess`
pub struct UnitAccess<E>(PhantomData<E>);

impl<E: Error> UnitAccess<E> {
    /// Returns a new [`UnitAccess`].
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<E: Error> Default for UnitAccess<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de, E: Error> SeqProductAccess<'de> for UnitAccess<E> {
    type Error = E;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, _seed: T) -> Result<Option<T::Output>, Self::Error> {
        Ok(None)
    }
}

impl<'de, E: Error> NamedProductAccess<'de> for UnitAccess<E> {
    type Error = E;

    fn get_field_ident<V: FieldNameVisitor<'de>>(&mut self, _visitor: V) -> Result<Option<V::Output>, Self::Error> {
        Ok(None)
    }

    fn get_field_value_seed<T: DeserializeSeed<'de>>(&mut self, _seed: T) -> Result<T::Output, Self::Error> {
        unreachable!()
    }
}

impl<'de, E: Error> Deserializer<'de> for UnitAccess<E> {
    type Error = E;

    fn deserialize_product<V: ProductVisitor<'de>>(self, visitor: V) -> Result<V::Output, Self::Error> {
        visitor.visit_seq_product(self)
    }

    fn deserialize_sum<V: SumVisitor<'de>>(self, _visitor: V) -> Result<V::Output, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_bool(self) -> Result<bool, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_u8(self) -> Result<u8, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_u16(self) -> Result<u16, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_u32(self) -> Result<u32, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_u64(self) -> Result<u64, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_u128(self) -> Result<u128, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_u256(self) -> Result<u256, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_i8(self) -> Result<i8, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_i16(self) -> Result<i16, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_i32(self) -> Result<i32, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_i64(self) -> Result<i64, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_i128(self) -> Result<i128, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_i256(self) -> Result<i256, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_f32(self) -> Result<f32, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_f64(self) -> Result<f64, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_str<V: SliceVisitor<'de, str>>(self, _visitor: V) -> Result<V::Output, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_bytes<V: SliceVisitor<'de, [u8]>>(self, _visitor: V) -> Result<V::Output, Self::Error> {
        Err(E::custom("invalid type"))
    }

    fn deserialize_array_seed<V: ArrayVisitor<'de, T::Output>, T: DeserializeSeed<'de> + Clone>(
        self,
        _visitor: V,
        _seed: T,
    ) -> Result<V::Output, Self::Error> {
        Err(E::custom("invalid type"))
    }
}
