use spacetimedb_lib::de::serde::SerdeDeserializer;
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::{AlgebraicType, ProductType, ProductTypeElement, ProductValue, SumType};
use spacetimedb_sats::slim_slice::SlimSliceBoxCollected;
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

#[test]
fn test_json_mappings() {
    let schema = tuple([
        ("foo", AlgebraicType::U32),
        ("bar", AlgebraicType::bytes()),
        ("baz", AlgebraicType::array(AlgebraicType::String)),
        (
            "quux",
            AlgebraicType::Sum(enumm([
                ("Hash", AlgebraicType::bytes()),
                ("Unit", AlgebraicType::unit()),
            ])),
        ),
        ("and_peggy", AlgebraicType::option(AlgebraicType::F64)),
    ]);
    let data = r#"
{
    "foo": 42,
    "bar": "404040FFFF0A48656C6C6F",
    "baz": ["heyyyyyy", "hooo"],
    "quux": { "Hash": "54a3e6d2b0959deaacf102292b1cbd6fcbb8cf237f73306e27ed82c3153878aa" },
    "and_peggy": { "some": 3.141592653589793238426 }
}
"#; // all of those ^^^^^^ digits are from memory
    de_json_snapshot!(schema, data);
    let data = r#"
{
    "foo": 5654,
    "bar": [1, 15, 44],
    "baz": ["it's 🥶°C"],
    "quux": { "Unit": [] },
    "and_peggy": null
}
"#;
    de_json_snapshot!(schema, data);
}

fn tuple<'a>(elems: impl IntoIterator<Item = (&'a str, AlgebraicType)>) -> ProductType {
    ProductType {
        elements: elems
            .into_iter()
            .map(|(name, ty)| ProductTypeElement::new_named(ty, name))
            .collect::<SlimSliceBoxCollected<_>>()
            .unwrap(),
    }
}
fn enumm<'a>(elems: impl IntoIterator<Item = (&'a str, AlgebraicType)>) -> SumType {
    SumType {
        variants: elems
            .into_iter()
            .map(|(name, ty)| SumTypeVariant::new_named(ty, name))
            .collect::<SlimSliceBoxCollected<_>>()
            .unwrap(),
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
