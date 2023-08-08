use std::future::Future;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use spacetimedb::address::Address;
use spacetimedb::client::{ClientActorId, ClientConnection, Protocol};
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::db::Storage;
use spacetimedb::hash::hash_bytes;

use spacetimedb::messages::control_db::HostType;
use spacetimedb_client_api::{ControlCtx, ControlStateDelegate, WorkerCtx};
use spacetimedb_standalone::StandaloneEnv;
use tokio::runtime::{Builder, Runtime};

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

pub fn with_module_async<O, R, F>(name: &str, routine: R)
where
    R: FnOnce(ModuleHandle) -> F,
    F: Future<Output = O>,
{
    with_runtime(move |runtime| {
        runtime.block_on(async {
            let module = load_module(name).await;

            routine(module).await;
        });
    });
}

pub fn with_module<F>(name: &str, func: F)
where
    F: FnOnce(&Runtime, &ModuleHandle),
{
    with_runtime(move |runtime| {
        let module = runtime.block_on(async { load_module(name).await });

        func(runtime, &module);
    });
}

fn module_path(name: &str) -> PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.join("../../modules").join(name)
}

fn wasm_path(name: &str) -> PathBuf {
    module_path(name).join(format!(
        "target/wasm32-unknown-unknown/release/{}_module.wasm",
        name.replace('-', "_")
    ))
}

fn read_module(path: &str) -> Vec<u8> {
    println!("{}", wasm_path(path).to_str().unwrap());
    std::fs::read(wasm_path(path)).unwrap()
}

pub fn compile(path: &str) {
    let path = module_path(path);
    let output = Command::new("cargo")
        .current_dir(&path)
        .args([
            "build",
            "--target=wasm32-unknown-unknown",
            "--release",
            "--target-dir",
            path.join("target").to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute process to compile a test depdendency");

    if !output.status.success() {
        eprintln!(
            "There was a problem with compiling a test depenedency: {}",
            path.display()
        );
        io::stdout().write_all(&output.stdout).unwrap();
        io::stderr().write_all(&output.stderr).unwrap();

        panic!("Couldn't compile the module");
    }
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

pub async fn load_module(name: &str) -> ModuleHandle {
    // For testing, persist to disk by default, as many tests
    // exercise functionality like restarting the database.
    let storage = Storage::Disk;

    crate::set_key_env_vars();
    let env = spacetimedb_standalone::StandaloneEnv::init(storage).await.unwrap();
    let identity = env.control_db().alloc_spacetime_identity().await.unwrap();
    let address = env.control_db().alloc_spacetime_address().await.unwrap();
    let program_bytes = read_module(name);

    let program_bytes_addr = hash_bytes(&program_bytes);
    env.object_db().insert_object(program_bytes).unwrap();

    let host_type = HostType::Wasmer;

    env.insert_database(&address, &identity, &program_bytes_addr, host_type, 1, true)
        .await
        .unwrap();

    let database = env.get_database_by_address(&address).await.unwrap().unwrap();
    let instance = env.get_leader_database_instance_by_database(database.id).await.unwrap();

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
