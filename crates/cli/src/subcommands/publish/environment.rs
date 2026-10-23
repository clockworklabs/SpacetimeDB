//! Resolve explicit overrides without fetching stored secrets.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;

pub(super) use crate::schema_extract::{inspect, read_program};
use crate::util::ResponseExt;
use anyhow::Context;
use headers::HeaderMapExt;
use serde_json::Value;
use spacetimedb_client_api_messages::name::EnvironmentPublishError;
use spacetimedb_client_api_messages::publish::SpacetimeEnvironmentRemove;
use spacetimedb_lib::environment::{EnvironmentRemove, EnvironmentSchema};

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

impl fmt::Display for Resolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, source) in &self.sources {
            writeln!(f, "Environment {name} ({source})")?;
        }
        Ok(())
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
            spacetimedb_lib::environment::validate_key(name)?;
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
    schema.validate_supplied_values(&resolved.values)?;
    Ok(resolved)
}

#[cfg(test)]
mod tests;

/// Update configuration without inspecting or building a local module.
pub(super) async fn publish_only(
    config: &mut crate::config::Config,
    server: Option<&str>,
    database: &str,
    anonymous: bool,
    yes: super::YesFlags,
    input: Option<&Value>,
    options: &super::EnvironmentOptions,
) -> anyhow::Result<()> {
    use crate::util::{add_auth_header_opt, get_auth_header, y_or_n};
    use spacetimedb_client_api_messages::publish::EnvironmentMetadata;

    let host = config.get_host_url(server)?;
    let server_url = reqwest::Url::parse(&host)?;
    let hostname = server_url.host_str().context("Server URL has no hostname")?;
    if hostname != "localhost" && hostname != "127.0.0.1" {
        println!("You are about to publish environment values to a non-local server: {hostname}");
        anyhow::ensure!(
            y_or_n(yes.publish_to_remote, "Are you sure you want to proceed?")?,
            "Publish aborted by user"
        );
    }
    let auth = get_auth_header(config, anonymous, server, !yes.skip_login).await?;
    let encoded = percent_encoding::percent_encode(
        database.as_bytes(),
        const { &percent_encoding::NON_ALPHANUMERIC.remove(b'_').remove(b'-') },
    )
    .to_string();
    let url = format!("{host}/v1/database/{encoded}/environment");
    // Neither credentials nor publish bodies may be forwarded to redirect destinations.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = add_auth_header_opt(client.get(&url), &auth).send().await?;
    let metadata: EnvironmentMetadata = response
        .json_or_error()
        .await
        .context("failed to fetch environment schema")?;
    let schema = EnvironmentSchema::new(metadata.declarations)?;
    let resolved = resolve(&schema, input, |key| std::env::var_os(key))?;
    options.validate_values(&resolved.values)?;
    print!("{}", resolved);
    let request = if let EnvironmentRemove::All = options.remove {
        client.put(url)
    } else {
        client.patch(url)
    };
    let mut request = add_auth_header_opt(request, &auth)
        .query(&[("expected_module_hash", &metadata.module_hash)])
        .json(&resolved.values)
        .build()?;
    if let EnvironmentRemove::Keys(_) = options.remove {
        request
            .headers_mut()
            .typed_insert(SpacetimeEnvironmentRemove(options.remove.clone()));
    }
    let res = client.execute(request).await?;
    let response: Result<(), EnvironmentPublishError> = res.json_or_error().await?;
    match response {
        Ok(()) => {
            println!("Successfully updated environment");
            Ok(())
        }
        Err(EnvironmentPublishError::MissingRequiredEnvironment { keys }) => {
            anyhow::bail!("Missing required environment variable(s) {keys:?}")
        }
        Err(EnvironmentPublishError::VersionConflict(e)) => anyhow::bail!(e),
    }
}
