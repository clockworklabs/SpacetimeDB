use crate::errors::{ErrorType, ErrorVm};
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use std::fmt::Display;
use std::str::FromStr;

fn _parse<F>(value: &str, ty: &AlgebraicType) -> Result<AlgebraicValue, ErrorVm>
where
    F: FromStr + Into<AlgebraicValue>,
    <F as FromStr>::Err: Display,
{
    match value.parse::<F>() {
        Ok(x) => Ok(x.into()),
        Err(err) => Err(ErrorType::Parse {
            value: value.to_string(),
            ty: ty.to_satn(),
            err: err.to_string(),
        }
        .into()),
    }
}

/// Parse a `&str` into [AlgebraicValue] using the supplied [AlgebraicType].
///
/// ```
/// use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
/// use spacetimedb_vm::errors::ErrorLang;
/// use spacetimedb_vm::ops::parse::parse;
///
/// assert_eq!(parse("1", &AlgebraicType::I32).map_err(ErrorLang::from), Ok(AlgebraicValue::I32(1)));
/// assert_eq!(parse("true", &AlgebraicType::Bool).map_err(ErrorLang::from), Ok(AlgebraicValue::Bool(true)));
/// assert_eq!(parse("1.0", &AlgebraicType::F64).map_err(ErrorLang::from), Ok(AlgebraicValue::F64(1.0f64.into())));
/// assert!(parse("bananas", &AlgebraicType::I32).is_err());
/// ```
pub fn parse(value: &str, ty: &AlgebraicType) -> Result<AlgebraicValue, ErrorVm> {
    match ty {
        &AlgebraicType::Bool => _parse::<bool>(value, ty),
        &AlgebraicType::I8 => _parse::<i8>(value, ty),
        &AlgebraicType::U8 => _parse::<u8>(value, ty),
        &AlgebraicType::I16 => _parse::<i16>(value, ty),
        &AlgebraicType::U16 => _parse::<u16>(value, ty),
        &AlgebraicType::I32 => _parse::<i32>(value, ty),
        &AlgebraicType::U32 => _parse::<u32>(value, ty),
        &AlgebraicType::I64 => _parse::<i64>(value, ty),
        &AlgebraicType::U64 => _parse::<u64>(value, ty),
        &AlgebraicType::I128 => _parse::<i128>(value, ty),
        &AlgebraicType::U128 => _parse::<u128>(value, ty),
        &AlgebraicType::F32 => _parse::<f32>(value, ty),
        &AlgebraicType::F64 => _parse::<f64>(value, ty),
        &AlgebraicType::String => Ok(AlgebraicValue::String(value.to_string())),
        x => Err(ErrorVm::Unsupported(format!(
            "Can't parse '{value}' to {}",
            x.to_satn_pretty()
        ))),
    }
}
