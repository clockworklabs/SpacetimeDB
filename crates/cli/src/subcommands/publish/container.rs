//! Attach the container that `spacetime.json` configures to a published database.
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context};
use path_clean::PathClean;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use serde_json::Value;
use spacetimedb_lib::container::{
    is_local_image_id, ContainerInfo, ContainerResources, ContainerSpec, ContainerState, RestartPolicy,
};
use spacetimedb_lib::environment::EnvironmentSchema;
use spacetimedb_lib::Identity;

use crate::subcommands::container::{check_response, container_url, describe_state};
use crate::util::{add_auth_header_opt, AuthHeader};

/// The platforms to build for when pushing to a registry, so the image runs on any node.
const DEFAULT_PLATFORMS: [&str; 2] = ["linux/amd64", "linux/arm64"];

/// The `container` field of a database in `spacetime.json`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ContainerConfig {
    /// A directory with a Dockerfile, relative to the config file.
    build: Option<PathBuf>,
    /// The registry repository to push builds to for remote servers, or an image pinned to a digest.
    image: Option<String>,
    #[serde(default)]
    env_keys: Vec<String>,
    command: Option<Vec<String>>,
    restart: Option<Restart>,
    cpu_millicores: Option<u32>,
    /// A number of bytes, or a size like `"512MiB"`; see [`size`].
    memory_bytes: Option<Value>,
    scratch_bytes: Option<Value>,
    pids_max: Option<u32>,
    platforms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Restart {
    OnFailure,
    Always,
    Never,
}

impl From<Restart> for RestartPolicy {
    fn from(restart: Restart) -> Self {
        match restart {
            Restart::OnFailure => Self::OnFailure,
            Restart::Always => Self::Always,
            Restart::Never => Self::Never,
        }
    }
}

/// Read the field `name`: a number of bytes, or a size like `"512MiB"` or `"1GB"`.
fn size(value: Option<Value>, name: &str) -> anyhow::Result<Option<u64>> {
    let bytes = match &value {
        None => return Ok(None),
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => parse_size(s),
        Some(_) => None,
    };
    bytes
        .map(Some)
        .with_context(|| format!("`{name}` must be a number of bytes, or a size like \"512MiB\" or \"1GB\""))
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (number, unit) = s.split_at(split);
    let multiplier: u64 = match unit.trim() {
        "" | "B" => 1,
        "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "TB" => 1_000_000_000_000,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        "TiB" => 1 << 40,
        _ => return None,
    };
    number.parse::<u64>().ok()?.checked_mul(multiplier)
}

/// Where the container's image comes from.
#[derive(Debug, PartialEq)]
enum Image {
    /// Build the Dockerfile in `dir`. For a remote server, push the build to `repository`.
    Build {
        dir: PathBuf,
        repository: Option<String>,
        platforms: Vec<String>,
    },
    /// An image pinned to a digest.
    Pinned(String),
}

/// A database's container, as configured in `spacetime.json`.
#[derive(Debug)]
pub(super) struct Container {
    image: Image,
    /// The spec to attach. Its image is set once the image is built.
    spec: ContainerSpec,
}

impl Container {
    /// Read and check a target's `container` field. A missing or `null` field means no container.
    pub(super) fn from_config(value: Option<&Value>, config_dir: Option<&Path>) -> anyhow::Result<Option<Self>> {
        let Some(value) = value.filter(|value| !value.is_null()) else {
            return Ok(None);
        };
        let config: ContainerConfig = serde_json::from_value(value.clone()).map_err(|e| anyhow!("{e}"))?;
        ensure!(
            config.platforms.is_none() || config.build.is_some(),
            "`platforms` applies only to images built from `build`"
        );
        let image = match (config.build, config.image) {
            (None, None) => {
                bail!("set `build` to a directory with a Dockerfile, or `image` to an image pinned to a digest")
            }
            (Some(_), Some(image)) if is_pinned(&image) => {
                bail!("`image` {image} is pinned to a digest, so `build` would not be used; remove one of them")
            }
            (None, Some(image)) if !is_pinned(&image) => {
                bail!("`image` {image} must be pinned to a digest, like `name@sha256:<hex>`, unless `build` is set")
            }
            (None, Some(image)) => Image::Pinned(image),
            (Some(dir), repository) => {
                let dir = config_dir.map_or_else(|| dir.clone(), |base| base.join(&dir)).clean();
                ensure!(dir.is_dir(), "`build` directory {} does not exist", dir.display());
                let platforms = config
                    .platforms
                    .unwrap_or_else(|| DEFAULT_PLATFORMS.map(String::from).to_vec());
                ensure!(!platforms.is_empty(), "`platforms` must not be empty");
                Image::Build {
                    dir,
                    repository,
                    platforms,
                }
            }
        };
        let defaults = ContainerResources::default();
        let spec = ContainerSpec {
            image: match &image {
                Image::Pinned(image) => image.clone(),
                Image::Build { .. } => String::new(),
            },
            command: config.command,
            env_keys: config.env_keys,
            resources: ContainerResources {
                cpu_millicores: config.cpu_millicores.unwrap_or(defaults.cpu_millicores),
                memory_bytes: size(config.memory_bytes, "memory-bytes")?.unwrap_or(defaults.memory_bytes),
                scratch_bytes: size(config.scratch_bytes, "scratch-bytes")?.unwrap_or(defaults.scratch_bytes),
                pids_max: config.pids_max.unwrap_or(defaults.pids_max),
            },
            ports: Vec::new(),
            restart: config.restart.map(Into::into).unwrap_or_default(),
        };
        // Check everything but a built image, which is known only after building. The server
        // decides whether it accepts local image IDs.
        let placeholder = format!("sha256:{}", "0".repeat(64));
        ContainerSpec {
            image: if spec.image.is_empty() {
                placeholder
            } else {
                spec.image.clone()
            },
            ..spec.clone()
        }
        .validate_with(true)
        .map_err(anyhow::Error::msg)?;
        Ok(Some(Self { image, spec }))
    }

    /// Check, before publishing the module, that the image can be built for the server at `host_url`.
    pub(super) fn check(&self, host_url: &str) -> anyhow::Result<()> {
        match &self.image {
            Image::Build { repository, .. } => {
                ensure!(
                    repository.is_some() || is_loopback(host_url),
                    "the server {host_url} is not on this machine, so it can only run images from a registry. \
                     Set `container.image` in spacetime.json to a registry repository to push the build to, \
                     like `ghcr.io/<you>/<name>`, or publish with --no-container."
                );
                check_docker()
            }
            Image::Pinned(image) => {
                ensure!(
                    !is_local_image_id(image) || is_loopback(host_url),
                    "`container.image` {image} is a local image ID, which only a server on this machine can run"
                );
                Ok(())
            }
        }
    }

    /// Build the image if needed, then attach the container to the database unless it already
    /// has this one, and start it if it is stopped.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn attach(
        mut self,
        client: &reqwest::Client,
        host_url: &str,
        auth_header: &AuthHeader,
        database: &str,
        database_identity: Identity,
        declared: &EnvironmentSchema,
        supplied: &[String],
    ) -> anyhow::Result<()> {
        for key in &self.spec.env_keys {
            if declared.get(key).is_none() && !supplied.contains(key) {
                eprintln!(
                    "Warning: the module does not declare the container's environment key {key}, and this \
                     publish does not set it. The container starts only if the database environment has a \
                     value for it; set one in the `env` map of spacetime.json."
                );
            }
        }
        // Read the current container first, so that a server without containers fails before the build.
        let url = container_url(host_url, database_identity);
        let current = get(client, &url, auth_header).await?;
        // Ports are not configured in spacetime.json, so keep any set with `spacetime container set`.
        if let Some(info) = &current {
            self.spec.ports = info.spec.ports.clone();
        }
        if let Image::Build {
            dir,
            repository,
            platforms,
        } = &self.image
        {
            self.spec.image = match repository {
                Some(repository) if !is_loopback(host_url) => build_and_push(dir, repository, platforms)?,
                _ => build_local(dir, database)?,
            };
        }

        let Changes { set, start } = changes(current.as_ref(), &self.spec);
        if let Some(verb) = set {
            let request = client.put(&url).json(&self.spec);
            check_response(add_auth_header_opt(request, auth_header).send().await?).await?;
            println!("Container {verb}: {}", self.spec.image);
        } else {
            println!("Container unchanged: {}", self.spec.image);
        }
        if let Some(what) = start {
            let request = client.post(format!("{url}/start"));
            check_response(add_auth_header_opt(request, auth_header).send().await?).await?;
            println!("Container {what}");
        }

        let info = if set.is_some() || start.is_some() {
            wait_for_state(client, &url, auth_header).await?
        } else {
            current
        };
        let state = info.map_or_else(|| "pending".to_string(), |info| describe_state(&info.state));
        println!("Container status: {state}");
        Ok(())
    }
}

/// What to do so that a database with the container `current` runs the container `spec`.
#[derive(Debug, PartialEq)]
struct Changes {
    /// Set the container, describing it with this verb, unless the database already has it.
    /// Setting the same container again would restart it.
    set: Option<&'static str>,
    /// Start the container, describing it with this message. A new container starts by itself,
    /// and replacing a container keeps it running or stopped, and restarts it if running.
    start: Option<&'static str>,
}

fn changes(current: Option<&ContainerInfo>, spec: &ContainerSpec) -> Changes {
    let Some(info) = current else {
        return Changes {
            set: Some("attached"),
            start: None,
        };
    };
    let set = (info.spec != *spec).then_some("updated");
    let start = if !info.running {
        Some("started, since it was stopped")
    } else if set.is_none() && matches!(info.state, Some(ContainerState::Failed(_))) {
        // Retry now rather than after the server's backoff: publishing may have fixed the
        // cause, like a missing environment value.
        Some("restarted, since it had failed to start")
    } else {
        None
    };
    Changes { set, start }
}

/// Whether `image` names an image by digest, either in a registry or as a local image ID.
fn is_pinned(image: &str) -> bool {
    image.contains("@sha256:") || is_local_image_id(image)
}

/// Whether the server at `host_url` runs on this machine, so it shares this machine's Docker
/// daemon and can run images built here without a registry.
fn is_loopback(host_url: &str) -> bool {
    let Some(host) = Url::parse(host_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
    else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn check_docker() -> anyhow::Result<()> {
    let output = duct::cmd!("docker", "version", "--format", "{{.Server.Version}}")
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run();
    match output {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("building the container image needs Docker, but the `docker` command was not found. Install Docker, or publish with --no-container.")
        }
        Err(e) => Err(e).context("unable to run `docker`"),
        Ok(output) if !output.status.success() => bail!(
            "building the container image needs Docker, but the Docker daemon did not answer. Start Docker, or publish with --no-container.\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Ok(_) => Ok(()),
    }
}

/// Provenance attestations record when they were built, so with them, rebuilding an unchanged
/// image would give it a new ID or digest, and restart its container on every publish.
const NO_ATTESTATIONS: &str = "BUILDX_NO_DEFAULT_ATTESTATIONS";

/// Build the image with this machine's Docker daemon, which the server shares, and return its ID.
fn build_local(dir: &Path, database: &str) -> anyhow::Result<String> {
    let tag = format!("spacetimedb-local/{database}:latest");
    println!("Building container image {tag} from {}", dir.display());
    let id_file = tempfile::NamedTempFile::new()?;
    duct::cmd!("docker", "build", "--iidfile", id_file.path(), "--tag", &tag, dir)
        .env(NO_ATTESTATIONS, "1")
        .run()
        .context("`docker build` failed")?;
    let id = std::fs::read_to_string(id_file.path())?.trim().to_string();
    ensure!(is_pinned(&id), "`docker build` reported an unexpected image ID `{id}`");
    Ok(id)
}

/// Build the image for `platforms`, push it to `repository`, and return a reference pinned to
/// the digest of what was pushed.
fn build_and_push(dir: &Path, repository: &str, platforms: &[String]) -> anyhow::Result<String> {
    let (name, tagged) = name_and_tag(repository);
    println!("Building container image {tagged} for {}", platforms.join(", "));
    let metadata_file = tempfile::NamedTempFile::new()?;
    duct::cmd!(
        "docker",
        "buildx",
        "build",
        "--platform",
        platforms.join(","),
        "--tag",
        &tagged,
        "--push",
        "--metadata-file",
        metadata_file.path(),
        dir
    )
    .env(NO_ATTESTATIONS, "1")
    .run()
    .context(
        "`docker buildx build --push` failed. Check that you are logged in to the registry with `docker login`. \
         If your builder cannot build for several platforms, create one that can with \
         `docker buildx create --driver docker-container --use`, or set `platforms`.",
    )?;
    let metadata: Value = serde_json::from_slice(&std::fs::read(metadata_file.path())?)
        .context("unable to read the metadata of the pushed image")?;
    let digest = metadata["containerimage.digest"]
        .as_str()
        .context("`docker buildx build` did not report the digest of the pushed image")?;
    Ok(format!("{name}@{digest}"))
}

/// The repository of an image reference with an optional tag, and the reference to push, which
/// is tagged `latest` if it has no tag.
fn name_and_tag(repository: &str) -> (&str, String) {
    // A colon followed by a `/` separates a registry's host and port, not a tag.
    match repository.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => (name, repository.to_string()),
        _ => (repository, format!("{repository}:latest")),
    }
}

/// The database's container, or `None` if it has none.
async fn get(client: &reqwest::Client, url: &str, auth_header: &AuthHeader) -> anyhow::Result<Option<ContainerInfo>> {
    let response = add_auth_header_opt(client.get(url), auth_header).send().await?;
    // The server explains a missing container, while a server without the container API
    // answers an empty 404, which `check_response` reports.
    if response.status() == StatusCode::NOT_FOUND && response.content_length() != Some(0) {
        return Ok(None);
    }
    Ok(Some(check_response(response).await?.json().await?))
}

/// Wait briefly for the server to report how the container started.
async fn wait_for_state(
    client: &reqwest::Client,
    url: &str,
    auth_header: &AuthHeader,
) -> anyhow::Result<Option<ContainerInfo>> {
    let mut info = None;
    for _ in 0..20 {
        info = get(client, url, auth_header).await?;
        if !info
            .as_ref()
            .is_some_and(|info| matches!(info.state, None | Some(ContainerState::Starting)))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetime_config::SpacetimeConfig;
    use serde_json::json;

    fn digest() -> String {
        format!("sha256:{}", "a".repeat(64))
    }

    fn container(value: Value, dir: &Path) -> anyhow::Result<Option<Container>> {
        Container::from_config(Some(&value), Some(dir))
    }

    #[test]
    fn reads_a_build_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("agent")).unwrap();
        let c = container(json!({ "build": "./agent", "env-keys": ["API_KEY"] }), dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(
            c.image,
            Image::Build {
                dir: dir.path().join("agent"),
                repository: None,
                platforms: vec!["linux/amd64".into(), "linux/arm64".into()],
            }
        );
        assert_eq!(c.spec.env_keys, ["API_KEY"]);
        assert_eq!(c.spec.resources, ContainerResources::default());
        assert_eq!(c.spec.restart, RestartPolicy::OnFailure);
        assert_eq!(c.spec.command, None);
    }

    #[test]
    fn reads_a_pinned_image_with_limits() {
        let image = format!("ghcr.io/me/agent@{}", digest());
        let c = container(
            json!({
                "image": image,
                "command": ["node", "main.js"],
                "restart": "always",
                "cpu-millicores": 500,
                "memory-bytes": "512MiB",
                "scratch-bytes": 1000,
                "pids-max": 64,
            }),
            Path::new("."),
        )
        .unwrap()
        .unwrap();
        assert_eq!(c.image, Image::Pinned(image.clone()));
        assert_eq!(c.spec.image, image);
        assert_eq!(c.spec.command, Some(vec!["node".into(), "main.js".into()]));
        assert_eq!(c.spec.restart, RestartPolicy::Always);
        assert_eq!(
            c.spec.resources,
            ContainerResources {
                cpu_millicores: 500,
                memory_bytes: 512 << 20,
                scratch_bytes: 1000,
                pids_max: 64,
            }
        );
    }

    #[test]
    fn reads_a_local_image_id() {
        let c = container(json!({ "image": digest() }), Path::new("."))
            .unwrap()
            .unwrap();
        assert_eq!(c.image, Image::Pinned(digest()));
    }

    #[test]
    fn null_means_no_container() {
        assert!(container(Value::Null, Path::new(".")).unwrap().is_none());
        assert!(Container::from_config(None, None).unwrap().is_none());
    }

    #[test]
    fn rejects_invalid_configs() {
        let dir = tempfile::tempdir().unwrap();
        let pinned = format!("agent@{}", digest());
        for (config, error) in [
            (json!({}), "set `build`"),
            (json!({ "image": "agent:v1" }), "must be pinned"),
            (json!({ "build": ".", "image": pinned }), "would not be used"),
            (json!({ "build": "./missing" }), "does not exist"),
            (json!({ "image": pinned, "platforms": ["linux/amd64"] }), "applies only"),
            (json!({ "build": ".", "platforms": [] }), "must not be empty"),
            (json!({ "build": ".", "restart": "sometimes" }), "unknown variant"),
            (
                json!({ "build": ".", "memory-bytes": "1 lot" }),
                "`memory-bytes` must be",
            ),
            (json!({ "build": ".", "scratch-bytes": -1 }), "`scratch-bytes` must be"),
            (json!({ "build": ".", "env-keys": ["1BAD"] }), "environment key"),
            (json!({ "build": ".", "command": [] }), "command"),
            (json!({ "build": ".", "cpu-millicores": 0 }), "nonzero"),
            (json!({ "build": ".", "ports": [] }), "unknown field `ports`"),
        ] {
            let message = format!("{:#}", container(config.clone(), dir.path()).unwrap_err());
            assert!(message.contains(error), "{config}: {message}");
        }
    }

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("1GiB"), Some(1 << 30));
        assert_eq!(parse_size("1 GB"), Some(1_000_000_000));
        assert_eq!(parse_size("42"), Some(42));
        assert_eq!(parse_size("1.5GiB"), None);
        assert_eq!(parse_size("GiB"), None);
        assert_eq!(parse_size("1gb"), None);
    }

    #[test]
    fn children_inherit_the_whole_container() {
        let config: SpacetimeConfig = serde_json::from_value(json!({
            "database": "a",
            "container": { "build": "./agent", "env-keys": ["K"] },
            "children": [
                { "database": "b" },
                { "database": "c", "container": { "image": format!("x@{}", digest()) } },
                { "database": "d", "container": null },
            ],
        }))
        .unwrap();
        let targets = config.collect_all_targets_with_inheritance();
        let container = |i: usize| targets[i].fields.get("container").cloned();
        assert_eq!(container(1), container(0));
        assert_eq!(container(2), Some(json!({ "image": format!("x@{}", digest()) })));
        assert!(Container::from_config(container(3).as_ref(), None).unwrap().is_none());
    }

    #[test]
    fn sets_the_container_only_when_it_changed() {
        let spec = ContainerSpec {
            image: format!("x@{}", digest()),
            command: None,
            env_keys: vec!["K".into()],
            resources: ContainerResources::default(),
            ports: Vec::new(),
            restart: RestartPolicy::OnFailure,
        };
        let info = |spec: &ContainerSpec, running, state| ContainerInfo {
            spec: spec.clone(),
            running,
            generation: 1,
            state: Some(state),
        };
        let running = |spec: &ContainerSpec| info(spec, true, ContainerState::Running);
        let failed = |spec: &ContainerSpec| info(spec, true, ContainerState::Failed("no value".into()));
        let stopped = |spec: &ContainerSpec| info(spec, false, ContainerState::Exited(0));
        let changes = |current: Option<ContainerInfo>| {
            let Changes { set, start } = changes(current.as_ref(), &spec);
            (set, start)
        };
        let mut other = spec.clone();
        other.env_keys.clear();

        assert_eq!(changes(None), (Some("attached"), None));
        assert_eq!(changes(Some(running(&spec))), (None, None));
        assert_eq!(
            changes(Some(stopped(&spec))),
            (None, Some("started, since it was stopped"))
        );
        assert_eq!(
            changes(Some(failed(&spec))),
            (None, Some("restarted, since it had failed to start"))
        );
        assert_eq!(changes(Some(running(&other))), (Some("updated"), None));
        // Setting a new spec restarts a running container, failed or not.
        assert_eq!(changes(Some(failed(&other))), (Some("updated"), None));
        assert_eq!(
            changes(Some(stopped(&other))),
            (Some("updated"), Some("started, since it was stopped"))
        );
        for change in [
            |s: &mut ContainerSpec| s.image = format!("y@{}", digest()),
            |s: &mut ContainerSpec| s.command = Some(vec!["sh".into()]),
            |s: &mut ContainerSpec| s.restart = RestartPolicy::Never,
            |s: &mut ContainerSpec| s.resources.memory_bytes += 1,
            |s: &mut ContainerSpec| s.resources.pids_max += 1,
        ] {
            let mut other = spec.clone();
            change(&mut other);
            assert_eq!(changes(Some(running(&other))), (Some("updated"), None));
        }
    }

    #[test]
    fn tags_pushed_images() {
        for (repository, name, tagged) in [
            ("ghcr.io/me/agent", "ghcr.io/me/agent", "ghcr.io/me/agent:latest"),
            ("ghcr.io/me/agent:v1", "ghcr.io/me/agent", "ghcr.io/me/agent:v1"),
            (
                "localhost:5000/agent",
                "localhost:5000/agent",
                "localhost:5000/agent:latest",
            ),
            (
                "localhost:5000/agent:v1",
                "localhost:5000/agent",
                "localhost:5000/agent:v1",
            ),
        ] {
            assert_eq!(name_and_tag(repository), (name, tagged.to_string()), "{repository}");
        }
    }

    #[test]
    fn recognizes_servers_on_this_machine() {
        for url in [
            "http://localhost:3000",
            "http://127.0.0.1:3000",
            "http://127.1.2.3",
            "http://[::1]:3000",
        ] {
            assert!(is_loopback(url), "{url}");
        }
        for url in [
            "https://maincloud.spacetimedb.com",
            "http://10.0.0.1:3000",
            "http://[::2]",
            "nonsense",
        ] {
            assert!(!is_loopback(url), "{url}");
        }
    }
}
