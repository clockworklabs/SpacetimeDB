use spacetimedb_lib::de::serde::SerdeDeserializer;
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::{ElementDef, EnumDef, TupleDef, TupleValue, TypeDef};
use spacetimedb_sats::{satn::Satn, BuiltinType::*, SumTypeVariant, TypeInSpace, Typespace};

macro_rules! de_json_snapshot {
    ($schema:expr, $json:expr) => {
        let (schema, json) = (&$schema, &$json);
        let value = de_json(schema, json).unwrap();
        let value = TypeInSpace::new(&EMPTY_TYPESPACE, schema)
            .with_value(&value)
            .to_satn_pretty();
        let debug_expr = format!("de_json({})", json.trim());
        insta::assert_snapshot!(insta::internals::AutoName, value, &debug_expr);
    };
}

#[test]
fn test_json_mappings() {
    let schema = tuple([
        ("foo", TypeDef::Builtin(U32)),
        ("bar", TypeDef::bytes()),
        ("baz", vec(TypeDef::Builtin(String))),
        (
            "quux",
            TypeDef::Sum(enumm([("Hash", TypeDef::bytes()), ("Unit", TypeDef::UNIT_TYPE)])),
        ),
        ("and_peggy", TypeDef::Builtin(F64)),
    ]);
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
    "bar": [1, 15, 44],
    "baz": ["it's 🥶°C"],
    "quux": { "Unit": [] },
    "and_peggy": 9.8
}
"#;
    de_json_snapshot!(schema, data);
}

fn tuple<'a>(elems: impl IntoIterator<Item = (&'a str, TypeDef)>) -> TupleDef {
    TupleDef {
        elements: elems
            .into_iter()
            .map(|(name, algebraic_type)| ElementDef {
                name: Some(name.into()),
                algebraic_type,
            })
            .collect(),
    }
}
fn enumm<'a>(elems: impl IntoIterator<Item = (&'a str, TypeDef)>) -> EnumDef {
    EnumDef {
        variants: elems
            .into_iter()
            .map(|(name, algebraic_type)| SumTypeVariant {
                name: Some(name.into()),
                algebraic_type,
            })
            .collect(),
    }
}

fn vec(ty: TypeDef) -> TypeDef {
    TypeDef::Builtin(Array { ty: Box::new(ty) })
}

static EMPTY_TYPESPACE: Typespace = Typespace::new(Vec::new());

fn in_space<T>(x: &T) -> TypeInSpace<'_, T> {
    TypeInSpace::new(&EMPTY_TYPESPACE, x)
}

fn de_json(schema: &TupleDef, data: &str) -> serde_json::Result<TupleValue> {
    let mut de = serde_json::Deserializer::from_str(data);
    let val = in_space(schema)
        .deserialize(SerdeDeserializer::new(&mut de))
        .map_err(|e| e.0)?;
    de.end()?;
    Ok(val)
}
