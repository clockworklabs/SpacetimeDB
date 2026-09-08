//! Resolve a complete, declared environment without consulting stored values.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{ensure, Context};
use serde_json::Value;
use spacetimedb_lib::environment::EnvironmentSchema;
use spacetimedb_lib::{sats::serde::SerdeWrapper, RawModuleDef};
use spacetimedb_schema::def::ModuleDef;
use tokio::io::AsyncReadExt;

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

pub(super) fn read_program(path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(spacetimedb_client_api_messages::publish::MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= spacetimedb_client_api_messages::publish::MAX_MODULE_BYTES,
        "Module exceeds publish size limit"
    );
    Ok(bytes)
}

const MAX_SCHEMA_BYTES: u64 = 16 * 1024 * 1024;
const INSPECT_TIMEOUT: Duration = Duration::from_secs(60);

/// Inspect exactly the artifact bytes that will be uploaded. The private copy
/// prevents path replacement between inspection and upload, including --bin-path.
/// This invokes only local extraction, never a server or a saved CLI context.
pub(super) async fn inspect(program: &[u8], host_type: &str) -> anyhow::Result<ModuleDef> {
    let extractor = std::env::var_os("SPACETIMEDB_SCHEMA_EXTRACTOR")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| crate::util::resolve_sibling_binary("spacetimedb-standalone"))?;
    inspect_with(extractor, program.to_vec(), host_type.to_owned(), INSPECT_TIMEOUT).await
}

async fn inspect_with(
    extractor: PathBuf,
    program: Vec<u8>,
    host_type: String,
    deadline: Duration,
) -> anyhow::Result<ModuleDef> {
    let (mut send, mut recv) = tokio::sync::oneshot::channel();
    // This owner retains the child and private file until actual reaping, even
    // when its caller drops while extraction or stdout reading is in progress.
    tokio::spawn(async move {
        let result = async {
            let dir = tempfile::tempdir().context("Cannot create private module inspection directory")?;
            let module = dir.path().join("module");
            tokio::fs::write(&module, program)
                .await
                .context("Cannot prepare module inspection input")?;
            let mut child = tokio::process::Command::new(extractor)
                .arg("extract-schema")
                .arg(&module)
                .arg("--host-type")
                .arg(host_type.to_ascii_lowercase())
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .context("Cannot start local module schema inspection")?;
            let mut output = Vec::new();
            let mut stdout = child
                .stdout
                .take()
                .context("Module inspection stdout unavailable")?
                .take(MAX_SCHEMA_BYTES + 1);
            let result = tokio::select! {
                biased;
                _ = send.closed() => Err(anyhow::anyhow!("Module inspection cancelled")),
                result = tokio::time::timeout(deadline, async {
                    stdout.read_to_end(&mut output).await.context("Cannot read local module schema")?;
                    ensure!(output.len() as u64 <= MAX_SCHEMA_BYTES, "Local module schema exceeds output limit");
                    let status = child.wait().await.context("Cannot reap local module inspector")?;
                    ensure!(status.success(), "Local module schema inspection failed");
                    Ok(())
                }) => result.unwrap_or_else(|_| Err(anyhow::anyhow!("Local module schema inspection timed out"))),
            };
            if result.is_err() {
                // Queue termination, then retain ownership through positive reaping.
                let _ = child.start_kill();
                child
                    .wait()
                    .await
                    .context("Cannot reap failed local module inspector")?;
            }
            result?;
            // Neither parser nor validation diagnostics may echo schema literals.
            let SerdeWrapper::<RawModuleDef>(raw) = serde_json::from_slice(&output)
                .map_err(|_| anyhow::anyhow!("Local module inspector returned invalid schema data"))?;
            let schema =
                ModuleDef::try_from(raw).map_err(|_| anyhow::anyhow!("Local module schema validation failed"))?;
            Ok(schema)
        }
        .await;
        let _ = send.send(result);
    });
    (&mut recv).await.context("Local module inspection owner failed")?
}

#[cfg(test)]
mod tests;
