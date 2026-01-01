use crate::pg_server::PgError;
use pgwire::api::portal::Format;
use pgwire::api::results::{DataRowEncoder, FieldInfo};
use pgwire::api::Type;
use spacetimedb_lib::sats::satn::{PsqlChars, PsqlClient, PsqlPrintFmt, PsqlType, TypedWriter};
use spacetimedb_lib::sats::{satn, ArrayValue, ValueWithType};
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
            AlgebraicType::F32 => Type::FLOAT4_ARRAY,
            AlgebraicType::F64 => Type::FLOAT8_ARRAY,
            AlgebraicType::Ref(_) | AlgebraicType::Sum(_) | AlgebraicType::Product(_) | AlgebraicType::Array(_) => {
                Type::JSON_ARRAY
            }
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

impl<'a> PsqlFormatter<'a> {
    fn encode_variant(tag: u8, ty: PsqlType, name: Option<&str>, value: ValueWithType<AlgebraicValue>) -> String {
        // Is a simple enum?
        if let AlgebraicType::Sum(sum) = &ty.field.algebraic_type {
            if sum.is_simple_enum() {
                if let Some(variant_name) = name {
                    return variant_name.to_string();
                }
            }
        }

        if ty.field.algebraic_type.is_unit() {
            if let Some(variant_name) = name {
                return variant_name.to_string();
            }
        }

        let PsqlChars {
            start,
            sep,
            end,
            quote,
            start_array: _,
            end_array: _,
        } = ty.client.format_chars();
        let name = name.map(Cow::from).unwrap_or_else(|| Cow::from(tag.to_string()));
        format!(
            "{start}{quote}{name}{quote}{sep} {}{end}",
            satn::PsqlWrapper { ty, value }
        )
    }
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
        if let AlgebraicType::Sum(sum) = &ty.field.algebraic_type {
            if sum.is_simple_enum() {
                if let Some(variant_name) = name {
                    self.encoder.encode_field(&variant_name)?;
                    return Ok(());
                }
            }
        }

        let PsqlChars {
            start,
            sep,
            end,
            quote,
            start_array: _,
            end_array: _,
        } = ty.client.format_chars();
        let name = name.map(Cow::from).unwrap_or_else(|| Cow::from(tag.to_string()));
        let json = format!(
            "{start}{quote}{name}{quote}{sep} {}{end}",
            satn::PsqlWrapper { ty, value }
        );
        self.encoder.encode_field(&json)?;
        Ok(())
    }

    fn write_array(
        &mut self,
        value: &ValueWithType<'_, ArrayValue>,
        psql: &PsqlType,
        ty: &AlgebraicType,
    ) -> Result<bool, Self::Error> {
        if *ty == AlgebraicType::U8 {
            return Ok(false);
        }
        fn collect<I, O, F>(arr: &[I], map: F) -> Vec<O>
        where
            I: Clone,
            F: Fn(usize, &I) -> O,
        {
            arr.iter().enumerate().map(|(pos, v)| map(pos, v)).collect()
        }
        let ty = &value.ty().elem_ty;
        let type_space = &value.typespace();
        match value.value() {
            ArrayValue::Bool(arr) => self.encoder.encode_field(&arr.as_ref())?,
            ArrayValue::I8(arr) => self.encoder.encode_field(&arr.as_ref())?,
            ArrayValue::U8(arr) => self.encoder.encode_field(&arr.as_ref())?,
            ArrayValue::I16(arr) => self.encoder.encode_field(&arr.as_ref())?,
            ArrayValue::U16(arr) => self.encoder.encode_field(&collect(arr, |_, v| *v as i32))?,
            ArrayValue::I32(arr) => self.encoder.encode_field(&arr.as_ref())?,
            ArrayValue::U32(arr) => self.encoder.encode_field(&collect(arr, |_, v| *v as i64))?,
            ArrayValue::I64(arr) => self.encoder.encode_field(&arr.as_ref())?,
            ArrayValue::U64(arr) => self.encoder.encode_field(&collect(arr, |_, v| v.to_string()))?,
            ArrayValue::I128(arr) => self.encoder.encode_field(&collect(arr, |_, v| v.to_string()))?,
            ArrayValue::U128(arr) => self.encoder.encode_field(&collect(arr, |_, v| v.to_string()))?,
            ArrayValue::I256(arr) => self.encoder.encode_field(&collect(arr, |_, v| v.to_string()))?,
            ArrayValue::U256(arr) => self.encoder.encode_field(&collect(arr, |_, v| v.to_string()))?,
            ArrayValue::F32(arr) => self.encoder.encode_field(&collect(arr, |_, v| *v.as_ref()))?,
            ArrayValue::F64(arr) => self.encoder.encode_field(&collect(arr, |_, v| *v.as_ref()))?,
            ArrayValue::String(arr) => self.encoder.encode_field(&collect(arr, |_, v| v.to_string()))?,
            ArrayValue::Array(arr) => {
                let values = collect(arr, |_pos, val| {
                    let mut psql = psql.clone();
                    // Switching client because we are outputting nested arrays as JSON
                    psql.client = PsqlClient::SpacetimeDB;
                    satn::PsqlWrapper {
                        ty: psql,
                        value: val.clone(),
                    }
                    .to_string()
                });
                self.encoder.encode_field(&values)?;
            }
            ArrayValue::Sum(sum) => {
                let values = collect(sum, |_pos, val| {
                    let (tag, value) = match &**ty {
                        AlgebraicType::Sum(sum) => {
                            let field = sum.variants.get(val.tag as usize).expect("Invalid variant tag");
                            (field, val.value.clone())
                        }
                        _ => unreachable!("Expected sum type"),
                    };
                    let field = ProductTypeElement::new(tag.algebraic_type.clone(), tag.name.clone());

                    PsqlFormatter::encode_variant(
                        val.tag,
                        PsqlType {
                            client: psql.client,
                            field: &field.clone(),
                            tuple: &ProductType::new([field].into()),
                            idx: 0,
                        },
                        tag.name.as_deref(),
                        ValueWithType::new(type_space.with_type(&tag.algebraic_type), &value),
                    )
                });
                self.encoder.encode_field(&values)?;
            }
            ArrayValue::Product(value) => {
                let PsqlChars {
                    start,
                    sep,
                    end,
                    quote,
                    start_array: _,
                    end_array: _,
                } = psql.client.format_chars();
                let values = collect(value, |pos, value| {
                    let json = match &**ty {
                        AlgebraicType::Product(prod) => {
                            let mut json = String::new();
                            for (field, value) in prod.elements.iter().zip(value.elements.iter()) {
                                let psql_ty = PsqlType {
                                    client: psql.client,
                                    field,
                                    tuple: prod,
                                    idx: pos,
                                };
                                if !json.is_empty() {
                                    json.push(',');
                                }
                                let name = field
                                    .name
                                    .as_deref()
                                    .map(Cow::from)
                                    .unwrap_or_else(|| Cow::from(pos.to_string()));
                                let field_json =
                                    format!("{quote}{name}{quote}{sep} {}", satn::PsqlWrapper { ty: psql_ty, value });
                                json.push_str(&field_json);
                            }
                            json
                        }
                        _ => unreachable!("Expected product type"),
                    };
                    format!("{start}{}{end}", json)
                });

                self.encoder.encode_field(&values)?;
            }
        }

        Ok(true)
    }

    fn insert_sep(&mut self, _sep: &str) -> Result<(), Self::Error> {
        Ok(()) // No-op for PSQL format
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pg_server::to_rows;
    use futures::StreamExt;
    use spacetimedb_client_api_messages::http::SqlStmtResult;
    use spacetimedb_lib::sats::algebraic_value::Packed;
    use spacetimedb_lib::sats::{
        i256, product, u256, AlgebraicType, ArrayValue, ProductType, SumTypeVariant, SumValue,
    };
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

    #[tokio::test]
    async fn test_array() {
        // {a: [1,2,3], b: [{"a": 1}, {"b": true}], c: [0xDE, 0xAD, 0xBE, 0xEF]}
        let product = AlgebraicType::product([
            ProductTypeElement::new(AlgebraicType::I32, Some("a".into())),
            ProductTypeElement::new(AlgebraicType::Bool, Some("b".into())),
        ]);
        let schema = ProductType::from([
            AlgebraicType::array(AlgebraicType::I32),
            AlgebraicType::array(product.clone()),
            AlgebraicType::bytes(),
        ]);

        let value = product![
            AlgebraicValue::Array(ArrayValue::I32([1, 2, 3].into())),
            AlgebraicValue::Array(ArrayValue::Product([product![1, true]].into())),
            AlgebraicValue::Bytes([0xDE, 0xAD, 0xBE, 0xEF].into()),
        ];

        let row = run(schema.clone(), value.clone()).await;
        assert_eq!(
            row,
            "\0\0\0\u{7}{1,2,3}\0\0\0\u{1a}{\"{\\\"a\\\": 1,\\\"b\\\": true}\"}\0\0\0\n\\xdeadbeef"
        );

        // Check all the unnested arrays are encoded as native PG arrays, and nested arrays, sum & product arrays as JSON
        let arrays = vec![
            (
                ArrayValue::Bool([true, false, true].into()),
                AlgebraicType::Bool,
                "\u{7}{t,f,t}",
            ),
            (ArrayValue::I8([-1, 0, 1].into()), AlgebraicType::I8, "\u{8}{-1,0,1}"),
            (ArrayValue::U8([0, 1, 2].into()), AlgebraicType::U8, "\u{8}\\x000102"),
            (
                ArrayValue::I16([-256, 0, 256].into()),
                AlgebraicType::I16,
                "\u{c}{-256,0,256}",
            ),
            (
                ArrayValue::U16([0, 256, 65535].into()),
                AlgebraicType::U16,
                "\r{0,256,65535}",
            ),
            (
                ArrayValue::I32([-65536, 0, 65536].into()),
                AlgebraicType::I32,
                "\u{10}{-65536,0,65536}",
            ),
            (
                ArrayValue::U32([0, 65536, 4294967295].into()),
                AlgebraicType::U32,
                "\u{14}{0,65536,4294967295}",
            ),
            (
                ArrayValue::I64([-4294967296, 0, 4294967296].into()),
                AlgebraicType::I64,
                "\u{1a}{-4294967296,0,4294967296}",
            ),
            (
                ArrayValue::U64([0, 4294967296, 18446744073709551615].into()),
                AlgebraicType::U64,
                "#{0,4294967296,18446744073709551615}",
            ),
            (
                ArrayValue::I128([i128::MIN, 0, i128::MAX].into()),
                AlgebraicType::I128,
                "T{-170141183460469231731687303715884105728,0,170141183460469231731687303715884105727}",
            ),
            (
                ArrayValue::U128([0, u128::MAX].into()),
                AlgebraicType::U128,
                "+{0,340282366920938463463374607431768211455}",
            ),
            (
                ArrayValue::I256([i256::from(-1), i256::from(0), i256::from(1)].into()),
                AlgebraicType::I256,
                "\u{8}{-1,0,1}",
            ),
            (
                ArrayValue::U256([u256::ZERO, u256::ONE].into()),
                AlgebraicType::U256,
                "\u{5}{0,1}",
            ),
            (
                ArrayValue::F32([1.5.into(), 2.5.into(), 3.5.into()].into()),
                AlgebraicType::F32,
                "\r{1.5,2.5,3.5}",
            ),
            (
                ArrayValue::F64([1.5.into(), 2.5.into(), 3.5.into()].into()),
                AlgebraicType::F64,
                "\r{1.5,2.5,3.5}",
            ),
            (
                ArrayValue::String(["foo".into(), "bar".into(), "baz".into()].into()),
                AlgebraicType::String,
                "\r{foo,bar,baz}",
            ),
            (
                ArrayValue::Product([product![1], product![2], product![3]].into()),
                AlgebraicType::product([ProductTypeElement::new(AlgebraicType::I32, None)]),
                "({\"{\\\"0\\\": 1}\",\"{\\\"1\\\": 2}\",\"{\\\"2\\\": 3}\"}",
            ),
            // Array of arrays
            (
                ArrayValue::Array([ArrayValue::I32([1, 2].into()), ArrayValue::I32([3, 4].into())].into()),
                AlgebraicType::array(AlgebraicType::I32),
                "\u{13}{\"[1, 2]\",\"[3, 4]\"}",
            ),
            // Simple enum array
            (
                ArrayValue::Sum(
                    [
                        SumValue::new_simple(0),
                        SumValue::new_simple(1),
                        SumValue::new_simple(2),
                    ]
                    .into(),
                ),
                AlgebraicType::simple_enum(["A", "B", "C"].into_iter()),
                "\u{7}{A,B,C}",
            ),
            // Non-simple enum array
            (
                ArrayValue::Sum(
                    [
                        SumValue::new(0, AlgebraicValue::I64(1)),
                        SumValue::new(1, AlgebraicValue::unit()),
                    ]
                    .into(),
                ),
                AlgebraicType::option(AlgebraicType::I64),
                "\u{16}{\"{\\\"some\\\": 1}\",none}",
            ),
        ];

        for (array_value, ty, expected_encoding) in arrays {
            let schema = ProductType::from([AlgebraicType::array(ty.clone())]);
            let value = product![AlgebraicValue::Array(array_value)];
            let row = run(schema, value).await;
            let expected_row = format!("\0\0\0{}", expected_encoding);
            assert_eq!(row, expected_row, "Failed for array encoding for {ty:?}");
        }
    }
}
