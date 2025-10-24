use std::env;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;

use spacetimedb::config::CertificateAuthority;
use spacetimedb::messages::control_db::HostType;
use spacetimedb::util::jobs::JobCores;
use spacetimedb::Identity;
use spacetimedb_client_api::auth::SpacetimeAuth;
use spacetimedb_client_api::routes::subscribe::{generate_random_connection_id, WebSocketOptions};
use spacetimedb_paths::{RootDir, SpacetimePaths};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use spacetimedb_schema::def::ModuleDef;
use tokio::runtime::{Builder, Runtime};

use spacetimedb::client::{ClientActorId, ClientConfig, ClientConnection, DataMessage};
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::db::{Config, Storage};
use spacetimedb::host::FunctionArgs;
use spacetimedb::messages::websocket::CallReducerFlags;
use spacetimedb_client_api::{ControlStateReadAccess, ControlStateWriteAccess, DatabaseDef, NodeDelegate};
use spacetimedb_lib::{bsatn, sats};

pub use spacetimedb::database_logger::LogLevel;

use spacetimedb_standalone::StandaloneEnv;

pub fn start_runtime() -> Runtime {
    Builder::new_multi_thread().enable_all().build().unwrap()
}

fn with_runtime<F>(func: F)
where
    F: FnOnce(&Runtime),
{
    let runtime = start_runtime();

    func(&runtime);
}

pub(crate) fn module_path(name: &str) -> PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.join("../../modules").join(name)
}

#[derive(Clone)]
pub struct ModuleHandle {
    // Needs to hold a reference to the standalone env.
    _env: Arc<StandaloneEnv>,
    pub client: ClientConnection,
    pub db_identity: Identity,
}

impl ModuleHandle {
    async fn call_reducer(&self, reducer: &str, args: FunctionArgs) -> anyhow::Result<()> {
        let result = self
            .client
            .call_reducer(reducer, args, 0, Instant::now(), CallReducerFlags::FullUpdate)
            .await;
        let result = match result {
            Ok(result) => result.into(),
            Err(err) => Err(err.into()),
        };
        match result {
            Ok(()) => Ok(()),
            Err(err) => Err(err.context(format!("Logs:\n{}", self.read_log(None).await))),
        }
    }

    pub async fn call_reducer_json(&self, reducer: &str, args: &sats::ProductValue) -> anyhow::Result<()> {
        let args = serde_json::to_string(&args).unwrap();
        self.call_reducer(reducer, FunctionArgs::Json(args.into())).await
    }

    pub async fn call_reducer_binary(&self, reducer: &str, args: &sats::ProductValue) -> anyhow::Result<()> {
        let args = bsatn::to_vec(&args).unwrap();
        self.call_reducer(reducer, FunctionArgs::Bsatn(args.into())).await
    }

    pub async fn send(&self, message: impl Into<DataMessage>) -> anyhow::Result<()> {
        let timer = Instant::now();
        self.client.handle_message(message, timer).await.map_err(Into::into)
    }

    pub async fn read_log(&self, size: Option<u32>) -> String {
        let logs_dir = self._env.data_dir().replica(self.client.replica_id).module_logs();
        DatabaseLogger::read_latest(logs_dir, size).await
    }
}

pub struct CompiledModule {
    name: String,
    path: PathBuf,
    pub(super) host_type: HostType,
    program_bytes: OnceLock<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CompilationMode {
    Debug,
    Release,
}

impl CompiledModule {
    pub fn compile(name: &str, mode: CompilationMode) -> Self {
        let (path, host_type) = spacetimedb_cli::build(
            &module_path(name),
            Some(PathBuf::from("src")).as_deref(),
            mode == CompilationMode::Debug,
        )
        .unwrap();
        Self {
            name: name.to_owned(),
            path,
            host_type: host_type.parse().unwrap(),
            program_bytes: OnceLock::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn program_bytes(&self) -> &[u8] {
        self.program_bytes.get_or_init(|| std::fs::read(&self.path).unwrap())
    }

    pub async fn extract_schema(&self) -> ModuleDef {
        spacetimedb::host::extract_schema(self.program_bytes().into(), self.host_type)
            .await
            .unwrap()
    }

    pub fn extract_schema_blocking(&self) -> ModuleDef {
        start_runtime().block_on(self.extract_schema())
    }

    pub fn with_module_async<O, R, F>(&self, config: Config, routine: R)
    where
        R: FnOnce(ModuleHandle) -> F,
        F: Future<Output = O>,
    {
        with_runtime(move |runtime| {
            runtime.block_on(async {
                let module = self.load_module(config, None).await;

                routine(module).await;
            });
        });
    }

    pub fn with_module<F>(&self, config: Config, func: F)
    where
        F: FnOnce(&Runtime, &ModuleHandle),
    {
        with_runtime(move |runtime| {
            let module = runtime.block_on(async { self.load_module(config, None).await });

            func(runtime, &module);
        });
    }

    /// Load a module with the given config.
    /// If "reuse_db_path" is set, the module will be loaded in the given path,
    /// without resetting the database.
    /// This is used to speed up benchmarks running under callgrind (it allows them to reuse native-compiled wasm modules).
    pub async fn load_module(&self, config: Config, reuse_db_path: Option<&RootDir>) -> ModuleHandle {
        let paths = match reuse_db_path {
            Some(path) => SpacetimePaths::from_root_dir(path),
            None => {
                let root_dir = RootDir(env::temp_dir().join("stdb").join(&self.name));

                // The database created in the `temp` folder can't be randomized,
                // so it persists after running the test.
                std::fs::remove_dir(&root_dir).ok();

                SpacetimePaths::from_root_dir(&root_dir)
            }
        };

        let certs = CertificateAuthority::in_cli_config_dir(&paths.cli_config_dir);
        let env = spacetimedb_standalone::StandaloneEnv::init(
            spacetimedb_standalone::StandaloneOptions {
                db_config: config,
                websocket: WebSocketOptions::default(),
            },
            &certs,
            paths.data_dir.into(),
            JobCores::without_pinned_cores(tokio::runtime::Handle::current()),
        )
        .await
        .unwrap();
        // TODO: Fix this when we update identity generation.
        let identity = Identity::ZERO;
        let db_identity = SpacetimeAuth::alloc(&env).await.unwrap().claims.identity;
        let connection_id = generate_random_connection_id();

        let program_bytes = self.program_bytes().to_owned();

        env.publish_database(
            &identity,
            DatabaseDef {
                database_identity: db_identity,
                program_bytes,
                num_replicas: None,
                host_type: self.host_type,
            },
            MigrationPolicy::Compatible,
        )
        .await
        .unwrap();

        let database = env.get_database_by_identity(&db_identity).unwrap().unwrap();
        let instance = env.get_leader_replica_by_database(database.id).unwrap();

        let client_id = ClientActorId {
            identity,
            connection_id,
            name: env.client_actor_index().next_client_name(),
        };

        let host = env
            .leader(database.id)
            .await
            .expect("host should be running")
            .expect("host should be running");
        let module_rx = host.module_watcher().await.unwrap();

        // TODO: it might be neat to add some functionality to module handle to make
        // it easier to interact with the database. For example it could include
        // the runtime on which a module was created and then we could add impl
        // for stuff like "get logs" or "get message log"
        ModuleHandle {
            _env: env,
            client: ClientConnection::dummy(client_id, ClientConfig::for_test(), instance.id, module_rx),
            db_identity,
        }
    }
}

/// For testing, persist to disk by default, as many tests
/// exercise functionality like restarting the database.
pub static DEFAULT_CONFIG: Config = Config {
    storage: Storage::Disk,
    page_pool_max_size: None,
};

/// For performance tests, do not persist to disk.
pub static IN_MEMORY_CONFIG: Config = Config {
    storage: Storage::Disk,
    // For some reason, a large page pool capacity causes `test_index_scans` to slow down,
    // and makes the perf test for `chunk` go over 1ms.
    // The threshold for failure on i7-7700K, 64GB RAM seems to be at 1 << 26.
    // TODO(centril): investigate further why this size affects the benchmark.
    page_pool_max_size: Some(1 << 16),
};

/// Used to parse output from module logs.
///
/// Sync with: `core::database_logger::Record`. We can't use it
/// directly because the types are wrong for deserialization.
/// (Rust!)
#[derive(serde::Deserialize)]
pub struct LoggerRecord {
    pub level: LogLevel,
    pub target: Option<String>,
    pub filename: Option<String>,
    pub line_number: Option<u32>,
    pub message: String,
}

const COMPILATION_MODE: CompilationMode = if cfg!(debug_assertions) {
    CompilationMode::Debug
} else {
    CompilationMode::Release
};

pub trait ModuleLanguage {
    const NAME: &'static str;

    fn get_module() -> &'static CompiledModule;
}

pub struct Csharp;

impl ModuleLanguage for Csharp {
    const NAME: &'static str = "csharp";

    fn get_module() -> &'static CompiledModule {
        lazy_static::lazy_static! {
            pub static ref MODULE: CompiledModule = CompiledModule::compile("benchmarks-cs", COMPILATION_MODE);
        }

        &MODULE
    }
}

pub struct Rust;

impl ModuleLanguage for Rust {
    const NAME: &'static str = "rust";

    fn get_module() -> &'static CompiledModule {
        lazy_static::lazy_static! {
            pub static ref MODULE: CompiledModule = CompiledModule::compile("benchmarks", COMPILATION_MODE);
        }

        &MODULE
    }
}

pub struct TypeScript;

impl ModuleLanguage for TypeScript {
    const NAME: &'static str = "typescript";

    fn get_module() -> &'static CompiledModule {
        lazy_static::lazy_static! {
            pub static ref MODULE: CompiledModule = CompiledModule::compile("benchmarks-ts", COMPILATION_MODE);
        }

        &MODULE
    }
}
