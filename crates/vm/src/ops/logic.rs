use spacetimedb_sats::algebraic_value::AlgebraicValue;

use crate::expr::Code;
use crate::functions::Args;
use crate::ops::shared::to_bool;
use crate::program::ProgramRef;

fn _bool_op<F>(args: Args<'_>, f: F) -> Code
where
    F: Fn(bool, bool) -> bool,
{
    let result = match args {
        Args::Unary(x) => f(to_bool(x).unwrap(), true),
        Args::Binary(lhs, rhs) => f(to_bool(lhs).unwrap(), to_bool(rhs).unwrap()),
        Args::Splat(args) => args.iter().fold(true, |lhs, rhs| f(lhs, to_bool(rhs).unwrap())),
    };

    Code::Value(result.into())
}

fn _cmp_op<F>(args: Args<'_>, f: F) -> Code
where
    F: Fn(&AlgebraicValue, &AlgebraicValue) -> bool,
{
    let result = match args {
        Args::Unary(_) => unreachable!("Calling a binary op with one parameter"),
        Args::Binary(lhs, rhs) => f(lhs, rhs),
        Args::Splat(args) => args.iter().fold(true, |lhs, rhs| f(&AlgebraicValue::from(lhs), rhs)),
    };

    Code::Value(result.into())
}

pub(crate) fn eq(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _cmp_op(args, |a, b| a == b)
}

pub(crate) fn not_eq(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _cmp_op(args, |a, b| a != b)
}

pub(crate) fn less(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _cmp_op(args, |a, b| a < b)
}

pub(crate) fn less_than(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _cmp_op(args, |a, b| a <= b)
}

pub(crate) fn greater(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _cmp_op(args, |a, b| a > b)
}

pub(crate) fn greater_than(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _cmp_op(args, |a, b| a >= b)
}

pub(crate) fn and(__p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _bool_op(args, |a, b| a && b)
}

pub(crate) fn not(__p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _bool_op(args, |a, b| !(a && b))
}

pub(crate) fn or(__p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _bool_op(args, |a, b| a || b)
}
