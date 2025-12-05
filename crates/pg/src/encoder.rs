use crate::pg_server::PgError;
use pgwire::api::portal::Format;
use pgwire::api::results::{DataRowEncoder, FieldInfo};
use pgwire::api::Type;
use spacetimedb_lib::sats::satn::{PsqlChars, PsqlPrintFmt, PsqlType, TypedWriter};
use spacetimedb_lib::sats::{satn, ValueWithType};
use spacetimedb_lib::{
    ser, AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue, TimeDuration, Timestamp,
};
use std::borrow::Cow;
use std::sync::Arc;

pub(crate) fn row_desc(schema: &ProductType, format: &Format) -> Arc<Vec<FieldInfo>> {
    Arc::new(
        schema
            .elements
            .iter()
            .enumerate()
            .map(|(pos, ty)| {
                let field_name = ty.name.clone().map(Into::into).unwrap_or_else(|| format!("col_{pos}"));
                let field_type = type_of(schema, ty);
                FieldInfo::new(field_name, None, None, field_type, format.format_for(pos))
            })
            .collect(),
    )
}

pub(crate) fn type_of(schema: &ProductType, ty: &ProductTypeElement) -> Type {
    let format = PsqlPrintFmt::use_fmt(schema, ty, ty.name());
    match &ty.algebraic_type {
        AlgebraicType::String => Type::VARCHAR,
        AlgebraicType::Bool => Type::BOOL,
        AlgebraicType::U8 | AlgebraicType::I8 | AlgebraicType::I16 => Type::INT2,
        AlgebraicType::U16 | AlgebraicType::I32 => Type::INT4,
        AlgebraicType::U32 | AlgebraicType::I64 => Type::INT8,
        AlgebraicType::U64 | AlgebraicType::I128 | AlgebraicType::U128 | AlgebraicType::I256 | AlgebraicType::U256 => {
            Type::NUMERIC
        }
        AlgebraicType::F32 => Type::FLOAT4,
        AlgebraicType::F64 => Type::FLOAT8,
        AlgebraicType::Array(ty) => match *ty.elem_ty {
            AlgebraicType::String => Type::VARCHAR_ARRAY,
            AlgebraicType::Bool => Type::BOOL_ARRAY,
            AlgebraicType::U8 => Type::BYTEA,
            AlgebraicType::I8 | AlgebraicType::I16 => Type::INT2_ARRAY,
            AlgebraicType::U16 | AlgebraicType::I32 => Type::INT4_ARRAY,
            AlgebraicType::U32 | AlgebraicType::I64 => Type::INT8_ARRAY,
            AlgebraicType::U64
            | AlgebraicType::I128
            | AlgebraicType::U128
            | AlgebraicType::I256
            | AlgebraicType::U256 => Type::NUMERIC_ARRAY,
            _ => Type::ANYARRAY,
        },
        AlgebraicType::Product(_) => match format {
            PsqlPrintFmt::Hex => Type::BYTEA_ARRAY,
            PsqlPrintFmt::Timestamp => Type::TIMESTAMP,
            PsqlPrintFmt::Duration => Type::INTERVAL,
            _ => Type::JSON,
        },
        AlgebraicType::Sum(sum) if sum.is_simple_enum() => Type::ANYENUM,
        AlgebraicType::Sum(_) => Type::JSON,
        _ => Type::UNKNOWN,
    }
}

impl ser::Error for PgError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        PgError::Other(anyhow::anyhow!(msg.to_string()))
    }
}

pub(crate) struct PsqlFormatter<'a> {
    pub(crate) encoder: &'a mut DataRowEncoder,
}

impl TypedWriter for PsqlFormatter<'_> {
    type Error = PgError;

    fn write<W: std::fmt::Display>(&mut self, value: W) -> Result<(), Self::Error> {
        self.encoder.encode_field(&value.to_string())?;
        Ok(())
    }

    fn write_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        self.encoder.encode_field(&value)?;
        Ok(())
    }

    fn write_string(&mut self, value: &str) -> Result<(), Self::Error> {
        self.encoder.encode_field(&value)?;
        Ok(())
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<(), Self::Error> {
        self.encoder.encode_field(&value)?;
        Ok(())
    }

    fn write_hex(&mut self, value: &[u8]) -> Result<(), Self::Error> {
        self.encoder.encode_field(&value)?;
        Ok(())
    }

    fn write_timestamp(&mut self, value: Timestamp) -> Result<(), Self::Error> {
        self.encoder.encode_field(&value.to_rfc3339()?)?;
        Ok(())
    }

    fn write_duration(&mut self, value: TimeDuration) -> Result<(), Self::Error> {
        self.encoder.encode_field(&value.to_iso8601())?;
        Ok(())
    }

    fn write_alt_record(
        &mut self,
        ty: &PsqlType,
        value: &ValueWithType<'_, ProductValue>,
    ) -> Result<bool, Self::Error> {
        let json = satn::PsqlWrapper { ty: ty.clone(), value }.to_string();
        self.encoder.encode_field(&json)?;
        Ok(true)
    }

    fn write_record(
        &mut self,
        _fields: Vec<(Cow<str>, PsqlType, ValueWithType<AlgebraicValue>)>,
    ) -> Result<(), Self::Error> {
        unreachable!("Use `write_alt_record` for records in PSQL format");
    }

    fn write_variant(
        &mut self,
        tag: u8,
        ty: PsqlType,
        name: Option<&str>,
        value: ValueWithType<AlgebraicValue>,
    ) -> Result<(), Self::Error> {
        // Is a simple enum?
        if let AlgebraicType::Sum(sum) = &ty.field.algebraic_type
            && sum.is_simple_enum()
            && let Some(variant_name) = name
        {
            self.encoder.encode_field(&variant_name)?;
            return Ok(());
        }

        let PsqlChars { start, sep, end, quote } = ty.client.format_chars();
        let name = name.map(Cow::from).unwrap_or_else(|| Cow::from(tag.to_string()));
        let json = format!(
            "{start}{quote}{name}{quote}{sep} {}{end}",
            satn::PsqlWrapper { ty, value }
        );
        self.encoder.encode_field(&json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pg_server::to_rows;
    use futures::StreamExt;
    use spacetimedb_client_api_messages::http::SqlStmtResult;
    use spacetimedb_lib::sats::algebraic_value::Packed;
    use spacetimedb_lib::sats::{i256, product, u256, AlgebraicType, ProductType, SumTypeVariant};
    use spacetimedb_lib::{ConnectionId, Identity};

    async fn run(schema: ProductType, row: ProductValue) -> String {
        let header = row_desc(&schema, &Format::UnifiedText);

        let stmt = SqlStmtResult {
            schema,
            rows: vec![row],
            total_duration_micros: 0,
            stats: Default::default(),
        };
        let mut stream = to_rows(stmt, header).unwrap();
        let mut result = String::new();
        if let Some(row) = stream.next().await {
            result = String::from_utf8_lossy(row.unwrap().data.freeze().as_ref()).to_string();
        }
        result
    }

    #[tokio::test]
    async fn test_primitives() {
        let schema = ProductType::from([
            AlgebraicType::U8,
            AlgebraicType::I8,
            AlgebraicType::I16,
            AlgebraicType::U16,
            AlgebraicType::I32,
            AlgebraicType::U32,
            AlgebraicType::I64,
            AlgebraicType::U64,
            AlgebraicType::I128,
            AlgebraicType::U128,
            AlgebraicType::I256,
            AlgebraicType::U256,
            AlgebraicType::F32,
            AlgebraicType::F64,
            AlgebraicType::String,
            AlgebraicType::Bool,
        ]);
        let value = product![
            1u8,
            -1i8,
            -2i16,
            3u16,
            -4i32,
            5u32,
            -6i64,
            7u64,
            Packed::from(-8i128),
            Packed::from(9u128),
            i256::from(-10),
            u256::from(11u128),
            12.34f32,
            56.78f64,
            "test".to_string(),
            true,
        ];

        let row = run(schema, value).await;
        assert_eq!(row, "\0\0\0\u{1}1\0\0\0\u{2}-1\0\0\0\u{2}-2\0\0\0\u{1}3\0\0\0\u{2}-4\0\0\0\u{1}5\0\0\0\u{2}-6\0\0\0\u{1}7\0\0\0\u{2}-8\0\0\0\u{1}9\0\0\0\u{3}-10\0\0\0\u{2}11\0\0\0\u{5}12.34\0\0\0\u{5}56.78\0\0\0\u{4}test\0\0\0\u{1}t");
    }

    #[tokio::test]
    async fn test_enum() {
        let some = AlgebraicType::option(AlgebraicType::I64);
        let schema = ProductType::from([some.clone(), some]);
        let value = product![
            AlgebraicValue::sum(0, AlgebraicValue::I64(1)), // Some(1)
            AlgebraicValue::sum(1, AlgebraicValue::unit()), // None
        ];

        let row = run(schema, value).await;
        assert_eq!(row, "\0\0\0\u{b}{\"some\": 1}\0\0\0\u{c}{\"none\": {}}");

        let color = AlgebraicType::Sum([SumTypeVariant::new_named(AlgebraicType::I64, "Gray")].into());
        let nested = AlgebraicType::option(color.clone());
        let schema = ProductType::from([color, nested]);
        // {"Gray": 1}, {"some": {"Gray": 2}}
        let value = product![
            AlgebraicValue::sum(0, AlgebraicValue::I64(1)), // Gray(1)
            AlgebraicValue::sum(0, AlgebraicValue::sum(0, AlgebraicValue::I64(2))), // Some(Gray(2))
        ];
        let row = run(schema.clone(), value.clone()).await;
        assert_eq!(row, "\0\0\0\u{b}{\"Gray\": 1}\0\0\0\u{15}{\"some\": {\"Gray\": 2}}");

        // Now nested product
        let product = AlgebraicType::product([
            ProductTypeElement::new(AlgebraicType::Product(schema), Some("x".into())),
            ProductTypeElement::new(AlgebraicType::String, Some("y".into())),
        ]);
        let schema = ProductType::from([product.clone()]);
        let value = product![AlgebraicValue::product(vec![
            value.into(),
            AlgebraicValue::String("a".into()),
        ])];
        let row = run(schema, value).await;
        assert_eq!(
            row,
            "\0\0\0G{\"x\": {\"col_0\": {\"Gray\": 1}, \"col_1\": {\"some\": {\"Gray\": 2}}}, \"y\": \"a\"}"
        );

        // Now a simple enum
        let names = AlgebraicType::simple_enum(["A", "B", "C"].into_iter());
        let schema = ProductType::from([names.clone(), names.clone(), names]);
        let value = product![
            AlgebraicValue::enum_simple(0), // A
            AlgebraicValue::enum_simple(1), // B
            AlgebraicValue::enum_simple(2), // C
        ];
        let row = run(schema, value).await;
        assert_eq!(row, "\0\0\0\u{1}A\0\0\0\u{1}B\0\0\0\u{1}C");
    }

    #[tokio::test]
    async fn test_special_types() {
        let schema = ProductType::from([
            AlgebraicType::identity(),
            AlgebraicType::connection_id(),
            AlgebraicType::time_duration(),
            AlgebraicType::timestamp(),
            AlgebraicType::bytes(),
        ]);
        let value = product![
            Identity::ZERO,
            ConnectionId::ZERO,
            TimeDuration::from_micros(0),
            Timestamp::from_micros_since_unix_epoch(1622545800000),
            AlgebraicValue::Bytes("test".as_bytes().into()),
        ];

        let row = run(schema, value).await;
        assert_eq!(row, "\0\0\0B\\x0000000000000000000000000000000000000000000000000000000000000000\0\0\0\"\\x00000000000000000000000000000000\0\0\0\u{3}P0D\0\0\0\u{1d}1970-01-19T18:42:25.800+00:00\0\0\0\n\\x74657374");
    }
}
