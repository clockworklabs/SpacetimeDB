use ethnum::{i256, u256};

use crate::{algebraic_value::Packed, AlgebraicValue, ArrayValue, ProductValue, SumValue, F32, F64};

pub trait SizeOf {
    /// Returns the unpadded size in bytes of an [AlgebraicValue] or primitive
    fn size_of(&self) -> usize;
}

macro_rules! impl_size_of_primitive {
    ($prim:ty) => {
        impl SizeOf for $prim {
            fn size_of(&self) -> usize {
                std::mem::size_of::<Self>()
            }
        }
    };
    ($($prim:ty,)*) => {
        $(impl_size_of_primitive!($prim);)*
    };
}

impl_size_of_primitive!(
    bool,
    u8,
    i8,
    u16,
    i16,
    u32,
    i32,
    u64,
    i64,
    u128,
    i128,
    Packed<u128>,
    Packed<i128>,
    u256,
    i256,
    F32,
    F64,
);

impl SizeOf for Box<str> {
    fn size_of(&self) -> usize {
        self.len()
    }
}

impl SizeOf for AlgebraicValue {
    fn size_of(&self) -> usize {
        match self {
            Self::Min | Self::Max => unreachable!(),
            Self::String(x) => x.size_of(),
            Self::Bool(x) => x.size_of(),
            Self::U8(x) => x.size_of(),
            Self::I8(x) => x.size_of(),
            Self::U16(x) => x.size_of(),
            Self::I16(x) => x.size_of(),
            Self::U32(x) => x.size_of(),
            Self::I32(x) => x.size_of(),
            Self::U64(x) => x.size_of(),
            Self::I64(x) => x.size_of(),
            Self::U128(x) => x.size_of(),
            Self::I128(x) => x.size_of(),
            Self::U256(x) => x.size_of(),
            Self::I256(x) => x.size_of(),
            Self::F32(x) => x.size_of(),
            Self::F64(x) => x.size_of(),
            Self::Sum(x) => x.size_of(),
            Self::Product(x) => x.size_of(),
            Self::Array(x) => x.size_of(),
        }
    }
}

impl SizeOf for SumValue {
    fn size_of(&self) -> usize {
        1 + self.value.size_of()
    }
}

impl SizeOf for ProductValue {
    fn size_of(&self) -> usize {
        self.elements.size_of()
    }
}

impl<T> SizeOf for [T]
where
    T: SizeOf,
{
    fn size_of(&self) -> usize {
        self.iter().map(|elt| elt.size_of()).sum()
    }
}

impl SizeOf for ArrayValue {
    fn size_of(&self) -> usize {
        match self {
            Self::Sum(elts) => elts.size_of(),
            Self::Product(elts) => elts.size_of(),
            Self::Bool(elts) => elts.size_of(),
            Self::I8(elts) => elts.size_of(),
            Self::U8(elts) => elts.size_of(),
            Self::I16(elts) => elts.size_of(),
            Self::U16(elts) => elts.size_of(),
            Self::I32(elts) => elts.size_of(),
            Self::U32(elts) => elts.size_of(),
            Self::I64(elts) => elts.size_of(),
            Self::U64(elts) => elts.size_of(),
            Self::I128(elts) => elts.size_of(),
            Self::U128(elts) => elts.size_of(),
            Self::I256(elts) => elts.size_of(),
            Self::U256(elts) => elts.size_of(),
            Self::F32(elts) => elts.size_of(),
            Self::F64(elts) => elts.size_of(),
            Self::String(elts) => elts.size_of(),
            Self::Array(elts) => elts.size_of(),
        }
    }
}
