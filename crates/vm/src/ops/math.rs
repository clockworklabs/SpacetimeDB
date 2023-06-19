use std::ops::*;

use crate::expr::Code;
use crate::functions::Args;
use crate::ops::shared::bin_op;
use crate::program::ProgramRef;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::builtin_value::BuiltinValue;

macro_rules! math_op {
    ($name:ident, $op:path) => {
        pub(crate) fn $name(lhs: &AlgebraicValue, rhs: &AlgebraicValue) -> AlgebraicValue {
            match (lhs.as_builtin(), rhs.as_builtin()) {
                (Some(lhs), Some(rhs)) => match (lhs, rhs) {
                    (BuiltinValue::U8(a), BuiltinValue::U8(b)) => bin_op::<u8, _>($op, *a, *b),
                    (BuiltinValue::I8(a), BuiltinValue::I8(b)) => bin_op::<i8, _>($op, *a, *b),
                    (BuiltinValue::U16(a), BuiltinValue::U16(b)) => bin_op::<u16, _>($op, *a, *b),
                    (BuiltinValue::I16(a), BuiltinValue::I16(b)) => bin_op::<i16, _>($op, *a, *b),
                    (BuiltinValue::U32(a), BuiltinValue::U32(b)) => bin_op::<u32, _>($op, *a, *b),
                    (BuiltinValue::I32(a), BuiltinValue::I32(b)) => bin_op::<i32, _>($op, *a, *b),
                    (BuiltinValue::U64(a), BuiltinValue::U64(b)) => bin_op::<u64, _>($op, *a, *b),
                    (BuiltinValue::I64(a), BuiltinValue::I64(b)) => bin_op::<i64, _>($op, *a, *b),
                    (BuiltinValue::U128(a), BuiltinValue::U128(b)) => bin_op::<u128, _>($op, *a, *b),
                    (BuiltinValue::I128(a), BuiltinValue::I128(b)) => bin_op::<i128, _>($op, *a, *b),
                    (BuiltinValue::F32(a), BuiltinValue::F32(b)) => {
                        bin_op::<f32, _>($op, a.into_inner(), b.into_inner())
                    }
                    (BuiltinValue::F64(a), BuiltinValue::F64(b)) => {
                        bin_op::<f64, _>($op, a.into_inner(), b.into_inner())
                    }
                    _ => unreachable!("Calling a math op with invalid param value"),
                },
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
