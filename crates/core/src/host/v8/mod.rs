use crate::{
    host::{
        module_host::{DynModule, Module, ModuleInfo, ModuleInstance, ModuleRuntime},
        Scheduler,
    },
    module_host_context::ModuleCreationContext,
    replica_context::ReplicaContext,
};
use anyhow::anyhow;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use std::sync::{Arc, LazyLock};

use super::module_host::CallReducerParams;

/// The V8 runtime, for modules written in e.g., JS or TypeScript.
#[derive(Default)]
pub struct V8Runtime {
    _priv: (),
}

impl ModuleRuntime for V8Runtime {
    fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<impl Module> {
        V8_RUNTIME_GLOBAL.make_actor(mcc)
    }
}

static V8_RUNTIME_GLOBAL: LazyLock<V8RuntimeInner> = LazyLock::new(V8RuntimeInner::init);

/// The actual V8 runtime, with initialization of V8.
struct V8RuntimeInner {
    _priv: (),
}

impl V8RuntimeInner {
    fn init() -> Self {
        // Our current configuration:
        // - will pick a number of worker threads for background jobs based on the num CPUs.
        // - does not allow idle tasks
        let platform = v8::new_default_platform(0, false).make_shared();
        // Initialize V8. Internally, this uses a global lock so it's safe that we don't.
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();

        Self { _priv: () }
    }

    fn make_actor(&self, _: ModuleCreationContext<'_>) -> anyhow::Result<impl Module> {
        Err::<JsModule, _>(anyhow!("v8_todo"))
    }
}

#[derive(Clone)]
struct JsModule;

impl DynModule for JsModule {
    fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        todo!()
    }

    fn scheduler(&self) -> &Scheduler {
        todo!()
    }
}

impl Module for JsModule {
    type Instance = JsInstance;

    type InitialInstances<'a> = std::iter::Empty<JsInstance>;

    fn initial_instances(&mut self) -> Self::InitialInstances<'_> {
        std::iter::empty()
    }

    fn info(&self) -> Arc<ModuleInfo> {
        todo!()
    }

    fn create_instance(&self) -> Self::Instance {
        todo!()
    }
}

struct JsInstance;

impl ModuleInstance for JsInstance {
    fn trapped(&self) -> bool {
        todo!()
    }

    fn update_database(
        &mut self,
        _program: spacetimedb_datastore::traits::Program,
        _old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<super::UpdateDatabaseResult> {
        todo!()
    }

    fn call_reducer(&mut self, _tx: Option<MutTxId>, _params: CallReducerParams) -> super::ReducerCallResult {
        todo!()
    }
}
