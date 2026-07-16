use std::env;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;

use bytes::{Bytes, BytesMut};
use futures::{FutureExt as _, TryStreamExt as _};
use spacetimedb::config::CertificateAuthority;
use spacetimedb::host::ModuleHost;
use spacetimedb::messages::control_db::HostType;
use spacetimedb::util::jobs::JobCores;
use spacetimedb::Identity;
use spacetimedb_client_api::auth::SpacetimeAuth;
use spacetimedb_client_api::routes::subscribe::{generate_random_connection_id, WebSocketOptions};
use spacetimedb_lib::http as st_http;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_paths::{RootDir, SpacetimePaths};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use spacetimedb_schema::def::ModuleDef;
use tokio::runtime::{Builder, Runtime};

use spacetimedb::client::messages::SerializableMessage;
use spacetimedb::client::{
    ClientActorId, ClientConfig, ClientConnection, ClientConnectionReceiver, DataMessage, OutboundMessage,
};
use spacetimedb::db::{Config, Storage};
use spacetimedb::host::module_host::EventStatus;
use spacetimedb::host::FunctionArgs;
use spacetimedb::host::ReducerCallResult;
use spacetimedb_client_api::{ControlStateReadAccess, ControlStateWriteAccess, DatabaseDef, NodeDelegate};
use spacetimedb_client_api_messages::websocket::v1 as ws_v1;
use spacetimedb_lib::identity::RequestId;
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

pub struct ModuleHandle {
    // Keep the in-process standalone env alive for the lifetime of module.
    // The teardown helper will use it to drive the same shutdown path a standalone server would use.
    env: Arc<StandaloneEnv>,
    pub client: ClientConnection,
    receiver: ClientConnectionReceiver,
    pub db_identity: Identity,
}

impl ModuleHandle {
    async fn call_reducer_result(&self, reducer: &str, args: FunctionArgs) -> anyhow::Result<ReducerCallResult> {
        let result = self
            .client
            .call_reducer(reducer, args, 0, Instant::now(), ws_v1::CallReducerFlags::FullUpdate)
            .await;
        let result: anyhow::Result<ReducerCallResult> = match result {
            Ok(result) => Ok(result),
            Err(err) => Err(err.into()),
        };
        match result {
            Ok(result) if result.is_ok() => Ok(result),
            Ok(result) => {
                let err = Result::<(), anyhow::Error>::from(result)
                    .expect_err("non-committed reducer outcome should produce an error");
                Err(err.context(format!("Logs:\n{}", self.read_log(None).await)))
            }
            Err(err) => Err(err.context(format!("Logs:\n{}", self.read_log(None).await))),
        }
    }

    async fn call_reducer(&self, reducer: &str, args: FunctionArgs) -> anyhow::Result<()> {
        self.call_reducer_result(reducer, args).await.map(drop)
    }

    pub async fn call_reducer_json(&self, reducer: &str, args: &sats::ProductValue) -> anyhow::Result<()> {
        let args = serde_json::to_string(&args).unwrap();
        self.call_reducer(reducer, FunctionArgs::Json(args.into())).await
    }

    pub async fn call_reducer_binary(&self, reducer: &str, args: &sats::ProductValue) -> anyhow::Result<()> {
        let args = bsatn::to_vec(&args).unwrap();
        self.call_reducer(reducer, FunctionArgs::Bsatn(args.into())).await
    }

    pub async fn call_reducer_binary_result(
        &self,
        reducer: &str,
        args: &sats::ProductValue,
    ) -> anyhow::Result<ReducerCallResult> {
        let args = bsatn::to_vec(&args).unwrap();
        self.call_reducer_result(reducer, FunctionArgs::Bsatn(args.into()))
            .await
    }

    pub async fn send(&self, message: impl Into<DataMessage>) -> anyhow::Result<()> {
        let timer = Instant::now();
        self.client.handle_message(message, timer).await.map_err(Into::into)
    }

    pub async fn send_reducer_and_recv_update(
        &mut self,
        message: impl Into<DataMessage>,
        request_id: RequestId,
    ) -> anyhow::Result<()> {
        self.send(message).await?;
        self.recv_reducer_update(request_id).await
    }

    pub async fn recv_message(&mut self) -> Option<OutboundMessage> {
        let mut buf = Vec::with_capacity(1);
        (self.receiver.recv_many(&mut buf, 1).await != 0).then(|| buf.remove(0))
    }

    pub async fn recv_reducer_update(&mut self, request_id: RequestId) -> anyhow::Result<()> {
        let message = self
            .recv_message()
            .await
            .ok_or_else(|| anyhow::anyhow!("client receiver closed before reducer update {request_id}"))?;
        let OutboundMessage::V1(SerializableMessage::TxUpdate(update)) = message else {
            anyhow::bail!("expected reducer transaction update {request_id}, got {message:?}");
        };
        let Some(event) = update.event else {
            anyhow::bail!("expected full reducer transaction update {request_id}, got light update");
        };
        if event.request_id != Some(request_id) {
            anyhow::bail!(
                "expected reducer transaction update {request_id}, got request id {:?}",
                event.request_id
            );
        }
        match &event.status {
            EventStatus::Committed(_) => Ok(()),
            EventStatus::FailedUser(err) | EventStatus::FailedInternal(err) => {
                anyhow::bail!("reducer transaction update {request_id} failed: {err}")
            }
            EventStatus::OutOfEnergy => anyhow::bail!("reducer transaction update {request_id} ran out of energy"),
        }
    }

    pub async fn read_log(&self, size: Option<u32>) -> String {
        let bytes = self
            .client
            .module()
            .database_logger()
            .tail(size, false)
            .await
            .unwrap()
            .try_collect::<BytesMut>()
            .await
            .expect("failed to collect log stream");
        String::from_utf8(bytes.into()).unwrap()
    }

    async fn module_host(&self) -> ModuleHost {
        let database = self
            .env
            .get_database_by_identity(&self.db_identity)
            .await
            .unwrap()
            .unwrap();
        let host = self.env.leader(database.id).await.expect("host should be running");
        host.module().await.expect("module should be running")
    }

    /// Call a procedure by name with JSON-encoded args, returning the raw `AlgebraicValue` on success.
    pub async fn call_procedure_with_args(&self, procedure: &str, args_json: &str) -> anyhow::Result<AlgebraicValue> {
        let module = self.module_host().await;
        let ret = module
            .call_procedure(
                Identity::ZERO,
                None,
                None,
                procedure,
                FunctionArgs::Json(args_json.into()),
            )
            .await;
        ret.result
            .map(|r| r.return_val)
            .map_err(|e| anyhow::anyhow!("procedure {procedure} failed: {e:#}"))
    }

    /// Dispatch a GET request to a module HTTP route by path, returning the response body on success.
    pub async fn call_http_route_get(&self, path: &str) -> anyhow::Result<Bytes> {
        let module = self.module_host().await;
        let (handler_id, _, _) = module
            .info()
            .module_def
            .match_http_route(&st_http::Method::Get, path)
            .ok_or_else(|| anyhow::anyhow!("no GET route registered for {path}"))?;
        let request = st_http::Request {
            method: st_http::Method::Get,
            headers: std::iter::empty::<(Option<Box<str>>, Box<[u8]>)>().collect(),
            timeout: None,
            uri: format!("http://localhost{path}"),
            version: st_http::Version::Http11,
        };
        let (_response, body) = module
            .call_http_handler(handler_id, request, Bytes::new())
            .await
            .map_err(|e| anyhow::anyhow!("HTTP handler error: {e}"))?;
        Ok(body)
    }
}

pub struct CompiledModule {
    name: String,
    path: PathBuf,
    pub(super) host_type: HostType,
    program_bytes: OnceLock<Bytes>,
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
            None,
        )
        .expect("Module compilation failed");
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

    pub fn program_bytes(&self) -> Bytes {
        self.program_bytes
            .get_or_init(|| std::fs::read(&self.path).unwrap().into())
            .clone()
    }

    pub async fn extract_schema(&self) -> ModuleDef {
        // TODO: extract_schema should accept &[u8]
        let boxed_bytes: Box<[u8]> = self.program_bytes()[..].into();
        spacetimedb::host::extract_schema(boxed_bytes, self.host_type)
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
                let env = module.env.clone();
                let db_identity = module.db_identity;
                let routine_result = AssertUnwindSafe(routine(module)).catch_unwind().await.map(drop);
                finish_module_test(&env, db_identity, routine_result).await;
            });
        });
    }

    pub fn with_module<F>(&self, config: Config, func: F)
    where
        F: FnOnce(&Runtime, &ModuleHandle),
    {
        with_runtime(move |runtime| {
            let module = runtime.block_on(async { self.load_module(config, None).await });
            let env = module.env.clone();
            let db_identity = module.db_identity;
            let func_result = std::panic::catch_unwind(AssertUnwindSafe(|| func(runtime, &module)));
            drop(module);

            runtime.block_on(async { finish_module_test(&env, db_identity, func_result).await });
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
                durability: Default::default(),
                websocket: WebSocketOptions::default(),
                wasm: Default::default(),
                v8: Default::default(),
            },
            &certs,
            paths.data_dir.into(),
            JobCores::without_pinned_cores(),
        )
        .await
        .unwrap();
        // TODO: Fix this when we update identity generation.
        let identity = Identity::ZERO;
        let db_identity = SpacetimeAuth::alloc(&env).await.unwrap().claims.identity;
        let connection_id = generate_random_connection_id();

        env.publish_database(
            &identity,
            DatabaseDef {
                database_identity: db_identity,
                program_bytes: self.program_bytes(),
                num_replicas: None,
                host_type: self.host_type,
                parent: None,
                organization: None,
            },
            MigrationPolicy::Compatible,
        )
        .await
        .unwrap();

        let database = env.get_database_by_identity(&db_identity).await.unwrap().unwrap();
        let instance = env.get_leader_replica_by_database(database.id).await.unwrap();

        let client_id = ClientActorId {
            identity,
            connection_id,
            name: env.client_actor_index().next_client_name(),
        };

        let host = env.leader(database.id).await.expect("host should be running");
        let module_rx = host.module_watcher().await.unwrap();

        // TODO: it might be neat to add some functionality to module handle to make
        // it easier to interact with the database. For example it could include
        // the runtime on which a module was created and then we could add impl
        // for stuff like "get logs" or "get message log"
        let (client, receiver) =
            ClientConnection::dummy_with_receiver(client_id, ClientConfig::for_test(), instance.id, module_rx);

        ModuleHandle {
            env,
            client,
            receiver,
            db_identity,
        }
    }
}

/// These standalone module tests run a `StandaloneEnv` in-process.
/// That means [`CompiledModule::with_module_async`] and [`CompiledModule::with_module`]
/// own the Tokio runtime, host controller, module host, scheduler, `RelationalDB`,
/// JS worker threads, etc.
///
/// Some modules schedule repeating reducers during `init`.
/// If these helpers return without explicitly shutting the database down,
/// scheduled reducer calls can still be queued or running
/// while the Tokio runtime and V8 worker state are being torn down.
///
/// [`StandaloneEnv::delete_database`] is the public standalone shutdown path.
/// It removes the database and replica from standalone control state,
/// asks the host controller to exit the module host, closes and waits for the scheduler,
/// and then shuts down the `RelationalDB`.
async fn finish_module_test(env: &StandaloneEnv, db_identity: Identity, test_result: std::thread::Result<()>) {
    let cleanup_result = env.delete_database(&Identity::ZERO, &db_identity).await;

    // Cleanup should not hide the result of the test body.
    // If the test already panicked, resume that panic after attempting shutdown.
    // If the test passed, make cleanup failure a test failure.
    match (test_result, cleanup_result) {
        (Ok(()), Ok(())) => {}
        (Ok(()), Err(err)) => panic!("failed to delete test database {db_identity}: {err:#}"),
        (Err(panic), Ok(())) => std::panic::resume_unwind(panic),
        (Err(panic), Err(err)) => {
            log::error!("failed to delete test database {db_identity} after test panic: {err:#}");
            std::panic::resume_unwind(panic);
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

pub struct Cpp;

impl ModuleLanguage for Cpp {
    const NAME: &'static str = "cpp";

    fn get_module() -> &'static CompiledModule {
        lazy_static::lazy_static! {
            pub static ref MODULE: CompiledModule = CompiledModule::compile("benchmarks-cpp", COMPILATION_MODE);
        }

        &MODULE
    }
}
