use crate::errors::{ErrorType, ErrorVm};
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{i256, u256, AlgebraicType, AlgebraicValue, ProductType, SumType};
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

/// Try to parse `tag_name` for a simple enum on `sum` into a valid `tag` value of `AlgebraicValue`
pub fn parse_simple_enum(sum: &SumType, tag_name: &str) -> Result<AlgebraicValue, ErrorVm> {
    if let Some((pos, _tag)) = sum.get_variant_simple(tag_name) {
        Ok(AlgebraicValue::enum_simple(pos))
    } else {
        Err(ErrorVm::Unsupported(format!(
            "Not found enum tag '{tag_name}' or not a simple enum: {}",
            sum.to_satn_pretty()
        )))
    }
}

/// Try to parse `value` as [`Identity`] or [`ConnectionId`].
pub fn parse_product(product: &ProductType, value: &str) -> Result<AlgebraicValue, ErrorVm> {
    if product.is_identity() {
        return Ok(Identity::from_hex(value.trim_start_matches("0x"))
            .map_err(|err| ErrorVm::Other(err.into()))?
            .into());
    }
    if product.is_connection_id() {
        return Ok(ConnectionId::from_hex(value.trim_start_matches("0x"))
            .map_err(ErrorVm::Other)?
            .into());
    }
    Err(ErrorVm::Unsupported(format!(
        "Can't parse '{value}' to {}",
        product.to_satn_pretty()
    )))
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
/// assert_eq!(parse("Player", &AlgebraicType::simple_enum(["Player"].into_iter())).map_err(ErrorLang::from), Ok(AlgebraicValue::enum_simple(0)));
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
        &AlgebraicType::I256 => _parse::<i256>(value, ty),
        &AlgebraicType::U256 => _parse::<u256>(value, ty),
        &AlgebraicType::F32 => _parse::<f32>(value, ty),
        &AlgebraicType::F64 => _parse::<f64>(value, ty),
        &AlgebraicType::String => Ok(AlgebraicValue::String(value.into())),
        AlgebraicType::Sum(sum) => parse_simple_enum(sum, value),
        AlgebraicType::Product(product) => parse_product(product, value),
        x => Err(ErrorVm::Unsupported(format!(
            "Can't parse '{value}' to {}",
            x.to_satn_pretty()
        ))),
    }
}
