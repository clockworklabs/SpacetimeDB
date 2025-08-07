use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("Config error: The option `{key}` not found")]
    Config { key: String },
    #[error("Config error: The option `{key}` is not a `{kind}`, found: `{type}: {value}`",
        type=found.type_name(),
        value=found
    )]
    ConfigType {
        key: String,
        kind: &'static str,
        found: Box<toml_edit::Item>,
    },
}
