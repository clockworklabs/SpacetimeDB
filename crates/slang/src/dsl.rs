//! Utilities for build valid constructs for the vm.
use spacetimedb_lib::relation::{Header, MemTable};
use spacetimedb_sats::{AlgebraicValue, ProductValue};

pub fn scalar<T: Into<AlgebraicValue>>(of: T) -> AlgebraicValue {
    of.into()
}

pub fn mem_table<H, I, T>(head: H, iter: I) -> MemTable
where
    H: Into<Header>,
    I: IntoIterator<Item = T>,
    T: Into<ProductValue>,
{
    MemTable::from_iter(head.into(), iter.into_iter().map(Into::into))
}
