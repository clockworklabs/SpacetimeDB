use serde::de::DeserializeSeed;
use spacetimedb_lib::{ElementDef, EnumDef, TupleDef, TupleValue, TypeDef};

macro_rules! de_json_snapshot {
    ($schema:expr, $json:expr) => {
        let (schema, json) = (&$schema, &$json);
        let value = de_json(schema, json).unwrap();
        let value = format!("{value:#}");
        let debug_expr = format!("de_json({})", json.trim());
        insta::assert_snapshot!(insta::internals::AutoName, value, &debug_expr);
    };
}

#[test]
fn test_json_mappings() {
    let schema = tuple(
        "args",
        [
            ("foo", TypeDef::U32),
            ("bar", TypeDef::Bytes),
            ("baz", vec(TypeDef::String)),
            (
                "quux",
                TypeDef::Enum(enumm([("Hash", TypeDef::Hash), ("Unit", TypeDef::Unit)])),
            ),
            ("and_peggy", TypeDef::F64),
        ],
    );
    let data = r#"
{
    "foo": 42,
    "bar": "404040FFFF0A48656C6C6F",
    "baz": ["heyyyyyy", "hooo"],
    "quux": { "Hash": "54a3e6d2b0959deaacf102292b1cbd6fcbb8cf237f73306e27ed82c3153878aa" },
    "and_peggy": 3.141592653589793238426
}
"#; // all of those ^^^^^^ digits are from memory
    de_json_snapshot!(schema, data);
    let data = r#"
{
    "foo": 5654,
    "bar": "010F2C",
    "baz": ["it's 🥶°C"],
    "quux": { "Unit": null },
    "and_peggy": 9.8
}
"#;
    de_json_snapshot!(schema, data);
}

fn tuple<'a>(name: &str, elems: impl IntoIterator<Item = (&'a str, TypeDef)>) -> TupleDef {
    TupleDef {
        name: Some(name.into()),
        elements: elements(elems),
    }
}
fn enumm<'a>(elems: impl IntoIterator<Item = (&'a str, TypeDef)>) -> EnumDef {
    EnumDef {
        variants: elements(elems),
    }
}

fn elements<'a>(elems: impl IntoIterator<Item = (&'a str, TypeDef)>) -> Vec<ElementDef> {
    elems
        .into_iter()
        .enumerate()
        .map(|(i, (name, element_type))| ElementDef {
            tag: i.try_into().unwrap(),
            name: Some(name.into()),
            element_type,
        })
        .collect()
}

fn vec(element_type: TypeDef) -> TypeDef {
    TypeDef::Vec {
        element_type: Box::new(element_type),
    }
}

fn de_json(schema: &TupleDef, data: &str) -> serde_json::Result<TupleValue> {
    let mut de = serde_json::Deserializer::from_str(data);
    let val = schema.deserialize(&mut de)?;
    de.end()?;
    Ok(val)
}
