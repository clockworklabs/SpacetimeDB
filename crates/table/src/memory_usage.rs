use std::hash::{BuildHasher, Hash};
use std::mem;

use spacetimedb_sats::{
    algebraic_value::Packed, i256, u256, AlgebraicType, AlgebraicValue, ArrayType, ArrayValue, MapType, ProductType,
    ProductTypeElement, ProductValue, SumType, SumTypeVariant, SumValue,
};

/// For inspecting how much memory a value is using.
///
/// This trait specifically measures heap memory. If you want to measure stack memory too, add
/// `mem::size_of_val()` to it. (This only really matters for the outermost type in a hierarchy.)
pub trait MemoryUsage {
    /// The **heap** memory usage of this type. The default implementation returns 0.
    #[inline(always)]
    fn memory_usage(&self) -> usize {
        0
    }
}

impl MemoryUsage for bool {}
impl MemoryUsage for u8 {}
impl MemoryUsage for u16 {}
impl MemoryUsage for u32 {}
impl MemoryUsage for u64 {}
impl MemoryUsage for u128 {}
impl MemoryUsage for u256 {}
impl MemoryUsage for usize {}
impl MemoryUsage for i8 {}
impl MemoryUsage for i16 {}
impl MemoryUsage for i32 {}
impl MemoryUsage for i64 {}
impl MemoryUsage for i128 {}
impl MemoryUsage for i256 {}
impl MemoryUsage for isize {}
impl MemoryUsage for f32 {}
impl MemoryUsage for f64 {}

impl MemoryUsage for spacetimedb_sats::F32 {}
impl MemoryUsage for spacetimedb_sats::F64 {}

impl<T: MemoryUsage + ?Sized> MemoryUsage for Box<T> {
    fn memory_usage(&self) -> usize {
        mem::size_of_val::<T>(self) + T::memory_usage(self)
    }
}

impl<T: MemoryUsage + ?Sized> MemoryUsage for std::sync::Arc<T> {
    fn memory_usage(&self) -> usize {
        let refcounts = mem::size_of::<usize>() * 2;
        refcounts + mem::size_of_val::<T>(self) + T::memory_usage(self)
    }
}

impl<T: MemoryUsage + ?Sized> MemoryUsage for std::rc::Rc<T> {
    fn memory_usage(&self) -> usize {
        let refcounts = mem::size_of::<usize>() * 2;
        refcounts + mem::size_of_val::<T>(self) + T::memory_usage(self)
    }
}

impl<T: MemoryUsage> MemoryUsage for [T] {
    fn memory_usage(&self) -> usize {
        self.iter().map(T::memory_usage).sum()
    }
}

impl MemoryUsage for str {}

impl<T: MemoryUsage> MemoryUsage for Option<T> {
    fn memory_usage(&self) -> usize {
        self.as_ref().map_or(0, T::memory_usage)
    }
}

impl<A: MemoryUsage, B: MemoryUsage> MemoryUsage for (A, B) {
    fn memory_usage(&self) -> usize {
        self.0.memory_usage() + self.1.memory_usage()
    }
}

impl MemoryUsage for String {
    fn memory_usage(&self) -> usize {
        self.capacity()
    }
}

impl<T: MemoryUsage> MemoryUsage for Vec<T> {
    fn memory_usage(&self) -> usize {
        self.capacity() * mem::size_of::<T>() + self.iter().map(T::memory_usage).sum::<usize>()
    }
}

impl<K: MemoryUsage + Eq + Hash, V: MemoryUsage, S: BuildHasher> MemoryUsage
    for spacetimedb_data_structures::map::HashMap<K, V, S>
{
    fn memory_usage(&self) -> usize {
        self.allocation_size()
            + self
                .iter()
                .map(|(k, v)| k.memory_usage() + v.memory_usage())
                .sum::<usize>()
    }
}

impl<K: MemoryUsage, V: MemoryUsage> MemoryUsage for std::collections::BTreeMap<K, V> {
    fn memory_usage(&self) -> usize {
        self.iter()
            .map(|(k, v)| k.memory_usage() + v.memory_usage())
            .sum::<usize>()
    }
}

impl<A: smallvec::Array> MemoryUsage for smallvec::SmallVec<A>
where
    A::Item: MemoryUsage,
{
    fn memory_usage(&self) -> usize {
        self.as_slice().memory_usage()
            + if self.spilled() {
                self.capacity() * mem::size_of::<A::Item>()
            } else {
                0
            }
    }
}

impl MemoryUsage for spacetimedb_primitives::TableId {}
impl MemoryUsage for spacetimedb_primitives::SequenceId {}
impl MemoryUsage for spacetimedb_primitives::ConstraintId {}
impl MemoryUsage for spacetimedb_primitives::IndexId {}
impl MemoryUsage for spacetimedb_primitives::ColId {}
impl MemoryUsage for spacetimedb_primitives::ColList {
    fn memory_usage(&self) -> usize {
        self.heap_size()
    }
}

impl MemoryUsage for AlgebraicValue {
    fn memory_usage(&self) -> usize {
        match self {
            AlgebraicValue::Sum(x) => x.memory_usage(),
            AlgebraicValue::Product(x) => x.memory_usage(),
            AlgebraicValue::Array(x) => x.memory_usage(),
            AlgebraicValue::Map(x) => x.memory_usage(),
            AlgebraicValue::String(x) => x.memory_usage(),
            _ => 0,
        }
    }
}
impl MemoryUsage for SumValue {
    fn memory_usage(&self) -> usize {
        self.value.memory_usage()
    }
}
impl MemoryUsage for ProductValue {
    fn memory_usage(&self) -> usize {
        self.elements.memory_usage()
    }
}
impl MemoryUsage for ArrayValue {
    fn memory_usage(&self) -> usize {
        match self {
            ArrayValue::Sum(v) => v.memory_usage(),
            ArrayValue::Product(v) => v.memory_usage(),
            ArrayValue::Bool(v) => v.memory_usage(),
            ArrayValue::I8(v) => v.memory_usage(),
            ArrayValue::U8(v) => v.memory_usage(),
            ArrayValue::I16(v) => v.memory_usage(),
            ArrayValue::U16(v) => v.memory_usage(),
            ArrayValue::I32(v) => v.memory_usage(),
            ArrayValue::U32(v) => v.memory_usage(),
            ArrayValue::I64(v) => v.memory_usage(),
            ArrayValue::U64(v) => v.memory_usage(),
            ArrayValue::I128(v) => v.memory_usage(),
            ArrayValue::U128(v) => v.memory_usage(),
            ArrayValue::I256(v) => v.memory_usage(),
            ArrayValue::U256(v) => v.memory_usage(),
            ArrayValue::F32(v) => v.memory_usage(),
            ArrayValue::F64(v) => v.memory_usage(),
            ArrayValue::String(v) => v.memory_usage(),
            ArrayValue::Array(v) => v.memory_usage(),
            ArrayValue::Map(v) => v.memory_usage(),
        }
    }
}
impl MemoryUsage for AlgebraicType {
    fn memory_usage(&self) -> usize {
        match self {
            AlgebraicType::Ref(_) => 0,
            AlgebraicType::Sum(x) => x.memory_usage(),
            AlgebraicType::Product(x) => x.memory_usage(),
            AlgebraicType::Array(x) => x.memory_usage(),
            AlgebraicType::Map(x) => x.memory_usage(),
            AlgebraicType::String
            | AlgebraicType::Bool
            | AlgebraicType::I8
            | AlgebraicType::U8
            | AlgebraicType::I16
            | AlgebraicType::U16
            | AlgebraicType::I32
            | AlgebraicType::U32
            | AlgebraicType::I64
            | AlgebraicType::U64
            | AlgebraicType::I128
            | AlgebraicType::U128
            | AlgebraicType::I256
            | AlgebraicType::U256
            | AlgebraicType::F32
            | AlgebraicType::F64 => 0,
        }
    }
}
impl MemoryUsage for SumType {
    fn memory_usage(&self) -> usize {
        self.variants.memory_usage()
    }
}
impl MemoryUsage for SumTypeVariant {
    fn memory_usage(&self) -> usize {
        self.name.memory_usage() + self.algebraic_type.memory_usage()
    }
}
impl MemoryUsage for ProductType {
    fn memory_usage(&self) -> usize {
        self.elements.memory_usage()
    }
}
impl MemoryUsage for ProductTypeElement {
    fn memory_usage(&self) -> usize {
        self.name.memory_usage() + self.algebraic_type.memory_usage()
    }
}
impl MemoryUsage for ArrayType {
    fn memory_usage(&self) -> usize {
        self.elem_ty.memory_usage()
    }
}
impl MemoryUsage for MapType {
    fn memory_usage(&self) -> usize {
        self.key_ty.memory_usage() + self.ty.memory_usage()
    }
}

impl<T: MemoryUsage + Copy> MemoryUsage for Packed<T> {
    fn memory_usage(&self) -> usize {
        { self.0 }.memory_usage()
    }
}

impl MemoryUsage for spacetimedb_lib::Address {}
impl MemoryUsage for spacetimedb_lib::Identity {}
