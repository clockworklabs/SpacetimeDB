use crate::{AlgebraicValue, ArrayValue, MapValue, ProductValue, SumValue, F32, F64};
use core::cmp::Ordering;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum AVRefGen<'a, P> {
    Sum(&'a SumValue),
    Product(P),
    Array(&'a ArrayValue),
    Map(&'a MapValue),
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    I128(i128),
    U128(u128),
    F32(F32),
    F64(F64),
    String(&'a str),
}

type AVRef<'a, I> = AVRefGen<'a, ProductValueRef<'a, I>>;

#[derive(Debug, Copy, Clone, Hash)]
enum ProductValueRef<'a, I> {
    Immediate(&'a ProductValue),
    Iter(I),
}

impl<'a, I: Clone + Iterator<Item = &'a AlgebraicValue>> PartialEq for ProductValueRef<'a, I> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Immediate(l), Self::Immediate(r)) => l == r,
            (Self::Iter(l), Self::Iter(r)) => l.clone().eq(r.clone()),
            (Self::Immediate(l), Self::Iter(r)) => l.elements.iter().eq(r.clone()),
            (Self::Iter(l), Self::Immediate(r)) => r.elements.iter().eq(l.clone()),
        }
    }
}

impl<'a, I: Clone + Iterator<Item = &'a AlgebraicValue>> Eq for ProductValueRef<'a, I> {}

impl<'a, I: Clone + Iterator<Item = &'a AlgebraicValue>> PartialOrd for ProductValueRef<'a, I> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a, I: Clone + Iterator<Item = &'a AlgebraicValue>> Ord for ProductValueRef<'a, I> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Immediate(l), Self::Immediate(r)) => l.cmp(r),
            (Self::Iter(l), Self::Iter(r)) => l.clone().cmp(r.clone()),
            (Self::Immediate(l), Self::Iter(r)) => l.elements.iter().cmp(r.clone()),
            (Self::Iter(l), Self::Immediate(r)) => r.elements.iter().cmp(l.clone()),
        }
    }
}

input = [av0, av1]
select * from foo where foo.bar = #0 AND foo.baz = #1
index on foo.(bar,baz)

let compound = Vec::new();
compound.push(av0.clone());
compound.push(av1.clone());
