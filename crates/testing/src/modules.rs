use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;

use spacetimedb::messages::control_db::HostType;
use spacetimedb::Identity;
use spacetimedb_client_api::routes::subscribe::generate_random_address;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use tokio::runtime::{Builder, Runtime};

use spacetimedb::client::{ClientActorId, ClientConnection, DataMessage, Protocol};
use spacetimedb::config::{FilesLocal, SpacetimeDbFiles};
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::db::{Config, Storage};
use spacetimedb::messages::websocket as ws;
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
    pub async fn call_reducer_json(&self, reducer: &str, args: sats::ProductValue) -> anyhow::Result<()> {
        let args = serde_json::to_string(&args).unwrap();
        let message = ws::ClientMessage::CallReducer(ws::CallReducer {
            reducer: reducer.into(),
            args,
            request_id: 0,
        });
        self.send(serde_json::to_string(&SerializeWrapper::new(message)).unwrap())
            .await
    }

    pub async fn call_reducer_binary(&self, reducer: &str, args: sats::ProductValue) -> anyhow::Result<()> {
        let message = ws::ClientMessage::CallReducer(ws::CallReducer {
            reducer: reducer.into(),
            args: bsatn::to_vec(&args).unwrap(),
            request_id: 0,
        });
        self.send(bsatn::to_vec(&message).unwrap()).await
    }

    pub async fn send(&self, message: impl Into<DataMessage>) -> anyhow::Result<()> {
        let timer = Instant::now();
        self.client.handle_message(message, timer).await.map_err(Into::into)
    }

    pub async fn read_log(&self, size: Option<u32>) -> String {
        let filepath = DatabaseLogger::filepath(&self.db_identity, self.client.replica_id);
        DatabaseLogger::read_latest(&filepath, size).await
    }
}

pub struct CompiledModule {
    name: String,
    path: PathBuf,
    program_bytes: OnceLock<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CompilationMode {
    Debug,
    Release,
}

impl CompiledModule {
    pub fn compile(name: &str, mode: CompilationMode) -> Self {
        let path = spacetimedb_cli::build(&module_path(name), false, mode == CompilationMode::Debug).unwrap();
        Self {
            name: name.to_owned(),
            path,
            program_bytes: OnceLock::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
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
    pub async fn load_module(&self, config: Config, reuse_db_path: Option<&Path>) -> ModuleHandle {
        let paths = match reuse_db_path {
            Some(path) => FilesLocal::hidden(path),
            None => {
                let paths = FilesLocal::temp(&self.name);

                // The database created in the `temp` folder can't be randomized,
                // so it persists after running the test.
                std::fs::remove_dir(paths.db_path()).ok();
                paths
            }
        };

        crate::set_key_env_vars(&paths);
        let env = spacetimedb_standalone::StandaloneEnv::init(config).await.unwrap();
        // TODO: Fix this when we update identity generation.
        let identity = Identity::ZERO;
        let db_identity = Identity::placeholder();
        let client_address = generate_random_address();

        let program_bytes = self
            .program_bytes
            .get_or_init(|| std::fs::read(&self.path).unwrap())
            .clone();

        env.publish_database(
            &identity,
            DatabaseDef {
                database_identity: db_identity,
                program_bytes,
                num_replicas: 1,
                host_type: HostType::Wasm,
            },
        )
        .await
        .unwrap();

        let database = env.get_database_by_identity(&db_identity).unwrap().unwrap();
        let instance = env.get_leader_replica_by_database(database.id).unwrap();

        let client_id = ClientActorId {
            identity,
            address: client_address,
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
            client: ClientConnection::dummy(client_id, Protocol::Text, instance.id, module_rx),
            db_identity,
        }
    }
}

/// For testing, persist to disk by default, as many tests
/// exercise functionality like restarting the database.
pub static DEFAULT_CONFIG: Config = Config { storage: Storage::Disk };

/// For performance tests, do not persist to disk.
pub static IN_MEMORY_CONFIG: Config = Config { storage: Storage::Disk };

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
