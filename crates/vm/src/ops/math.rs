use std::ops::*;

use crate::expr::Code;
use crate::functions::Args;
use crate::ops::shared::bin_op;
use crate::program::ProgramRef;
use spacetimedb_sats::algebraic_value::AlgebraicValue;

macro_rules! math_op {
    ($name:ident, $op:path) => {
        pub(crate) fn $name(lhs: &AlgebraicValue, rhs: &AlgebraicValue) -> AlgebraicValue {
            use AlgebraicValue::*;
            match (lhs, rhs) {
                (U8(a), U8(b)) => bin_op::<u8, _>($op, *a, *b),
                (I8(a), I8(b)) => bin_op::<i8, _>($op, *a, *b),
                (U16(a), U16(b)) => bin_op::<u16, _>($op, *a, *b),
                (I16(a), I16(b)) => bin_op::<i16, _>($op, *a, *b),
                (U32(a), U32(b)) => bin_op::<u32, _>($op, *a, *b),
                (I32(a), I32(b)) => bin_op::<i32, _>($op, *a, *b),
                (U64(a), U64(b)) => bin_op::<u64, _>($op, *a, *b),
                (I64(a), I64(b)) => bin_op::<i64, _>($op, *a, *b),
                (U128(a), U128(b)) => bin_op::<u128, _>($op, *a, *b),
                (I128(a), I128(b)) => bin_op::<i128, _>($op, *a, *b),
                (F32(a), F32(b)) => bin_op::<f32, _>($op, a.into_inner(), b.into_inner()),
                (F64(a), F64(b)) => bin_op::<f64, _>($op, a.into_inner(), b.into_inner()),
                _ => unreachable!("Calling a math op with invalid param value"),
            }
        }
    };
}

math_op!(math_add, Add::add);
math_op!(math_minus, Sub::sub);
math_op!(math_mul, Mul::mul);
math_op!(math_div, Div::div);

fn _math_op<F>(args: Args<'_>, f: F) -> Code
where
    F: Fn(&AlgebraicValue, &AlgebraicValue) -> AlgebraicValue,
{
    let result = match args {
        Args::Binary(lhs, rhs) => f(lhs, rhs),
        Args::Splat(args) => {
            assert!(args.len() >= 2);
            let first = args[0].clone();

            args[1..].iter().fold(first, |ref a, b| f(a, b))
        }
        Args::Unary(_) => unreachable!("Calling a binary op with one parameter"),
    };
    Code::Value(result)
}

pub(crate) fn add(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _math_op(args, math_add)
}

pub(crate) fn minus(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _math_op(args, math_minus)
}

pub(crate) fn mul(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _math_op(args, math_mul)
}

pub(crate) fn div(_p: ProgramRef<'_>, args: Args<'_>) -> Code {
    _math_op(args, math_div)
}
