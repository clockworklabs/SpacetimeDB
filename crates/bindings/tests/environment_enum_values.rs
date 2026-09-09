use spacetimedb::rt::EnvironmentValue as _;
use spacetimedb::spacetimedb_lib::environment::{EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, spacetimedb::SpacetimeType, spacetimedb::EnvironmentValue)]
enum Mode {
    #[env(value = "in progress")]
    InProgress,
    Ready,
    #[env(value = "")]
    Empty,
    #[env(value = "héllo\0世界")]
    Unicode,
}

#[derive(Debug, PartialEq, Eq, spacetimedb::EnvironmentValue)]
enum Literal {
    #[env(value = "only")]
    Only,
}

#[test]
fn typed_mappings_match_exact_schema_strings_and_optional_absence() {
    let cases = [
        ("in progress", Mode::InProgress),
        ("Ready", Mode::Ready),
        ("", Mode::Empty),
        ("héllo\0世界", Mode::Unicode),
    ];
    assert_eq!(
        Mode::constraint(),
        EnvironmentConstraint::OneOf(cases.iter().map(|(s, _)| s.to_string()).collect())
    );
    assert_eq!(Option::<Mode>::constraint(), Mode::constraint());
    let schema = EnvironmentSchema::new(vec![EnvironmentDeclaration {
        name: "MODE".into(),
        constraint: Mode::constraint(),
        optional: false,
    }])
    .unwrap();
    for (value, variant) in cases {
        schema
            .validate_values(&BTreeMap::from([("MODE".into(), value.into())]))
            .unwrap();
        assert_eq!(Mode::from_environment(Some(value.into()), "MODE"), variant);
        assert_eq!(
            Option::<Mode>::from_environment(Some(value.into()), "MODE"),
            Some(variant)
        );
    }
    assert_eq!(Option::<Mode>::from_environment(None, "MODE"), None);
    assert_eq!(Literal::constraint(), EnvironmentConstraint::Literal("only".into()));
    assert_eq!(Literal::from_environment(Some("only".into()), "VALUE"), Literal::Only);
    for rejected in ["InProgress", "ready", "in progress ", "private-unmapped-value"] {
        assert!(schema
            .validate_values(&BTreeMap::from([("MODE".into(), rejected.into())]))
            .is_err());
    }
}

#[test]
fn decode_errors_report_the_key_without_the_supplied_value() {
    for value in [None, Some("private-unmapped-value".into())] {
        let error = std::panic::catch_unwind(|| Mode::from_environment(value, "MODE")).unwrap_err();
        let message = error.downcast_ref::<String>().unwrap();
        assert!(message.contains("MODE"));
        assert!(!message.contains("private-unmapped-value"));
        assert!(!message.contains("in progress"));
    }
}

#[test]
fn environment_mapping_does_not_change_ordinary_enum_serialization() {
    use spacetimedb::spacetimedb_lib::bsatn;
    assert_eq!(bsatn::to_vec(&Mode::InProgress).unwrap(), vec![0]);
    assert_eq!(bsatn::to_vec(&Mode::Ready).unwrap(), vec![1]);
    assert_eq!(bsatn::from_slice::<Mode>(&[2]).unwrap(), Mode::Empty);
}
