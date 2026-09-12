use super::*;
use spacetimedb_lib::{
    db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section},
    RawModuleDef,
};
use spacetimedb_schema::def::ModuleDef;
use std::collections::BTreeMap;

fn schema(declared: bool) -> ModuleDef {
    let mut sections = vec![RawModuleDefV10Section::Typespace(Default::default())];
    if declared {
        sections.push(RawModuleDefV10Section::Environment(vec![]));
    }
    ModuleDef::try_from(RawModuleDef::V10(RawModuleDefV10 { sections })).unwrap()
}

#[test]
fn ordinary_and_explicit_empty_declarations_choose_distinct_wire_formats() {
    let bytes = b"exact selected module bytes\0\xff".to_vec();
    let (kind, body) = publication_body(
        &schema(false),
        bytes.clone(),
        BTreeMap::new(),
        &EnvironmentOptions::default(),
    )
    .unwrap();
    assert_eq!(kind, "application/octet-stream");
    assert_eq!(body, bytes);
    let (kind, body) = publication_body(
        &schema(true),
        bytes.clone(),
        BTreeMap::new(),
        &EnvironmentOptions::default(),
    )
    .unwrap();
    assert_eq!(kind, spacetimedb_client_api_messages::publish::CONTENT_TYPE);
    let envelope = spacetimedb_client_api_messages::publish::PublishRequest::decode(&body).unwrap();
    assert_eq!(envelope.module, Some(bytes.clone()));
    assert!(envelope.environment.is_empty());
    let (kind, body) = publication_body(
        &schema(false),
        bytes,
        BTreeMap::from([("KEY".into(), "secret-sentinel".into())]),
        &EnvironmentOptions::default(),
    )
    .unwrap();
    assert_eq!(kind, spacetimedb_client_api_messages::publish::CONTENT_TYPE);
    let envelope = spacetimedb_client_api_messages::publish::PublishRequest::decode(&body).unwrap();
    assert_eq!(envelope.environment["KEY"], "secret-sentinel");
}

#[test]
fn short_help_is_concise_and_long_help_explains_environment_modes() {
    let short = cli().render_help().to_string();
    let long = cli()
        .render_long_help()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(!short.contains("Every publish replaces"));
    assert!(short.contains("spacetime help publish"));
    for text in [
        "Publishing preserves unspecified environment values",
        "including empty strings",
        "--replace-env replaces all stored values",
        "--env selects config file layers",
    ] {
        assert!(long.contains(text), "missing long-help guidance: {text}");
    }
}

#[test]
fn environment_flags_are_explicit_and_incompatible_modes_are_rejected() {
    for args in [
        vec!["publish", "db", "--replace-env", "--unset-env", "KEY"],
        vec!["publish", "db", "--env-only", "--bin-path", "module.wasm"],
        vec!["publish", "db", "--env-only", "--clear-database"],
    ] {
        assert!(cli().try_get_matches_from(args).is_err());
    }
    let args = cli()
        .try_get_matches_from(["publish", "db", "--env-only", "--unset-env", "A", "--unset-env", "B"])
        .unwrap();
    let options = EnvironmentOptions::from_args(&args).unwrap();
    assert!(options.only);
    assert_eq!(options.remove, ["A", "B"]);
    assert!(options
        .validate_values(&BTreeMap::from([("A".into(), "secret-sentinel".into())]))
        .is_err());
    let args = cli()
        .try_get_matches_from(["publish", "db", "--env-only", "--replace-env"])
        .unwrap();
    let options = EnvironmentOptions::from_args(&args).unwrap();
    assert!(options.only && options.replace);
}

#[test]
fn removal_and_replacement_use_json_even_for_legacy_modules() {
    for options in [
        EnvironmentOptions {
            remove: vec!["OLD".into()],
            ..Default::default()
        },
        EnvironmentOptions {
            replace: true,
            ..Default::default()
        },
    ] {
        let (kind, body) = publication_body(&schema(false), vec![1], BTreeMap::new(), &options).unwrap();
        assert_eq!(kind, spacetimedb_client_api_messages::publish::CONTENT_TYPE);
        let request = spacetimedb_client_api_messages::publish::PublishRequest::decode(&body).unwrap();
        assert_eq!(request.environment_remove, options.remove);
        assert_eq!(request.environment_replace, options.replace);
    }
}
