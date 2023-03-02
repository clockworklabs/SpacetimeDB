use std::fs::File;
use std::future::Future;
use std::io::Read;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use spacetimedb::address::Address;
use spacetimedb::client::ClientActorId;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::db::message_log::MessageLog;
use spacetimedb::hash::hash_bytes;

use prost::Message as OtherMessage;
use spacetimedb::protobuf::client_api::FunctionCall;
use spacetimedb::protobuf::client_api::{message, Message};
use spacetimedb::{control_db::CONTROL_DB, protobuf::control_db::HostType};
use spacetimedb_client_api::ControllerCtx;
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

    runtime.shutdown_background();
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
fn module_path(path: &str) -> PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.join("../modules").join(path)
}

fn read_module(path: &str) -> Vec<u8> {
    let wasm_path = module_path(path).join("target/wasm32-unknown-unknown/release/spacetime_module.wasm");
    let path = Path::new("modules").join(path).join(wasm_path);
    let mut f = File::open(path.clone()).unwrap();
    let metadata = std::fs::metadata(path).unwrap();
    let mut buffer = vec![0; metadata.len() as usize];
    f.read_exact(&mut buffer).unwrap();
    buffer
}

pub fn compile(path: &str) {
    let path = module_path(path);
    let output = Command::new("cargo")
        .current_dir(path.clone())
        .args(["build", "--target=wasm32-unknown-unknown", "--release"])
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
    pub client_id: ClientActorId,
    pub instance_id: u64,
    pub message_log: Arc<Mutex<MessageLog>>,
    pub db_address: Address,
}

impl ModuleHandle {
    // TODO(drogus): args here is a string, but it might be nice to use a more specific type here,
    // sth along the lines of Vec<serde_json::Value>
    pub async fn call_reducer(&self, reducer: &str, args: String) -> anyhow::Result<()> {
        let message = Message {
            r#type: Some(message::Type::FunctionCall(FunctionCall {
                reducer: reducer.into(),
                arg_bytes: args.into(),
            })),
        };

        let mut buf = Vec::new();
        message.encode(&mut buf).unwrap();
        spacetimedb::client::message_handlers::handle_binary(self.client_id, self.instance_id, buf).await
    }

    // TODO(drogus): not sure if it's the best name, maybe it should be about calling
    // a reducer?
    pub async fn send(&self, json: String) -> anyhow::Result<()> {
        spacetimedb::client::message_handlers::handle_text(self.client_id, self.instance_id, json).await?;

        Ok(())
    }

    pub async fn read_log(&self, size: Option<u32>) -> String {
        let filepath = DatabaseLogger::filepath(&self.db_address, self.instance_id);
        DatabaseLogger::read_latest(&filepath, size).await
    }
}

pub async fn load_module(name: &str) -> ModuleHandle {
    let env: &dyn ControllerCtx = &spacetimedb_standalone::StandaloneEnv::init().unwrap();
    let identity = CONTROL_DB.alloc_spacetime_identity().await.unwrap();
    let address = CONTROL_DB.alloc_spacetime_address().await.unwrap();
    let program_bytes = read_module(name);

    let program_bytes_addr = hash_bytes(&program_bytes);
    env.object_db().insert_object(program_bytes).unwrap();

    let host_type = HostType::Wasmer;

    spacetimedb_client_api::Controller::insert_database(
        env,
        &address,
        &identity,
        &program_bytes_addr,
        host_type,
        1,
        true,
        false,
    )
    .await
    .unwrap();

    let database = CONTROL_DB.get_database_by_address(&address).await.unwrap().unwrap();
    let instance = CONTROL_DB
        .get_leader_database_instance_by_database(database.id)
        .await
        .unwrap();

    let client_id = ClientActorId { identity, name: 0 };

    let dicc = env.database_instance_context_controller();
    let message_log = dicc.get(instance.id).unwrap().message_log;

    // TODO: it might be neat to add some functionality to module handle to make
    // it easier to interact with the database. For example it could include
    // the runtime on which a module was created and then we could add impl
    // for stuff like "get logs" or "get message log"
    ModuleHandle {
        instance_id: instance.id,
        client_id,
        message_log,
        db_address: address,
    }
}
