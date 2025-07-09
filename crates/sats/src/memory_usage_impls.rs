use crate::{
    algebraic_value::Packed, AlgebraicType, AlgebraicValue, ArrayType, ArrayValue, ProductType, ProductTypeElement,
    ProductValue, SumType, SumTypeVariant, SumValue,
};
use spacetimedb_memory_usage::MemoryUsage;

impl MemoryUsage for AlgebraicValue {
    fn heap_usage(&self) -> usize {
        match self {
            AlgebraicValue::Sum(x) => x.heap_usage(),
            AlgebraicValue::Product(x) => x.heap_usage(),
            AlgebraicValue::Array(x) => x.heap_usage(),
            AlgebraicValue::String(x) => x.heap_usage(),
            _ => 0,
        }
    }
}
impl MemoryUsage for SumValue {
    fn heap_usage(&self) -> usize {
        self.value.heap_usage()
    }
}
impl MemoryUsage for ProductValue {
    fn heap_usage(&self) -> usize {
        self.elements.heap_usage()
    }
}
impl MemoryUsage for ArrayValue {
    fn heap_usage(&self) -> usize {
        match self {
            ArrayValue::Sum(v) => v.heap_usage(),
            ArrayValue::Product(v) => v.heap_usage(),
            ArrayValue::Bool(v) => v.heap_usage(),
            ArrayValue::I8(v) => v.heap_usage(),
            ArrayValue::U8(v) => v.heap_usage(),
            ArrayValue::I16(v) => v.heap_usage(),
            ArrayValue::U16(v) => v.heap_usage(),
            ArrayValue::I32(v) => v.heap_usage(),
            ArrayValue::U32(v) => v.heap_usage(),
            ArrayValue::I64(v) => v.heap_usage(),
            ArrayValue::U64(v) => v.heap_usage(),
            ArrayValue::I128(v) => v.heap_usage(),
            ArrayValue::U128(v) => v.heap_usage(),
            ArrayValue::I256(v) => v.heap_usage(),
            ArrayValue::U256(v) => v.heap_usage(),
            ArrayValue::F32(v) => v.heap_usage(),
            ArrayValue::F64(v) => v.heap_usage(),
            ArrayValue::String(v) => v.heap_usage(),
            ArrayValue::Array(v) => v.heap_usage(),
        }
    }
}
impl MemoryUsage for AlgebraicType {
    fn heap_usage(&self) -> usize {
        match self {
            AlgebraicType::Ref(_) => 0,
            AlgebraicType::Sum(x) => x.heap_usage(),
            AlgebraicType::Product(x) => x.heap_usage(),
            AlgebraicType::Array(x) => x.heap_usage(),
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
    fn heap_usage(&self) -> usize {
        self.variants.heap_usage()
    }
}
impl MemoryUsage for SumTypeVariant {
    fn heap_usage(&self) -> usize {
        self.name.heap_usage() + self.algebraic_type.heap_usage()
    }
}
impl MemoryUsage for ProductType {
    fn heap_usage(&self) -> usize {
        self.elements.heap_usage()
    }
}
impl MemoryUsage for ProductTypeElement {
    fn heap_usage(&self) -> usize {
        self.name.heap_usage() + self.algebraic_type.heap_usage()
    }
}
impl MemoryUsage for ArrayType {
    fn heap_usage(&self) -> usize {
        self.elem_ty.heap_usage()
    }
}

impl<T: MemoryUsage + Copy> MemoryUsage for Packed<T> {
    fn heap_usage(&self) -> usize {
        { self.0 }.heap_usage()
    }
}
