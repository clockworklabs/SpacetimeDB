use std::cell::OnceCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use spacetimedb::address::Address;
use spacetimedb::client::{ClientActorId, ClientConnection, Protocol};
use spacetimedb::config::{FilesLocal, SpacetimeDbFiles};
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::db::{Config, FsyncPolicy, Storage};
use spacetimedb_client_api::{ControlStateReadAccess, ControlStateWriteAccess, DatabaseDef, NodeDelegate};
use spacetimedb_standalone::StandaloneEnv;

fn start_runtime() -> Runtime {
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
    pub db_address: Address,
}

impl ModuleHandle {
    // TODO(drogus): args here is a string, but it might be nice to use a more specific type here,
    // sth along the lines of Vec<serde_json::Value>
    pub async fn call_reducer(&self, reducer: &str, args: String) -> anyhow::Result<()> {
        let args = format!(r#"{{"call": {{"fn": "{reducer}", "args": {args}}}}}"#);
        self.send(args).await
    }

    // TODO(drogus): not sure if it's the best name, maybe it should be about calling
    // a reducer?
    pub async fn send(&self, json: String) -> anyhow::Result<()> {
        self.client.handle_message(json).await.map_err(Into::into)
    }

    pub async fn read_log(&self, size: Option<u32>) -> String {
        let filepath = DatabaseLogger::filepath(&self.db_address, self.client.database_instance_id);
        DatabaseLogger::read_latest(&filepath, size).await
    }
}

pub struct CompiledModule {
    name: String,
    path: PathBuf,
    program_bytes: OnceCell<Vec<u8>>,
}

impl CompiledModule {
    pub fn compile(name: &str) -> Self {
        let path = spacetimedb_cli::build(&module_path(name), false, true).unwrap();
        Self {
            name: name.to_owned(),
            path,
            program_bytes: OnceCell::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn with_module_async<O, R, F>(&self, routine: R)
    where
        R: FnOnce(ModuleHandle) -> F,
        F: Future<Output = O>,
    {
        with_runtime(move |runtime| {
            runtime.block_on(async {
                let module = self.load_module().await;
                routine(module).await;
            });
        });
    }

    pub fn with_module<F>(&self, func: F)
    where
        F: FnOnce(&Runtime, &ModuleHandle),
    {
        with_runtime(move |runtime| {
            let module = runtime.block_on(async { self.load_module().await });

            func(runtime, &module);
        });
    }

    async fn load_module(&self) -> ModuleHandle {
        // For testing, persist to disk by default, as many tests
        // exercise functionality like restarting the database.
        let storage = Storage::Disk;
        let fsync = FsyncPolicy::Never;
        let config = Config { storage, fsync };

        let paths = FilesLocal::temp(&self.name);
        // The database created in the `temp` folder can't be randomized,
        // so it persists after running the test.
        std::fs::remove_dir(paths.db_path()).ok();

        crate::set_key_env_vars(&paths);
        let env = spacetimedb_standalone::StandaloneEnv::init(config).await.unwrap();
        let identity = env.create_identity().await.unwrap();
        let address = env.create_address().await.unwrap();

        let program_bytes = self
            .program_bytes
            .get_or_init(|| std::fs::read(&self.path).unwrap())
            .clone();

        env.publish_database(
            &identity,
            DatabaseDef {
                address,
                program_bytes,
                num_replicas: 1
            }
        ).await
        .unwrap();

        let database = env.get_database_by_address(&address).unwrap().unwrap();
        let instance = env.get_leader_database_instance_by_database(database.id).unwrap();

        let client_id = ClientActorId {
            identity,
            name: env.client_actor_index().next_client_name(),
        };

        let module = env.host_controller().get_module_host(instance.id).unwrap();

        // TODO: it might be neat to add some functionality to module handle to make
        // it easier to interact with the database. For example it could include
        // the runtime on which a module was created and then we could add impl
        // for stuff like "get logs" or "get message log"
        ModuleHandle {
            _env: env,
            client: ClientConnection::dummy(client_id, Protocol::Text, instance.id, module),
            db_address: address,
        }
    }
}
