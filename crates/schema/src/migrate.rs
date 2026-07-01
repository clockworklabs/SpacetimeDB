use auto_migrate::{format_plan, ponder_auto_migrate, ColorScheme, TermColorFormatter};
use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_lib::{hash_bytes, Identity};
use thiserror::Error;

use crate::def::ModuleDef;

mod auto_migrate;
mod manual_migrate;

pub use auto_migrate::{AutoMigrateError, AutoMigratePlan, AutoMigratePrecheck, AutoMigrateStep};
pub use manual_migrate::ManualMigratePlan;

/// A plan for a migration.
#[derive(Debug)]
pub enum MigratePlan<'def> {
    Manual(ManualMigratePlan<'def>),
    Auto(AutoMigratePlan<'def>),
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum PrettyPrintStyle {
    AnsiColor,
    NoColor,
}

impl<'def> MigratePlan<'def> {
    /// Get the old `ModuleDef` for this migration plan.
    pub fn old_def(&self) -> &'def ModuleDef {
        match self {
            MigratePlan::Manual(plan) => plan.old,
            MigratePlan::Auto(plan) => plan.old,
        }
    }

    /// Get the new `ModuleDef` for this migration plan.
    pub fn new_def(&self) -> &'def ModuleDef {
        match self {
            MigratePlan::Manual(plan) => plan.new,
            MigratePlan::Auto(plan) => plan.new,
        }
    }

    pub fn breaks_client(&self) -> bool {
        match self {
            //TODO: fix it when support for manual migration plans is added.
            MigratePlan::Manual(_) => true,
            MigratePlan::Auto(plan) => plan
                .steps
                .iter()
                .any(|step| matches!(step, AutoMigrateStep::DisconnectAllUsers)),
        }
    }

    pub fn pretty_print(&self, style: PrettyPrintStyle) -> anyhow::Result<String> {
        use PrettyPrintStyle::*;
        match self {
            MigratePlan::Manual(_) => {
                anyhow::bail!("Manual migration plans are not yet supported for pretty printing.")
            }

            MigratePlan::Auto(plan) => match style {
                NoColor => {
                    let mut fmt = TermColorFormatter::new(ColorScheme::default(), termcolor::ColorChoice::Never);
                    format_plan(&mut fmt, plan).map(|_| fmt.to_string())
                }
                AnsiColor => {
                    let mut fmt = TermColorFormatter::new(ColorScheme::default(), termcolor::ColorChoice::AlwaysAnsi);
                    format_plan(&mut fmt, plan).map(|_| fmt.to_string())
                }
            }
            .map_err(|e| anyhow::anyhow!("Failed to format migration plan: {e}")),
        }
    }
}

/// A migration policy that determines whether a module update is allowed to break client compatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationPolicy {
    /// Migration must maintain backward compatibility with existing clients.
    Compatible,
    /// To use this, a valid [`MigrationToken`] must be provided.
    /// The token is issued through the pre-publish API (see the `client-api` crate)
    /// and proves that the publisher explicitly acknowledged the breaking change.
    BreakClients(spacetimedb_lib::Hash),
}

impl MigrationPolicy {
    /// Verifies whether the given migration plan is allowed under the current policy.
    ///
    /// Returns `Ok(())` if allowed, otherwise an appropriate `MigrationPolicyError`
    fn permits_plan(&self, plan: &MigratePlan<'_>, token: &MigrationToken) -> anyhow::Result<(), MigrationPolicyError> {
        match self {
            MigrationPolicy::Compatible => {
                if plan.breaks_client() {
                    Err(MigrationPolicyError::ClientBreakingChangeDisallowed)
                } else {
                    Ok(())
                }
            }
            MigrationPolicy::BreakClients(expected_hash) => {
                if token.hash() == *expected_hash {
                    Ok(())
                } else {
                    Err(MigrationPolicyError::InvalidToken)
                }
            }
        }
    }

    /// Attempts to generate a migration plan and validate it under this policy.
    ///
    /// Fails if migration is not permitted by the policy or migration planning fails.
    pub fn try_migrate<'def>(
        &self,
        database_identity: Identity,
        old_module_hash: spacetimedb_lib::Hash,
        old_module_def: &'def ModuleDef,
        new_module_hash: spacetimedb_lib::Hash,
        new_module_def: &'def ModuleDef,
    ) -> anyhow::Result<MigratePlan<'def>, MigrationPolicyError> {
        let plan = ponder_migrate(old_module_def, new_module_def).map_err(MigrationPolicyError::AutoMigrateFailure)?;
        self.permits_migrate_plan(database_identity, old_module_hash, new_module_hash, &plan)?;
        Ok(plan)
    }

    /// Validate an already-generated migration plan under this policy.
    pub fn permits_migrate_plan(
        &self,
        database_identity: Identity,
        old_module_hash: spacetimedb_lib::Hash,
        new_module_hash: spacetimedb_lib::Hash,
        plan: &MigratePlan<'_>,
    ) -> anyhow::Result<(), MigrationPolicyError> {
        let token = MigrationToken {
            database_identity,
            old_module_hash,
            new_module_hash,
        };
        self.permits_plan(plan, &token)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum MigrationPolicyError {
    #[error("Automatic migration planning failed")]
    AutoMigrateFailure(ErrorStream<AutoMigrateError>),

    #[error("Token provided is invalid or does not match expected hash")]
    InvalidToken,

    #[error("Migration plan contains a client-breaking change which is disallowed under current policy")]
    ClientBreakingChangeDisallowed,
}

/// A token acknowledging a breaking migration.
///
/// Note: This token is only intended as a UX safeguard, not as a security measure.
/// No secret is used in its generation, which means anyone can reproduce it given
/// the inputs. That is acceptable for our purposes since it only signals user intent,
/// not authorization.
pub struct MigrationToken {
    pub database_identity: Identity,
    pub old_module_hash: spacetimedb_lib::Hash,
    pub new_module_hash: spacetimedb_lib::Hash,
}

impl MigrationToken {
    pub fn hash(&self) -> spacetimedb_lib::Hash {
        hash_bytes(
            format!(
                "{}{}{}",
                self.database_identity.to_hex(),
                self.old_module_hash.to_hex(),
                self.new_module_hash.to_hex()
            )
            .as_str(),
        )
    }
}

/// Construct a migration plan.
/// If `new` has an `__update__` reducer, return a manual migration plan.
/// Otherwise, try to plan an automatic migration. This may fail.
pub fn ponder_migrate<'def>(
    old: &'def ModuleDef,
    new: &'def ModuleDef,
) -> Result<MigratePlan<'def>, ErrorStream<AutoMigrateError>> {
    // TODO(1.0): Implement this function.
    // Currently we only can do automatic migrations.
    ponder_auto_migrate(old, new).map(MigratePlan::Auto)
}
