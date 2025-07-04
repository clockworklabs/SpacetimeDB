use crate::{
    db::datastore::locking_tx_datastore::MutTxId,
    host::{
        module_host::{DynModule, Module, ModuleInfo, ModuleInstance},
        Scheduler,
    },
    module_host_context::ModuleCreationContext,
    replica_context::ReplicaContext,
};
use anyhow::anyhow;
use std::sync::{Arc, Once};

use super::module_host::CallReducerParams;

pub struct V8Runtime {
    _priv: (),
}

impl V8Runtime {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        static V8_INIT: Once = Once::new();
        V8_INIT.call_once(|| {
            // TODO
        });

        Self { _priv: () }
    }

    pub fn make_actor(&self, _: ModuleCreationContext<'_>) -> anyhow::Result<impl Module> {
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
        _program: crate::db::datastore::traits::Program,
        _old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<super::UpdateDatabaseResult> {
        todo!()
    }

    fn call_reducer(&mut self, _tx: Option<MutTxId>, _params: CallReducerParams) -> super::ReducerCallResult {
        todo!()
    }
}
