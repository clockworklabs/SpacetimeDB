use core::{cmp::Ordering, ops::BitOr};

use crate::{
    def::*,
    error::PrettyAlgebraicType,
    identifier::{Identifier, NamespacedIdentifier},
};
use formatter::format_plan;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_data_structures::{
    error_stream::{CollectAllErrors, CombineErrors, ErrorStream},
    map::{HashCollectionExt as _, HashSet},
};
use spacetimedb_lib::{db::raw_def::v9::TableType, hash_bytes, Identity};
use spacetimedb_sats::{
    layout::{HasLayout, SumTypeLayout},
    raw_identifier::RawIdentifier,
    AlgebraicType, WithTypespace,
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
    pub prechecks: Vec<AutoMigratePrecheck>,
    /// The migration steps to perform.
    /// Order matters: `Remove`s of a particular `Def` must be ordered before `Add`s.
    pub steps: Vec<AutoMigrateStep>,
}

impl AutoMigratePlan<'_> {
    fn any_step(&self, f: impl Fn(&AutoMigrateStep) -> bool) -> bool {
        self.steps.iter().any(f)
    }

    fn disconnects_all_users(&self) -> bool {
        self.any_step(|step| matches!(step, AutoMigrateStep::DisconnectAllUsers))
    }

    /// Ensures that `DisconnectAllUsers` is present in the plan.
    /// If it's already there, this is a no-op.
    fn ensure_disconnect_all_users(&mut self) {
        if !self.disconnects_all_users() {
            self.steps.push(AutoMigrateStep::DisconnectAllUsers);
        }
    }
}

/// Checks that must be performed before performing an automatic migration.
/// These checks can access table contents and other database state.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum AutoMigratePrecheck {
    /// Perform a check that adding a sequence is valid (the relevant column contains no values
    /// greater than the sequence's start value).
    /// Payload is the full namespaced sequence name (e.g., `"lib.library_table_id_seq"`).
    CheckAddSequenceRangeValid(NamespacedIdentifier),
}

/// A step in an automatic migration.
///
/// All variant payloads are full namespaced names (e.g., `"lib.library_table"` for a submodule
/// table, or `"user"` for a root-level table). This allows submodule and root-level items to be
/// handled uniformly. Row-level security payloads are SQL text (`Box<str>`) rather than
/// identifiers.
///
/// IMPORTANT: Remove variants MUST be declared before Add variants in this enum. The ordering
/// is used to sort steps of an auto-migration so that removes precede adds for the same name.
#[derive(PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum AutoMigrateStep {
    /// Remove an index. Payload is the full namespaced index name.
    RemoveIndex(NamespacedIdentifier),
    /// Remove a constraint. Payload is the full namespaced constraint name.
    RemoveConstraint(NamespacedIdentifier),
    /// Remove a sequence. Payload is the full namespaced sequence name.
    RemoveSequence(NamespacedIdentifier),
    /// Remove a schedule annotation from a table. Payload is the full namespaced TABLE name.
    RemoveSchedule(NamespacedIdentifier),
    /// Remove a view and corresponding view table. Payload is the full namespaced view name.
    RemoveView(NamespacedIdentifier),
    /// Remove a row-level security query. Payload is the SQL text.
    RemoveRowLevelSecurity(Box<str>),

    /// Remove an empty table and all its sub-objects (indexes, constraints, sequences).
    /// Validated at execution time: fails if the table contains data.
    /// Payload is the full namespaced table name (e.g., `"lib.library_table"` or `"user"`).
    RemoveTable(NamespacedIdentifier),

    /// Change the column types of a table, in a layout compatible way.
    /// Payload is the full namespaced table name.
    ChangeColumns(NamespacedIdentifier),

    /// Change the column types of an event table, in a way that may not be layout-compatible.
    /// Payload is the full namespaced table name.
    ReschemaEventTable(NamespacedIdentifier),

    /// Add columns to a table, in a layout-INCOMPATIBLE way.
    ///
    /// This is a destructive operation that requires first running a `DisconnectAllUsers`.
    /// The added columns are guaranteed to be contiguous and at the end of the table.
    /// They are also guaranteed to have default values set.
    /// When this step is present, no `ChangeColumns` steps will be, for the same table.
    /// Payload is the full namespaced table name.
    AddColumns(NamespacedIdentifier),

    /// Add a table, including all indexes, constraints, and sequences.
    /// There will NOT be separate steps in the plan for adding indexes, constraints, and sequences.
    /// Payload is the full namespaced table name.
    AddTable(NamespacedIdentifier),
    /// Add an index. Payload is the full namespaced index name.
    AddIndex(NamespacedIdentifier),
    /// Add a constraint to an existing table (with data validation precheck).
    /// Payload is the full namespaced constraint name.
    AddConstraint(NamespacedIdentifier),
    /// Add a sequence. Payload is the full namespaced sequence name.
    AddSequence(NamespacedIdentifier),
    /// Add a schedule annotation to a table. Payload is the full namespaced TABLE name.
    AddSchedule(NamespacedIdentifier),
    /// Add a view and corresponding view table. Payload is the full namespaced view name.
    AddView(NamespacedIdentifier),
    /// Add a row-level security query. Payload is the SQL text.
    AddRowLevelSecurity(Box<str>),

    /// Change the access of a table or view. Payload is the full namespaced table/view name.
    ChangeAccess(NamespacedIdentifier),

    /// Change the primary key of a table.
    ///
    /// This updates the `table_primary_key` field in `st_table` to match the new module definition.
    /// Without this step, a stale primary key in the stored schema causes `check_compatible` to
    /// fail on the next publish. See: <https://github.com/clockworklabs/SpacetimeDB/issues/3934>
    /// Payload is the full namespaced table name.
    ChangePrimaryKey(NamespacedIdentifier),

    /// Recompute a view, update its backing table, and push updates to clients.
    /// Payload is the full namespaced view name.
    UpdateView(NamespacedIdentifier),

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
    AddUniqueConstraint { constraint: RawIdentifier },

    #[error("Changing a unique constraint {constraint} requires a manual migration")]
    ChangeUniqueConstraint { constraint: RawIdentifier },

    #[error("Changing the table type of table {table} from {type1:?} to {type2:?} requires a manual migration")]
    ChangeTableType {
        table: Identifier,
        type1: TableType,
        type2: TableType,
    },

    #[error("Changing the event flag of table {table} requires a manual migration")]
    ChangeTableEventFlag { table: Identifier },

    #[error(
        "Changing the accessor name on index {index} from {old_accessor:?} to {new_accessor:?} requires a manual migration"
    )]
    ChangeIndexAccessor {
        index: RawIdentifier,
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

    // Compute full-name sets for added/removed tables (across all submodules).
    // Sub-objects of added/removed tables are handled by AddTable/RemoveTable, not individually.
    //
    // Keyed by canonical names (prefix + table.name) to match how `auto_migrate_tables`
    // decides adds/removes. If these sets used accessor names while table matching used
    // canonical names, a table whose accessor changed (but whose canonical name did not)
    // would land in both sets and its index/sequence/constraint diffs would be silently
    // dropped without any AddTable/RemoveTable step to compensate.
    let old_module = plan.old;
    let new_module = plan.new;
    let old_table_names: HashSet<String> = old_module
        .all_tables_with_prefix()
        .into_iter()
        .map(|(prefix, _, t)| format!("{}{}", prefix, &*t.name))
        .collect();
    let new_table_names: HashSet<String> = new_module
        .all_tables_with_prefix()
        .into_iter()
        .map(|(prefix, _, t)| format!("{}{}", prefix, &*t.name))
        .collect();
    let added_tables: HashSet<String> = new_table_names.difference(&old_table_names).cloned().collect();
    let removed_tables: HashSet<String> = old_table_names.difference(&new_table_names).cloned().collect();

    let indexes_ok = auto_migrate_indexes(&mut plan, &added_tables, &removed_tables);
    let sequences_ok = auto_migrate_sequences(&mut plan, &added_tables, &removed_tables);
    let constraints_ok = auto_migrate_constraints(&mut plan, &added_tables, &removed_tables);
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

fn auto_migrate_views(plan: &mut AutoMigratePlan<'_>) -> Result<()> {
    let old_module = plan.old;
    let new_module = plan.new;

    // Build full-name maps for views across all submodules.
    let old_views: HashMap<NamespacedIdentifier, (&ModuleDef, &ViewDef)> = old_module
        .all_views_with_prefix()
        .into_iter()
        .map(|(prefix, owning, view)| (prefix.join(view.name.clone()), (owning, view)))
        .collect();
    let new_views: HashMap<NamespacedIdentifier, (&ModuleDef, &ViewDef)> = new_module
        .all_views_with_prefix()
        .into_iter()
        .map(|(prefix, owning, view)| (prefix.join(view.name.clone()), (owning, view)))
        .collect();

    type ViewEntry<'a> = (
        NamespacedIdentifier,
        (&'a ModuleDef, &'a ViewDef),
        (&'a ModuleDef, &'a ViewDef),
    );
    let mut maybe_change: Vec<ViewEntry<'_>> = vec![];
    for (full_name, old_entry) in &old_views {
        match new_views.get(full_name) {
            Some(new_entry) => maybe_change.push((full_name.clone(), *old_entry, *new_entry)),
            None => {
                plan.steps.push(AutoMigrateStep::RemoveView(full_name.clone()));
                plan.ensure_disconnect_all_users();
            }
        }
    }
    for full_name in new_views.keys() {
        if !old_views.contains_key(full_name) {
            plan.steps.push(AutoMigrateStep::AddView(full_name.clone()));
        }
    }

    let results: Vec<Result<()>> = maybe_change
        .into_iter()
        .map(|(full_name, (old_owning, old_view), (new_owning, new_view))| {
            auto_migrate_view(plan, full_name, old_owning, old_view, new_owning, new_view)
        })
        .collect();
    results.into_iter().collect_all_errors::<Vec<()>>().map(|_| ())
}

fn auto_migrate_view(
    plan: &mut AutoMigratePlan,
    full_name: NamespacedIdentifier,
    old_owning: &ModuleDef,
    old: &ViewDef,
    new_owning: &ModuleDef,
    new: &ViewDef,
) -> Result<()> {
    // We can always auto-migrate a view because we can always re-compute it.
    // However certain things require us to disconnect clients:
    // 1. If we add or remove a column or parameter
    // 2. If we change the order of the columns or parameters
    // 3. If we change the types of the columns or parameters
    // 4. If we change the context parameter
    let old_return_cols: HashMap<&Identifier, &ViewColumnDef> =
        old.return_columns.iter().map(|c| (&c.name, c)).collect();
    let new_return_cols: HashMap<&Identifier, &ViewColumnDef> =
        new.return_columns.iter().map(|c| (&c.name, c)).collect();

    let Any(incompatible_return_type) = old
        .return_columns
        .iter()
        .map(|old_col| match new_return_cols.get(&old_col.name) {
            None => Any(true),
            Some(new_col) => {
                if old_col.col_id != new_col.col_id {
                    return Any(true);
                }
                ensure_old_ty_upgradable_to_new(
                    false,
                    &|| old_col.view_name.clone(),
                    &|| old_col.name.clone(),
                    &WithTypespace::new(old_owning.typespace(), &old_col.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                    &WithTypespace::new(new_owning.typespace(), &new_col.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                )
                .unwrap_or(Any(true))
            }
        })
        .chain(new.return_columns.iter().map(|new_col| {
            if old_return_cols.contains_key(&new_col.name) {
                Any(false)
            } else {
                Any(true) // added column → incompatible
            }
        }))
        .collect();

    let old_param_cols: HashMap<&Identifier, &ViewParamDef> = old.param_columns.iter().map(|c| (&c.name, c)).collect();
    let new_param_cols: HashMap<&Identifier, &ViewParamDef> = new.param_columns.iter().map(|c| (&c.name, c)).collect();

    let Any(incompatible_param_types) = old
        .param_columns
        .iter()
        .map(|old_col| match new_param_cols.get(&old_col.name) {
            None => Any(true),
            Some(new_col) => {
                if old_col.col_id != new_col.col_id {
                    return Any(true);
                }
                ensure_old_ty_upgradable_to_new(
                    false,
                    &|| old_col.view_name.clone(),
                    &|| old_col.name.clone(),
                    &WithTypespace::new(old_owning.typespace(), &old_col.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                    &WithTypespace::new(new_owning.typespace(), &new_col.ty)
                        .resolve_refs()
                        .expect("valid ViewDefs must have valid type refs"),
                )
                .unwrap_or(Any(true))
            }
        })
        .chain(new.param_columns.iter().map(|new_col| {
            if old_param_cols.contains_key(&new_col.name) {
                Any(false)
            } else {
                Any(true)
            }
        }))
        .collect();

    if old.is_public != new.is_public {
        plan.steps.push(AutoMigrateStep::ChangeAccess(full_name.clone()));
    }

    if old.is_anonymous != new.is_anonymous
        || old.primary_key != new.primary_key
        || incompatible_return_type
        || incompatible_param_types
    {
        plan.steps.push(AutoMigrateStep::AddView(full_name.clone()));
        plan.steps.push(AutoMigrateStep::RemoveView(full_name));
        plan.ensure_disconnect_all_users();
    } else {
        plan.steps.push(AutoMigrateStep::UpdateView(full_name));
    }

    Ok(())
}

fn auto_migrate_tables(plan: &mut AutoMigratePlan<'_>) -> Result<()> {
    let old_module = plan.old;
    let new_module = plan.new;

    // Map canonical name (table_def.name, after case conversion) → (accessor full name, owning, table).
    // Matching by canonical name means tables like `Events` (accessor) / `events` (canonical) in
    // the old module and `events` (accessor + canonical) in the new module are treated as the same
    // logical table, preventing spurious Remove+Add steps when only the accessor casing changed.
    // Step payloads use accessor names (prefix + accessor_name) since that's what the DB stores.
    let old_tables: HashMap<String, (NamespacedIdentifier, &ModuleDef, &TableDef)> = old_module
        .all_tables_with_prefix()
        .into_iter()
        .map(|(prefix, owning, table)| {
            let canonical = format!("{}{}", prefix, &*table.name);
            let accessor = prefix.join(table.accessor_name.clone());
            (canonical, (accessor, owning, table))
        })
        .collect();
    let new_tables: HashMap<String, (NamespacedIdentifier, &ModuleDef, &TableDef)> = new_module
        .all_tables_with_prefix()
        .into_iter()
        .map(|(prefix, owning, table)| {
            let canonical = format!("{}{}", prefix, &*table.name);
            let accessor = prefix.join(table.accessor_name.clone());
            (canonical, (accessor, owning, table))
        })
        .collect();

    for (canonical, (accessor, _, _)) in &old_tables {
        if !new_tables.contains_key(canonical.as_str()) {
            plan.steps.push(AutoMigrateStep::RemoveTable(accessor.clone()));
            plan.ensure_disconnect_all_users();
        }
    }
    for (canonical, (accessor, _, _)) in &new_tables {
        if !old_tables.contains_key(canonical.as_str()) {
            plan.steps.push(AutoMigrateStep::AddTable(accessor.clone()));
        }
    }

    type TableEntry<'a> = (
        NamespacedIdentifier,
        (&'a ModuleDef, &'a TableDef),
        (&'a ModuleDef, &'a TableDef),
    );
    let maybe_change: Vec<TableEntry<'_>> = old_tables
        .iter()
        .filter_map(|(canonical, (_, old_owning, old_table))| {
            new_tables
                .get(canonical.as_str())
                .map(|(new_accessor, new_owning, new_table)| {
                    (
                        new_accessor.clone(),
                        (*old_owning, *old_table),
                        (*new_owning, *new_table),
                    )
                })
        })
        .collect();

    let results: Vec<Result<()>> = maybe_change
        .into_iter()
        .map(|(full_name, (old_owning, old_table), (new_owning, new_table))| {
            auto_migrate_table(plan, full_name, old_owning, old_table, new_owning, new_table)
        })
        .collect();
    results.into_iter().collect_all_errors::<Vec<()>>().map(|_| ())
}

fn auto_migrate_table(
    plan: &mut AutoMigratePlan,
    full_name: NamespacedIdentifier,
    old_owning: &ModuleDef,
    old: &TableDef,
    new_owning: &ModuleDef,
    new: &TableDef,
) -> Result<()> {
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
    let event_ok: Result<()> = if old.is_event == new.is_event {
        Ok(())
    } else {
        Err(AutoMigrateError::ChangeTableEventFlag {
            table: old.name.clone(),
        }
        .into())
    };

    // Combined with our validation of `event_ok`, `old.is_event` is sufficient to identify this as an event table.
    let is_event = old.is_event;

    if old.table_access != new.table_access {
        plan.steps.push(AutoMigrateStep::ChangeAccess(full_name.clone()));
    }
    if old.primary_key != new.primary_key {
        plan.steps.push(AutoMigrateStep::ChangePrimaryKey(full_name.clone()));
    }
    if old.schedule != new.schedule {
        // Schedule steps are keyed by full TABLE name (schedules are 1:1 with tables).
        if old.schedule.is_some() {
            plan.steps.push(AutoMigrateStep::RemoveSchedule(full_name.clone()));
        }
        if new.schedule.is_some() {
            plan.steps.push(AutoMigrateStep::AddSchedule(full_name.clone()));
        }
    }

    // Diff columns directly using the table defs (avoids root-only ModuleDefLookup).
    let new_col_by_name: HashMap<&Identifier, &ColumnDef> = new.columns.iter().map(|c| (&c.name, c)).collect();
    let old_col_by_name: HashMap<&Identifier, &ColumnDef> = old.columns.iter().map(|c| (&c.name, c)).collect();

    let columns_ok = old
        .columns
        .iter()
        .map(|old_col| -> Result<ArrayMonoid<Any, 3>> {
            match new_col_by_name.get(&old_col.name) {
                None => {
                    if is_event {
                        // Event tables never have any resident rows, so removing a column is not a
                        // data migration. However, changing the schema will break clients.
                        // `row_type_changed`, `columns_added`, `event_schema_changed`
                        Ok(ArrayMonoid([Any(false), Any(false), Any(true)]))
                    } else {
                        Err(AutoMigrateError::RemoveColumn {
                            table: old_col.table_name.clone(),
                            column: old_col.name.clone(),
                        }
                        .into())
                    }
                }
                Some(new_col) => {
                    let old_ty = WithTypespace::new(old_owning.typespace(), &old_col.ty)
                        .resolve_refs()
                        .expect("valid TableDef must have valid type refs");
                    let new_ty = WithTypespace::new(new_owning.typespace(), &new_col.ty)
                        .resolve_refs()
                        .expect("valid TableDef must have valid type refs");
                    let types_ok = ensure_old_ty_upgradable_to_new(
                        false,
                        &|| old_col.table_name.clone(),
                        &|| old_col.name.clone(),
                        &old_ty,
                        &new_ty,
                    )
                    .or_else(|err| {
                        if is_event {
                            // Event tables have no rows, so layout-incompatible type changes are fine.
                            Ok(Any(true))
                        } else {
                            Err(err)
                        }
                    });
                    // Reject reordering of existing columns (unless it's an event table).
                    let positions_ok = if old_col.col_id == new_col.col_id {
                        Ok(Any(false))
                    } else if is_event {
                        Ok(Any(true))
                    } else {
                        Err(AutoMigrateError::ReorderTable {
                            table: old_col.table_name.clone(),
                        }
                        .into())
                    };
                    (types_ok, positions_ok)
                        .combine_errors()
                        // `row_type_changed`, `columns_added`, `event_schema_changed`
                        .map(|(types_changed, positions_changed)| {
                            if is_event {
                                ArrayMonoid([Any(false), Any(false), types_changed | positions_changed])
                            } else {
                                assert!(!positions_changed.0);
                                ArrayMonoid([types_changed, Any(false), Any(false)])
                            }
                        })
                }
            }
        })
        .chain(new.columns.iter().map(|new_col| -> Result<ArrayMonoid<Any, 3>> {
            if old_col_by_name.contains_key(&new_col.name) {
                Ok(ArrayMonoid([Any(false), Any(false), Any(false)]))
            } else if is_event {
                // Event tables never have any resident rows, so adding a column is not a data
                // migration. However, changing the schema will break clients.
                // `row_type_changed`, `columns_added`, `event_schema_changed`
                Ok(ArrayMonoid([Any(false), Any(false), Any(true)]))
            } else if new_col.default_value.is_some() {
                // `row_type_changed`, `columns_added`, `event_schema_changed`
                Ok(ArrayMonoid([Any(false), Any(true), Any(false)]))
            } else {
                Err(AutoMigrateError::AddColumn {
                    table: new_col.table_name.clone(),
                    column: new_col.name.clone(),
                }
                .into())
            }
        }))
        .collect_all_errors::<ArrayMonoid<Any, 3>>();

    let ((), (), ArrayMonoid([Any(row_type_changed), Any(columns_added), Any(event_schema_changed)])) =
        (type_ok, event_ok, columns_ok).combine_errors()?;

    if event_schema_changed {
        // If we're rewriting an event table, there's no data migration to do.
        // But incompatibly changing the schema can break clients.
        plan.ensure_disconnect_all_users();
        plan.steps.push(AutoMigrateStep::ReschemaEventTable(full_name));
    } else if columns_added {
        // If we're adding a column, we'll rewrite the whole table.
        // That makes any `ChangeColumns` moot, so we can skip it.
        plan.ensure_disconnect_all_users();
        plan.steps.push(AutoMigrateStep::AddColumns(full_name));
    } else if row_type_changed {
        plan.steps.push(AutoMigrateStep::ChangeColumns(full_name));
    }

    Ok(())
}

/// An "any" monoid with `false` as identity and `|` as the operator.
#[derive(Default, Copy, Clone)]
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

/// A monoid that allows running a number of `Any`s in parallel.
struct ArrayMonoid<Monoid, const N: usize>([Monoid; N]);

impl<Monoid, const N: usize> Default for ArrayMonoid<Monoid, N>
where
    [Monoid; N]: Default,
{
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<Monoid: BitOr<Output = Monoid> + Copy, const N: usize> BitOr for ArrayMonoid<Monoid, N> {
    type Output = Self;

    fn bitor(mut self, rhs: Self) -> Self::Output {
        for n in 0..N {
            self.0[n] = self.0[n] | rhs.0[n]
        }
        self
    }
}

impl<Monoid: BitOr<Output = Monoid> + Copy, const N: usize> FromIterator<ArrayMonoid<Monoid, N>>
    for ArrayMonoid<Monoid, N>
where
    ArrayMonoid<Monoid, N>: Default,
{
    fn from_iter<T: IntoIterator<Item = ArrayMonoid<Monoid, N>>>(iter: T) -> Self {
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

/// Convert a def-level name to a validated `Identifier`.
/// Index/sequence/constraint names are validated as identifiers when the `ModuleDef` is built,
/// but stored as `RawIdentifier`.
fn def_ident(name: &RawIdentifier) -> Identifier {
    Identifier::new(name.clone()).expect("names in a validated ModuleDef are valid identifiers")
}

fn auto_migrate_indexes(
    plan: &mut AutoMigratePlan<'_>,
    new_tables: &HashSet<String>,
    removed_tables: &HashSet<String>,
) -> Result<()> {
    let old_module = plan.old;
    let new_module = plan.new;

    // key = full index name (e.g. "lib.library_table_id_idx_btree")
    // value = (full_table_name, &IndexDef)
    let old_indexes: HashMap<NamespacedIdentifier, (String, &IndexDef)> = {
        let mut map = HashMap::new();
        for (prefix, _, table) in old_module.all_tables_with_prefix() {
            // Canonical table name: must match the added/removed table sets computed in `ponder_auto_migrate`.
            let table_full = format!("{}{}", prefix, &*table.name);
            for idx in table.indexes.values() {
                map.insert(prefix.join(def_ident(&idx.name)), (table_full.clone(), idx));
            }
        }
        map
    };
    let new_indexes: HashMap<NamespacedIdentifier, (String, &IndexDef)> = {
        let mut map = HashMap::new();
        for (prefix, _, table) in new_module.all_tables_with_prefix() {
            // Canonical table name: must match the added/removed table sets computed in `ponder_auto_migrate`.
            let table_full = format!("{}{}", prefix, &*table.name);
            for idx in table.indexes.values() {
                map.insert(prefix.join(def_ident(&idx.name)), (table_full.clone(), idx));
            }
        }
        map
    };

    // Removed indexes: in old but not in new, and not part of a removed table.
    for (full_idx_name, (table_full_name, _)) in &old_indexes {
        if !new_indexes.contains_key(full_idx_name) && !removed_tables.contains(table_full_name) {
            plan.steps.push(AutoMigrateStep::RemoveIndex(full_idx_name.clone()));
        }
    }

    // Added indexes: in new but not in old, and not part of a newly added table.
    for (full_idx_name, (table_full_name, _)) in &new_indexes {
        if !old_indexes.contains_key(full_idx_name) && !new_tables.contains(table_full_name) {
            plan.steps.push(AutoMigrateStep::AddIndex(full_idx_name.clone()));
        }
    }

    // Changed indexes: same name in both.
    let change_results: Vec<Result<()>> = old_indexes
        .iter()
        .filter_map(|(full_idx_name, (_, old_idx))| {
            new_indexes
                .get(full_idx_name)
                .map(|(_, new_idx)| (full_idx_name, old_idx, new_idx))
        })
        .map(|(full_idx_name, old_idx, new_idx)| {
            if old_idx.accessor_name != new_idx.accessor_name {
                Err(AutoMigrateError::ChangeIndexAccessor {
                    index: old_idx.name.clone(),
                    old_accessor: old_idx.accessor_name.clone(),
                    new_accessor: new_idx.accessor_name.clone(),
                }
                .into())
            } else {
                if old_idx.algorithm != new_idx.algorithm {
                    plan.steps.push(AutoMigrateStep::RemoveIndex(full_idx_name.clone()));
                    plan.steps.push(AutoMigrateStep::AddIndex(full_idx_name.clone()));
                }
                Ok(())
            }
        })
        .collect();
    change_results.into_iter().collect_all_errors::<Vec<()>>().map(|_| ())
}

fn auto_migrate_sequences(
    plan: &mut AutoMigratePlan,
    new_tables: &HashSet<String>,
    removed_tables: &HashSet<String>,
) -> Result<()> {
    let old_module = plan.old;
    let new_module = plan.new;

    // key = full sequence name (e.g. "lib.Bananas_id_seq")
    // value = (full_table_name, &SequenceDef)
    let old_seqs: HashMap<NamespacedIdentifier, (String, &SequenceDef)> = {
        let mut map = HashMap::new();
        for (prefix, _, table) in old_module.all_tables_with_prefix() {
            // Canonical table name: must match the added/removed table sets computed in `ponder_auto_migrate`.
            let table_full = format!("{}{}", prefix, &*table.name);
            for seq in table.sequences.values() {
                map.insert(prefix.join(def_ident(&seq.name)), (table_full.clone(), seq));
            }
        }
        map
    };
    let new_seqs: HashMap<NamespacedIdentifier, (String, &SequenceDef)> = {
        let mut map = HashMap::new();
        for (prefix, _, table) in new_module.all_tables_with_prefix() {
            // Canonical table name: must match the added/removed table sets computed in `ponder_auto_migrate`.
            let table_full = format!("{}{}", prefix, &*table.name);
            for seq in table.sequences.values() {
                map.insert(prefix.join(def_ident(&seq.name)), (table_full.clone(), seq));
            }
        }
        map
    };

    // Removed sequences: in old but not in new, and not part of a removed table.
    for (full_seq_name, (table_full_name, _)) in &old_seqs {
        if !new_seqs.contains_key(full_seq_name) && !removed_tables.contains(table_full_name) {
            plan.steps.push(AutoMigrateStep::RemoveSequence(full_seq_name.clone()));
        }
    }

    // Added or changed sequences.
    for (full_seq_name, (table_full_name, new_seq)) in &new_seqs {
        if let Some((_, old_seq)) = old_seqs.get(full_seq_name) {
            // we do not need to check column ids, since in an automigrate, column ids are not changed.
            if *old_seq != *new_seq {
                plan.prechecks
                    .push(AutoMigratePrecheck::CheckAddSequenceRangeValid(full_seq_name.clone()));
                plan.steps.push(AutoMigrateStep::RemoveSequence(full_seq_name.clone()));
                plan.steps.push(AutoMigrateStep::AddSequence(full_seq_name.clone()));
            }
        } else if !new_tables.contains(table_full_name) {
            plan.prechecks
                .push(AutoMigratePrecheck::CheckAddSequenceRangeValid(full_seq_name.clone()));
            plan.steps.push(AutoMigrateStep::AddSequence(full_seq_name.clone()));
        }
    }

    Ok(())
}

fn auto_migrate_constraints(
    plan: &mut AutoMigratePlan,
    new_tables: &HashSet<String>,
    removed_tables: &HashSet<String>,
) -> Result<()> {
    let old_module = plan.old;
    let new_module = plan.new;

    // key = full constraint name (e.g. "lib.Apples_id_key")
    // value = (full_table_name, &ConstraintDef)
    let old_constraints: HashMap<NamespacedIdentifier, (String, &ConstraintDef)> = {
        let mut map = HashMap::new();
        for (prefix, _, table) in old_module.all_tables_with_prefix() {
            // Canonical table name: must match the added/removed table sets computed in `ponder_auto_migrate`.
            let table_full = format!("{}{}", prefix, &*table.name);
            for constraint in table.constraints.values() {
                map.insert(
                    prefix.join(def_ident(&constraint.name)),
                    (table_full.clone(), constraint),
                );
            }
        }
        map
    };
    let new_constraints: HashMap<NamespacedIdentifier, (String, &ConstraintDef)> = {
        let mut map = HashMap::new();
        for (prefix, _, table) in new_module.all_tables_with_prefix() {
            // Canonical table name: must match the added/removed table sets computed in `ponder_auto_migrate`.
            let table_full = format!("{}{}", prefix, &*table.name);
            for constraint in table.constraints.values() {
                map.insert(
                    prefix.join(def_ident(&constraint.name)),
                    (table_full.clone(), constraint),
                );
            }
        }
        map
    };

    let mut results: Vec<Result<()>> = vec![];

    // Added constraints.
    for (full_constraint_name, (table_full_name, _new_constraint)) in &new_constraints {
        if !old_constraints.contains_key(full_constraint_name) && !new_tables.contains(table_full_name) {
            // existing table — duplicate detection happens inside create_constraint
            plan.steps
                .push(AutoMigrateStep::AddConstraint(full_constraint_name.clone()));
            // it's okay to add a constraint in a new table — AddTable covers it.
        }
    }

    // Removed constraints: not part of a removed table.
    for (full_constraint_name, (table_full_name, _)) in &old_constraints {
        if !new_constraints.contains_key(full_constraint_name) && !removed_tables.contains(table_full_name) {
            plan.steps
                .push(AutoMigrateStep::RemoveConstraint(full_constraint_name.clone()));
        }
    }

    // Changed constraints.
    for (full_constraint_name, (_, new_constraint)) in &new_constraints {
        if let Some((_, old_constraint)) = old_constraints.get(full_constraint_name)
            && *old_constraint != *new_constraint
        {
            results.push(Err(AutoMigrateError::ChangeUniqueConstraint {
                constraint: old_constraint.name.clone(),
            }
            .into()));
        }
    }

    results.into_iter().collect_all_errors::<Vec<()>>().map(|_| ())
}

// Because we can refer to many tables and fields on the row level-security query, we need to remove all of them,
// then add the new ones, instead of trying to track the graph of dependencies.
fn auto_migrate_row_level_security(plan: &mut AutoMigratePlan) -> Result<()> {
    let old_sqls: Vec<Box<str>> = plan.old.row_level_security().map(|rls| rls.sql.clone()).collect();
    let new_sqls: Vec<Box<str>> = plan.new.row_level_security().map(|rls| rls.sql.clone()).collect();

    let changed = {
        let old_set: HashSet<&str> = old_sqls.iter().map(|s| &**s).collect();
        let new_set: HashSet<&str> = new_sqls.iter().map(|s| &**s).collect();
        old_set != new_set
    };

    for sql in old_sqls {
        plan.steps.push(AutoMigrateStep::RemoveRowLevelSecurity(sql));
    }
    for sql in new_sqls {
        plan.steps.push(AutoMigrateStep::AddRowLevelSecurity(sql));
    }

    // We can force flush the cache by force disconnecting all clients if an RLS rule has been added, removed, or updated.
    if changed {
        plan.ensure_disconnect_all_users();
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
    use v10::{ExplicitNames, RawModuleDefV10Builder, RawModuleDefV10Section, RawSubmoduleV10};
    use v9::{RawModuleDefV9Builder, TableAccess};
    use validate::tests::expect_identifier;

    /// Test helper: build a `NamespacedIdentifier` from a dotted string.
    fn namespaced_ident(s: &str) -> NamespacedIdentifier {
        s.split('.')
            .map(|seg| Identifier::new(RawIdentifier::new(seg)).expect("test identifiers are valid"))
            .collect()
    }

    fn create_module_def(build_module: impl Fn(&mut RawModuleDefV9Builder)) -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        build_module(&mut builder);
        builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition")
    }

    fn create_module_def_v10(build_module: impl Fn(&mut RawModuleDefV10Builder)) -> ModuleDef {
        let mut builder = RawModuleDefV10Builder::new();
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
            0,
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
            0,
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

        assert!(plan.prechecks.is_sorted());

        assert_eq!(plan.prechecks.len(), 1);
        assert_eq!(
            plan.prechecks[0],
            AutoMigratePrecheck::CheckAddSequenceRangeValid(namespaced_ident("Bananas_id_seq"))
        );

        let steps = &plan.steps[..];

        assert!(steps.is_sorted());

        assert!(
            steps.contains(&AutoMigrateStep::RemoveSequence(namespaced_ident("Apples_id_seq"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::RemoveConstraint(namespaced_ident("Apples_id_key"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::RemoveIndex(namespaced_ident(
                "Apples_id_name_idx_btree"
            ))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddIndex(namespaced_ident(
                "Apples_id_count_idx_btree"
            ))),
            "{steps:?}"
        );

        assert!(
            steps.contains(&AutoMigrateStep::ChangeAccess(namespaced_ident("Bananas"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddSequence(namespaced_ident("Bananas_id_seq"))),
            "{steps:?}"
        );

        assert!(
            steps.contains(&AutoMigrateStep::AddTable(namespaced_ident("Oranges"))),
            "{steps:?}"
        );

        // Schedule steps are keyed by TABLE name (schedules are 1:1 with tables).
        assert!(
            steps.contains(&AutoMigrateStep::RemoveSchedule(namespaced_ident("Deliveries"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddSchedule(namespaced_ident("Inspections"))),
            "{steps:?}"
        );

        assert!(
            steps.contains(&AutoMigrateStep::RemoveRowLevelSecurity(Box::from(
                "SELECT * FROM Apples"
            ))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddRowLevelSecurity(Box::from(
                "SELECT * FROM Bananas"
            ))),
            "{steps:?}"
        );

        assert!(
            steps.contains(&AutoMigrateStep::ChangeColumns(namespaced_ident("Apples"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::ChangeColumns(namespaced_ident("Deliveries"))),
            "{steps:?}"
        );

        assert!(steps.contains(&AutoMigrateStep::DisconnectAllUsers), "{steps:?}");
        assert!(
            steps.contains(&AutoMigrateStep::AddColumns(namespaced_ident("Bananas"))),
            "{steps:?}"
        );
        // Column is changed but it will not reflect in steps due to `AutoMigrateStep::AddColumns`
        assert!(
            !steps.contains(&AutoMigrateStep::ChangeColumns(namespaced_ident("Bananas"))),
            "{steps:?}"
        );

        assert!(
            steps.contains(&AutoMigrateStep::RemoveView(namespaced_ident("my_view"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddView(namespaced_ident("my_view"))),
            "{steps:?}"
        );
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
        let _bananas = expect_identifier("Bananas");

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

        // Note: `AddUniqueConstraint` is no longer an error — adding unique constraints
        // to existing tables is now allowed; duplicate detection happens inside create_constraint.

        expect_error_matching!(
            result,
            AutoMigrateError::ChangeTableType { table, type1, type2 } => table == &apples && type1 == &TableType::User && type2 == &TableType::System
        );

        // Note: RemoveTable is no longer an error — removing tables is now allowed
        // for empty tables; the emptiness check happens at execution time in update.rs.

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
                0,
                true,
                true,
                ProductType::from([("x", AlgebraicType::U32)]),
                AlgebraicType::array(AlgebraicType::Ref(return_type_ref)),
            );
        });

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        let steps = &plan.steps[..];

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::AddView(namespaced_ident("my_view"))),
            "{steps:?}"
        );
        assert!(
            !steps.contains(&AutoMigrateStep::RemoveView(namespaced_ident("my_view"))),
            "{steps:?}"
        );
    }

    #[test]
    fn change_view_visibility_is_a_cheap_access_change() {
        let view_module = |is_public: bool| {
            create_module_def(move |builder| {
                let return_type_ref = builder.add_algebraic_type(
                    [],
                    "my_view_return_type",
                    AlgebraicType::product([("a", AlgebraicType::U64)]),
                    true,
                );
                builder.add_view(
                    "my_view",
                    0,
                    is_public,
                    true,
                    ProductType::from([("x", AlgebraicType::U32)]),
                    AlgebraicType::array(AlgebraicType::Ref(return_type_ref)),
                );
            })
        };

        let old_def = view_module(false);
        let new_def = view_module(true);

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        let steps = &plan.steps[..];
        let my_view = namespaced_ident("my_view");

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::ChangeAccess(my_view.clone())),
            "{steps:?}"
        );
        assert!(!steps.contains(&AutoMigrateStep::AddView(my_view.clone())), "{steps:?}");
        assert!(!steps.contains(&AutoMigrateStep::RemoveView(my_view)), "{steps:?}");
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
                0,
                true,
                true,
                ProductType::from([("x", AlgebraicType::U32)]),
                AlgebraicType::array(AlgebraicType::Ref(return_type_ref)),
            );
        });
        let new_def = create_module_def(|_| {});

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        let steps = &plan.steps[..];

        assert!(plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::RemoveView(namespaced_ident("my_view"))),
            "{steps:?}"
        );
        assert!(
            !steps.contains(&AutoMigrateStep::AddView(namespaced_ident("my_view"))),
            "{steps:?}"
        );
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
                        0,
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
                        0,
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
                        0,
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
                        0,
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
        ] {
            let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
            let steps = &plan.steps[..];

            assert!(!plan.disconnects_all_users(), "{name}, plan: {plan:#?}");

            assert!(
                steps.contains(&AutoMigrateStep::UpdateView(namespaced_ident("my_view"))),
                "{name}, steps: {steps:?}"
            );
            assert!(
                !steps.contains(&AutoMigrateStep::AddView(namespaced_ident("my_view"))),
                "{name}, steps: {steps:?}"
            );
            assert!(
                !steps.contains(&AutoMigrateStep::RemoveView(namespaced_ident("my_view"))),
                "{name}, steps: {steps:?}"
            );
        }
    }

    #[test]
    fn migrate_view_with_explicit_name() {
        fn module_def() -> ModuleDef {
            create_module_def_v10(|builder| {
                let return_type_ref = builder.add_algebraic_type(
                    [],
                    "Person",
                    AlgebraicType::product([("PersonId", AlgebraicType::U64)]),
                    true,
                );
                builder.add_view(
                    "PersonAtLevel2",
                    0,
                    true,
                    true,
                    ProductType::from([("Level", AlgebraicType::U32)]),
                    AlgebraicType::array(AlgebraicType::Ref(return_type_ref)),
                );

                let mut explicit = ExplicitNames::default();
                explicit.insert_function("PersonAtLevel2", "Level2Person");
                builder.add_explicit_names(explicit);
            })
        }

        let old_def = module_def();
        let new_def = module_def();
        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        let steps = &plan.steps[..];

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::UpdateView(namespaced_ident("Level2Person"))),
            "steps: {steps:?}"
        );
        assert!(
            !steps.contains(&AutoMigrateStep::AddView(namespaced_ident("Level2Person"))),
            "steps: {steps:?}"
        );
        assert!(
            !steps.contains(&AutoMigrateStep::RemoveView(namespaced_ident("Level2Person"))),
            "steps: {steps:?}"
        );
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
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
                        0,
                        true,
                        true,
                        ProductType::from([("x", AlgebraicType::U32)]),
                        AlgebraicType::option(AlgebraicType::Ref(return_type_ref)),
                    );
                }),
            },
        ] {
            let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
            let steps = &plan.steps[..];

            assert!(plan.disconnects_all_users(), "{name}, plan: {plan:?}");

            assert!(
                steps.contains(&AutoMigrateStep::AddView(namespaced_ident("my_view"))),
                "{name}, steps: {steps:?}"
            );
            assert!(
                steps.contains(&AutoMigrateStep::RemoveView(namespaced_ident("my_view"))),
                "{name}, steps: {steps:?}"
            );
            assert!(
                !steps.contains(&AutoMigrateStep::UpdateView(namespaced_ident("my_view"))),
                "{name}, steps: {steps:?}"
            );
        }
    }

    #[test]
    fn change_rls_disconnect_clients() {
        let old_def = create_module_def(|_builder| {});

        let new_def = create_module_def(|_builder| {});

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        assert!(!plan.disconnects_all_users(), "{plan:#?}");

        let old_def = create_module_def(|builder| {
            builder.add_row_level_security("SELECT true;");
        });
        let new_def = create_module_def(|builder| {
            builder.add_row_level_security("SELECT false;");
        });

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        assert!(plan.disconnects_all_users(), "{plan:#?}");

        let old_def = create_module_def(|builder| {
            builder.add_row_level_security("SELECT true;");
        });

        let new_def = create_module_def(|_builder| {
            // Remove RLS
        });
        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        assert!(plan.disconnects_all_users(), "{plan:#?}");

        let old_def = create_module_def(|_builder| {});

        let new_def = create_module_def(|builder| {
            builder.add_row_level_security("SELECT false;");
        });
        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        assert!(plan.disconnects_all_users(), "{plan:#?}");

        let old_def = create_module_def(|builder| {
            builder.add_row_level_security("SELECT true;");
        });

        let new_def = create_module_def(|builder| {
            builder.add_row_level_security("SELECT true;");
        });
        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");
        assert!(!plan.disconnects_all_users(), "{plan:#?}");
    }

    fn create_v10_module_def(build_module: impl Fn(&mut v10::RawModuleDefV10Builder)) -> ModuleDef {
        let mut builder = v10::RawModuleDefV10Builder::new();
        build_module(&mut builder);
        builder
            .finish()
            .try_into()
            .expect("should be a valid module definition")
    }

    #[test]
    fn test_change_event_flag_rejected() {
        // non-event → event
        let old = create_v10_module_def(|builder| {
            builder
                .build_table_with_new_type("Events", ProductType::from([("id", AlgebraicType::U64)]), true)
                .finish();
        });
        let new = create_v10_module_def(|builder| {
            builder
                .build_table_with_new_type("events", ProductType::from([("id", AlgebraicType::U64)]), true)
                .with_event(true)
                .finish();
        });

        let result = ponder_auto_migrate(&old, &new);
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeTableEventFlag { table } => &table[..] == "events"
        );

        // event → non-event (reverse direction)
        let result = ponder_auto_migrate(&new, &old);
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeTableEventFlag { table } => &table[..] == "events"
        );
    }

    #[test]
    fn test_same_event_flag_accepted() {
        // Both event → no error
        let old = create_v10_module_def(|builder| {
            builder
                .build_table_with_new_type("Events", ProductType::from([("id", AlgebraicType::U64)]), true)
                .with_event(true)
                .finish();
        });
        let new = create_v10_module_def(|builder| {
            builder
                .build_table_with_new_type("Events", ProductType::from([("id", AlgebraicType::U64)]), true)
                .with_event(true)
                .finish();
        });

        ponder_auto_migrate(&old, &new).expect("same event flag should succeed");
    }

    #[test]
    fn remove_table_produces_step() {
        let old = create_module_def(|builder| {
            builder
                .build_table_with_new_type("Keep", ProductType::from([("id", AlgebraicType::U64)]), true)
                .with_access(TableAccess::Public)
                .finish();
            builder
                .build_table_with_new_type("Drop", ProductType::from([("id", AlgebraicType::U64)]), true)
                .with_access(TableAccess::Public)
                .finish();
        });
        let new = create_module_def(|builder| {
            builder
                .build_table_with_new_type("Keep", ProductType::from([("id", AlgebraicType::U64)]), true)
                .with_access(TableAccess::Public)
                .finish();
        });

        let plan = ponder_auto_migrate(&old, &new).expect("removing a table should produce a valid plan");
        assert_eq!(
            plan.steps,
            &[
                AutoMigrateStep::RemoveTable(namespaced_ident("Drop")),
                AutoMigrateStep::DisconnectAllUsers,
            ],
        );
    }

    #[test]
    fn remove_table_does_not_produce_orphan_sub_object_steps() {
        let old = create_module_def(|builder| {
            builder
                .build_table_with_new_type("Drop", ProductType::from([("id", AlgebraicType::U64)]), true)
                .with_unique_constraint(0)
                .with_index(btree(0), "Drop_id_idx")
                .with_access(TableAccess::Public)
                .finish();
        });
        let new = create_module_def(|_builder| {});

        let plan = ponder_auto_migrate(&old, &new).expect("removing a table should produce a valid plan");
        assert_eq!(
            plan.steps,
            &[
                AutoMigrateStep::RemoveTable(namespaced_ident("Drop")),
                AutoMigrateStep::DisconnectAllUsers,
            ],
            "plan should only contain RemoveTable + DisconnectAllUsers, no orphan sub-object steps"
        );
    }

    fn make_submodule(namespace: &str, build: impl Fn(&mut RawModuleDefV10Builder)) -> RawSubmoduleV10 {
        let mut builder = RawModuleDefV10Builder::new();
        build(&mut builder);
        RawSubmoduleV10 {
            namespace: namespace.to_string(),
            module: builder.finish(),
        }
    }

    fn create_module_def_with_submodules(
        build_root: impl Fn(&mut RawModuleDefV10Builder),
        submodules: Vec<RawSubmoduleV10>,
    ) -> ModuleDef {
        let mut builder = RawModuleDefV10Builder::new();
        build_root(&mut builder);
        let mut raw = builder.finish();
        if !submodules.is_empty() {
            raw.sections.push(RawModuleDefV10Section::Submodules(submodules));
        }
        raw.try_into().expect("should be a valid module definition")
    }

    #[test]
    fn submodule_table_unchanged() {
        let submodule = || {
            make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })
        };
        let old = create_module_def_with_submodules(|_| {}, vec![submodule()]);
        let new = create_module_def_with_submodules(|_| {}, vec![submodule()]);

        let plan = ponder_auto_migrate(&old, &new).expect("no-op migration should succeed");
        let namespaced: Vec<_> = plan
            .steps
            .iter()
            .filter(|s| format!("{s:?}").contains("lib.sessions"))
            .collect();
        assert!(
            namespaced.is_empty(),
            "unchanged submodule should produce no steps for lib.sessions: {plan:#?}"
        );
    }

    #[test]
    fn submodule_add_table() {
        let old = create_module_def_with_submodules(
            |_| {},
            vec![make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })],
        );
        let new = create_module_def_with_submodules(
            |_| {},
            vec![make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
                b.build_table_with_new_type("tokens", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })],
        );

        let plan = ponder_auto_migrate(&old, &new).expect("adding a submodule table should succeed");
        let steps = &plan.steps[..];

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::AddTable(namespaced_ident("lib.tokens"))),
            "{steps:?}"
        );
    }

    #[test]
    fn submodule_remove_table() {
        let old = create_module_def_with_submodules(
            |_| {},
            vec![make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
                b.build_table_with_new_type("tokens", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })],
        );
        let new = create_module_def_with_submodules(
            |_| {},
            vec![make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })],
        );

        let plan = ponder_auto_migrate(&old, &new).expect("removing a submodule table should succeed");
        let steps = &plan.steps[..];

        assert!(plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::RemoveTable(namespaced_ident("lib.tokens"))),
            "{steps:?}"
        );
    }

    #[test]
    fn submodule_add_index() {
        let sessions_without_index = || {
            make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })
        };
        let sessions_with_index = || {
            make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .with_index(btree(0), "sessions_id_idx", "sessions_id_idx")
                    .finish();
            })
        };

        let old = create_module_def_with_submodules(|_| {}, vec![sessions_without_index()]);
        let new = create_module_def_with_submodules(|_| {}, vec![sessions_with_index()]);

        let plan = ponder_auto_migrate(&old, &new).expect("adding a submodule index should succeed");
        let steps = &plan.steps[..];

        assert!(
            steps.contains(&AutoMigrateStep::AddIndex(namespaced_ident(
                "lib.sessions_id_idx_btree"
            ))),
            "{steps:?}"
        );
    }

    #[test]
    fn submodule_remove_index() {
        let sessions_without_index = || {
            make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })
        };
        let sessions_with_index = || {
            make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .with_index(btree(0), "sessions_id_idx", "sessions_id_idx")
                    .finish();
            })
        };

        let old = create_module_def_with_submodules(|_| {}, vec![sessions_with_index()]);
        let new = create_module_def_with_submodules(|_| {}, vec![sessions_without_index()]);

        let plan = ponder_auto_migrate(&old, &new).expect("removing a submodule index should succeed");
        let steps = &plan.steps[..];

        assert!(
            steps.contains(&AutoMigrateStep::RemoveIndex(namespaced_ident(
                "lib.sessions_id_idx_btree"
            ))),
            "{steps:?}"
        );
    }

    #[test]
    fn submodule_add_sequence() {
        let without_seq = || {
            make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })
        };
        let with_seq = || {
            make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .with_column_sequence(0)
                    .finish();
            })
        };

        let old = create_module_def_with_submodules(|_| {}, vec![without_seq()]);
        let new = create_module_def_with_submodules(|_| {}, vec![with_seq()]);

        let plan = ponder_auto_migrate(&old, &new).expect("adding a submodule sequence should succeed");
        let steps = &plan.steps[..];

        assert!(
            steps.iter().any(|s| matches!(
                s,
                AutoMigrateStep::AddSequence(n) if n.starts_with("lib.sessions")
            )),
            "expected AddSequence for lib.sessions_*: {steps:?}"
        );
        assert!(
            plan.prechecks.iter().any(|p| matches!(
                p,
                AutoMigratePrecheck::CheckAddSequenceRangeValid(n) if n.starts_with("lib.sessions")
            )),
            "expected CheckAddSequenceRangeValid precheck: {:?}",
            plan.prechecks
        );
    }

    #[test]
    fn submodule_add_view() {
        let without_view = || make_submodule("lib", |_| {});
        let with_view = || {
            make_submodule("lib", |b| {
                let ret_ref = b.add_algebraic_type(
                    [],
                    "lib_view_return",
                    AlgebraicType::product([("a", AlgebraicType::U64)]),
                    true,
                );
                b.add_view(
                    "lib_view",
                    0,
                    true,
                    true,
                    ProductType::from([("x", AlgebraicType::U32)]),
                    AlgebraicType::array(AlgebraicType::Ref(ret_ref)),
                );
            })
        };

        let old = create_module_def_with_submodules(|_| {}, vec![without_view()]);
        let new = create_module_def_with_submodules(|_| {}, vec![with_view()]);

        let plan = ponder_auto_migrate(&old, &new).expect("adding a submodule view should succeed");
        let steps = &plan.steps[..];

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::AddView(namespaced_ident("lib.lib_view"))),
            "{steps:?}"
        );
    }

    #[test]
    fn submodule_remove_view() {
        let without_view = || make_submodule("lib", |_| {});
        let with_view = || {
            make_submodule("lib", |b| {
                let ret_ref = b.add_algebraic_type(
                    [],
                    "lib_view_return",
                    AlgebraicType::product([("a", AlgebraicType::U64)]),
                    true,
                );
                b.add_view(
                    "lib_view",
                    0,
                    true,
                    true,
                    ProductType::from([("x", AlgebraicType::U32)]),
                    AlgebraicType::array(AlgebraicType::Ref(ret_ref)),
                );
            })
        };

        let old = create_module_def_with_submodules(|_| {}, vec![with_view()]);
        let new = create_module_def_with_submodules(|_| {}, vec![without_view()]);

        let plan = ponder_auto_migrate(&old, &new).expect("removing a submodule view should succeed");
        let steps = &plan.steps[..];

        assert!(plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::RemoveView(namespaced_ident("lib.lib_view"))),
            "{steps:?}"
        );
    }

    #[test]
    fn add_whole_submodule() {
        let old = create_module_def_with_submodules(|_| {}, vec![]);
        let new = create_module_def_with_submodules(
            |_| {},
            vec![make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
                b.build_table_with_new_type("tokens", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })],
        );

        let plan = ponder_auto_migrate(&old, &new).expect("adding a whole submodule should succeed");
        let steps = &plan.steps[..];

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::AddTable(namespaced_ident("lib.sessions"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::AddTable(namespaced_ident("lib.tokens"))),
            "{steps:?}"
        );
    }

    #[test]
    fn remove_whole_submodule() {
        let old = create_module_def_with_submodules(
            |_| {},
            vec![make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
                b.build_table_with_new_type("tokens", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
            })],
        );
        let new = create_module_def_with_submodules(|_| {}, vec![]);

        let plan = ponder_auto_migrate(&old, &new).expect("removing a whole submodule should succeed");
        let steps = &plan.steps[..];

        assert!(plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::RemoveTable(namespaced_ident("lib.sessions"))),
            "{steps:?}"
        );
        assert!(
            steps.contains(&AutoMigrateStep::RemoveTable(namespaced_ident("lib.tokens"))),
            "{steps:?}"
        );
    }

    #[test]
    fn nested_submodule_add_table() {
        let make_nested_def = |add_baz_items: bool| {
            let baz_submodule = make_submodule("baz", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .finish();
                if add_baz_items {
                    b.build_table_with_new_type("baz_items", ProductType::from([("id", AlgebraicType::U32)]), true)
                        .finish();
                }
            });

            let mut auth_builder = RawModuleDefV10Builder::new();
            auth_builder
                .build_table_with_new_type("auth_users", ProductType::from([("id", AlgebraicType::U64)]), true)
                .finish();
            let mut auth_raw = auth_builder.finish();
            auth_raw
                .sections
                .push(RawModuleDefV10Section::Submodules(vec![baz_submodule]));

            let auth_submodule = RawSubmoduleV10 {
                namespace: "auth".to_string(),
                module: auth_raw,
            };

            let root_builder = RawModuleDefV10Builder::new();
            let mut root_raw = root_builder.finish();
            root_raw
                .sections
                .push(RawModuleDefV10Section::Submodules(vec![auth_submodule]));
            root_raw.try_into().expect("should be a valid module definition")
        };

        let old: ModuleDef = make_nested_def(false);
        let new: ModuleDef = make_nested_def(true);

        let plan = ponder_auto_migrate(&old, &new).expect("adding a deeply nested table should succeed");
        let steps = &plan.steps[..];

        assert!(!plan.disconnects_all_users(), "{plan:#?}");
        assert!(
            steps.contains(&AutoMigrateStep::AddTable(namespaced_ident("auth.baz.baz_items"))),
            "{steps:?}"
        );
    }

    #[test]
    fn submodule_remove_table_no_orphan_sub_objects() {
        let old = create_module_def_with_submodules(
            |_| {},
            vec![make_submodule("lib", |b| {
                b.build_table_with_new_type("sessions", ProductType::from([("id", AlgebraicType::U64)]), true)
                    .with_primary_key(0)
                    .with_unique_constraint(0)
                    .with_index(btree(0), "sessions_id_idx", "sessions_id_idx")
                    .finish();
            })],
        );
        let new = create_module_def_with_submodules(|_| {}, vec![make_submodule("lib", |_| {})]);

        let plan = ponder_auto_migrate(&old, &new).expect("removing a submodule table with sub-objects should succeed");
        assert_eq!(
            plan.steps,
            &[
                AutoMigrateStep::RemoveTable(namespaced_ident("lib.sessions")),
                AutoMigrateStep::DisconnectAllUsers,
            ],
            "should only contain RemoveTable + DisconnectAllUsers, no orphan sub-object steps"
        );
    }
}
