use spacetimedb_lib::de::serde::SerdeDeserializer;
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::{AlgebraicType, Identity, ProductType, ProductTypeElement, ProductValue, SumType};
use spacetimedb_sats::algebraic_value::de::ValueDeserializer;
use spacetimedb_sats::algebraic_value::ser::value_serialize;
use spacetimedb_sats::{satn::Satn, SumTypeVariant, Typespace, WithTypespace};

macro_rules! de_json_snapshot {
    ($schema:expr, $json:expr) => {
        let (schema, json) = (&$schema, &$json);
        let value = de_json(schema, json).unwrap();
        let value = WithTypespace::new(&EMPTY_TYPESPACE, schema)
            .with_value(&value)
            .to_satn_pretty();
        let debug_expr = format!("de_json({})", json.trim());
        insta::assert_snapshot!(insta::internals::AutoName, value, &debug_expr);
    };
}

#[derive(
    Debug,
    PartialEq,
    spacetimedb_sats::de::Deserialize,
    spacetimedb_sats::ser::Serialize,
    serde::Serialize,
    serde::Deserialize,
)]
struct Sample {
    identity: Identity,
}

#[test]
fn test_roundtrip() {
    let original = Sample {
        identity: Identity::__dummy(),
    };

    let s = value_serialize(&original);
    let result: Sample = spacetimedb_sats::de::Deserialize::deserialize(ValueDeserializer::new(s)).unwrap();
    assert_eq!(&original, &result);

    let s = serde_json::ser::to_string(&original).unwrap();
    let result: Sample = serde_json::from_str(&s).unwrap();
    assert_eq!(&original, &result);
}

#[test]
fn test_json_mappings() {
    let schema = tuple([
        ("foo", AlgebraicType::U32),
        ("bar", AlgebraicType::bytes()),
        ("baz", AlgebraicType::array(AlgebraicType::String)),
        (
            "quux",
            enumm([("Hash", AlgebraicType::bytes()), ("Unit", AlgebraicType::unit())]).into(),
        ),
        ("and_peggy", AlgebraicType::option(AlgebraicType::F64)),
        ("identity", Identity::get_type()),
    ]);
    let data = r#"
{
    "foo": 42,
    "bar": "404040FFFF0A48656C6C6F",
    "baz": ["heyyyyyy", "hooo"],
    "quux": { "Hash": "54a3e6d2b0959deaacf102292b1cbd6fcbb8cf237f73306e27ed82c3153878aa" },
    "and_peggy": { "some": 3.141592653589793238426 },
    "identity": ["0000000000000000000000000000000000000000000000000000000000000000"]
}
"#; // all of those ^^^^^^ digits are from memory
    de_json_snapshot!(schema, data);
    let data = r#"
{
    "foo": 5654,
    "bar": [1, 15, 44],
    "baz": ["it's 🥶°C"],
    "quux": { "Unit": [] },
    "and_peggy": null,
    "identity": ["0000000000000000000000000000000000000000000000000000000000000000"]
}
"#;
    de_json_snapshot!(schema, data);
}

fn tuple<'a>(elems: impl IntoIterator<Item = (&'a str, AlgebraicType)>) -> ProductType {
    ProductType {
        elements: elems
            .into_iter()
            .map(|(name, ty)| ProductTypeElement::new_named(ty, name))
            .collect(),
    }
}
fn enumm<'a>(elems: impl IntoIterator<Item = (&'a str, AlgebraicType)>) -> SumType {
    SumType {
        variants: elems
            .into_iter()
            .map(|(name, ty)| SumTypeVariant::new_named(ty, name))
            .collect(),
    }
}

static EMPTY_TYPESPACE: Typespace = Typespace::new(Vec::new());

fn in_space<T>(x: &T) -> WithTypespace<'_, T> {
    WithTypespace::new(&EMPTY_TYPESPACE, x)
}

fn de_json(schema: &ProductType, data: &str) -> serde_json::Result<ProductValue> {
    let mut de = serde_json::Deserializer::from_str(data);
    let val = in_space(schema)
        .deserialize(SerdeDeserializer::new(&mut de))
        .map_err(|e| e.0)?;
    de.end()?;
    Ok(val)
}
