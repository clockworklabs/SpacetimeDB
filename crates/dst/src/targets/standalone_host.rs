//! Standalone host DST target (single scenario, no migration/subscriptions).

use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use spacetimedb_client_api::{
    auth::SpacetimeAuth,
    routes::subscribe::{generate_random_connection_id, WebSocketOptions},
    ControlStateReadAccess, ControlStateWriteAccess, NodeDelegate,
};
use spacetimedb_client_api_messages::websocket::v1 as ws_v1;
use spacetimedb_core::{
    client::{ClientActorId, ClientConfig, ClientConnection},
    config::CertificateAuthority,
    db::{Config as DbConfig, Storage},
    host::FunctionArgs,
    messages::control_db::HostType,
    util::jobs::JobCores,
};
use spacetimedb_lib::Identity;
use spacetimedb_paths::{RootDir, SpacetimePaths};
use spacetimedb_sats::ProductValue;
use spacetimedb_schema::{auto_migrate::MigrationPolicy, def::FunctionVisibility};
use spacetimedb_standalone::{StandaloneEnv, StandaloneOptions};
use tracing::trace;

use crate::{
    config::RunConfig,
    core::NextInteractionSource,
    seed::DstSeed,
    workload::module_ops::{
        HostScenarioId, ModuleInteraction, ModuleReducerSpec, ModuleWorkloadOutcome, NextInteractionGenerator,
    },
};

pub type StandaloneHostOutcome = ModuleWorkloadOutcome;

pub fn prepare_generated_run() -> anyhow::Result<()> {
    let _ = compiled_module()?;
    Ok(())
}

pub async fn run_generated_with_config_and_scenario(
    seed: DstSeed,
    scenario: HostScenarioId,
    config: RunConfig,
) -> anyhow::Result<StandaloneHostOutcome> {
    let (outcome, _) = run_once_async(seed, scenario, config).await?;
    Ok(outcome)
}

async fn run_once_async(
    seed: DstSeed,
    scenario: HostScenarioId,
    config: RunConfig,
) -> anyhow::Result<(StandaloneHostOutcome, Vec<ModuleInteraction>)> {
    let module = compiled_module()?;
    let reducers = extract_reducer_specs(module.clone()).await?;
    let mut generator = NextInteractionGenerator::new(
        seed,
        scenario,
        reducers.clone(),
        config.max_interactions_or_default(usize::MAX),
    );
    let mut engine = StandaloneHostEngine::new(seed, module).await?;
    let deadline = config.deadline();
    let mut trace_log = Vec::new();

    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            generator.request_finish();
        }
        let Some(interaction) = generator.next_interaction() else {
            break;
        };
        trace!(?interaction, "standalone_host interaction");
        engine
            .execute(&interaction)
            .await
            .map_err(|e| anyhow::anyhow!("interaction failed: {e}"))?;
        trace_log.push(interaction);
    }

    // Replay contract: same seed/scenario/config must produce same interaction sequence.
    let mut replay =
        NextInteractionGenerator::new(seed, scenario, reducers, config.max_interactions_or_default(usize::MAX));
    let replayed = (0..trace_log.len())
        .filter_map(|_| replay.next_interaction())
        .collect::<Vec<_>>();
    if replayed != trace_log {
        anyhow::bail!("interaction sequence replay mismatch");
    }

    Ok((engine.finish(), trace_log))
}

#[derive(Clone)]
struct CompiledModuleInfo {
    program_bytes: Bytes,
    host_type: HostType,
}

fn compiled_module() -> anyhow::Result<Arc<CompiledModuleInfo>> {
    static CACHE: OnceLock<Arc<CompiledModuleInfo>> = OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return Ok(cached.clone());
    }
    let module_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../modules/module-test");
    let (path, host_type) = spacetimedb_cli::build(&module_root, Some(PathBuf::from("src")).as_deref(), true, None)?;
    let host_type: HostType = host_type.parse()?;
    let program_bytes = std::fs::read(path)?;
    let compiled = Arc::new(CompiledModuleInfo {
        program_bytes: program_bytes.into(),
        host_type,
    });
    let _ = CACHE.set(compiled.clone());
    Ok(CACHE.get().expect("cache set or raced").clone())
}

async fn extract_reducer_specs(module: Arc<CompiledModuleInfo>) -> anyhow::Result<Vec<ModuleReducerSpec>> {
    let module_def = spacetimedb_core::host::extract_schema(
        module.program_bytes.clone().to_vec().into_boxed_slice(),
        module.host_type,
    )
    .await?;
    Ok(module_def
        .reducers()
        .filter(|reducer| reducer.visibility == FunctionVisibility::ClientCallable)
        .map(|reducer| ModuleReducerSpec {
            name: reducer.name.to_string(),
            params: reducer
                .params
                .elements
                .iter()
                .map(|arg| arg.algebraic_type.clone())
                .collect::<Vec<_>>(),
        })
        .collect::<Vec<_>>())
}

struct HostSession {
    _env: Arc<StandaloneEnv>,
    client: ClientConnection,
    db_identity: Identity,
}

struct StandaloneHostEngine {
    root_dir: RootDir,
    session: Option<HostSession>,
    module: Arc<CompiledModuleInfo>,
    step: usize,
    reducer_calls: usize,
    scheduler_waits: usize,
    reopens: usize,
    noops: usize,
    expected_errors: usize,
}

impl StandaloneHostEngine {
    async fn new(seed: DstSeed, module: Arc<CompiledModuleInfo>) -> anyhow::Result<Self> {
        let root_dir = RootDir(std::env::temp_dir().join(format!(
            "spacetimedb-dst-standalone-host-{}-{}-{}",
            seed.0,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        )));
        let _ = std::fs::remove_dir_all(&root_dir);
        let session = open_session(&root_dir, &module, None)
            .await
            .map_err(anyhow::Error::msg)?;
        Ok(Self {
            root_dir,
            session: Some(session),
            module,
            step: 0,
            reducer_calls: 0,
            scheduler_waits: 0,
            reopens: 0,
            noops: 0,
            expected_errors: 0,
        })
    }

    async fn execute(&mut self, interaction: &ModuleInteraction) -> Result<(), String> {
        self.step = self.step.saturating_add(1);
        match interaction {
            ModuleInteraction::CallReducer { reducer, args } => {
                self.reducer_calls = self.reducer_calls.saturating_add(1);
                let request_id = (self.step as u32).saturating_sub(1);
                let product = ProductValue::from_iter(args.iter().cloned());
                let payload = spacetimedb_sats::bsatn::to_vec(&product).map_err(|e| e.to_string())?;
                let res = self
                    .session
                    .as_mut()
                    .ok_or_else(|| "host session missing".to_string())?
                    .client
                    .call_reducer(
                        reducer,
                        FunctionArgs::Bsatn(payload.into()),
                        request_id,
                        Instant::now(),
                        ws_v1::CallReducerFlags::FullUpdate,
                    )
                    .await;
                match res {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        let msg = err.to_string();
                        if is_expected_error(reducer, &msg) {
                            self.expected_errors = self.expected_errors.saturating_add(1);
                            Ok(())
                        } else {
                            Err(format!("unexpected reducer error reducer={reducer}: {msg}"))
                        }
                    }
                }
            }
            ModuleInteraction::WaitScheduled { millis } => {
                self.scheduler_waits = self.scheduler_waits.saturating_add(1);
                tokio::time::sleep(std::time::Duration::from_millis(*millis)).await;
                Ok(())
            }
            ModuleInteraction::CloseReopen => {
                self.reopens = self.reopens.saturating_add(1);
                let db_identity = self
                    .session
                    .as_ref()
                    .ok_or_else(|| "host session missing".to_string())?
                    .db_identity;
                let old = self.session.take();
                drop(old);
                self.session = Some(open_session(&self.root_dir, &self.module, Some(db_identity)).await?);
                Ok(())
            }
            ModuleInteraction::NoOp => {
                self.noops = self.noops.saturating_add(1);
                Ok(())
            }
        }
    }

    fn finish(self) -> StandaloneHostOutcome {
        StandaloneHostOutcome {
            steps_executed: self.step,
            reducer_calls: self.reducer_calls,
            scheduler_waits: self.scheduler_waits,
            reopens: self.reopens,
            noops: self.noops,
            expected_errors: self.expected_errors,
        }
    }
}

fn is_expected_error(_reducer: &str, msg: &str) -> bool {
    msg.contains("permission denied")
}

async fn open_session(
    root_dir: &RootDir,
    module: &CompiledModuleInfo,
    maybe_db_identity: Option<Identity>,
) -> Result<HostSession, String> {
    let paths = SpacetimePaths::from_root_dir(root_dir);
    let certs = CertificateAuthority::in_cli_config_dir(&paths.cli_config_dir);
    let env = StandaloneEnv::init(
        StandaloneOptions {
            db_config: DbConfig {
                storage: Storage::Disk,
                page_pool_max_size: None,
            },
            websocket: WebSocketOptions::default(),
            v8_heap_policy: Default::default(),
        },
        &certs,
        paths.data_dir.into(),
        JobCores::without_pinned_cores(),
    )
    .await
    .map_err(|e| format!("standalone init failed: {e:#}"))?;

    let caller_identity = Identity::ZERO;
    let db_identity = match maybe_db_identity {
        Some(identity) => identity,
        None => {
            SpacetimeAuth::alloc(&env)
                .await
                .map_err(|e| format!("db identity allocation failed: {e:#?}"))?
                .claims
                .identity
        }
    };

    if env
        .get_database_by_identity(&db_identity)
        .await
        .map_err(|e| format!("database lookup failed: {e:#}"))?
        .is_none()
    {
        env.publish_database(
            &caller_identity,
            spacetimedb_client_api::DatabaseDef {
                database_identity: db_identity,
                program_bytes: module.program_bytes.clone(),
                num_replicas: None,
                host_type: module.host_type,
                parent: None,
                organization: None,
            },
            MigrationPolicy::Compatible,
        )
        .await
        .map_err(|e| format!("publish module failed: {e:#}"))?;
    }

    let database = env
        .get_database_by_identity(&db_identity)
        .await
        .map_err(|e| format!("database lookup after publish failed: {e:#}"))?
        .ok_or_else(|| "database not found after publish".to_string())?;
    let replica = env
        .get_leader_replica_by_database(database.id)
        .await
        .ok_or_else(|| "leader replica not found".to_string())?;
    let host = env
        .leader(database.id)
        .await
        .map_err(|e| format!("leader host unavailable: {e:#}"))?;
    let module_rx = host
        .module_watcher()
        .await
        .map_err(|e| format!("module watcher failed: {e:#}"))?;
    let client_id = ClientActorId {
        identity: caller_identity,
        connection_id: generate_random_connection_id(),
        name: env.client_actor_index().next_client_name(),
    };
    let client = ClientConnection::dummy(client_id, ClientConfig::for_test(), replica.id, module_rx);
    Ok(HostSession {
        _env: env,
        client,
        db_identity,
    })
}
