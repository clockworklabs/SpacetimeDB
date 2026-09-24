//! Computing the instances of materialized views.
//!
//! Views are computed in several contexts:
//! when they are first subscribed to or queried,
//! when a transaction commits writes to data they read,
//! and when a module is updated.
//! The latter happens both on a module's main instance,
//! and, when a procedure commits a transaction, from within a guest call on Wasmtime or V8.
//!
//! Each context knows how to call into the guest, which is abstracted by [`ViewComputer`].
//! This module implements, once for all contexts, which guest calls to make
//! for each kind of view instance, in particular for scoped views,
//! where refreshing the instance of a resolver may move its subscriber to a different scope.

use super::module_host::{
    CallScopeResolverParams, ModuleHost, RefInstance, ResolvedViewForRefresh, ViewCallResult, ViewOutcome,
};
use super::wasm_common::module_host_actor::WasmInstance;
use super::{ArgsTuple, FunctionArgs};
use crate::energy::FunctionBudget;
use crate::identity::Identity;
use anyhow::{anyhow, Context as _};
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_datastore::locking_tx_datastore::{MutTxId, ViewCallInfo, ViewInstanceArgs, ViewScopeKey};
use spacetimedb_lib::Timestamp;
use spacetimedb_primitives::ViewFnPtr;
use spacetimedb_schema::def::ModuleDef;
use std::sync::Arc;
use std::time::Duration;

/// The guest calls needed to compute the instances of materialized views.
pub(crate) trait ViewComputer {
    type Error;

    /// Compute the rows of the instance `call` of `view` and materialize them,
    /// calling the view with `sender`, if it takes one, and `args`.
    fn compute_view(
        &mut self,
        tx: MutTxId,
        view: &ResolvedViewForRefresh<'_>,
        call: &ViewCallInfo,
        sender: Option<Identity>,
        args: ArgsTuple,
    ) -> (MutTxId, Result<(), Self::Error>);

    /// Call the scope resolver of the scoped `view` for `subscriber`,
    /// recording its reads under the resolver's instance `call`,
    /// and return the scope key it returned, if any.
    fn resolve_scope(
        &mut self,
        tx: MutTxId,
        view: &ResolvedViewForRefresh<'_>,
        resolver_fn_ptr: ViewFnPtr,
        call: &ViewCallInfo,
        subscriber: Identity,
    ) -> (MutTxId, Result<Option<ViewScopeKey>, Self::Error>);

    /// Convert an error which arose outside of a guest call.
    fn error(&mut self, err: anyhow::Error) -> Self::Error;
}

/// Recompute the materialized view instances `calls`, e.g. because they were made stale by writes in `tx`.
///
/// Stops at, and returns, the first error.
pub(crate) fn refresh_view_calls<C: ViewComputer>(
    computer: &mut C,
    mut tx: MutTxId,
    module_def: &ModuleDef,
    calls: impl IntoIterator<Item = ViewCallInfo>,
) -> (MutTxId, Result<(), C::Error>) {
    let mut refreshed = HashSet::default();
    for call in calls {
        let (next_tx, res) = refresh_view_call(computer, tx, module_def, &call, &mut refreshed);
        tx = next_tx;
        if res.is_err() {
            return (tx, res);
        }
    }
    (tx, Ok(()))
}

/// Recompute the materialized view instance `call`, unless it is in `refreshed`,
/// recording in `refreshed` every instance it computes.
fn refresh_view_call<C: ViewComputer>(
    computer: &mut C,
    tx: MutTxId,
    module_def: &ModuleDef,
    call: &ViewCallInfo,
    refreshed: &mut HashSet<ViewCallInfo>,
) -> (MutTxId, Result<(), C::Error>) {
    if refreshed.contains(call) {
        return (tx, Ok(()));
    }
    let view = match super::module_host::resolve_view_for_refresh(&tx, module_def, call) {
        Ok(view) => view,
        Err(err) => return (tx, Err(computer.error(err))),
    };
    let Some(args) = tx.view_instance_args(call) else {
        let err = anyhow!("failed to look up materialized view args for view {}", call.view_id);
        return (tx, Err(computer.error(err)));
    };
    refreshed.insert(call.clone());

    match args {
        ViewInstanceArgs::Anonymous => computer.compute_view(tx, &view, call, None, ArgsTuple::nullary()),
        ViewInstanceArgs::Sender(sender) => computer.compute_view(tx, &view, call, Some(sender), ArgsTuple::nullary()),
        ViewInstanceArgs::ScopeBody(scope) => compute_scope_body(computer, tx, &view, &scope),
        ViewInstanceArgs::ScopeResolver(subscriber) => {
            let (mut tx, scope) = resolve_scope(computer, tx, &view, subscriber);
            let scope = match scope {
                Ok(scope) => scope,
                Err(err) => return (tx, Err(err)),
            };
            // Moving the subscriber into a scope whose body is not materialized
            // requires materializing it in this transaction.
            match tx.set_view_scope(view.view_id, subscriber, scope) {
                Ok(Some(scope)) if refreshed.insert(ViewCallInfo::scope_body(view.view_id, &scope)) => {
                    compute_scope_body(computer, tx, &view, &scope)
                }
                Ok(_) => (tx, Ok(())),
                Err(err) => {
                    let err = computer.error(err.into());
                    (tx, Err(err))
                }
            }
        }
    }
}

/// Ensure that the scoped `view` is materialized for `subscriber`,
/// as needed to subscribe to or query it:
/// that its resolver has run for `subscriber`, and that the body for their scope is materialized.
///
/// This does not subscribe `subscriber`, nor update the instances' timestamps.
pub(crate) fn materialize_scoped_view<C: ViewComputer>(
    computer: &mut C,
    mut tx: MutTxId,
    view: &ResolvedViewForRefresh<'_>,
    subscriber: Identity,
) -> (MutTxId, Result<(), C::Error>) {
    let scope = match tx.view_scope(view.view_id, subscriber) {
        // The resolver's instance is kept up to date by refreshes,
        // so its scope is current.
        Some(scope) => scope,
        None => {
            let (next_tx, scope) = resolve_scope(computer, tx, view, subscriber);
            tx = next_tx;
            // Like a view which fails when it is first materialized,
            // a resolver which fails leaves its subscriber with an empty view.
            // Its instance still records the reads it made,
            // so that it is retried when they change.
            let (scope, res) = match scope {
                Ok(scope) => (scope, Ok(())),
                Err(err) => (None, Err(err)),
            };
            if let Err(err) = tx.set_view_scope(view.view_id, subscriber, scope.clone()) {
                let err = computer.error(err.into());
                return (tx, Err(err));
            }
            if res.is_err() {
                return (tx, res);
            }
            scope
        }
    };

    match scope {
        Some(scope) if !is_materialized(&tx, &ViewCallInfo::scope_body(view.view_id, &scope)) => {
            compute_scope_body(computer, tx, view, &scope)
        }
        _ => (tx, Ok(())),
    }
}

fn is_materialized(tx: &MutTxId, call: &ViewCallInfo) -> bool {
    tx.is_view_materialized(call).unwrap_or(false)
}

/// Call the scope resolver of the scoped `view` for `subscriber`.
fn resolve_scope<C: ViewComputer>(
    computer: &mut C,
    tx: MutTxId,
    view: &ResolvedViewForRefresh<'_>,
    subscriber: Identity,
) -> (MutTxId, Result<Option<ViewScopeKey>, C::Error>) {
    let Some(resolver_fn_ptr) = view.scope_resolver_fn_ptr else {
        let err = anyhow!("view `{}` is not scoped in the current module", view.view_name);
        return (tx, Err(computer.error(err)));
    };
    let call = ViewCallInfo::scope_resolver(view.view_id, subscriber);
    computer.resolve_scope(tx, view, resolver_fn_ptr, &call, subscriber)
}

/// Compute the body of the scoped `view` for `scope`.
fn compute_scope_body<C: ViewComputer>(
    computer: &mut C,
    tx: MutTxId,
    view: &ResolvedViewForRefresh<'_>,
    scope: &ViewScopeKey,
) -> (MutTxId, Result<(), C::Error>) {
    let args = FunctionArgs::Bsatn(scope.args_bsatn().to_vec().into())
        .into_tuple_for_def(view.owning_def, view.view_def)
        .with_context(|| format!("invalid scope key for view `{}`", view.view_name));
    let args = match args {
        Ok(args) => args,
        Err(err) => return (tx, Err(computer.error(err))),
    };
    let call = ViewCallInfo::scope_body(view.view_id, scope);
    computer.compute_view(tx, view, &call, None, args)
}

/// Computes views on a module's main instance, accumulating the outcome of every call.
///
/// Its [`ViewComputer::Error`] carries no information;
/// on error, the outcome of the failed call is in [`Self::outcome`].
pub(crate) struct InstanceViewComputer<'r, 'a, I: WasmInstance> {
    instance: &'r mut RefInstance<'a, I>,
    /// The identity to which view calls are attributed, e.g. the caller of a reducer.
    caller: Identity,
    timestamp: Timestamp,
    pub outcome: ViewOutcome,
    pub trapped: bool,
    pub num_views_evaluated: u32,
    pub energy_used: FunctionBudget,
    pub total_duration: Duration,
    pub abi_duration: Duration,
    pub call_duration: Duration,
}

impl<'r, 'a, I: WasmInstance> InstanceViewComputer<'r, 'a, I> {
    pub(crate) fn new(instance: &'r mut RefInstance<'a, I>, caller: Identity, timestamp: Timestamp) -> Self {
        Self {
            instance,
            caller,
            timestamp,
            outcome: ViewOutcome::Success,
            trapped: false,
            num_views_evaluated: 0,
            energy_used: FunctionBudget::ZERO,
            total_duration: Duration::ZERO,
            abi_duration: Duration::ZERO,
            call_duration: Duration::ZERO,
        }
    }

    /// Account for the result of a call, returning its transaction and whether it succeeded.
    fn record(&mut self, result: ViewCallResult, trapped: bool) -> (MutTxId, Result<(), ()>) {
        self.num_views_evaluated += 1;
        self.energy_used += result.execution_budget_used;
        self.total_duration += result.total_duration;
        self.abi_duration += result.abi_duration;
        self.call_duration += result.call_duration;
        self.trapped |= trapped;
        let succeeded = !trapped && matches!(result.outcome, ViewOutcome::Success);
        self.outcome = result.outcome;
        (result.tx, if succeeded { Ok(()) } else { Err(()) })
    }

    /// Convert into a [`ViewCallResult`] accumulating every call.
    pub(crate) fn into_result(self, tx: MutTxId) -> ViewCallResult {
        ViewCallResult {
            outcome: self.outcome,
            tx,
            execution_budget_used: self.energy_used,
            total_duration: self.total_duration,
            abi_duration: self.abi_duration,
            call_duration: self.call_duration,
        }
    }
}

impl<I: WasmInstance> ViewComputer for InstanceViewComputer<'_, '_, I> {
    type Error = ();

    fn compute_view(
        &mut self,
        tx: MutTxId,
        view: &ResolvedViewForRefresh<'_>,
        call: &ViewCallInfo,
        sender: Option<Identity>,
        args: ArgsTuple,
    ) -> (MutTxId, Result<(), ()>) {
        let (result, trapped) = ModuleHost::call_view_inner(
            self.instance,
            tx,
            &view.view_name,
            view.view_id,
            view.table_id,
            view.global_fn_ptr,
            self.caller,
            sender,
            call.clone(),
            args,
            view.view_def.product_type_ref,
            self.timestamp,
            Arc::new(view.owning_def.typespace().clone()),
        );
        self.record(result, trapped)
    }

    fn resolve_scope(
        &mut self,
        tx: MutTxId,
        view: &ResolvedViewForRefresh<'_>,
        resolver_fn_ptr: ViewFnPtr,
        call: &ViewCallInfo,
        subscriber: Identity,
    ) -> (MutTxId, Result<Option<ViewScopeKey>, ()>) {
        let Some(key_type) = view.view_def.scope_key_type() else {
            self.error(anyhow!("view `{}` is not scoped", view.view_name));
            return (tx, Err(()));
        };
        let params = CallScopeResolverParams {
            view_name: view.view_name.clone(),
            view_id: view.view_id,
            fn_ptr: resolver_fn_ptr,
            caller: self.caller,
            sender: subscriber,
            call: call.clone(),
            key_type: key_type.clone(),
            timestamp: self.timestamp,
            view_typespace: Arc::new(view.owning_def.typespace().clone()),
        };
        let (result, trapped) = self
            .instance
            .common
            .call_scope_resolver_with_tx(tx, params, self.instance.instance);
        let (tx, res) = self.record(result.result, trapped);
        (tx, res.map(|()| result.scope))
    }

    fn error(&mut self, err: anyhow::Error) {
        self.outcome = ViewOutcome::Failed(err.to_string());
    }
}
