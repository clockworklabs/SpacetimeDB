use super::*;
use std::collections::BTreeMap;

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
        "--env selects which config file to use",
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
    assert_eq!(options.remove, EnvironmentRemove::Keys(vec!["A".into(), "B".into()]));
    assert!(options
        .validate_values(&BTreeMap::from([("A".into(), "secret-sentinel".into())]))
        .is_err());
    let args = cli()
        .try_get_matches_from(["publish", "db", "--env-only", "--replace-env"])
        .unwrap();
    let options = EnvironmentOptions::from_args(&args).unwrap();
    assert!(options.only && options.remove == EnvironmentRemove::All);
}
