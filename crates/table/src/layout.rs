//! Defines [`Layout`], which encompasses the fixed size and alignment of an object,
//! e.g., a row, or a column, or some other sub-division of a row.
//!
//! `Layout` annotated versions of SATS types are also provided,
//! such as [`ProductTypeLayout`] and [`AlgebraicTypeLayout`].
//! These, and others, determine what the layout of objects typed at those types are.
//! They also implement [`HasLayout`] which generalizes over layout annotated types.

use crate::MemoryUsage;

use super::{
    indexes::Size,
    var_len::{VarLenGranule, VarLenRef},
};
use core::mem;
use core::ops::Index;
use enum_as_inner::EnumAsInner;
use spacetimedb_sats::{
    bsatn, AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue, SumType, SumTypeVariant,
};
pub use spacetimedb_schema::type_for_generate::PrimitiveType;

/// Aligns a `base` offset to the `required_alignment` (in the positive direction) and returns it.
///
/// When `base` is already aligned, `base` will be returned.
pub const fn align_to(base: usize, required_alignment: usize) -> usize {
    if required_alignment == 0 {
        // Avoid computing the remainder below, as that panics.
        // This path is reachable for e.g., uninhabited types.
        base
    } else {
        let misalignment = base % required_alignment;
        if misalignment == 0 {
            base
        } else {
            let padding = required_alignment - misalignment;
            base + padding
        }
    }
}

// TODO(perf): try out using just an offset relative to the row start itself.
// The main drawback is that nested types start at non-zero.
// Primitives and var-len refs now also need to store more data
// but this shouldn't cost anything as this would be padding anyways.
// The main upside is that ser/de/eq/row_hash
// need not do any alignment adjustments and carry a current offset.
// This removes a data dependence and could possibly improve instruction-level parallelism.

/// The layout of a fixed object
/// or the layout that fixed objects of a type will have.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The size object / expected object in bytes.
    pub size: u16,
    /// The alignment of the object / expected object in bytes.
    pub align: u16,
}

impl MemoryUsage for Layout {}

/// A type which knows what its layout is.
///
/// This does not refer to layout in Rust.
pub trait HasLayout {
    /// Returns the layout for objects of this type.
    fn layout(&self) -> &Layout;

    /// Returns the size, in bytes, for objects of this type.
    ///
    /// Intentionally returns `usize` rather than [`Size`],
    /// so callers will have to explicitly convert
    /// with [`row_size_for_bytes`].
    fn size(&self) -> usize {
        self.layout().size as usize
    }

    /// Returns the alignment, in bytes, for objects of this type.
    ///
    /// Intentionally returns `usize` rather than [`Size`],
    /// so callers will have to explicitly convert
    /// with [`row_size_for_bytes`].
    fn align(&self) -> usize {
        self.layout().align as usize
    }
}

/// Mostly a mirror of [`AlgebraicType`] annotated with a [`Layout`].
///
/// Notable differences from `AlgebraicType`:
///
/// - `Ref`s are not supported.
///   Supporting recursive types remains a TODO(future-work).
///   Note that the previous Spacetime datastore did not support recursive types in tables.
///
/// - Scalar types (`ty.is_scalar()`) are separated into [`PrimitveType`] (atomically-sized types like integers).
/// - Variable length types are separated into [`VarLenType`] (strings, arrays, and maps).
///   This separation allows cleaner pattern-matching, e.g. in `HasLayout::layout`,
///   where `VarLenType` returns a static ref to [`VAR_LEN_REF_LAYOUT`],
///   and `PrimitiveType` dispatches on its variant to return a static ref
///   to a type-specific `Layout`.
#[derive(Debug, PartialEq, Eq, Clone, EnumAsInner)]
pub enum AlgebraicTypeLayout {
    /// A sum type, annotated with its layout.
    Sum(SumTypeLayout),
    /// A product type, annotated with its layout.
    Product(ProductTypeLayout),
    /// A primitive type, annotated with its layout.
    Primitive(PrimitiveType),
    /// A variable length type, annotated with its layout.
    VarLen(VarLenType),
}

impl MemoryUsage for AlgebraicTypeLayout {
    fn heap_usage(&self) -> usize {
        match self {
            AlgebraicTypeLayout::Sum(x) => x.heap_usage(),
            AlgebraicTypeLayout::Product(x) => x.heap_usage(),
            AlgebraicTypeLayout::Primitive(x) => x.heap_usage(),
            AlgebraicTypeLayout::VarLen(x) => x.heap_usage(),
        }
    }
}

impl HasLayout for AlgebraicTypeLayout {
    fn layout(&self) -> &Layout {
        match self {
            Self::Sum(ty) => ty.layout(),
            Self::Product(ty) => ty.layout(),
            Self::Primitive(ty) => ty.layout(),
            Self::VarLen(ty) => ty.layout(),
        }
    }
}

#[allow(non_upper_case_globals)]
impl AlgebraicTypeLayout {
    pub const Bool: Self = Self::Primitive(PrimitiveType::Bool);
    pub const I8: Self = Self::Primitive(PrimitiveType::I8);
    pub const U8: Self = Self::Primitive(PrimitiveType::U8);
    pub const I16: Self = Self::Primitive(PrimitiveType::I16);
    pub const U16: Self = Self::Primitive(PrimitiveType::U16);
    pub const I32: Self = Self::Primitive(PrimitiveType::I32);
    pub const U32: Self = Self::Primitive(PrimitiveType::U32);
    pub const I64: Self = Self::Primitive(PrimitiveType::I64);
    pub const U64: Self = Self::Primitive(PrimitiveType::U64);
    pub const I128: Self = Self::Primitive(PrimitiveType::I128);
    pub const U128: Self = Self::Primitive(PrimitiveType::U128);
    pub const I256: Self = Self::Primitive(PrimitiveType::I256);
    pub const U256: Self = Self::Primitive(PrimitiveType::U256);
    pub const F32: Self = Self::Primitive(PrimitiveType::F32);
    pub const F64: Self = Self::Primitive(PrimitiveType::F64);
    pub const String: Self = Self::VarLen(VarLenType::String);
}

/// A collection of items, so that we can easily swap out the backing type.
type Collection<T> = Box<[T]>;

/// Fixed-length row portions must be at least large enough to store a `FreeCellRef`.
pub const MIN_ROW_SIZE: Size = Size(2);

/// Fixed-length row portions must also be sufficiently aligned to store a `FreeCellRef`.
pub const MIN_ROW_ALIGN: Size = Size(2);

/// Returns the minimum row size needed to store `required_bytes`
/// accounting for the minimum row size and alignment.
pub const fn row_size_for_bytes(required_bytes: usize) -> Size {
    // Manual `Ord::max` because that function is not `const`.
    if required_bytes > MIN_ROW_SIZE.len() {
        Size(align_to(required_bytes, MIN_ROW_ALIGN.len()) as u16)
    } else {
        MIN_ROW_SIZE
    }
}

/// Returns the minimum row size needed to store a `T`,
/// accounting for the minimum row size and alignment.
pub const fn row_size_for_type<T>() -> Size {
    row_size_for_bytes(mem::size_of::<T>())
}

/// The type of a row, annotated with a [`Layout`].
///
/// This type ensures that the minimum row size is adhered to.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RowTypeLayout(ProductTypeLayout);

impl MemoryUsage for RowTypeLayout {
    fn heap_usage(&self) -> usize {
        let Self(layout) = self;
        layout.heap_usage()
    }
}

impl RowTypeLayout {
    /// Returns a view of this row type as a product type.
    pub fn product(&self) -> &ProductTypeLayout {
        &self.0
    }

    /// Returns the row size for this row type.
    pub fn size(&self) -> Size {
        Size(self.0.size() as u16)
    }
}

impl From<ProductTypeLayout> for RowTypeLayout {
    fn from(mut cols: ProductTypeLayout) -> Self {
        cols.layout.size = row_size_for_bytes(cols.layout.size as usize).0;
        Self(cols)
    }
}

impl From<ProductType> for RowTypeLayout {
    fn from(ty: ProductType) -> Self {
        ProductTypeLayout::from(ty).into()
    }
}

impl HasLayout for RowTypeLayout {
    fn layout(&self) -> &Layout {
        self.0.layout()
    }
}

impl Index<usize> for RowTypeLayout {
    type Output = AlgebraicTypeLayout;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0.elements[index].ty
    }
}

/// A mirror of [`ProductType`] annotated with a [`Layout`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProductTypeLayout {
    /// The memoized layout of the product type.
    pub layout: Layout,
    /// The fields of the product type with their own layout annotations.
    pub elements: Collection<ProductTypeElementLayout>,
}

impl MemoryUsage for ProductTypeLayout {
    fn heap_usage(&self) -> usize {
        let Self { layout, elements } = self;
        layout.heap_usage() + elements.heap_usage()
    }
}

impl HasLayout for ProductTypeLayout {
    fn layout(&self) -> &Layout {
        &self.layout
    }
}

/// A mirrior of [`ProductTypeElement`] annotated with a [`Layout`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProductTypeElementLayout {
    /// The relative offset of a field's value to its parent product value.
    pub offset: u16,

    /// The type of the field.
    pub ty: AlgebraicTypeLayout,

    /// An optional name of the field.
    ///
    /// This allows us to convert back to `ProductTypeElement`,
    /// which we do when reporting type errors.
    pub name: Option<Box<str>>,
}

impl MemoryUsage for ProductTypeElementLayout {
    fn heap_usage(&self) -> usize {
        let Self { offset, ty, name } = self;
        offset.heap_usage() + ty.heap_usage() + name.heap_usage()
    }
}

/// A mirrior of [`SumType`] annotated with a [`Layout`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SumTypeLayout {
    /// The layout of a sum value of this sum type.
    pub layout: Layout,
    /// The variants of the sum type.
    pub variants: Collection<SumTypeVariantLayout>,
    /// The relative offset of a sum value's payload for sums of this type.
    /// Sum value tags are always at offset 0.
    pub payload_offset: u16,
}

impl MemoryUsage for SumTypeLayout {
    fn heap_usage(&self) -> usize {
        let Self {
            layout,
            variants,
            payload_offset,
        } = self;
        layout.heap_usage() + variants.heap_usage() + payload_offset.heap_usage()
    }
}

impl HasLayout for SumTypeLayout {
    fn layout(&self) -> &Layout {
        &self.layout
    }
}

/// A mirrior of [`SumTypeVariant`] annotated with a [`Layout`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SumTypeVariantLayout {
    /// The type of the variant.
    pub ty: AlgebraicTypeLayout,

    /// An optional name of the variant.
    ///
    /// This allows us to convert back to `SumTypeVariant`,
    /// which we do when reporting type errors.
    pub name: Option<Box<str>>,
}

impl MemoryUsage for SumTypeVariantLayout {
    fn heap_usage(&self) -> usize {
        let Self { ty, name } = self;
        ty.heap_usage() + name.heap_usage()
    }
}

impl MemoryUsage for PrimitiveType {}

impl HasLayout for PrimitiveType {
    fn layout(&self) -> &'static Layout {
        match self {
            Self::Bool | Self::I8 | Self::U8 => &Layout { size: 1, align: 1 },
            Self::I16 | Self::U16 => &Layout { size: 2, align: 2 },
            Self::I32 | Self::U32 | Self::F32 => &Layout { size: 4, align: 4 },
            Self::I64 | Self::U64 | Self::F64 => &Layout { size: 8, align: 8 },
            Self::I128 | Self::U128 => &Layout { size: 16, align: 16 },
            Self::I256 | Self::U256 => &Layout { size: 32, align: 32 },
        }
    }
}

/// Types requiring a `VarLenRef` indirection,
/// i.e. strings, arrays, and maps.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VarLenType {
    /// The string type corresponds to `AlgebraicType::String`.
    String,
    /// An array type. The whole outer `AlgebraicType` is stored here.
    ///
    /// Storing the whole `AlgebraicType` here allows us to directly call BSATN ser/de,
    /// and to report type errors.
    Array(Box<AlgebraicType>),
}

impl MemoryUsage for VarLenType {
    fn heap_usage(&self) -> usize {
        match self {
            VarLenType::String => 0,
            VarLenType::Array(x) => x.heap_usage(),
        }
    }
}

/// The layout of var-len objects. Aligned at a `u16` which it has 2 of.
const VAR_LEN_REF_LAYOUT: Layout = Layout { size: 4, align: 2 };
const _: () = assert!(VAR_LEN_REF_LAYOUT.size as usize == mem::size_of::<VarLenRef>());
const _: () = assert!(VAR_LEN_REF_LAYOUT.align as usize == mem::align_of::<VarLenRef>());

impl HasLayout for VarLenType {
    fn layout(&self) -> &Layout {
        &VAR_LEN_REF_LAYOUT
    }
}

// # Conversions from `AlgebraicType` and friends

impl From<AlgebraicType> for AlgebraicTypeLayout {
    fn from(ty: AlgebraicType) -> Self {
        match ty {
            AlgebraicType::Sum(sum) => AlgebraicTypeLayout::Sum(sum.into()),
            AlgebraicType::Product(prod) => AlgebraicTypeLayout::Product(prod.into()),

            AlgebraicType::String => AlgebraicTypeLayout::VarLen(VarLenType::String),
            AlgebraicType::Array(_) => AlgebraicTypeLayout::VarLen(VarLenType::Array(Box::new(ty))),

            AlgebraicType::Bool => AlgebraicTypeLayout::Bool,
            AlgebraicType::I8 => AlgebraicTypeLayout::I8,
            AlgebraicType::U8 => AlgebraicTypeLayout::U8,
            AlgebraicType::I16 => AlgebraicTypeLayout::I16,
            AlgebraicType::U16 => AlgebraicTypeLayout::U16,
            AlgebraicType::I32 => AlgebraicTypeLayout::I32,
            AlgebraicType::U32 => AlgebraicTypeLayout::U32,
            AlgebraicType::I64 => AlgebraicTypeLayout::I64,
            AlgebraicType::U64 => AlgebraicTypeLayout::U64,
            AlgebraicType::I128 => AlgebraicTypeLayout::I128,
            AlgebraicType::U128 => AlgebraicTypeLayout::U128,
            AlgebraicType::I256 => AlgebraicTypeLayout::I256,
            AlgebraicType::U256 => AlgebraicTypeLayout::U256,
            AlgebraicType::F32 => AlgebraicTypeLayout::F32,
            AlgebraicType::F64 => AlgebraicTypeLayout::F64,

            AlgebraicType::Ref(_) => todo!("Refs unsupported without typespace context"),
        }
    }
}

impl From<ProductType> for ProductTypeLayout {
    fn from(ty: ProductType) -> Self {
        let mut current_offset: usize = 0;

        // Minimum possible alignment is 1, even though minimum possible size is 0.
        // This is consistent with Rust.
        let mut max_child_align = 1;

        let elements = Vec::from(ty.elements)
            .into_iter()
            .map(|elem| {
                let layout_type: AlgebraicTypeLayout = elem.algebraic_type.into();
                let this_offset = align_to(current_offset, layout_type.align());
                max_child_align = usize::max(max_child_align, layout_type.align());

                current_offset = this_offset + layout_type.size();

                ProductTypeElementLayout {
                    offset: this_offset as u16,
                    name: elem.name,
                    ty: layout_type,
                }
            })
            .collect::<Vec<_>>()
            .into();

        let layout = Layout {
            align: max_child_align as u16,
            size: align_to(current_offset, max_child_align) as u16,
        };

        Self { layout, elements }
    }
}

impl From<SumType> for SumTypeLayout {
    fn from(ty: SumType) -> Self {
        let mut max_child_size = 0;

        // Minimum possible alignment is 1, even though minimum possible size is 0.
        // This is consistent with Rust.
        let mut max_child_align = 0;

        let variants = Vec::from(ty.variants)
            .into_iter()
            .map(|variant| {
                let layout_type: AlgebraicTypeLayout = variant.algebraic_type.into();

                max_child_align = usize::max(max_child_align, layout_type.align());
                max_child_size = usize::max(max_child_size, layout_type.size());

                SumTypeVariantLayout {
                    ty: layout_type,
                    name: variant.name,
                }
            })
            .collect::<Vec<_>>()
            .into();

        // Guarantees that tag fits inside align.
        let align = u16::max(max_child_align as u16, 1);

        // Ensure the payload field is sufficiently aligned for all its members.
        // `max_child_size` and `max_child_align` will already be consistent
        // if the most-aligned variant is also the largest,
        // but this is not necessarily the case.
        // E.g. if variant A is a product of 31 `u8`s, and variant B is a single `u64`,
        // `max_child_size` will be 31 and `max_child_align` will be 8.
        // Note that `payload_size` may be 0.
        let payload_size = align_to(max_child_size, max_child_align);

        // [tag | pad to align | payload]
        let size = align + payload_size as u16;
        let payload_offset = align;
        let layout = Layout { align, size };
        Self {
            layout,
            payload_offset,
            variants,
        }
    }
}

// # Conversions to `AlgebraicType` and friends
// Used for error reporting.

impl AlgebraicTypeLayout {
    /// Convert an `AlgebraicTypeLayout` back into an `AlgebraicType`,
    /// removing layout information.
    ///
    /// This operation is O(n) in the number of nodes in the argument,
    /// and may heap-allocate.
    /// It is intended for use in error paths, where performance is a secondary concern.
    pub fn algebraic_type(&self) -> AlgebraicType {
        match self {
            AlgebraicTypeLayout::Primitive(prim) => prim.algebraic_type(),
            AlgebraicTypeLayout::VarLen(var_len) => var_len.algebraic_type(),
            AlgebraicTypeLayout::Product(prod) => AlgebraicType::Product(prod.product_type()),
            AlgebraicTypeLayout::Sum(sum) => AlgebraicType::Sum(sum.sum_type()),
        }
    }
}

impl VarLenType {
    fn algebraic_type(&self) -> AlgebraicType {
        match self {
            VarLenType::String => AlgebraicType::String,
            VarLenType::Array(ty) => ty.as_ref().clone(),
        }
    }
}

impl ProductTypeLayout {
    pub(crate) fn product_type(&self) -> ProductType {
        ProductType {
            elements: self
                .elements
                .iter()
                .map(ProductTypeElementLayout::product_type_element)
                .collect(),
        }
    }

    /// Convert a `ProductTypeLayout` back into an `AlgebraicType::Product`,
    /// removing layout information.
    ///
    /// This operation is O(n) in the number of nodes in the argument,
    /// and will heap-allocate.
    /// It is intended for use in error paths, where performance is a secondary concern.
    pub fn algebraic_type(&self) -> AlgebraicType {
        AlgebraicType::Product(self.product_type())
    }
}

impl ProductTypeElementLayout {
    fn product_type_element(&self) -> ProductTypeElement {
        ProductTypeElement {
            algebraic_type: self.ty.algebraic_type(),
            name: self.name.clone(),
        }
    }
}

impl SumTypeLayout {
    fn sum_type(&self) -> SumType {
        SumType {
            variants: self
                .variants
                .iter()
                .map(SumTypeVariantLayout::sum_type_variant)
                .collect(),
        }
    }
}

impl SumTypeVariantLayout {
    fn sum_type_variant(&self) -> SumTypeVariant {
        SumTypeVariant {
            algebraic_type: self.ty.algebraic_type(),
            name: self.name.clone(),
        }
    }
}

// # Inspecting layout

impl SumTypeLayout {
    pub fn offset_of_variant_data(&self, _variant_tag: u8) -> usize {
        // Store the tag at the start, similar to BSATN.
        // Unlike BSATN, there is also padding.
        //
        // ```ignore
        // [ tag | padding to variant data align | variant data ]
        // ```
        //
        self.payload_offset as usize
    }

    pub fn offset_of_tag(&self) -> usize {
        // Store the tag at the start, similar to BSATN.
        //
        // ```ignore
        // [ tag | padding to variant data align | variant data ]
        // ```
        //
        0
    }
}

impl ProductTypeLayout {
    /// Returns the offset of the element at `field_idx`.
    pub fn offset_of_element(&self, field_idx: usize) -> usize {
        self.elements[field_idx].offset as usize
    }
}

/// Counts the number of [`VarLenGranule`] allocations required to store `val` in a page.
pub fn required_var_len_granules_for_row(val: &ProductValue) -> usize {
    fn traverse_av(val: &AlgebraicValue, count: &mut usize) {
        match val {
            AlgebraicValue::Product(val) => traverse_product(val, count),
            AlgebraicValue::Sum(val) => traverse_av(&val.value, count),
            AlgebraicValue::Array(_) => add_for_bytestring(bsatn_len(val), count),
            AlgebraicValue::String(val) => add_for_bytestring(val.len(), count),
            _ => (),
        }
    }

    fn traverse_product(val: &ProductValue, count: &mut usize) {
        for elt in val {
            traverse_av(elt, count);
        }
    }

    fn add_for_bytestring(len_in_bytes: usize, count: &mut usize) {
        *count += VarLenGranule::bytes_to_granules(len_in_bytes).0;
    }

    let mut required_granules: usize = 0;
    traverse_product(val, &mut required_granules);
    required_granules
}

/// Computes the size of `val` when BSATN encoding without actually encoding.
pub fn bsatn_len(val: &AlgebraicValue) -> usize {
    // We store arrays and maps BSATN-encoded,
    // so we need to go through BSATN encoding to determine the size of the resulting byte blob,
    // but we don't actually need that byte blob in this calculation,
    // instead, we can just count them as a serialization format.
    bsatn::to_len(val).unwrap()
}

#[cfg(test)]
mod test {
    use super::*;
    use itertools::Itertools as _;
    use proptest::collection::vec;
    use proptest::prelude::*;
    use spacetimedb_sats::proptest::generate_algebraic_type;

    #[test]
    fn align_to_expected() {
        fn assert_alignment(offset: usize, alignment: usize, expected: usize) {
            assert_eq!(
                align_to(offset, alignment),
                expected,
                "align_to({}, {}): expected {} but found {}",
                offset,
                alignment,
                expected,
                align_to(offset, alignment)
            );
        }

        for align in [1usize, 2, 4, 8, 16, 32, 64] {
            assert_alignment(0, align, 0);

            for offset in 1..=align {
                assert_alignment(offset, align, align);
            }
            for offset in (align + 1)..=(align * 2) {
                assert_alignment(offset, align, align * 2);
            }
        }
    }

    fn assert_size_align(ty: AlgebraicType, size: usize, align: usize) {
        let layout = AlgebraicTypeLayout::from(ty);
        assert_eq!(layout.size(), size);
        assert_eq!(layout.align(), align);
    }

    #[test]
    fn known_product_expected_size_align() {
        for (ty, size, align) in [
            (AlgebraicType::product::<[AlgebraicType; 0]>([]), 0, 1),
            (AlgebraicType::product([AlgebraicType::U8]), 1, 1),
            (AlgebraicType::product([AlgebraicType::I8]), 1, 1),
            (AlgebraicType::product([AlgebraicType::Bool]), 1, 1),
            (AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U8]), 2, 1),
            (AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U16]), 4, 2),
            (
                AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U8, AlgebraicType::U16]),
                4,
                2,
            ),
            (
                AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U16, AlgebraicType::U8]),
                6,
                2,
            ),
            (
                AlgebraicType::product([AlgebraicType::U16, AlgebraicType::U8, AlgebraicType::U8]),
                4,
                2,
            ),
            (AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U32]), 8, 4),
            (AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U64]), 16, 8),
            (AlgebraicType::product([AlgebraicType::U8, AlgebraicType::U128]), 32, 16),
            (AlgebraicType::product([AlgebraicType::U16, AlgebraicType::U8]), 4, 2),
            (AlgebraicType::product([AlgebraicType::U32, AlgebraicType::U8]), 8, 4),
            (AlgebraicType::product([AlgebraicType::U64, AlgebraicType::U8]), 16, 8),
            (AlgebraicType::product([AlgebraicType::U128, AlgebraicType::U8]), 32, 16),
            (AlgebraicType::product([AlgebraicType::U16, AlgebraicType::U16]), 4, 2),
            (AlgebraicType::product([AlgebraicType::U32, AlgebraicType::U32]), 8, 4),
            (AlgebraicType::product([AlgebraicType::U64, AlgebraicType::U64]), 16, 8),
            (
                AlgebraicType::product([AlgebraicType::U128, AlgebraicType::U128]),
                32,
                16,
            ),
            (AlgebraicType::product([AlgebraicType::String]), 4, 2),
            (
                AlgebraicType::product([AlgebraicType::String, AlgebraicType::U16]),
                6,
                2,
            ),
            (AlgebraicType::product([AlgebraicType::I8, AlgebraicType::I8]), 2, 1),
            (AlgebraicType::product([AlgebraicType::I8, AlgebraicType::I16]), 4, 2),
            (AlgebraicType::product([AlgebraicType::I8, AlgebraicType::I32]), 8, 4),
            (AlgebraicType::product([AlgebraicType::I8, AlgebraicType::I64]), 16, 8),
            (AlgebraicType::product([AlgebraicType::I8, AlgebraicType::I128]), 32, 16),
            (AlgebraicType::product([AlgebraicType::I16, AlgebraicType::I8]), 4, 2),
            (AlgebraicType::product([AlgebraicType::I32, AlgebraicType::I8]), 8, 4),
            (AlgebraicType::product([AlgebraicType::I64, AlgebraicType::I8]), 16, 8),
            (AlgebraicType::product([AlgebraicType::I128, AlgebraicType::I8]), 32, 16),
            (AlgebraicType::product([AlgebraicType::I16, AlgebraicType::I16]), 4, 2),
            (AlgebraicType::product([AlgebraicType::I32, AlgebraicType::I32]), 8, 4),
            (AlgebraicType::product([AlgebraicType::I64, AlgebraicType::I64]), 16, 8),
            (
                AlgebraicType::product([AlgebraicType::I128, AlgebraicType::I128]),
                32,
                16,
            ),
            (
                AlgebraicType::product([AlgebraicType::I256, AlgebraicType::U256]),
                64,
                32,
            ),
            (
                AlgebraicType::product([AlgebraicType::String, AlgebraicType::I16]),
                6,
                2,
            ),
        ] {
            assert_size_align(ty, size, align);
        }
    }

    #[test]
    fn known_sum_expected_size_align() {
        for (ty, size, align) in [
            (AlgebraicType::sum([AlgebraicType::U8]), 2, 1),
            (AlgebraicType::sum([AlgebraicType::I8]), 2, 1),
            (AlgebraicType::sum([AlgebraicType::Bool]), 2, 1),
            (AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::U8]), 2, 1),
            (AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::U16]), 4, 2),
            (AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::U32]), 8, 4),
            (AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::U64]), 16, 8),
            (AlgebraicType::sum([AlgebraicType::U8, AlgebraicType::U128]), 32, 16),
            (AlgebraicType::sum([AlgebraicType::U16, AlgebraicType::U8]), 4, 2),
            (AlgebraicType::sum([AlgebraicType::U32, AlgebraicType::U8]), 8, 4),
            (AlgebraicType::sum([AlgebraicType::U64, AlgebraicType::U8]), 16, 8),
            (AlgebraicType::sum([AlgebraicType::U128, AlgebraicType::U8]), 32, 16),
            (AlgebraicType::sum([AlgebraicType::U16, AlgebraicType::U16]), 4, 2),
            (AlgebraicType::sum([AlgebraicType::U32, AlgebraicType::U32]), 8, 4),
            (AlgebraicType::sum([AlgebraicType::U64, AlgebraicType::U64]), 16, 8),
            (AlgebraicType::sum([AlgebraicType::U128, AlgebraicType::U128]), 32, 16),
            (AlgebraicType::sum([AlgebraicType::String]), 6, 2),
            (AlgebraicType::sum([AlgebraicType::String, AlgebraicType::U16]), 6, 2),
            (AlgebraicType::sum([AlgebraicType::I8, AlgebraicType::I8]), 2, 1),
            (AlgebraicType::sum([AlgebraicType::I8, AlgebraicType::I16]), 4, 2),
            (AlgebraicType::sum([AlgebraicType::I8, AlgebraicType::I32]), 8, 4),
            (AlgebraicType::sum([AlgebraicType::I8, AlgebraicType::I64]), 16, 8),
            (AlgebraicType::sum([AlgebraicType::I8, AlgebraicType::I128]), 32, 16),
            (AlgebraicType::sum([AlgebraicType::I16, AlgebraicType::I8]), 4, 2),
            (AlgebraicType::sum([AlgebraicType::I32, AlgebraicType::I8]), 8, 4),
            (AlgebraicType::sum([AlgebraicType::I64, AlgebraicType::I8]), 16, 8),
            (AlgebraicType::sum([AlgebraicType::I128, AlgebraicType::I8]), 32, 16),
            (AlgebraicType::sum([AlgebraicType::I16, AlgebraicType::I16]), 4, 2),
            (AlgebraicType::sum([AlgebraicType::I32, AlgebraicType::I32]), 8, 4),
            (AlgebraicType::sum([AlgebraicType::I64, AlgebraicType::I64]), 16, 8),
            (AlgebraicType::sum([AlgebraicType::I128, AlgebraicType::I128]), 32, 16),
            (AlgebraicType::sum([AlgebraicType::I256, AlgebraicType::I128]), 64, 32),
            (AlgebraicType::sum([AlgebraicType::I256, AlgebraicType::U256]), 64, 32),
            (AlgebraicType::sum([AlgebraicType::String, AlgebraicType::I16]), 6, 2),
        ] {
            assert_size_align(ty, size, align);
        }
    }

    proptest! {
        fn variant_order_irrelevant_for_layout(
            variants in vec(generate_algebraic_type(), 0..5)
        ) {
            use spacetimedb_sats::SumTypeVariant;

            let len = variants.len();
            // Compute all permutations of the sum type with `variants`.
            let sum_permutations = variants
                .into_iter()
                .permutations(len)
                .map(|vars| vars.into_iter().map(SumTypeVariant::from).collect::<Box<[_]>>())
                .map(AlgebraicType::sum);
            // Compute the layouts of each equivalent sum type.
            let mut sum_layout_perms = sum_permutations
                .map(AlgebraicTypeLayout::from)
                .map(|ty| *ty.layout());
            // Assert that they are in fact equal in terms of layout.
            prop_assert!(sum_layout_perms.all_equal());
        }

        #[test]
        fn size_always_multiple_of_align(ty in generate_algebraic_type()) {
            let layout = AlgebraicTypeLayout::from(ty);

            if layout.size() == 0 {
                assert_eq!(layout.align(), 1);
            } else {
                assert_eq!(layout.size() % layout.align(), 0);
            }
        }
    }
}
