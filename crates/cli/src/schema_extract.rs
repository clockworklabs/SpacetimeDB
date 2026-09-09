//! Local schema extraction shared by publish and generate. Each invocation
//! inspects a private copy of exact bounded input bytes, bounds its output and
//! lifetime, and owns the child through kill/wait on failure or cancellation.
use anyhow::{ensure, Context};
use spacetimedb_lib::{sats::serde::SerdeWrapper, RawModuleDef};
use spacetimedb_schema::def::ModuleDef;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncReadExt;

fn extractor_path() -> anyhow::Result<PathBuf> {
    std::env::var_os("SPACETIMEDB_SCHEMA_EXTRACTOR")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| crate::util::resolve_sibling_binary("spacetimedb-standalone"))
}

/// Keep generate's synchronous injectable function API. Its small dedicated
/// runtime works both inside and outside a caller's Tokio runtime; process
/// ownership and validation are identical to publish's async path.
pub(crate) fn from_path(path: &Path) -> anyhow::Result<ModuleDef> {
    let bytes = read_program(path)?;
    let host_type = match path.extension().and_then(|ext| ext.to_str()) {
        Some("wasm") => "Wasm",
        Some("js") => "Js",
        _ => anyhow::bail!("Cannot determine module type from file extension"),
    };
    inspect_blocking(extractor_path()?, bytes, host_type.into())
}

fn inspect_blocking(extractor: PathBuf, bytes: Vec<u8>, host_type: String) -> anyhow::Result<ModuleDef> {
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(inspect_with(extractor, bytes, host_type, INSPECT_TIMEOUT))
    })
    .join()
    .map_err(|_| anyhow::anyhow!("Local module inspection thread failed"))?
}

pub(crate) fn read_program(path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
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
pub(crate) async fn inspect(program: &[u8], host_type: &str) -> anyhow::Result<ModuleDef> {
    let extractor = extractor_path()?;
    inspect_with(extractor, program.to_vec(), host_type.to_owned(), INSPECT_TIMEOUT).await
}

pub(crate) async fn inspect_with(
    extractor: PathBuf,
    program: Vec<u8>,
    host_type: String,
    deadline: Duration,
) -> anyhow::Result<ModuleDef> {
    inspect_observed(extractor, program, host_type, deadline, Observation::default()).await
}

#[derive(Default)]
struct Observation {
    #[cfg(test)]
    started: Option<tokio::sync::oneshot::Sender<u32>>,
    #[cfg(test)]
    reaped: Option<tokio::sync::oneshot::Sender<std::process::ExitStatus>>,
}
impl Observation {
    fn started(&mut self, _pid: Option<u32>) {
        #[cfg(test)]
        if let Some(send) = self.started.take() {
            let _ = send.send(_pid.expect("new child has a PID"));
        }
    }
    fn reaped(&mut self, _status: std::process::ExitStatus) {
        #[cfg(test)]
        if let Some(send) = self.reaped.take() {
            let _ = send.send(_status);
        }
    }
}

async fn inspect_observed(
    extractor: PathBuf,
    program: Vec<u8>,
    host_type: String,
    deadline: Duration,
    mut observation: Observation,
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
            observation.started(child.id());
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
                    observation.reaped(status);
                    ensure!(status.success(), "Local module schema inspection failed");
                    Ok(())
                }) => result.unwrap_or_else(|_| Err(anyhow::anyhow!("Local module schema inspection timed out"))),
            };
            if result.is_err() {
                // Queue termination, then retain ownership through positive reaping.
                let _ = child.start_kill();
                let status = child
                    .wait()
                    .await
                    .context("Cannot reap failed local module inspector")?;
                observation.reaped(status);
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
