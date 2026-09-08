//! Authority carried from authenticated admission to module execution.
//!
//! Identity equality never establishes internal authority. A hosted proof can
//! only originate in signature verification against trusted platform state.

use super::hosted_tokens::VerifiedHostedAuth;
use spacetimedb_auth::identity::ConnectionAuthCtx;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_lib::Identity;
use spacetimedb_schema::def::ModuleDef;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct InvocationCaller {
    pub(crate) identity: Identity,
    pub(crate) hosted: Option<std::sync::Arc<VerifiedHostedAuth>>,
}

impl From<Identity> for InvocationCaller {
    fn from(identity: Identity) -> Self {
        Self { identity, hosted: None }
    }
}

impl From<&ConnectionAuthCtx> for InvocationCaller {
    fn from(auth: &ConnectionAuthCtx) -> Self {
        Self {
            identity: auth.claims.identity,
            hosted: auth.hosted.clone().map(std::sync::Arc::new),
        }
    }
}

/// SQL permissions and the authenticated container restrictions are independent.
/// Internal status never impersonates the database owner or grants SQL rights.
#[derive(Clone)]
pub struct SqlCallAuth {
    permissions: spacetimedb_lib::identity::AuthCtx,
    pub(crate) hosted: Option<std::sync::Arc<VerifiedHostedAuth>>,
}

impl From<spacetimedb_lib::identity::AuthCtx> for SqlCallAuth {
    fn from(permissions: spacetimedb_lib::identity::AuthCtx) -> Self {
        Self {
            permissions,
            hosted: None,
        }
    }
}

impl std::ops::Deref for SqlCallAuth {
    type Target = spacetimedb_lib::identity::AuthCtx;
    fn deref(&self) -> &Self::Target {
        &self.permissions
    }
}

impl SqlCallAuth {
    pub fn authenticated(
        permissions: spacetimedb_lib::identity::AuthCtx,
        auth: &ConnectionAuthCtx,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            permissions.caller() == auth.claims.identity,
            "SQL caller does not match authenticated sender"
        );
        if let Some(proof) = &auth.hosted {
            anyhow::ensure!(
                proof.source_database() == auth.claims.identity,
                "SQL hosted proof does not match authenticated sender"
            );
        }
        Ok(Self {
            permissions,
            hosted: auth.hosted.clone().map(std::sync::Arc::new),
        })
    }
}

impl InvocationCaller {
    pub(crate) fn flags_for(&self, target: Identity, module: &ModuleDef) -> anyhow::Result<u32> {
        let Some(proof) = &self.hosted else { return Ok(0) };
        anyhow::ensure!(
            proof.source_database() == self.identity,
            "hosted caller does not match its proof"
        );
        anyhow::ensure!(
            proof.target_database() == target,
            "hosted credential targets another database"
        );
        anyhow::ensure!(
            module.supports_hosted_auth_v1(),
            "module does not support hosted authentication"
        );
        proof.check_at(SystemTime::now())?;
        Ok(u32::from(proof.is_internal()))
    }
}

/// Must run while holding the transaction that admits the database operation.
/// A check before queueing does not serialize with generation revocation.
pub(crate) fn check_hosted_admission<S: StateView>(
    state: &S,
    database: &crate::db::relational_db::RelationalDB,
    proof: Option<&VerifiedHostedAuth>,
) -> anyhow::Result<()> {
    let Some(proof) = proof else { return Ok(()) };
    anyhow::ensure!(
        database.hosted_admission().is_open(),
        "receiving database has not reconciled hosted admission"
    );
    anyhow::ensure!(
        proof.target_database() == database.database_identity(),
        "hosted credential targets another database"
    );
    proof.check_at(SystemTime::now())?;
    crate::db::deployment::check_container_fence(
        state,
        proof.source_database(),
        proof.generation(),
        proof.grant_revision(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
