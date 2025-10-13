//! Module backend infrastructure, shared between different runtimes,
//! like WASM and V8.

use crate::{
    energy::EnergyMonitor,
    host::{
        module_host::ModuleInfo,
        wasm_common::{module_host_actor::DescribeError, DESCRIBE_MODULE_DUNDER},
        Scheduler,
    },
    module_host_context::ModuleCreationContextLimited,
    replica_context::ReplicaContext,
};
use spacetimedb_lib::{Identity, RawModuleDef};
use spacetimedb_schema::{def::ModuleDef, error::ValidationErrors};
use std::sync::Arc;

/// Builds a [`ModuleCommon`] from a [`RawModuleDef`].
pub fn build_common_module_from_raw(
    mcc: ModuleCreationContextLimited,
    raw_def: RawModuleDef,
) -> Result<ModuleCommon, ValidationErrors> {
    // Perform a bunch of validation on the raw definition.
    let def: ModuleDef = raw_def.try_into()?;

    let replica_ctx = mcc.replica_ctx;
    let log_tx = replica_ctx.logger.tx.clone();

    // Note: assigns Reducer IDs based on the alphabetical order of reducer names.
    let info = ModuleInfo::new(
        def,
        replica_ctx.owner_identity,
        replica_ctx.database_identity,
        mcc.program_hash,
        log_tx,
        replica_ctx.subscriptions.clone(),
    );

    Ok(ModuleCommon::new(replica_ctx, mcc.scheduler, info, mcc.energy_monitor))
}

/// Non-runtime-specific parts of a module.
#[derive(Clone)]
pub(crate) struct ModuleCommon {
    replica_context: Arc<ReplicaContext>,
    scheduler: Scheduler,
    info: Arc<ModuleInfo>,
    energy_monitor: Arc<dyn EnergyMonitor>,
}

impl ModuleCommon {
    /// Returns a new common module.
    fn new(
        replica_context: Arc<ReplicaContext>,
        scheduler: Scheduler,
        info: Arc<ModuleInfo>,
        energy_monitor: Arc<dyn EnergyMonitor>,
    ) -> Self {
        Self {
            replica_context,
            scheduler,
            info,
            energy_monitor,
        }
    }

    /// Returns the module info.
    pub fn info(&self) -> Arc<ModuleInfo> {
        self.info.clone()
    }

    /// Returns the identity of the database.
    pub fn database_identity(&self) -> &Identity {
        &self.info.database_identity
    }

    /// Returns the energy monitor.
    pub fn energy_monitor(&self) -> Arc<dyn EnergyMonitor> {
        self.energy_monitor.clone()
    }
}

impl ModuleCommon {
    pub fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        &self.replica_context
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }
}

/// Runs the describer of modules in `run` and does some logging around it.
pub(crate) fn run_describer<T>(
    log_traceback: impl Copy + FnOnce(&str, &str, &anyhow::Error),
    run: impl FnOnce() -> anyhow::Result<T>,
) -> Result<T, DescribeError> {
    let describer_func_name = DESCRIBE_MODULE_DUNDER;

    let start = std::time::Instant::now();
    log::trace!("Start describer \"{describer_func_name}\"...");

    let result = run();

    let duration = start.elapsed();
    log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros());

    result
        .inspect_err(|err| log_traceback("describer", describer_func_name, err))
        .map_err(DescribeError::RuntimeError)
}
