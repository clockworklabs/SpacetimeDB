//! Runs the containers assigned to this node through Docker Engine.
//!
//! The supervisor periodically compares the `container` assignments in the control database
//! with the Docker containers labelled with its supervisor ID. It claims containers whose leader
//! replica is local, starts assigned generations that are missing, restarts exited ones according
//! to their restart policy with backoff, and removes every container whose generation is no longer
//! assigned here. Containers survive a node restart and are adopted if their generation is still
//! assigned. The supervisor ID is persisted in the state directory, so that separate clusters
//! sharing one Docker Engine never remove each other's containers.
//!
//! Each running generation receives a short-lived token for its database, signed with this
//! cluster's key. The token is a file in a directory bind-mounted at `/run/spacetimedb`,
//! rewritten well before it expires. Hosts accept it only while its generation is current.
//!
//! The server provides the assignments and records what the supervisor reports through
//! [`ContainerControl`]. It may isolate containers on the network with a [`NetworkIsolation`]
//! hook; without one, containers use Docker's default bridge network.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{bail, Context as _};
use async_trait::async_trait;
use bollard::{
    models::{ContainerCreateBody, HostConfig},
    query_parameters::{
        CreateContainerOptions, CreateImageOptions, ListContainersOptions, ListNetworksOptions, RemoveContainerOptions,
        StopContainerOptions,
    },
    Docker,
};
use futures::TryStreamExt as _;
use spacetimedb::auth::identity::ContainerClaim;
use spacetimedb::Identity;
use spacetimedb_client_api::auth::{JwtAuthProvider as _, TokenClaims};
use spacetimedb_client_api::NodeDelegate;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_lib::container::{is_local_image_id, ContainerSpec, ContainerState, RestartPolicy};
use tracing::{info, warn};

const LABEL_SUPERVISOR: &str = "spacetimedb.supervisor";
const LABEL_NODE: &str = "spacetimedb.node_id";
const LABEL_DATABASE: &str = "spacetimedb.database_id";
const LABEL_GENERATION: &str = "spacetimedb.generation";

const TICK: Duration = Duration::from_secs(2);
/// How long to wait before treating an exit as final: Docker's default shutdown timeout.
const FINAL_EXIT_SETTLE: Duration = Duration::from_secs(15);
/// As Kubernetes' default `terminationGracePeriodSeconds`.
const STOP_GRACE_SECS: i32 = 30;
// Restart backoff, as in Kubernetes: doubling from one second to five minutes, and reset once a
// container has run for ten minutes.
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);
const BACKOFF_RESET: Duration = Duration::from_secs(10 * 60);
const TOKEN_TTL: Duration = Duration::from_secs(10 * 60);
const TOKEN_REFRESH: Duration = Duration::from_secs(5 * 60);
/// Where the credential directory is mounted in the container.
const CREDENTIALS_MOUNT: &str = "/run/spacetimedb";

/// Operator configuration for container hosting on this node.
#[derive(Clone, Debug)]
pub struct ContainerOptions {
    /// The Docker Engine, e.g. `unix:///run/docker.sock`. `None` uses `DOCKER_HOST`, or else the
    /// platform's default socket.
    pub docker_host: Option<String>,
    /// The OCI runtime Docker uses, e.g. `runc` or `kata`. `None` uses the daemon default.
    pub runtime: Option<String>,
    /// Limit the writable layer with `--storage-opt size`. Requires overlay2 on XFS with
    /// project quotas; otherwise container creation fails. Disabling it lets one container fill
    /// the node's disk, so it is only for development.
    pub scratch_quota: bool,
    /// The total scratch space this node offers containers. `None` does not limit it.
    pub scratch_capacity: Option<u64>,
    /// The URL containers use to reach this cluster's API.
    pub api_url: String,
    /// Holds a credential directory for each running generation.
    pub state_dir: PathBuf,
    /// Include the first 8 hex digits of the supervisor ID in container names, so that several
    /// servers can share one Docker Engine.
    pub name_with_supervisor_id: bool,
}

/// The control state the supervisor works from.
#[async_trait]
pub trait ContainerControl: NodeDelegate {
    /// This node's ID, which labels the containers it creates.
    async fn node_id(&self) -> anyhow::Result<u64>;

    /// The databases whose container node `node_id` should claim, and the generations assigned
    /// to it.
    async fn assignments(&self, node_id: u64) -> anyhow::Result<Assignments>;

    /// Assign the container of a database whose leader replica is local to this node, under a
    /// new generation.
    async fn claim(&self, database_id: u64) -> anyhow::Result<()>;

    /// Record the state of a generation. Reports for a generation that is no longer assigned
    /// must be ignored.
    async fn report(&self, database_id: u64, generation: u64, state: ContainerState) -> anyhow::Result<()>;

    /// Record the CPU and memory Docker has, and the configured scratch space, for placement.
    async fn report_capacity(
        &self,
        _cpu_millicores: u32,
        _memory_bytes: u64,
        _scratch_bytes: Option<u64>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Isolates containers on the network: each instance runs on its own network, which the hook
/// creates, behind host rules that the hook installs.
#[async_trait]
pub trait NetworkIsolation: Send + Sync {
    /// Install the host rules, replacing any earlier ones.
    fn install(&self) -> anyhow::Result<()>;

    /// Whether the host rules are in place. Checked before starting containers, since a
    /// firewall reload could remove them.
    fn installed(&self) -> bool;

    /// DNS servers for containers.
    fn dns(&self) -> Vec<String>;

    /// Create the network for an instance named `name`.
    async fn create(&self, docker: &Docker, name: &str, labels: HashMap<String, String>) -> anyhow::Result<()>;
}

/// What [`ContainerControl::assignments`] returns.
pub struct Assignments {
    /// Databases whose container this node should claim.
    pub claims: Vec<u64>,
    /// The generation assigned to this node for each database, by database ID.
    pub assigned: HashMap<u64, Assigned>,
}

/// An assigned generation this node should run.
pub struct Assigned {
    pub database_identity: Identity,
    pub generation: u64,
    pub spec: ContainerSpec,
    /// `None` while leadership is unknown, e.g. during a reconnection: a running container is
    /// kept, but none is started.
    pub leader_replica: Option<u64>,
    /// This generation already exited in a way its restart policy does not restart.
    pub finished: bool,
}

/// Whether a generation whose last reported state is `state` exited in a way `restart` does not
/// restart. Such an exit is final, also across a node restart.
pub fn is_finished(restart: RestartPolicy, state: &ContainerState) -> bool {
    match state {
        ContainerState::Exited(code) => !restart.restarts(*code, false),
        ContainerState::OutOfMemory => !restart.restarts(-1, true),
        _ => false,
    }
}

/// A Docker container created by this node.
struct Observed {
    id: String,
    database_id: u64,
    generation: u64,
    running: bool,
    exit_code: Option<i64>,
    oom_killed: bool,
    /// The container stopped before the host or Docker daemon last started, so the
    /// platform stopped it rather than the program exiting on its own.
    stopped_by_platform: bool,
    finished_at: Option<std::time::SystemTime>,
}

#[derive(Default)]
struct Attempts {
    failures: u32,
    retry_at: Option<Instant>,
}

/// Supervise the containers assigned to `node`, isolating them with `network` if given.
/// Without `network`, containers can reach private addresses and the host's services, which the
/// caller should point out.
pub fn spawn<N: ContainerControl + 'static>(
    node: Arc<N>,
    options: ContainerOptions,
    network: Option<Box<dyn NetworkIsolation>>,
) -> anyhow::Result<()> {
    let docker = connect(options.docker_host.as_deref())?;
    let docker_host = docker_host_name(options.docker_host.as_deref());
    let id = supervisor_id(&options.state_dir)?;
    // The state directory holds every running generation's token.
    #[cfg(unix)]
    std::fs::set_permissions(&options.state_dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .with_context(|| format!("unable to restrict {}", options.state_dir.display()))?;
    if let Some(network) = &network {
        network
            .install()
            .context("unable to isolate containers on the network")?;
    }
    info!(docker_host, runtime = ?options.runtime, supervisor = %id, "container hosting enabled");
    tokio::spawn(async move {
        // Use the daemon's API version when it is older than the client's.
        let docker = loop {
            match docker.clone().negotiate_version().await {
                Ok(docker) => break docker,
                Err(e) => warn!("unable to reach Docker: {e:#}"),
            }
            tokio::time::sleep(TICK).await;
        };
        let mut supervisor = Supervisor {
            node,
            docker,
            options,
            id,
            reported: HashMap::new(),
            attempts: HashMap::new(),
            issued: HashMap::new(),
            started: HashMap::new(),
            finished: HashSet::new(),
            reported_capacity: None,
            network,
        };
        loop {
            if let Err(e) = supervisor.reconcile().await {
                warn!("container reconciliation failed: {e:#}");
            }
            tokio::time::sleep(TICK).await;
        }
    });
    Ok(())
}

/// Check that the Docker Engine answers within `timeout`. See [`ContainerOptions::docker_host`].
pub async fn ping(docker_host: Option<&str>, timeout: Duration) -> anyhow::Result<()> {
    let docker = connect(docker_host)?;
    tokio::time::timeout(timeout, docker.ping())
        .await
        .map_err(|_| anyhow::anyhow!("timed out after {timeout:?}"))
        .and_then(|answer| Ok(answer?))
        .with_context(|| format!("Docker did not answer at {}", docker_host_name(docker_host)))?;
    Ok(())
}

fn connect(docker_host: Option<&str>) -> anyhow::Result<Docker> {
    let docker = match docker_host {
        Some(host) => Docker::connect_with_host(host),
        None => Docker::connect_with_defaults(),
    };
    docker.with_context(|| format!("unable to connect to Docker at {}", docker_host_name(docker_host)))
}

fn docker_host_name(docker_host: Option<&str>) -> &str {
    docker_host.unwrap_or("DOCKER_HOST or the default socket")
}

/// Read the selected keys from a module's database environment.
fn environment(module: &spacetimedb::host::ModuleHost, keys: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    if keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut stored = module
        .relational_db()
        .with_read_only(Workload::Internal, |tx| {
            spacetimedb::db::environment::snapshot(tx).map_err(anyhow::Error::from)
        })
        .context("unable to read the database environment")?;
    let mut env = BTreeMap::new();
    for key in keys {
        let Some(value) = stored.remove(key) else {
            bail!("environment key `{key}` has no value");
        };
        env.insert(key.clone(), value);
    }
    Ok(env)
}

/// Read this node's supervisor ID from the state directory, creating it on first use.
fn supervisor_id(state_dir: &std::path::Path) -> anyhow::Result<String> {
    let path = state_dir.join("supervisor-id");
    match std::fs::read_to_string(&path) {
        Ok(id) => return Ok(id.trim().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("unable to read {}", path.display())),
    }
    std::fs::create_dir_all(state_dir).with_context(|| format!("unable to create {}", state_dir.display()))?;
    let mut bytes = [0u8; 16];
    openssl::rand::rand_bytes(&mut bytes)?;
    let id = format!("{:032x}", u128::from_le_bytes(bytes));
    std::fs::write(&path, &id).with_context(|| format!("unable to write {}", path.display()))?;
    Ok(id)
}

struct Supervisor<N> {
    node: Arc<N>,
    docker: Docker,
    options: ContainerOptions,
    /// Labels every container this supervisor creates.
    id: String,
    /// The last state reported per database, to report changes only.
    reported: HashMap<u64, (u64, ContainerState)>,
    /// Start attempts per assigned generation, for backoff.
    attempts: HashMap<(u64, u64), Attempts>,
    /// When each assigned generation's token was last written.
    issued: HashMap<(u64, u64), Instant>,
    /// When each assigned generation was last started or adopted.
    started: HashMap<(u64, u64), Instant>,
    /// Assigned generations that exited and are not restarted.
    finished: HashSet<(u64, u64)>,
    /// The capacity last reported to the control database.
    reported_capacity: Option<(u32, u64, Option<u64>)>,
    /// `None` when containers are not isolated on the network.
    network: Option<Box<dyn NetworkIsolation>>,
}

impl<N: ContainerControl> Supervisor<N> {
    async fn reconcile(&mut self) -> anyhow::Result<()> {
        let node_id = self.node.node_id().await?;
        self.report_capacity().await;
        let Assignments { claims, assigned } = self.node.assignments(node_id).await?;

        for database_id in claims {
            if let Err(e) = self.node.claim(database_id).await {
                warn!(database_id, "unable to claim container: {e:#}");
            }
        }

        let mut running = BTreeMap::new();
        for observed in self.observe(node_id).await? {
            let wanted = assigned
                .get(&observed.database_id)
                .is_some_and(|a| a.generation == observed.generation);
            if !wanted {
                info!(
                    database_id = observed.database_id,
                    generation = observed.generation,
                    "removing unassigned container"
                );
                self.remove(&observed.id).await?;
            } else if observed.running {
                running.insert(observed.database_id, ());
                self.started
                    .entry((observed.database_id, observed.generation))
                    .or_insert_with(Instant::now);
                self.report(observed.database_id, observed.generation, ContainerState::Running)
                    .await;
            } else {
                let key = (observed.database_id, observed.generation);
                let restart = assigned[&observed.database_id].spec.restart;
                let code = observed.exit_code.unwrap_or(-1);
                // As with Kubernetes' graceful node shutdown, an exit the program did not cause
                // counts as a failure, whatever its status.
                let killed = observed.oom_killed || observed.stopped_by_platform;
                let restarts = restart.restarts(code, killed);
                let state = if observed.oom_killed {
                    ContainerState::OutOfMemory
                } else if observed.stopped_by_platform && restarts {
                    // Not `Exited`, which the next read of assignments could take as final.
                    ContainerState::Failed(format!("stopped by the platform (exit code {code})"))
                } else {
                    ContainerState::Exited(code)
                };
                // Docker stops containers before its daemon exits and a new one starts, so an exit
                // that looks final may still turn out to be a daemon restart. Decide only once the
                // exit is older than Docker's shutdown timeout, keeping the container meanwhile.
                let settling = observed
                    .finished_at
                    .and_then(|at| at.elapsed().ok())
                    .is_some_and(|age| age < FINAL_EXIT_SETTLE);
                if !restarts && settling {
                    running.insert(observed.database_id, ());
                    continue;
                }
                self.report(observed.database_id, observed.generation, state).await;
                self.remove(&observed.id).await?;
                if restarts {
                    self.backoff(observed.database_id, observed.generation);
                } else {
                    info!(database_id = key.0, generation = key.1, code, "container finished");
                    self.finished.insert(key);
                }
            }
        }

        let is_assigned = |(db, generation): &(u64, u64)| assigned.get(db).is_some_and(|a| a.generation == *generation);
        self.attempts.retain(|key, _| is_assigned(key));
        self.issued.retain(|key, _| is_assigned(key));
        self.started.retain(|key, _| is_assigned(key));
        self.finished.retain(is_assigned);
        self.remove_stale_credentials(&assigned)?;
        for (&database_id, assigned) in &assigned {
            let key = (database_id, assigned.generation);
            if self.issued.get(&key).is_none_or(|at| at.elapsed() >= TOKEN_REFRESH) {
                self.write_token(database_id, assigned)?;
            }
        }

        // With a network hook, containers start only while it isolates them, e.g. after a
        // firewall reload.
        let isolated = self.network.as_ref().is_none_or(|network| {
            network.installed() || {
                let installed = network.install();
                if let Err(e) = &installed {
                    warn!("not starting containers: {e:#}");
                }
                installed.is_ok()
            }
        });
        for (&database_id, assigned) in &assigned {
            let key = (database_id, assigned.generation);
            if !isolated {
                break;
            }
            if running.contains_key(&database_id)
                || assigned.finished
                || assigned.leader_replica.is_none()
                || self.finished.contains(&key)
            {
                continue;
            }
            if self
                .attempts
                .get(&key)
                .and_then(|a| a.retry_at)
                .is_some_and(|at| at > Instant::now())
            {
                continue;
            }
            self.report(database_id, assigned.generation, ContainerState::Starting)
                .await;
            let started = self.start(node_id, database_id, assigned).await;
            if started.is_ok() {
                self.started.insert(key, Instant::now());
            }
            if let Err(e) = started {
                warn!(
                    database_id,
                    generation = assigned.generation,
                    "unable to start container: {e:#}"
                );
                self.report(
                    database_id,
                    assigned.generation,
                    ContainerState::Failed(format!("{e:#}")),
                )
                .await;
                self.backoff(database_id, assigned.generation);
            }
        }
        self.remove_orphan_networks().await
    }

    async fn observe(&self, node_id: u64) -> anyhow::Result<Vec<Observed>> {
        let filters = HashMap::from([(
            "label".to_string(),
            vec![
                format!("{LABEL_SUPERVISOR}={}", self.id),
                format!("{LABEL_NODE}={node_id}"),
            ],
        )]);
        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: Some(filters),
                ..Default::default()
            }))
            .await
            .context("unable to list containers")?;
        let mut observed = Vec::new();
        for summary in containers {
            let (Some(id), Some(labels)) = (summary.id, summary.labels) else {
                continue;
            };
            let label = |key: &str| labels.get(key).and_then(|v| v.parse::<u64>().ok());
            let (Some(database_id), Some(generation)) = (label(LABEL_DATABASE), label(LABEL_GENERATION)) else {
                // Not ours to interpret, but labelled with our node: remove it.
                self.remove(&id).await?;
                continue;
            };
            let inspect = self
                .docker
                .inspect_container(&id, None)
                .await
                .with_context(|| format!("unable to inspect container {id}"))?;
            let state = inspect.state.unwrap_or_default();
            observed.push(Observed {
                id,
                database_id,
                generation,
                running: state.running.unwrap_or(false),
                exit_code: state.exit_code,
                oom_killed: state.oom_killed.unwrap_or(false),
                stopped_by_platform: !state.running.unwrap_or(false)
                    && state.finished_at.as_deref().is_some_and(stopped_before_platform_start),
                finished_at: state
                    .finished_at
                    .as_deref()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(std::time::SystemTime::from),
            });
        }
        Ok(observed)
    }

    async fn start(&self, node_id: u64, database_id: u64, assigned: &Assigned) -> anyhow::Result<()> {
        let spec = &assigned.spec;
        let module = self.leader_module(database_id).await?;
        let mut env = environment(&module, &spec.env_keys)?;
        env.insert("SPACETIMEDB_URI".into(), self.options.api_url.clone());
        env.insert(
            "SPACETIMEDB_DATABASE_IDENTITY".into(),
            assigned.database_identity.to_hex().to_string(),
        );
        env.insert("SPACETIMEDB_TOKEN_FILE".into(), format!("{CREDENTIALS_MOUNT}/token"));
        self.ensure_image(&spec.image).await?;
        self.check_image(&spec.image).await?;
        let credentials = self.credentials_dir(database_id, assigned.generation);

        let labels = HashMap::from([
            (LABEL_SUPERVISOR.to_string(), self.id.clone()),
            (LABEL_NODE.to_string(), node_id.to_string()),
            (LABEL_DATABASE.to_string(), database_id.to_string()),
            (LABEL_GENERATION.to_string(), assigned.generation.to_string()),
        ]);
        let memory = i64::try_from(spec.resources.memory_bytes).context("memory limit too large")?;
        let name = self.container_name(database_id, assigned.generation);
        if let Some(network) = &self.network {
            // A network left from an instance that crashed before cleanup is replaced.
            self.remove_network(&name).await?;
            network.create(&self.docker, &name, labels.clone()).await?;
        }
        let host_config = HostConfig {
            nano_cpus: Some(i64::from(spec.resources.cpu_millicores) * 1_000_000),
            memory: Some(memory),
            // Equal to `memory`: no swap.
            memory_swap: Some(memory),
            pids_limit: Some(i64::from(spec.resources.pids_max)),
            storage_opt: self
                .options
                .scratch_quota
                .then(|| HashMap::from([("size".to_string(), spec.resources.scratch_bytes.to_string())])),
            runtime: self.options.runtime.clone(),
            binds: Some(vec![format!("{}:{CREDENTIALS_MOUNT}:ro", credentials.display())]),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            // With a network hook, each instance has its own network; otherwise, Docker's default.
            network_mode: self.network.is_some().then(|| name.clone()),
            dns: self.network.as_ref().map(|network| network.dns()),
            init: Some(true),
            privileged: Some(false),
            security_opt: Some(vec!["no-new-privileges".to_string()]),
            ..Default::default()
        };
        let config = ContainerCreateBody {
            image: Some(spec.image.clone()),
            entrypoint: spec.command.as_ref().map(|_| Vec::new()),
            cmd: spec.command.clone(),
            env: Some(env.into_iter().map(|(k, v)| format!("{k}={v}")).collect()),
            labels: Some(labels),
            host_config: Some(host_config),
            ..Default::default()
        };
        let created = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(name.clone()),
                    ..Default::default()
                }),
                config,
            )
            .await
            .with_context(|| format!("unable to create container {name}"));
        let created = match created {
            Ok(created) => created,
            Err(e) => {
                self.remove_network(&name).await?;
                return Err(e);
            }
        };
        if let Err(e) = self.docker.start_container(&created.id, None).await {
            self.remove(&created.id).await?;
            return Err(e).with_context(|| format!("unable to start container {name}"));
        }
        info!(database_id, generation = assigned.generation, "started container");
        Ok(())
    }

    /// The module of the local leader replica, launching the replica if the node has not yet
    /// done so, e.g. after a restart.
    async fn leader_module(&self, database_id: u64) -> anyhow::Result<spacetimedb::host::ModuleHost> {
        self.node
            .leader(database_id)
            .await
            .map_err(|e| anyhow::anyhow!("the database's leader replica is unavailable: {e}"))?
            .module()
            .await
            .context("the database's leader replica is not running")
    }

    async fn ensure_image(&self, image: &str) -> anyhow::Result<()> {
        let inspected = self.docker.inspect_image(image).await;
        if inspected.is_ok() {
            return Ok(());
        }
        if is_local_image_id(image) {
            return inspected.map(drop).with_context(|| {
                format!("image {image} is not in this server's Docker daemon, and local image IDs are never pulled")
            });
        }
        info!(image, "pulling image");
        self.docker
            .create_image(
                Some(CreateImageOptions {
                    from_image: Some(image.to_string()),
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await
            .with_context(|| format!("unable to pull image {image}"))?;
        Ok(())
    }

    /// Reject images that declare `VOLUME` paths: Docker backs each with an anonymous volume
    /// that the writable-layer quota does not limit.
    async fn check_image(&self, image: &str) -> anyhow::Result<()> {
        let inspect = self
            .docker
            .inspect_image(image)
            .await
            .with_context(|| format!("unable to inspect image {image}"))?;
        let volumes = inspect.config.and_then(|config| config.volumes).unwrap_or_default();
        if !volumes.is_empty() {
            bail!(
                "image declares VOLUME paths ({}), which containers do not support; \
                 remove the VOLUME instructions from the image",
                volumes.join(", ")
            );
        }
        Ok(())
    }

    async fn remove(&self, id: &str) -> anyhow::Result<()> {
        let name = self
            .docker
            .inspect_container(id, None)
            .await
            .ok()
            .and_then(|c| c.name)
            .map(|name| name.trim_start_matches('/').to_string());
        // Give the main process a chance to exit before `remove` kills it.
        let _ = self
            .docker
            .stop_container(
                id,
                Some(StopContainerOptions {
                    t: Some(STOP_GRACE_SECS),
                    ..Default::default()
                }),
            )
            .await;
        match self
            .docker
            .remove_container(
                id,
                Some(RemoveContainerOptions {
                    force: true,
                    v: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(()) | Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {}
            Err(e) => return Err(e).with_context(|| format!("unable to remove container {id}")),
        }
        if let Some(name) = name {
            self.remove_network(&name).await?;
        }
        Ok(())
    }

    /// The name of a generation's container, and of its network.
    fn container_name(&self, database_id: u64, generation: u64) -> String {
        if self.options.name_with_supervisor_id {
            let id = self.id.get(..8).unwrap_or(&self.id);
            format!("stdb-{id}-{database_id}-{generation}")
        } else {
            format!("stdb-{database_id}-{generation}")
        }
    }

    /// Remove the network named `name`, if it exists and no container uses it.
    async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
        if self.network.is_none() {
            return Ok(());
        }
        match self.docker.remove_network(name).await {
            Ok(()) | Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => Ok(()),
            Err(e) => Err(e).with_context(|| format!("unable to remove network {name}")),
        }
    }

    /// Remove networks of this node whose container no longer exists.
    async fn remove_orphan_networks(&self) -> anyhow::Result<()> {
        if self.network.is_none() {
            return Ok(());
        }
        let label = format!("{LABEL_SUPERVISOR}={}", self.id);
        let containers: HashSet<String> = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: Some(HashMap::from([("label".to_string(), vec![label.clone()])])),
                ..Default::default()
            }))
            .await
            .context("unable to list containers")?
            .into_iter()
            .flat_map(|c| c.names.unwrap_or_default())
            .map(|name| name.trim_start_matches('/').to_string())
            .collect();
        let networks = self
            .docker
            .list_networks(Some(ListNetworksOptions {
                filters: Some(HashMap::from([("label".to_string(), vec![label])])),
            }))
            .await
            .context("unable to list networks")?;
        for network in networks.into_iter().filter_map(|network| network.name) {
            if !containers.contains(&network) {
                self.remove_network(&network).await?;
            }
        }
        Ok(())
    }

    fn credentials_dir(&self, database_id: u64, generation: u64) -> PathBuf {
        self.options.state_dir.join(format!("{database_id}-{generation}"))
    }

    /// Atomically replace the token of an assigned generation.
    fn write_token(&mut self, database_id: u64, assigned: &Assigned) -> anyhow::Result<()> {
        let claim = ContainerClaim {
            database: assigned.database_identity,
            generation: assigned.generation,
        };
        let provider = self.node.jwt_auth_provider();
        let claims = TokenClaims {
            issuer: provider.local_issuer().into(),
            subject: format!("container:{}:{}", claim.database.to_hex(), claim.generation).into(),
            audience: [claim.audience().into()].into(),
            extra: None,
        };
        let (_, token) = claims.encode_and_sign_with_expiry(provider, Some(TOKEN_TTL))?;

        let dir = self.credentials_dir(database_id, assigned.generation);
        std::fs::create_dir_all(&dir).with_context(|| format!("unable to create {}", dir.display()))?;
        let tmp = dir.join(".token");
        std::fs::write(&tmp, token)?;
        // Readable by the container's user, whichever it is. Other users of the host cannot reach
        // it, since the state directory is private.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))?;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
        }
        std::fs::rename(&tmp, dir.join("token"))?;
        self.issued.insert((database_id, assigned.generation), Instant::now());
        Ok(())
    }

    /// Delete credential directories of generations no longer assigned here.
    fn remove_stale_credentials(&self, assigned: &HashMap<u64, Assigned>) -> anyhow::Result<()> {
        let entries = match std::fs::read_dir(&self.options.state_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).context("unable to read the container state directory"),
        };
        for entry in entries {
            let entry = entry?;
            // Other files, such as the supervisor ID, are not credential directories.
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let current = name
                .to_str()
                .and_then(|name| name.split_once('-'))
                .and_then(|(db, generation)| Some((db.parse::<u64>().ok()?, generation.parse::<u64>().ok()?)))
                .is_some_and(|(db, generation)| assigned.get(&db).is_some_and(|a| a.generation == generation));
            if !current {
                std::fs::remove_dir_all(entry.path())
                    .with_context(|| format!("unable to remove {}", entry.path().display()))?;
            }
        }
        Ok(())
    }

    /// Report the CPU and memory Docker has, and the configured scratch space, so that the
    /// control database places containers where they fit.
    async fn report_capacity(&mut self) {
        let result = async {
            let info = self.docker.info().await.context("unable to read Docker's system info")?;
            let cpus = info.ncpu.context("Docker did not report its CPU count")?;
            let memory = info.mem_total.context("Docker did not report its memory")?;
            let capacity = (
                u32::try_from(cpus)?.saturating_mul(1000),
                u64::try_from(memory)?,
                self.options.scratch_capacity,
            );
            if self.reported_capacity == Some(capacity) {
                return Ok(());
            }
            self.node.report_capacity(capacity.0, capacity.1, capacity.2).await?;
            info!(cpu_millicores = capacity.0, memory_bytes = capacity.1, scratch_bytes = ?capacity.2, "reported container capacity");
            self.reported_capacity = Some(capacity);
            anyhow::Ok(())
        }
        .await;
        if let Err(e) = result {
            warn!("unable to report container capacity: {e:#}");
        }
    }

    fn backoff(&mut self, database_id: u64, generation: u64) {
        let key = (database_id, generation);
        let attempts = self.attempts.entry(key).or_default();
        // A container that ran long enough starts its backoff over.
        if self.started.get(&key).is_some_and(|at| at.elapsed() >= BACKOFF_RESET) {
            attempts.failures = 0;
        }
        attempts.failures += 1;
        let delay = Duration::from_secs(1u64 << (attempts.failures - 1).min(9)).min(MAX_BACKOFF);
        attempts.retry_at = Some(Instant::now() + delay);
    }

    async fn report(&mut self, database_id: u64, generation: u64, state: ContainerState) {
        if self.reported.get(&database_id) == Some(&(generation, state.clone())) {
            return;
        }
        let result = self.node.report(database_id, generation, state.clone()).await;
        match result {
            Ok(()) => {
                self.reported.insert(database_id, (generation, state));
            }
            Err(e) => warn!(database_id, generation, "unable to report container state: {e:#}"),
        }
    }
}

/// Whether a container that finished at `finished_at` (RFC 3339) stopped before the host booted
/// or the Docker daemon started. Both are best effort, and unknown on non-Linux development hosts.
fn stopped_before_platform_start(finished_at: &str) -> bool {
    let Ok(finished) = chrono::DateTime::parse_from_rfc3339(finished_at) else {
        return false;
    };
    let finished = std::time::SystemTime::from(finished);
    let host_boot = std::fs::read_to_string("/proc/stat").ok().and_then(|stat| {
        let secs = stat
            .lines()
            .find_map(|line| line.strip_prefix("btime "))?
            .trim()
            .parse()
            .ok()?;
        Some(std::time::UNIX_EPOCH + Duration::from_secs(secs))
    });
    // The daemon writes its PID file when it starts.
    let daemon_start = std::fs::metadata("/var/run/docker.pid").and_then(|m| m.modified()).ok();
    [host_boot, daemon_start]
        .into_iter()
        .flatten()
        .any(|start| finished < start)
}
