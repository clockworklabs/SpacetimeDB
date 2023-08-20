use spacetimedb_sats::algebraic_value::AlgebraicValue;

pub fn bin_op<T, Op>(op: Op, x: T, y: T) -> AlgebraicValue
where
    Op: Fn(T, T) -> T,
    AlgebraicValue: From<T>,
{
    op(x, y).into()
}

pub(crate) fn to_bool(of: &AlgebraicValue) -> Option<bool> {
    of.as_bool().copied()
}
