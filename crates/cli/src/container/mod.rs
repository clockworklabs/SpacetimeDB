//! Local image preparation shared by build-only commands and managed publication.
pub mod config;
pub mod oci;
pub mod process;
pub mod publish;

#[cfg(test)]
pub(crate) mod tests;

use anyhow::{ensure, Context, Result};
use config::{ContainerConfig, ImageSource, SourceBuild};
use oci::{LocalArtifact, VerifiedImage};
use process::{Invocation, Runner};
use serde::{Deserialize, Serialize};
use spacetimedb_lib::container::{ContainerSpec, ImagePlatform};
use spacetimedb_oci::Descriptor;
use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

pub const RAILPACK_VERSION: &str = "0.35.0";
pub const RAILPACK_FRONTEND: &str = "ghcr.io/railwayapp/railpack-frontend:v0.35.0";
const BUILD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(5 * 60);
static VERIFIERS: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));

pub struct BuildSecret {
    pub name: String,
    pub file: PathBuf,
}
pub struct BuildTools {
    pub buildctl: PathBuf,
    pub buildkit_host: Option<String>,
    pub railpack: PathBuf,
    pub skopeo: PathBuf,
    pub registry_auth_file: Option<PathBuf>,
    pub secrets: Vec<BuildSecret>,
}
impl Default for BuildTools {
    fn default() -> Self {
        Self {
            buildctl: "buildctl".into(),
            buildkit_host: None,
            railpack: "railpack".into(),
            skopeo: "skopeo".into(),
            registry_auth_file: None,
            secrets: vec![],
        }
    }
}

/// Public metadata contains immutable descriptors and relative paths, not image
/// environment values or credentials. Reopening output must verify the bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedMetadata {
    pub version: u32,
    pub container: ContainerSpec,
    pub manifest: Descriptor,
    pub objects: Vec<LocalArtifact>,
}
/// Owns all temporary output. Drop removes it; persist transfers only verified
/// artifacts. A failed/cancelled prepare never yields this type.
pub struct PreparedContainer {
    workspace: Arc<TempDir>,
    pub metadata: PreparedMetadata,
}
impl PreparedContainer {
    pub fn layout(&self) -> PathBuf {
        self.workspace.path().join("verified")
    }
    pub fn persist(self, output: &Path) -> Result<()> {
        ensure!(
            !output.exists(),
            "output directory already exists: {}",
            output.display()
        );
        // The final transition must not replace a directory created after the
        // initial check. The workspace is placed alongside the requested output.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            self.layout(),
            rustix::fs::CWD,
            output,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .context("could not publish local OCI output without replacing an existing path")?;
        #[cfg(windows)]
        fs::rename(self.layout(), output)
            .context("could not publish local OCI output without replacing an existing path")?;
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        anyhow::bail!("atomic container output is supported on Linux, macOS, and Windows");
        Ok(())
    }
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn path_argument(key: &str, path: &Path) -> Result<OsString> {
    let path = path.to_str().context("image tools require UTF-8 paths")?;
    Ok(format!("{key}={path}").into())
}
fn csv_argument(key: &str, path: &Path) -> Result<String> {
    let path = path.to_str().context("image tools require UTF-8 paths")?;
    Ok(format!("\"{key}={}\"", path.replace('"', "\"\"")))
}
fn read_credential(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    fs::File::open(path)?.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "explicit build credential file exceeds 1 MiB"
    );
    Ok(bytes)
}
fn local_buildkit_host(value: Option<&str>) -> Result<&str> {
    let value = value.context("source builds require --buildkit-host unix:///absolute/path/to/buildkitd.sock")?;
    let path = value
        .strip_prefix("unix://")
        .context("local source builds require an explicit BuildKit Unix socket")?;
    ensure!(
        Path::new(path).is_absolute() && !path.contains(['\0', '\n', '\r']),
        "invalid local BuildKit socket path"
    );
    Ok(value)
}

pub async fn prepare_container(
    declaration: &ContainerConfig,
    config_dir: &Path,
    platform: ImagePlatform,
    tools: &BuildTools,
    workspace_parent: &Path,
    runner: &impl Runner,
    cancel: CancellationToken,
) -> Result<PreparedContainer> {
    ensure!(
        platform.os == "linux" && matches!(platform.architecture.as_str(), "amd64" | "arm64"),
        "choose linux/amd64 or linux/arm64 explicitly"
    );
    ensure!(
        declaration.mounts.is_empty(),
        "container mounts are not supported in Stage 1"
    );
    let cancel = cancel.child_token();
    let _cancel = CancelOnDrop(cancel.clone());
    let workspace = Arc::new(
        tempfile::Builder::new()
            .prefix(".spacetime-image-")
            .tempdir_in(workspace_parent)?,
    );
    let base = workspace.path();
    fs::create_dir(base.join("auth"))?;
    let auth_file = base.join("auth/config.json");
    let auth = tools
        .registry_auth_file
        .as_deref()
        .map(read_credential)
        .transpose()?
        .unwrap_or_else(|| br#"{"auths":{}}"#.to_vec());
    fs::write(&auth_file, auth)?;
    let environment = vec![
        ("DOCKER_CONFIG".into(), base.join("auth").into_os_string()),
        ("REGISTRY_AUTH_FILE".into(), auth_file.clone().into_os_string()),
        ("XDG_CONFIG_HOME".into(), base.join("tool-config").into_os_string()),
        ("XDG_CACHE_HOME".into(), base.join("tool-cache").into_os_string()),
    ];
    let invoke = |tool: PathBuf, label, args, cwd: PathBuf| Invocation {
        tool,
        label,
        args,
        env: environment.clone(),
        cwd,
        workspace: workspace.clone(),
        timeout: BUILD_TIMEOUT,
        cancel: cancel.clone(),
    };
    let input = base.join("input");
    let mut archive = None;
    match &declaration.image {
        ImageSource::Prebuilt(image) => {
            ensure!(
                tools.secrets.is_empty(),
                "build secrets cannot be supplied with a prebuilt image"
            );
            if let Some(path) = image.oci_ref.strip_prefix("oci:") {
                let path = config_dir
                    .join(path)
                    .canonicalize()
                    .context("prebuilt OCI layout does not exist")?;
                ensure!(path.is_dir(), "oci: must name an OCI layout directory");
                return verify(declaration.clone(), platform, workspace.clone(), path, None, cancel).await;
            }
            ensure!(
                !image.oci_ref.is_empty()
                    && !image.oci_ref.starts_with('-')
                    && !image.oci_ref.contains(['\0', '\n', '\r']),
                "invalid OCI registry reference"
            );
            let reference = image.oci_ref.strip_prefix("docker://").unwrap_or(&image.oci_ref);
            ensure!(
                !reference.contains("://")
                    && (!reference.contains('@')
                        || reference.rsplit_once('@').is_some_and(|(_, digest)| digest
                            .parse::<spacetimedb_lib::container::OciDigest>()
                            .is_ok())),
                "invalid OCI registry reference"
            );
            let args = vec![
                "--override-os".into(),
                platform.os.clone().into(),
                "--override-arch".into(),
                platform.architecture.clone().into(),
                "copy".into(),
                "--preserve-digests".into(),
                "--authfile".into(),
                auth_file.into_os_string(),
                format!("docker://{reference}").into(),
                format!("oci:{}:prepared", input.display()).into(),
            ];
            runner
                .run(invoke(
                    tools.skopeo.clone(),
                    "Skopeo image import",
                    args,
                    config_dir.to_path_buf(),
                ))
                .await?;
        }
        ImageSource::Build(image) => {
            let endpoint = local_buildkit_host(tools.buildkit_host.as_deref())?;
            let (context, dockerfile, railpack) = match &image.build {
                SourceBuild::Dockerfile { context, dockerfile } => (context, Some(dockerfile), false),
                SourceBuild::Railpack { context } => (context, None, true),
            };
            let context = config_dir
                .join(context)
                .canonicalize()
                .context("build context does not exist")?;
            ensure!(context.is_dir(), "build context must be a directory");
            let mut secret_arguments = vec![];
            let mut secret_names = std::collections::BTreeSet::new();
            for (index, secret) in tools.secrets.iter().enumerate() {
                spacetimedb_lib::container::validate_env_key(&secret.name)?;
                ensure!(secret_names.insert(&secret.name), "duplicate build secret name");
                let path = base.join(format!("secret-{index}"));
                fs::write(&path, read_credential(&secret.file)?)?;
                secret_arguments.extend([
                    OsString::from("--secret"),
                    format!("id={},{}", secret.name, csv_argument("src", &path)?).into(),
                ]);
            }
            let dockerfile = if railpack {
                let version = runner
                    .run(invoke(
                        tools.railpack.clone(),
                        "Railpack version check",
                        vec!["--version".into()],
                        context.clone(),
                    ))
                    .await?;
                let version = std::str::from_utf8(&version.stdout).context("invalid Railpack version response")?;
                ensure!(
                    version
                        .split_whitespace()
                        .any(|word| word.trim_start_matches('v') == RAILPACK_VERSION),
                    "install Railpack {RAILPACK_VERSION} to match the pinned frontend"
                );
                let plan = base.join("railpack-plan.json");
                let mut args = vec![
                    "prepare".into(),
                    context.clone().into_os_string(),
                    "--plan-out".into(),
                    plan.clone().into_os_string(),
                    "--info-out".into(),
                    base.join("railpack-info.json").into_os_string(),
                ];
                // Only names enter the plan; BuildKit receives the actual files.
                for name in secret_names {
                    args.extend(["--env".into(), format!("{name}=").into()]);
                }
                runner
                    .run(invoke(
                        tools.railpack.clone(),
                        "Railpack detection",
                        args,
                        context.clone(),
                    ))
                    .await?;
                ensure!(
                    plan.is_file() && plan.metadata()?.len() <= 4 * 1024 * 1024,
                    "Railpack did not produce a bounded build plan"
                );
                plan
            } else {
                context
                    .join(dockerfile.unwrap())
                    .canonicalize()
                    .context("Dockerfile does not exist")?
            };
            ensure!(
                dockerfile.is_file(),
                "Dockerfile or Railpack plan must be a regular file"
            );
            let output = base.join("image.tar");
            let mut args = vec![
                "--addr".into(),
                endpoint.into(),
                "build".into(),
                "--frontend".into(),
                if railpack {
                    "gateway.v0".into()
                } else {
                    "dockerfile.v0".into()
                },
                "--local".into(),
                path_argument("context", &context)?,
                "--local".into(),
                path_argument("dockerfile", dockerfile.parent().unwrap())?,
                "--opt".into(),
                path_argument("filename", Path::new(dockerfile.file_name().unwrap()))?,
                "--opt".into(),
                format!("platform={}/{}", platform.os, platform.architecture).into(),
                "--output".into(),
                format!("type=oci,{}", csv_argument("dest", &output)?).into(),
            ];
            if railpack {
                args.extend(["--opt".into(), format!("source={RAILPACK_FRONTEND}").into()]);
            }
            if !secret_arguments.is_empty() {
                args.push("--no-cache".into());
                args.extend(secret_arguments);
            }
            runner
                .run(invoke(tools.buildctl.clone(), "BuildKit OCI build", args, context))
                .await?;
            archive = Some(output);
        }
    }
    verify(declaration.clone(), platform, workspace.clone(), input, archive, cancel).await
}

async fn verify(
    declaration: ContainerConfig,
    platform: ImagePlatform,
    workspace: Arc<TempDir>,
    input: PathBuf,
    archive: Option<PathBuf>,
    cancel: CancellationToken,
) -> Result<PreparedContainer> {
    let permit = VERIFIERS
        .clone()
        .try_acquire_owned()
        .context("two OCI images are already being verified")?;
    let owner = workspace.clone();
    let metadata = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        if let Some(archive) = archive {
            oci::extract_archive(&archive, &input, &cancel, deadline)?;
        }
        let VerifiedImage {
            manifest,
            config,
            objects,
        } = oci::verify_layout(&input, &owner.path().join("verified"), &platform, &cancel, deadline)?;
        let container = declaration.normalize(manifest.digest, platform, &config.config)?;
        let metadata = PreparedMetadata {
            version: 1,
            container,
            manifest,
            objects,
        };
        fs::write(
            owner.path().join("verified/prepared.json"),
            serde_json::to_vec_pretty(&metadata)?,
        )?;
        Ok::<_, anyhow::Error>(metadata)
    })
    .await
    .context("OCI verification worker stopped")??;
    Ok(PreparedContainer { workspace, metadata })
}
