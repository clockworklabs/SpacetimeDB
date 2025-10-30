use core::{cmp::Ordering, ops::BitOr};

use crate::{def::*, error::PrettyAlgebraicType, identifier::Identifier};
use formatter::format_plan;
use spacetimedb_data_structures::{
    error_stream::{CollectAllErrors, CombineErrors, ErrorStream},
    map::HashSet,
};
use spacetimedb_lib::{
    db::raw_def::v9::{RawRowLevelSecurityDefV9, TableType},
    hash_bytes, AlgebraicType, Identity,
};
use spacetimedb_sats::{
    layout::{HasLayout, SumTypeLayout},
    WithTypespace,
};
use termcolor_formatter::{ColorScheme, TermColorFormatter};
use thiserror::Error;
mod formatter;
mod termcolor_formatter;

pub type Result<T> = std::result::Result<T, ErrorStream<AutoMigrateError>>;

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

        let token = MigrationToken {
            database_identity,
            old_module_hash,
            new_module_hash,
        };
        self.permits_plan(&plan, &token)?;
        Ok(plan)
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

/// A plan for a manual migration.
/// `new` must have a reducer marked with `Lifecycle::Update`.
#[derive(Debug)]
pub struct ManualMigratePlan<'def> {
    pub old: &'def ModuleDef,
    pub new: &'def ModuleDef,
}

/// A plan for an automatic migration.
#[derive(Debug)]
pub struct AutoMigratePlan<'def> {
    /// The old database definition.
    pub old: &'def ModuleDef,
    /// The new database definition.
    pub new: &'def ModuleDef,
    /// The checks to perform before the automatic migration.
    /// There is also an implied check: that the schema in the database is compatible with the old ModuleDef.
    pub prechecks: Vec<AutoMigratePrecheck<'def>>,
    /// The migration steps to perform.
    /// Order matters: `Remove`s of a particular `Def` must be ordered before `Add`s.
    pub steps: Vec<AutoMigrateStep<'def>>,
}

impl AutoMigratePlan<'_> {
    fn any_step(&self, f: impl Fn(&AutoMigrateStep) -> bool) -> bool {
        self.steps.iter().any(f)
    }

    fn disconnects_all_users(&self) -> bool {
        self.any_step(|step| matches!(step, AutoMigrateStep::DisconnectAllUsers))
    }
}

/// Checks that must be performed before performing an automatic migration.
/// These checks can access table contents and other database state.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum AutoMigratePrecheck<'def> {
    /// Perform a check that adding a sequence is valid (the relevant column contains no values
    /// greater than the sequence's start value).
    CheckAddSequenceRangeValid(<SequenceDef as ModuleDefLookup>::Key<'def>),
}

/// A step in an automatic migration.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum AutoMigrateStep<'def> {
    // It is important FOR CORRECTNESS that `Remove` variants are declared before `Add` variants in this enum!
    //
    // The ordering is used to sort the steps of an auto-migration.
    // If adds go before removes, and the user tries to remove an index and then re-add it with new configuration,
    // the following can occur:
    //
    // 1. `AddIndex("indexname")`
    // 2. `RemoveIndex("indexname")`
    //
    // This results in the existing index being re-added -- which, at time of writing, does nothing -- and then removed,
    // resulting in the intended index not being created.
    //
    // For now, we just ensure that we declare all `Remove` variants before `Add` variants
    // and let `#[derive(PartialOrd)]` take care of the rest.
    //
    // TODO: when this enum is made serializable, a more durable fix will be needed here.
    // Probably we will want to have separate arrays of add and remove steps.
    //
    /// Remove an index.
    RemoveIndex(<IndexDef as ModuleDefLookup>::Key<'def>),
    /// Remove a constraint.
    RemoveConstraint(<ConstraintDef as ModuleDefLookup>::Key<'def>),
    /// Remove a sequence.
    RemoveSequence(<SequenceDef as ModuleDefLookup>::Key<'def>),
    /// Remove a schedule annotation from a table.
    RemoveSchedule(<ScheduleDef as ModuleDefLookup>::Key<'def>),
    /// Remove a view and corresponding view table
    RemoveView(<ViewDef as ModuleDefLookup>::Key<'def>),
    /// Remove a row-level security query.
    RemoveRowLevelSecurity(<RawRowLevelSecurityDefV9 as ModuleDefLookup>::Key<'def>),

    /// Change the column types of a table, in a layout compatible way.
    ///
    /// This should be done before any new indices are added.
    ChangeColumns(<TableDef as ModuleDefLookup>::Key<'def>),
    /// Add columns to a table, in a layout-INCOMPATIBLE way.
    ///
    /// This is a destructive operation that requires first running a `DisconnectAllUsers`.
    ///
    /// The added columns are guaranteed to be contiguous
    /// and at the end of the table.
    /// They are also guaranteed to have default values set.
    ///
    /// When this step is present,
    /// no `ChangeColumns` steps will be, for the same table.
    AddColumns(<TableDef as ModuleDefLookup>::Key<'def>),

    /// Add a table, including all indexes, constraints, and sequences.
    /// There will NOT be separate steps in the plan for adding indexes, constraints, and sequences.
    AddTable(<TableDef as ModuleDefLookup>::Key<'def>),
    /// Add an index.
    AddIndex(<IndexDef as ModuleDefLookup>::Key<'def>),
    /// Add a sequence.
    AddSequence(<SequenceDef as ModuleDefLookup>::Key<'def>),
    /// Add a schedule annotation to a table.
    AddSchedule(<ScheduleDef as ModuleDefLookup>::Key<'def>),
    /// Add a view and corresponding view table
    AddView(<ViewDef as ModuleDefLookup>::Key<'def>),
    /// Add a row-level security query.
    AddRowLevelSecurity(<RawRowLevelSecurityDefV9 as ModuleDefLookup>::Key<'def>),

    /// Change the access of a table.
    ChangeAccess(<TableDef as ModuleDefLookup>::Key<'def>),

    /// Recompute a view, update its backing table, and push updates to clients
    UpdateView(<ViewDef as ModuleDefLookup>::Key<'def>),

    /// Disconnect all users connected to the module.
    DisconnectAllUsers,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChangeColumnTypeParts {
    pub table: Identifier,
    pub column: Identifier,
    pub type1: PrettyAlgebraicType,
    pub type2: PrettyAlgebraicType,
}

/// Something that might prevent an automatic migration.
#[derive(thiserror::Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AutoMigrateError {
    #[error("Adding a column {column} to table {table} requires a default value annotation")]
    AddColumn { table: Identifier, column: Identifier },

    #[error("Removing a column {column} from table {table} requires a manual migration")]
    RemoveColumn { table: Identifier, column: Identifier },

    #[error("Reordering table {table} requires a manual migration")]
    ReorderTable { table: Identifier },

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?} requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeColumnType(ChangeColumnTypeParts),

    #[error(
        "Changing a type within column {} in table {} from {:?} to {:?} requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeWithinColumnType(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, with fewer variants, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeColumnTypeFewerVariants(ChangeColumnTypeParts),

    #[error(
        "Changing a type within column {} in table {} from {:?} to {:?}, with fewer variants, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeWithinColumnTypeFewerVariants(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, with a renamed variant, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeColumnTypeRenamedVariant(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, with a renamed variant, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeWithinColumnTypeRenamedVariant(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, requires a manual migration, due to size mismatch",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeColumnTypeSizeMismatch(ChangeColumnTypeParts),

    #[error(
        "Changing a type within column {} in table {} from {:?} to {:?}, requires a manual migration, due to size mismatch",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeWithinColumnTypeSizeMismatch(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, requires a manual migration, due to alignment mismatch",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeColumnTypeAlignMismatch(ChangeColumnTypeParts),

    #[error(
        "Changing a type within column {} in table {} from {:?} to {:?}, requires a manual migration, due to alignment mismatch",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeWithinColumnTypeAlignMismatch(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, with fewer fields, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeColumnTypeFewerFields(ChangeColumnTypeParts),

    #[error(
        "Changing a type within column {} in table {} from {:?} to {:?}, with fewer fields, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeWithinColumnTypeFewerFields(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, with a renamed field, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeColumnTypeRenamedField(ChangeColumnTypeParts),

    #[error(
        "Changing the type of column {} in table {} from {:?} to {:?}, with a renamed field, requires a manual migration",
        .0.column, .0.table, .0.type1, .0.type2
    )]
    ChangeWithinColumnTypeRenamedField(ChangeColumnTypeParts),

    #[error("Adding a unique constraint {constraint} requires a manual migration")]
    AddUniqueConstraint { constraint: Box<str> },

    #[error("Changing a unique constraint {constraint} requires a manual migration")]
    ChangeUniqueConstraint { constraint: Box<str> },

    #[error("Removing the table {table} requires a manual migration")]
    RemoveTable { table: Identifier },

    #[error("Changing the table type of table {table} from {type1:?} to {type2:?} requires a manual migration")]
    ChangeTableType {
        table: Identifier,
        type1: TableType,
        type2: TableType,
    },

    #[error(
        "Changing the accessor name on index {index} from {old_accessor:?} to {new_accessor:?} requires a manual migration"
    )]
    ChangeIndexAccessor {
        index: Box<str>,
        old_accessor: Option<Identifier>,
        new_accessor: Option<Identifier>,
    },
}

/// Construct a migration plan.
/// If `new` has an `__update__` reducer, return a manual migration plan.
/// Otherwise, try to plan an automatic migration. This may fail.
pub fn ponder_migrate<'def>(old: &'def ModuleDef, new: &'def ModuleDef) -> Result<MigratePlan<'def>> {
    // TODO(1.0): Implement this function.
    // Currently we only can do automatic migrations.
    ponder_auto_migrate(old, new).map(MigratePlan::Auto)
}

/// Construct an automatic migration plan, or reject with reasons why automatic migration can't be performed.
pub fn ponder_auto_migrate<'def>(old: &'def ModuleDef, new: &'def ModuleDef) -> Result<AutoMigratePlan<'def>> {
    // Both the old and new database definitions have already been validated (this is enforced by the types).
    // All we have to do is walk through and compare them.
    let mut plan = AutoMigratePlan {
        old,
        new,
        steps: Vec::new(),
        prechecks: Vec::new(),
    };

    let views_ok = auto_migrate_views(&mut plan);
    let tables_ok = auto_migrate_tables(&mut plan);

    // Our diffing algorithm will detect added constraints / indexes / sequences in new tables, we use this to filter those out.
    // They're handled by adding the root table.
    let new_tables: HashSet<&Identifier> = diff(plan.old, plan.new, ModuleDef::tables)
        .filter_map(|diff| match diff {
            Diff::Add { new } => Some(&new.name),
            _ => None,
        })
        .collect();
    let indexes_ok = auto_migrate_indexes(&mut plan, &new_tables);
    let sequences_ok = auto_migrate_sequences(&mut plan, &new_tables);
    let constraints_ok = auto_migrate_constraints(&mut plan, &new_tables);
    // IMPORTANT: RLS auto-migrate steps must come last,
    // since they assume that any schema changes, like adding or dropping tables,
    // have already been reflected in the database state.
    let rls_ok = auto_migrate_row_level_security(&mut plan);

    let ((), (), (), (), (), ()) =
        (views_ok, tables_ok, indexes_ok, sequences_ok, constraints_ok, rls_ok).combine_errors()?;

    plan.steps.sort();
    plan.prechecks.sort();

    Ok(plan)
}

/// A diff between two items.
/// `Add` means the item is present in the new `ModuleDef` but not the old.
/// `Remove` means the item is present in the old `ModuleDef` but not the new.
/// `MaybeChange` indicates the item is present in both.
#[derive(Debug)]
enum Diff<'def, T> {
    Add { new: &'def T },
    Remove { old: &'def T },
    MaybeChange { old: &'def T, new: &'def T },
}

/// Diff a collection of items, looking them up in both the old and new `ModuleDef` by their `ModuleDefLookup::Key`.
/// Keys are required to be stable across migrations, which makes this possible.
fn diff<'def, T: ModuleDefLookup, I: Iterator<Item = &'def T>>(
    old: &'def ModuleDef,
    new: &'def ModuleDef,
    iter: impl Fn(&'def ModuleDef) -> I,
) -> impl Iterator<Item = Diff<'def, T>> {
    iter(old)
        .map(move |old_item| match T::lookup(new, old_item.key()) {
            Some(new_item) => Diff::MaybeChange {
                old: old_item,
                new: new_item,
            },
            None => Diff::Remove { old: old_item },
        })
        .chain(iter(new).filter_map(move |new_item| {
            if T::lookup(old, new_item.key()).is_none() {
                Some(Diff::Add { new: new_item })
            } else {
                None
            }
        }))
}

fn auto_migrate_views(plan: &mut AutoMigratePlan<'_>) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::views)
        .map(|table_diff| -> Result<()> {
            match table_diff {
                Diff::Add { new } => {
                    plan.steps.push(AutoMigrateStep::AddView(new.key()));
                    Ok(())
                }
                // From the user's perspective, views do not have persistent state.
                // Hence removal does not require a manual migration - just disconnecting clients.
                Diff::Remove { old } if plan.disconnects_all_users() => {
                    plan.steps.push(AutoMigrateStep::RemoveView(old.key()));
                    Ok(())
                }
                Diff::Remove { old } => {
                    plan.steps.push(AutoMigrateStep::RemoveView(old.key()));
                    plan.steps.push(AutoMigrateStep::DisconnectAllUsers);
                    Ok(())
                }
                Diff::MaybeChange { old, new } => auto_migrate_view(plan, old, new),
            }
        })
        .collect_all_errors()
}

fn auto_migrate_view<'def>(plan: &mut AutoMigratePlan<'def>, old: &'def ViewDef, new: &'def ViewDef) -> Result<()> {
    let key = old.key();

    if old.is_public != new.is_public {
        plan.steps.push(AutoMigrateStep::ChangeAccess(key));
    }

    // We can always auto-migrate a view because we can always re-compute it.
    // However certain things require us to disconnect clients:
    // 1. If we add or remove a column or parameter
    // 2. If we change the order of the columns or parameters
    // 3. If we change the types of the columns or parameters
    // 4. If we change the context parameter
    let Any(incompatible_return_type) = diff(plan.old, plan.new, |def| {
        def.lookup_expect::<ViewDef>(key).return_columns.iter()
    })
    .map(|col_diff| {
        match col_diff {
            // We must disconnect clients if we add or remove a parameter or column
            Diff::Add { .. } | Diff::Remove { .. } => Any(true),
            Diff::MaybeChange { old, new } => {
                if old.col_id != new.col_id {
                    return Any(true);
                };

                ensure_old_ty_upgradable_to_new(
                    false,
                    &|| old.view_name.clone(),
                    &|| old.name.clone(),
                    &WithTypespace::new(plan.old.typespace(), &old.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                    &WithTypespace::new(plan.new.typespace(), &new.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                )
                .unwrap_or(Any(true))
            }
        }
    })
    .collect();

    let Any(incompatible_param_types) = diff(plan.old, plan.new, |def| {
        def.lookup_expect::<ViewDef>(key).param_columns.iter()
    })
    .map(|col_diff| {
        match col_diff {
            // We must disconnect clients if we add or remove a parameter or column
            Diff::Add { .. } | Diff::Remove { .. } => Any(true),
            Diff::MaybeChange { old, new } => {
                if old.col_id != new.col_id {
                    return Any(true);
                };

                ensure_old_ty_upgradable_to_new(
                    false,
                    &|| old.view_name.clone(),
                    &|| old.name.clone(),
                    &WithTypespace::new(plan.old.typespace(), &old.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                    &WithTypespace::new(plan.new.typespace(), &new.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                )
                .unwrap_or(Any(true))
            }
        }
    })
    .collect();

    if old.is_anonymous != new.is_anonymous || incompatible_return_type || incompatible_param_types {
        plan.steps.push(AutoMigrateStep::AddView(new.key()));
        plan.steps.push(AutoMigrateStep::RemoveView(old.key()));

        if !plan.disconnects_all_users() {
            plan.steps.push(AutoMigrateStep::DisconnectAllUsers);
        }
    } else {
        plan.steps.push(AutoMigrateStep::UpdateView(old.key()));
    }

    Ok(())
}

fn auto_migrate_tables(plan: &mut AutoMigratePlan<'_>) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::tables)
        .map(|table_diff| -> Result<()> {
            match table_diff {
                Diff::Add { new } => {
                    plan.steps.push(AutoMigrateStep::AddTable(new.key()));
                    Ok(())
                }
                // TODO: When we remove tables, we should also remove their dependencies, including row-level security.
                Diff::Remove { old } => Err(AutoMigrateError::RemoveTable {
                    table: old.name.clone(),
                }
                .into()),
                Diff::MaybeChange { old, new } => auto_migrate_table(plan, old, new),
            }
        })
        .collect_all_errors()
}

fn auto_migrate_table<'def>(plan: &mut AutoMigratePlan<'def>, old: &'def TableDef, new: &'def TableDef) -> Result<()> {
    let key = old.key();
    let type_ok: Result<()> = if old.table_type == new.table_type {
        Ok(())
    } else {
        Err(AutoMigrateError::ChangeTableType {
            table: old.name.clone(),
            type1: old.table_type,
            type2: new.table_type,
        }
        .into())
    };
    if old.table_access != new.table_access {
        plan.steps.push(AutoMigrateStep::ChangeAccess(key));
    }
    if old.schedule != new.schedule {
        // Note: this handles the case where there's an altered ScheduleDef for some reason.
        if let Some(old_schedule) = old.schedule.as_ref() {
            plan.steps.push(AutoMigrateStep::RemoveSchedule(old_schedule.key()));
        }
        if let Some(new_schedule) = new.schedule.as_ref() {
            plan.steps.push(AutoMigrateStep::AddSchedule(new_schedule.key()));
        }
    }

    let columns_ok = diff(plan.old, plan.new, |def| {
        def.lookup_expect::<TableDef>(key).columns.iter()
    })
    .map(|col_diff| -> Result<_> {
        match col_diff {
            Diff::Add { new } => {
                if new.default_value.is_some() {
                    // `row_type_changed`, `columns_added`
                    Ok(ProductMonoid(Any(false), Any(true)))
                } else {
                    Err(AutoMigrateError::AddColumn {
                        table: new.table_name.clone(),
                        column: new.name.clone(),
                    }
                    .into())
                }
            }
            Diff::Remove { old } => Err(AutoMigrateError::RemoveColumn {
                table: old.table_name.clone(),
                column: old.name.clone(),
            }
            .into()),
            Diff::MaybeChange { old, new } => {
                // Check column type upgradability.
                let old_ty = WithTypespace::new(plan.old.typespace(), &old.ty)
                    .resolve_refs()
                    .expect("valid TableDef must have valid type refs");
                let new_ty = WithTypespace::new(plan.new.typespace(), &new.ty)
                    .resolve_refs()
                    .expect("valid TableDef must have valid type refs");
                let types_ok = ensure_old_ty_upgradable_to_new(
                    false,
                    &|| old.table_name.clone(),
                    &|| old.name.clone(),
                    &old_ty,
                    &new_ty,
                );

                // Note that the diff algorithm relies on `ModuleDefLookup` for `ColumnDef`,
                // which looks up columns by NAME, NOT position: precisely to allow this step to work!

                // Note: We reject changes to positions. This means that, if a column was present in the old version of the table,
                // it must be in the same place in the new version of the table.
                // This guarantees that any added columns live at the end of the table.
                let positions_ok = if old.col_id == new.col_id {
                    Ok(())
                } else {
                    Err(AutoMigrateError::ReorderTable {
                        table: old.table_name.clone(),
                    }
                    .into())
                };

                (types_ok, positions_ok)
                    .combine_errors()
                    // row_type_changed, column_added
                    .map(|(x, _)| ProductMonoid(x, Any(false)))
            }
        }
    })
    .collect_all_errors::<ProductMonoid<Any, Any>>();

    let ((), ProductMonoid(Any(row_type_changed), Any(columns_added))) = (type_ok, columns_ok).combine_errors()?;

    // If we're adding a column, we'll rewrite the whole table.
    // That makes any `ChangeColumns` moot, so we can skip it.
    if columns_added {
        if !plan.disconnects_all_users() {
            plan.steps.push(AutoMigrateStep::DisconnectAllUsers);
        }
        plan.steps.push(AutoMigrateStep::AddColumns(key));
    } else if row_type_changed {
        plan.steps.push(AutoMigrateStep::ChangeColumns(key));
    }

    Ok(())
}

/// An "any" monoid with `false` as identity and `|` as the operator.
#[derive(Default)]
struct Any(bool);

impl FromIterator<Any> for Any {
    fn from_iter<T: IntoIterator<Item = Any>>(iter: T) -> Self {
        Any(iter.into_iter().any(|Any(x)| x))
    }
}

impl BitOr for Any {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// A monoid that allows running two `Any`s in parallel.
#[derive(Default)]
struct ProductMonoid<M1, M2>(M1, M2);

impl<M1: BitOr<Output = M1>, M2: BitOr<Output = M2>> BitOr for ProductMonoid<M1, M2> {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0, self.1 | rhs.1)
    }
}

impl<M1: BitOr<Output = M1> + Default, M2: BitOr<Output = M2> + Default> FromIterator<ProductMonoid<M1, M2>>
    for ProductMonoid<M1, M2>
{
    fn from_iter<T: IntoIterator<Item = ProductMonoid<M1, M2>>>(iter: T) -> Self {
        iter.into_iter().reduce(|p1, p2| p1 | p2).unwrap_or_default()
    }
}

fn ensure_old_ty_upgradable_to_new(
    within: bool,
    old_container_name: &impl Fn() -> Identifier,
    old_column_name: &impl Fn() -> Identifier,
    old_ty: &AlgebraicType,
    new_ty: &AlgebraicType,
) -> Result<Any> {
    use AutoMigrateError::*;

    // Ensures an `old_ty` within `old` is upgradable to `new_ty`.
    let ensure =
        |(old_ty, new_ty)| ensure_old_ty_upgradable_to_new(true, old_container_name, old_column_name, old_ty, new_ty);

    // Returns a `ChangeColumnTypeParts` error using the current `old_ty` and `new_ty`.
    let parts_for_error = || ChangeColumnTypeParts {
        table: old_container_name(),
        column: old_column_name(),
        type1: old_ty.clone().into(),
        type2: new_ty.clone().into(),
    };

    match (old_ty, new_ty) {
        // For sums, we allow the variants in `old_ty` to be a prefix of `new_ty`.
        (AlgebraicType::Sum(old_ty), AlgebraicType::Sum(new_ty)) => {
            let old_vars = &*old_ty.variants;
            let new_vars = &*new_ty.variants;

            // The number of variants in `new_ty` cannot decrease.
            let var_lens_ok = match old_vars.len().cmp(&new_vars.len()) {
                Ordering::Less => Ok(Any(true)),
                Ordering::Equal => Ok(Any(false)),
                Ordering::Greater if within => Err(ChangeWithinColumnTypeFewerVariants(parts_for_error()).into()),
                Ordering::Greater => Err(ChangeColumnTypeFewerVariants(parts_for_error()).into()),
            };

            // The variants in `old_ty` must be upgradable to those in `old_ty`.
            // Strict equality is *not* imposed in the prefix!
            let prefix_ok = old_vars
                .iter()
                .zip(new_vars)
                .map(|(o, n)| {
                    // Ensure type compatibility.
                    let res_ty = ensure((&o.algebraic_type, &n.algebraic_type));
                    // Ensure name doesn't change.
                    let res_name = if o.name() == n.name() {
                        Ok(())
                    } else if within {
                        Err(ChangeWithinColumnTypeRenamedVariant(parts_for_error()).into())
                    } else {
                        Err(ChangeColumnTypeRenamedVariant(parts_for_error()).into())
                    };
                    (res_ty, res_name).combine_errors().map(|(c, ())| c)
                })
                .collect_all_errors::<Any>();

            // The old and the new sum types must have matching layout sizes and alignments.
            let old_ty = SumTypeLayout::from(old_ty.clone());
            let new_ty = SumTypeLayout::from(new_ty.clone());
            let old_layout = old_ty.layout();
            let new_layout = new_ty.layout();
            let size_ok = if old_layout.size == new_layout.size {
                Ok(())
            } else if within {
                Err(ChangeWithinColumnTypeSizeMismatch(parts_for_error()).into())
            } else {
                Err(ChangeColumnTypeSizeMismatch(parts_for_error()).into())
            };
            let align_ok = if old_layout.align == new_layout.align {
                Ok(())
            } else if within {
                Err(ChangeWithinColumnTypeAlignMismatch(parts_for_error()).into())
            } else {
                Err(ChangeColumnTypeAlignMismatch(parts_for_error()).into())
            };

            let (len_changed, prefix_changed, ..) = (var_lens_ok, prefix_ok, size_ok, align_ok).combine_errors()?;
            Ok(len_changed | prefix_changed)
        }

        // For products,
        // we need to check each field's upgradability due to sums,
        // and there must be as many fields.
        // Note that we don't care about field names.
        (AlgebraicType::Product(old_ty), AlgebraicType::Product(new_ty)) => {
            // The number of variants in `new_ty` cannot decrease.
            let len_eq_ok = if old_ty.len() == new_ty.len() {
                Ok(())
            } else {
                Err(if within {
                    ChangeWithinColumnTypeFewerFields(parts_for_error())
                } else {
                    ChangeColumnTypeFewerFields(parts_for_error())
                }
                .into())
            };

            // The fields in `old_ty` must be upgradable to those in `old_ty`.
            let fields_ok = old_ty
                .iter()
                .zip(new_ty.iter())
                .map(|(o, n)| {
                    // Ensure type compatibility.
                    let res_ty = ensure((&o.algebraic_type, &n.algebraic_type));
                    // Ensure name doesn't change.
                    let res_name = if o.name() == n.name() {
                        Ok(())
                    } else if within {
                        Err(ChangeWithinColumnTypeRenamedField(parts_for_error()).into())
                    } else {
                        Err(ChangeColumnTypeRenamedField(parts_for_error()).into())
                    };
                    (res_ty, res_name).combine_errors().map(|(c, ())| c)
                })
                .collect_all_errors::<Any>();

            (len_eq_ok, fields_ok).combine_errors().map(|(_, x)| x)
        }

        // For arrays, we need to check each field's upgradability due to sums.
        (AlgebraicType::Array(old_ty), AlgebraicType::Array(new_ty)) => ensure_old_ty_upgradable_to_new(
            true,
            old_container_name,
            old_column_name,
            &old_ty.elem_ty,
            &new_ty.elem_ty,
        ),

        // We only have the simple cases left, and there, no change is good change.
        (old_ty, new_ty) if old_ty == new_ty => Ok(Any(false)),
        _ => Err(if within {
            ChangeWithinColumnType(parts_for_error())
        } else {
            ChangeColumnType(parts_for_error())
        }
        .into()),
    }
}

fn auto_migrate_indexes(plan: &mut AutoMigratePlan<'_>, new_tables: &HashSet<&Identifier>) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::indexes)
        .map(|index_diff| -> Result<()> {
            match index_diff {
                Diff::Add { new } => {
                    if !new_tables.contains(&plan.new.stored_in_table_def(&new.name).unwrap().name) {
                        plan.steps.push(AutoMigrateStep::AddIndex(new.key()));
                    }
                    Ok(())
                }
                Diff::Remove { old } => {
                    plan.steps.push(AutoMigrateStep::RemoveIndex(old.key()));
                    Ok(())
                }
                Diff::MaybeChange { old, new } => {
                    if old.accessor_name != new.accessor_name {
                        Err(AutoMigrateError::ChangeIndexAccessor {
                            index: old.name.clone(),
                            old_accessor: old.accessor_name.clone(),
                            new_accessor: new.accessor_name.clone(),
                        }
                        .into())
                    } else {
                        if old.algorithm != new.algorithm {
                            plan.steps.push(AutoMigrateStep::RemoveIndex(old.key()));
                            plan.steps.push(AutoMigrateStep::AddIndex(old.key()));
                        }
                        Ok(())
                    }
                }
            }
        })
        .collect_all_errors()
}

fn auto_migrate_sequences(plan: &mut AutoMigratePlan, new_tables: &HashSet<&Identifier>) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::sequences)
        .map(|sequence_diff| -> Result<()> {
            match sequence_diff {
                Diff::Add { new } => {
                    if !new_tables.contains(&plan.new.stored_in_table_def(&new.name).unwrap().name) {
                        plan.prechecks
                            .push(AutoMigratePrecheck::CheckAddSequenceRangeValid(new.key()));
                        plan.steps.push(AutoMigrateStep::AddSequence(new.key()));
                    }
                    Ok(())
                }
                Diff::Remove { old } => {
                    plan.steps.push(AutoMigrateStep::RemoveSequence(old.key()));
                    Ok(())
                }
                Diff::MaybeChange { old, new } => {
                    // we do not need to check column ids, since in an automigrate, column ids are not changed.
                    if old != new {
                        plan.prechecks
                            .push(AutoMigratePrecheck::CheckAddSequenceRangeValid(new.key()));
                        plan.steps.push(AutoMigrateStep::RemoveSequence(old.key()));
                        plan.steps.push(AutoMigrateStep::AddSequence(new.key()));
                    }
                    Ok(())
                }
            }
        })
        .collect_all_errors()
}

fn auto_migrate_constraints(plan: &mut AutoMigratePlan, new_tables: &HashSet<&Identifier>) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::constraints)
        .map(|constraint_diff| -> Result<()> {
            match constraint_diff {
                Diff::Add { new } => {
                    if new_tables.contains(&plan.new.stored_in_table_def(&new.name).unwrap().name) {
                        // it's okay to add a constraint in a new table.
                        Ok(())
                    } else {
                        // it's not okay to add a new constraint to an existing table.
                        Err(AutoMigrateError::AddUniqueConstraint {
                            constraint: new.name.clone(),
                        }
                        .into())
                    }
                }
                Diff::Remove { old } => {
                    plan.steps.push(AutoMigrateStep::RemoveConstraint(old.key()));
                    Ok(())
                }
                Diff::MaybeChange { old, new } => {
                    if old == new {
                        Ok(())
                    } else {
                        Err(AutoMigrateError::ChangeUniqueConstraint {
                            constraint: old.name.clone(),
                        }
                        .into())
                    }
                }
            }
        })
        .collect_all_errors()
}

// Because we can refer to many tables and fields on the row level-security query, we need to remove all of them,
// then add the new ones, instead of trying to track the graph of dependencies.
fn auto_migrate_row_level_security(plan: &mut AutoMigratePlan) -> Result<()> {
    for rls in plan.old.row_level_security() {
        plan.steps.push(AutoMigrateStep::RemoveRowLevelSecurity(rls.key()));
    }
    for rls in plan.new.row_level_security() {
        plan.steps.push(AutoMigrateStep::AddRowLevelSecurity(rls.key()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::{
        db::raw_def::{v9::btree, *},
        AlgebraicType, AlgebraicValue, ProductType, ScheduleAt,
    };
    use spacetimedb_primitives::ColId;
    use v9::{RawModuleDefV9Builder, TableAccess};
    use validate::tests::expect_identifier;

    fn create_module_def(build_module: impl Fn(&mut RawModuleDefV9Builder)) -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        build_module(&mut builder);
        builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition")
    }

    fn initial_module_def() -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        let schedule_at = builder.add_type::<ScheduleAt>();
        let sum_ty = AlgebraicType::sum([("v1", AlgebraicType::U64)]);
        let sum_refty = builder.add_algebraic_type([], "sum", sum_ty, true);
        builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                    ("sum", sum_refty.into()),
                ]),
                true,
            )
            .with_column_sequence(0)
            .with_unique_constraint(ColId(0))
            .with_index(btree(0), "id_index")
            .with_index(btree([0, 1]), "id_name_index")
            .finish();

        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                ]),
                true,
            )
            .with_access(TableAccess::Public)
            .finish();

        let deliveries_type = builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at.clone()),
                    ("sum", AlgebraicType::array(sum_refty.into())),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0))
            .with_schedule("check_deliveries", 1)
            .finish();
        builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", AlgebraicType::Ref(deliveries_type))]),
            None,
        );

        // Add a view and add its return type to the typespace
        let view_return_ty = AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::U64)]);
        let view_return_ty_ref = builder.add_algebraic_type([], "my_view_return", view_return_ty, true);
        builder.add_view(
            "my_view",
            true,
            true,
            ProductType::from([("x", AlgebraicType::U32), ("y", AlgebraicType::U32)]),
            AlgebraicType::option(AlgebraicType::Ref(view_return_ty_ref)),
        );

        builder
            .build_table_with_new_type(
                "Inspections",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at.clone()),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0))
            .finish();

        builder.add_row_level_security("SELECT * FROM Apples");

        builder
            .finish()
            .try_into()
            .expect("old_def should be a valid database definition")
    }

    fn updated_module_def() -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        let _ = builder.add_type::<u32>(); // reposition ScheduleAt in the typespace, should have no effect.
        let schedule_at = builder.add_type::<ScheduleAt>();
        let sum_ty = AlgebraicType::sum([("v1", AlgebraicType::U64), ("v2", AlgebraicType::Bool)]);
        let sum_refty = builder.add_algebraic_type([], "sum", sum_ty, true);
        builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                    ("sum", sum_refty.into()),
                ]),
                true,
            )
            // remove sequence
            // remove unique constraint
            .with_index(btree(0), "id_index")
            // remove ["id", "name"] index
            // add ["id", "count"] index
            .with_index(btree([0, 2]), "id_count_index")
            .finish();

        builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                    ("freshness", AlgebraicType::U32), // added column!
                ]),
                true,
            )
            // add column sequence
            .with_column_sequence(0)
            .with_default_column_value(3, AlgebraicValue::U32(5))
            // change access
            .with_access(TableAccess::Private)
            .finish();

        let deliveries_type = builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at.clone()),
                    ("sum", AlgebraicType::array(sum_refty.into())),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0))
            // remove schedule def
            .finish();

        builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", AlgebraicType::Ref(deliveries_type))]),
            None,
        );

        // Add a view and add its return type to the typespace
        let view_return_ty = AlgebraicType::product([("a", AlgebraicType::U64)]);
        let view_return_ty_ref = builder.add_algebraic_type([], "my_view_return", view_return_ty, true);
        builder.add_view(
            "my_view",
            true,
            true,
            ProductType::from([("x", AlgebraicType::U32)]),
            AlgebraicType::option(AlgebraicType::Ref(view_return_ty_ref)),
        );

        let new_inspections_type = builder
            .build_table_with_new_type(
                "Inspections",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at.clone()),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0))
            // add schedule def
            .with_schedule("perform_inspection", 1)
            .finish();

        // add reducer.
        builder.add_reducer(
            "perform_inspection",
            ProductType::from([("a", AlgebraicType::Ref(new_inspections_type))]),
            None,
        );

        // Add new table
        builder
            .build_table_with_new_type("Oranges", ProductType::from([("id", AlgebraicType::U32)]), true)
            .with_index(btree(0), "id_index")
            .with_column_sequence(0)
            .with_unique_constraint(0)
            .with_primary_key(0)
            .finish();

        builder.add_row_level_security("SELECT * FROM Bananas");

        builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition")
    }

    #[test]
    fn successful_auto_migration() {
        let old_def = initial_module_def();
        let new_def = updated_module_def();
        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");

        let apples = expect_identifier("Apples");
        let bananas = expect_identifier("Bananas");
        let deliveries = expect_identifier("Deliveries");
        let oranges = expect_identifier("Oranges");
        let my_view = expect_identifier("my_view");

        let bananas_sequence = "Bananas_id_seq";
        let apples_unique_constraint = "Apples_id_key";
        let apples_sequence = "Apples_id_seq";
        let apples_id_name_index = "Apples_id_name_idx_btree";
        let apples_id_count_index = "Apples_id_count_idx_btree";
        let deliveries_schedule = "Deliveries_sched";
        let inspections_schedule = "Inspections_sched";

        assert!(plan.prechecks.is_sorted());

        assert_eq!(plan.prechecks.len(), 1);
        assert_eq!(
            plan.prechecks[0],
            AutoMigratePrecheck::CheckAddSequenceRangeValid(bananas_sequence)
        );
        let sql_old = RawRowLevelSecurityDefV9 {
            sql: "SELECT * FROM Apples".into(),
        };

        let sql_new = RawRowLevelSecurityDefV9 {
            sql: "SELECT * FROM Bananas".into(),
        };

        let steps = &plan.steps[..];

        assert!(steps.is_sorted());

        assert!(
            steps.contains(&AutoMigrateStep::RemoveSequence(apples_sequence)),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::RemoveConstraint(apples_unique_constraint)),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::RemoveIndex(apples_id_name_index)),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddIndex(apples_id_count_index)),
            "{steps:?}"
        );

        assert!(steps.contains(&AutoMigrateStep::ChangeAccess(&bananas)), "{steps:?}");
        assert!(
            steps.contains(&AutoMigrateStep::AddSequence(bananas_sequence)),
            "{steps:?}"
        );

        assert!(steps.contains(&AutoMigrateStep::AddTable(&oranges)), "{steps:?}");

        assert!(
            steps.contains(&AutoMigrateStep::RemoveSchedule(deliveries_schedule)),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddSchedule(inspections_schedule)),
            "{steps:?}"
        );

        assert!(
            steps.contains(&AutoMigrateStep::RemoveRowLevelSecurity(&sql_old.sql)),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddRowLevelSecurity(&sql_new.sql)),
            "{steps:?}"
        );

        assert!(steps.contains(&AutoMigrateStep::ChangeColumns(&apples)), "{steps:?}");
        assert!(
            steps.contains(&AutoMigrateStep::ChangeColumns(&deliveries)),
            "{steps:?}"
        );

        assert!(steps.contains(&AutoMigrateStep::DisconnectAllUsers), "{steps:?}");
        assert!(steps.contains(&AutoMigrateStep::AddColumns(&bananas)), "{steps:?}");
        // Column is changed but it will not reflect in steps due to `AutoMigrateStep::AddColumns`
        assert!(!steps.contains(&AutoMigrateStep::ChangeColumns(&bananas)), "{steps:?}");

        assert!(steps.contains(&AutoMigrateStep::RemoveView(&my_view)), "{steps:?}");
        assert!(steps.contains(&AutoMigrateStep::AddView(&my_view)), "{steps:?}");
    }

    #[test]
    fn auto_migration_errors() {
        let mut old_builder = RawModuleDefV9Builder::new();

        let foo2_ty = AlgebraicType::sum([
            ("foo21", AlgebraicType::Bool),
            ("foo22", AlgebraicType::U32),
            ("foo23", AlgebraicType::U32),
        ]);
        let foo2_refty = old_builder.add_algebraic_type([], "foo2", foo2_ty.clone(), true);
        let foo_ty = AlgebraicType::product([
            ("foo1", AlgebraicType::String),
            ("foo2", foo2_refty.into()),
            ("foo3", AlgebraicType::I32),
        ]);
        let foo_refty = old_builder.add_algebraic_type([], "foo", foo_ty.clone(), true);
        let sum1_ty = AlgebraicType::sum([
            ("foo", AlgebraicType::array(foo_refty.into())),
            ("bar", AlgebraicType::U128),
        ]);
        let sum1_refty = old_builder.add_algebraic_type([], "sum1", sum1_ty.clone(), true);

        let prod1_ty = AlgebraicType::product([
            ("baz", AlgebraicType::Bool),
            // We'll remove this field.
            ("qux", AlgebraicType::Bool),
        ]);
        let prod1_refty = old_builder.add_algebraic_type([], "prod1", prod1_ty.clone(), true);

        old_builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("sum1", sum1_refty.into()),
                    ("prod1", prod1_refty.into()),
                    ("count", AlgebraicType::U16),
                ]),
                true,
            )
            .with_index(btree(0), "id_index")
            .with_unique_constraint([1, 2])
            .with_index_no_accessor_name(btree([1, 2]))
            .with_type(TableType::User)
            .finish();

        old_builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                ]),
                true,
            )
            .finish();

        let old_def: ModuleDef = old_builder
            .finish()
            .try_into()
            .expect("old_def should be a valid database definition");
        let resolve_old = |ty| old_def.typespace().with_type(ty).resolve_refs().unwrap();

        let mut new_builder = RawModuleDefV9Builder::new();

        // Remove variant `foo23` and rename variant `foo21` to `bad`.
        let new_foo2_ty = AlgebraicType::sum([
            ("bad", AlgebraicType::Bool),
            // U32 -> U64
            ("foo22", AlgebraicType::U64),
        ]);
        let new_foo2_refty = new_builder.add_algebraic_type([], "foo2", new_foo2_ty.clone(), true);
        let new_foo_ty = AlgebraicType::product([
            // Remove field `foo3` and rename `foo1` to `bad`.
            ("bad", AlgebraicType::String),
            ("foo2", new_foo2_refty.into()),
        ]);
        let new_foo_refty = new_builder.add_algebraic_type([], "foo", new_foo_ty.clone(), true);
        let new_sum1_ty = AlgebraicType::sum([
            // Remove variant `bar` and rename `foo` to `bad`.
            ("bad", AlgebraicType::array(new_foo_refty.into())),
        ]);
        let new_sum1_refty = new_builder.add_algebraic_type([], "sum1", new_sum1_ty.clone(), true);

        let new_prod1_ty = AlgebraicType::product([
            // Removed field `qux` and renamed `baz` to `bad`.
            ("bad", AlgebraicType::Bool),
        ]);
        let new_prod1_refty = new_builder.add_algebraic_type([], "prod1", new_prod1_ty.clone(), true);

        new_builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("name", AlgebraicType::U32), // change type of `name`
                    ("id", AlgebraicType::U64),   // change order
                    ("sum1", new_sum1_refty.into()),
                    ("prod1", new_prod1_refty.into()),
                    // remove count
                    ("weight", AlgebraicType::U16), // add weight; we don't set a default, which makes this an error.
                ]),
                true,
            )
            .with_index(
                btree(1),
                "id_index_new_accessor", // change accessor name
            )
            .with_unique_constraint([1, 0])
            .with_index_no_accessor_name(btree([1, 0]))
            .with_unique_constraint(0)
            .with_index_no_accessor_name(btree(0)) // add unique constraint
            .with_type(TableType::System) // change type
            .finish();

        // Invalid row-level security queries can't be detected in the ponder_auto_migrate function, they
        // are detected when executing the plan because they depend on the database state.
        // new_builder.add_row_level_security("SELECT wrong");

        // remove Bananas
        let new_def: ModuleDef = new_builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition");
        let resolve_new = |ty| new_def.typespace().with_type(ty).resolve_refs().unwrap();

        let result = ponder_auto_migrate(&old_def, &new_def);

        let apples = expect_identifier("Apples");
        let bananas = expect_identifier("Bananas");

        let apples_name_unique_constraint = "Apples_name_key";

        let weight = expect_identifier("weight");
        let count = expect_identifier("count");
        let name = expect_identifier("name");
        let sum1 = expect_identifier("sum1");
        let prod1 = expect_identifier("prod1");

        expect_error_matching!(
            result,
            // This is an error because we didn't set a default value.
            AutoMigrateError::AddColumn {
                table,
                column
            } => table == &apples && column == &weight
        );

        expect_error_matching!(
            result,
            AutoMigrateError::RemoveColumn {
                table,
                column
            } => table == &apples && column == &count
        );

        expect_error_matching!(
            result,
            AutoMigrateError::ReorderTable { table } => table == &apples
        );

        expect_error_matching!(
            result,
            AutoMigrateError::ChangeColumnType(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &name && type1.0 == AlgebraicType::String && type2.0 == AlgebraicType::U32
        );

        // Rename variant `foo21`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeWithinColumnTypeRenamedVariant(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == foo2_ty && type2.0 == new_foo2_ty
        );

        // foo22: U32 -> U64.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeWithinColumnType(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == AlgebraicType::U32 && type2.0 == AlgebraicType::U64
        );

        // Remove variant `foo23`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeWithinColumnTypeFewerVariants(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == foo2_ty && type2.0 == new_foo2_ty
        );

        // Size of inner sum changed.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeWithinColumnTypeSizeMismatch(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == foo2_ty && type2.0 == new_foo2_ty
        );

        // Align of inner sum changed.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeWithinColumnTypeAlignMismatch(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == foo2_ty && type2.0 == new_foo2_ty
        );

        // Rename field `foo1`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeWithinColumnTypeRenamedField(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == resolve_old(&foo_ty) && type2.0 == resolve_new(&new_foo_ty)
        );

        // Remove field `foo3`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeWithinColumnTypeFewerFields(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == resolve_old(&foo_ty) && type2.0 == resolve_new(&new_foo_ty)
        );

        // Rename variant `bar`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeColumnTypeRenamedVariant(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == resolve_old(&sum1_ty) && type2.0 == resolve_new(&new_sum1_ty)
        );

        // Remove variant `bar`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeColumnTypeFewerVariants(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == resolve_old(&sum1_ty) && type2.0 == resolve_new(&new_sum1_ty)
        );

        // Size of outer sum changed.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeColumnTypeSizeMismatch(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == resolve_old(&sum1_ty) && type2.0 == resolve_new(&new_sum1_ty)
        );

        // Align of outer sum changed.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeColumnTypeAlignMismatch(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &sum1
            && type1.0 == resolve_old(&sum1_ty) && type2.0 == resolve_new(&new_sum1_ty)
        );

        // Rename field `baz`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeColumnTypeRenamedField(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &prod1
            && type1.0 == prod1_ty && type2.0 == new_prod1_ty
        );

        // Remove field `qux`.
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeColumnTypeFewerFields(ChangeColumnTypeParts {
                table,
                column,
                type1,
                type2
            }) => table == &apples && column == &prod1
            && type1.0 == prod1_ty && type2.0 == new_prod1_ty
        );

        expect_error_matching!(
            result,
            AutoMigrateError::AddUniqueConstraint { constraint } => &constraint[..] == apples_name_unique_constraint
        );

        expect_error_matching!(
            result,
            AutoMigrateError::ChangeTableType { table, type1, type2 } => table == &apples && type1 == &TableType::User && type2 == &TableType::System
        );

        expect_error_matching!(
            result,
            AutoMigrateError::RemoveTable { table } => table == &bananas
        );

        let apples_id_index = "Apples_id_idx_btree";
        let accessor_old = expect_identifier("id_index");
        let accessor_new = expect_identifier("id_index_new_accessor");
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeIndexAccessor {
                index,
                old_accessor,
                new_accessor
            } => &index[..] == apples_id_index && old_accessor.as_ref() == Some(&accessor_old) && new_accessor.as_ref() == Some(&accessor_new)
        );

        // It is not currently possible to test for `ChangeUniqueConstraint`, because unique constraint names are now generated during validation,
        // and are determined by their columns and table name. So it's impossible to create a unique constraint with the same name
        // but different columns from an old one.
        // We've left the check in, just in case this changes in the future.
    }
    #[test]
    fn print_empty_to_populated_schema_migration() {
        // Start with completely empty schema
        let old_builder = RawModuleDefV9Builder::new();
        let old_def: ModuleDef = old_builder
            .finish()
            .try_into()
            .expect("old_def should be a valid database definition");

        let new_def = initial_module_def();
        let plan = ponder_migrate(&old_def, &new_def).expect("auto migration should succeed");

        insta::assert_snapshot!(
            "empty_to_populated_migration",
            plan.pretty_print(PrettyPrintStyle::AnsiColor)
                .expect("should pretty print")
        );
    }

    #[test]
    fn print_supervised_migration() {
        let old_def = initial_module_def();
        let new_def = updated_module_def();
        let plan = ponder_migrate(&old_def, &new_def).expect("auto migration should succeed");

        insta::assert_snapshot!(
            "updated pretty print",
            plan.pretty_print(PrettyPrintStyle::AnsiColor)
                .expect("should pretty print")
        );
    }

    #[test]
    fn no_color_print_supervised_migration() {
        let old_def = initial_module_def();
        let new_def = updated_module_def();
        let plan = ponder_migrate(&old_def, &new_def).expect("auto migration should succeed");

        insta::assert_snapshot!(
            "updated pretty print no color",
            plan.pretty_print(PrettyPrintStyle::NoColor)
                .expect("should pretty print")
        );
    }

    #[test]
    fn add_view() {
        let old_def = create_module_def(|_| {});
        let new_def = create_module_def(|builder| {
            let return_type_ref = builder.add_algebraic_type(
                [],
                "my_view_return_type",
                AlgebraicType::product([("a", AlgebraicType::U64)]),
                true,
            );
            builder.add_view(
                "my_view",
                true,
                true,
                ProductType::from([("x", AlgebraicType::U32)]),
                AlgebraicType::array(AlgebraicType::Ref(return_type_ref)),
            );
        });

        let my_view = expect_identifier("my_view");

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        let steps = &plan.steps[..];

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(steps.contains(&AutoMigrateStep::AddView(&my_view)), "{steps:?}");
        assert!(!steps.contains(&AutoMigrateStep::RemoveView(&my_view)), "{steps:?}");
    }

    #[test]
    fn remove_view() {
        let old_def = create_module_def(|builder| {
            let return_type_ref = builder.add_algebraic_type(
                [],
                "my_view_return_type",
                AlgebraicType::product([("a", AlgebraicType::U64)]),
                true,
            );
            builder.add_view(
                "my_view",
                true,
                true,
                ProductType::from([("x", AlgebraicType::U32)]),
                AlgebraicType::array(AlgebraicType::Ref(return_type_ref)),
            );
        });
        let new_def = create_module_def(|_| {});

        let my_view = expect_identifier("my_view");

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        let steps = &plan.steps[..];

        assert!(plan.disconnects_all_users(), "{plan:#?}");
        assert!(steps.contains(&AutoMigrateStep::RemoveView(&my_view)), "{steps:?}");
        assert!(!steps.contains(&AutoMigrateStep::AddView(&my_view)), "{steps:?}");
    }

    #[test]
    fn migrate_view_recompute() {
        struct TestCase {
            desc: &'static str,
            old_def: ModuleDef,
            new_def: ModuleDef,
        }

        for TestCase {
            desc: name,
            old_def,
            new_def,
        } in [
            TestCase {
                desc: "Return `Vec<T>` instead of `Option<T>`",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::array(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "No change; recompute view",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
        ] {
            let my_view = expect_identifier("my_view");

            let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
            let steps = &plan.steps[..];

            assert!(!plan.disconnects_all_users(), "{name}, plan: {plan:#?}");

            assert!(
                steps.contains(&AutoMigrateStep::UpdateView(&my_view)),
                "{name}, steps: {steps:?}"
            );
            assert!(
                !steps.contains(&AutoMigrateStep::AddView(&my_view)),
                "{name}, steps: {steps:?}"
            );
            assert!(
                !steps.contains(&AutoMigrateStep::RemoveView(&my_view)),
                "{name}, steps: {steps:?}"
            );
        }
    }

    #[test]
    fn migrate_view_disconnect_clients() {
        struct TestCase {
            desc: &'static str,
            old_def: ModuleDef,
            new_def: ModuleDef,
        }

        for TestCase {
            desc: name,
            old_def,
            new_def,
        } in [
            TestCase {
                desc: "Change context parameter",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        false,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Add parameter",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32), ("y", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Remove parameter",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32), ("y", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Reorder parameters",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32), ("y", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("y", AlgebraicType::U32), ("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Change parameter type",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::String)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Add column",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Remove column",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Reorder columns",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64), ("b", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("b", AlgebraicType::U64), ("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
            TestCase {
                desc: "Change column type",
                old_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::U64)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
                new_def: create_module_def(|builder| {
                    let return_type_ref = builder.add_algebraic_type(
                        [],
                        "my_view_return_type",
                        AlgebraicType::product([("a", AlgebraicType::String)]),
                        true,
                    );
                    builder.add_view(
                        "my_view",
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
        ] {
            let my_view = expect_identifier("my_view");

            let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
            let steps = &plan.steps[..];

            assert!(plan.disconnects_all_users(), "{name}, plan: {plan:?}");

            assert!(
                steps.contains(&AutoMigrateStep::AddView(&my_view)),
                "{name}, steps: {steps:?}"
            );
            assert!(
                steps.contains(&AutoMigrateStep::RemoveView(&my_view)),
                "{name}, steps: {steps:?}"
            );
            assert!(
                !steps.contains(&AutoMigrateStep::UpdateView(&my_view)),
                "{name}, steps: {steps:?}"
            );
        }
    }
}
