//! Resolve a complete, declared environment without consulting stored values.
use std::collections::BTreeMap;
use std::ffi::OsString;

pub(super) use crate::schema_extract::{inspect, read_program};
use anyhow::{ensure, Context};
use serde_json::Value;
use spacetimedb_lib::environment::EnvironmentSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Config,
    Shell,
}
impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Config => "config",
            Self::Shell => "shell",
        })
    }
}

// Deliberately no Debug: values are credentials, not diagnostics.
pub(super) struct Resolved {
    pub values: BTreeMap<String, String>,
    pub sources: BTreeMap<String, Source>,
}
impl Resolved {
    pub fn display(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();
        for (name, source) in &self.sources {
            let _ = writeln!(output, "Environment {name} ({source})");
        }
        output
    }
}

pub(super) fn resolve(
    schema: &EnvironmentSchema,
    config: Option<&Value>,
    mut shell: impl FnMut(&str) -> Option<OsString>,
) -> anyhow::Result<Resolved> {
    let mut resolved = Resolved {
        values: BTreeMap::new(),
        sources: BTreeMap::new(),
    };
    if let Some(config) = config {
        let config = config.as_object().context("Environment config must be an object")?;
        for (name, value) in config {
            ensure!(
                schema.get(name).is_some(),
                "Environment key {name:?}: key is not declared"
            );
            let value = match value {
                Value::String(value) => value.clone(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                _ => anyhow::bail!("Environment key {name:?}: config input must be a string, boolean or JSON number"),
            };
            resolved.values.insert(name.clone(), value);
            resolved.sources.insert(name.clone(), Source::Config);
        }
    }
    // Lookup only the new artifact's declared names, never enumerate ambient values.
    for declaration in schema.declarations() {
        if let Some(value) = shell(&declaration.name) {
            let value = value
                .into_string()
                .map_err(|_| anyhow::anyhow!("Environment key {:?}: shell input must be UTF-8", declaration.name))?;
            resolved.values.insert(declaration.name.clone(), value);
            resolved.sources.insert(declaration.name.clone(), Source::Shell);
        }
    }
    schema.validate_values(&resolved.values)?;
    Ok(resolved)
}

#[cfg(test)]
mod tests;
