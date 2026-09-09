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
    let (kind, body) = publication_body(&schema(false), bytes.clone(), BTreeMap::new()).unwrap();
    assert_eq!(kind, "application/octet-stream");
    assert_eq!(body, bytes);
    let (kind, body) = publication_body(&schema(true), bytes.clone(), BTreeMap::new()).unwrap();
    assert_eq!(kind, spacetimedb_client_api_messages::publish::CONTENT_TYPE);
    let envelope = spacetimedb_client_api_messages::publish::PublishRequest::decode(&body).unwrap();
    assert_eq!(envelope.module, bytes);
    assert!(envelope.environment.is_empty());
    let error = publication_body(
        &schema(false),
        bytes,
        BTreeMap::from([("KEY".into(), "secret-sentinel".into())]),
    )
    .unwrap_err();
    assert!(!format!("{error:#}").contains("secret-sentinel"));
}

#[test]
fn short_help_is_concise_and_long_help_explains_environment_replacement() {
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
        "Every publish replaces the complete declared environment",
        "including empty strings",
        "Optional values omitted",
        "--env selects config file layers",
    ] {
        assert!(long.contains(text), "missing long-help guidance: {text}");
    }
}
