use crate::{def::*, error::PrettyAlgebraicType, identifier::Identifier};
use spacetimedb_data_structures::{
    error_stream::{CollectAllErrors, CombineErrors, ErrorStream},
    map::HashSet,
};
use spacetimedb_lib::db::raw_def::v9::{RawRowLevelSecurityDefV9, TableType};
use spacetimedb_sats::WithTypespace;

pub type Result<T> = std::result::Result<T, ErrorStream<AutoMigrateError>>;

/// A plan for a migration.
#[derive(Debug)]
pub enum MigratePlan<'def> {
    Manual(ManualMigratePlan<'def>),
    Auto(AutoMigratePlan<'def>),
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
    /// Remove a row-level security query.
    RemoveRowLevelSecurity(<RawRowLevelSecurityDefV9 as ModuleDefLookup>::Key<'def>),

    /// Add a table, including all indexes, constraints, and sequences.
    /// There will NOT be separate steps in the plan for adding indexes, constraints, and sequences.
    AddTable(<TableDef as ModuleDefLookup>::Key<'def>),
    /// Add an index.
    AddIndex(<IndexDef as ModuleDefLookup>::Key<'def>),
    /// Add a sequence.
    AddSequence(<SequenceDef as ModuleDefLookup>::Key<'def>),
    /// Add a schedule annotation to a table.
    AddSchedule(<ScheduleDef as ModuleDefLookup>::Key<'def>),
    /// Add a row-level security query.
    AddRowLevelSecurity(<RawRowLevelSecurityDefV9 as ModuleDefLookup>::Key<'def>),

    /// Change the access of a table.
    ChangeAccess(<TableDef as ModuleDefLookup>::Key<'def>),
}

/// Something that might prevent an automatic migration.
#[derive(thiserror::Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AutoMigrateError {
    #[error("Adding a column {column} to table {table} requires a manual migration")]
    AddColumn { table: Identifier, column: Identifier },

    #[error("Removing a column {column} from table {table} requires a manual migration")]
    RemoveColumn { table: Identifier, column: Identifier },

    #[error("Reordering table {table} requires a manual migration")]
    ReorderTable { table: Identifier },

    #[error(
        "Changing the type of column {column} in table {table} from {type1:?} to {type2:?} requires a manual migration"
    )]
    ChangeColumnType {
        table: Identifier,
        column: Identifier,
        type1: PrettyAlgebraicType,
        type2: PrettyAlgebraicType,
    },

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

    let ((), (), (), (), ()) = (tables_ok, indexes_ok, sequences_ok, constraints_ok, rls_ok).combine_errors()?;

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

    let columns_ok: Result<()> = diff(plan.old, plan.new, |def| {
        def.lookup_expect::<TableDef>(key).columns.iter()
    })
    .map(|col_diff| -> Result<()> {
        match col_diff {
            Diff::Add { new } => Err(AutoMigrateError::AddColumn {
                table: new.table_name.clone(),
                column: new.name.clone(),
            }
            .into()),
            Diff::Remove { old } => Err(AutoMigrateError::RemoveColumn {
                table: old.table_name.clone(),
                column: old.name.clone(),
            }
            .into()),
            Diff::MaybeChange { old, new } => {
                let old_ty = WithTypespace::new(plan.old.typespace(), &old.ty)
                    .resolve_refs()
                    .expect("valid TableDef must have valid type refs");
                let new_ty = WithTypespace::new(plan.new.typespace(), &new.ty)
                    .resolve_refs()
                    .expect("valid TableDef must have valid type refs");
                let types_ok = if old_ty == new_ty {
                    Ok(())
                } else {
                    Err(AutoMigrateError::ChangeColumnType {
                        table: old.table_name.clone(),
                        column: old.name.clone(),
                        type1: old_ty.clone().into(),
                        type2: new_ty.clone().into(),
                    }
                    .into())
                };
                // Note that the diff algorithm relies on `ModuleDefLookup` for `ColumnDef`,
                // which looks up columns by NAME, NOT position: precisely to allow this step to work!
                let positions_ok = if old.col_id == new.col_id {
                    Ok(())
                } else {
                    Err(AutoMigrateError::ReorderTable {
                        table: old.table_name.clone(),
                    }
                    .into())
                };
                let ((), ()) = (types_ok, positions_ok).combine_errors()?;
                Ok(())
            }
        }
    })
    .collect_all_errors();

    let ((), ()) = (type_ok, columns_ok).combine_errors()?;
    Ok(())
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
    use spacetimedb_lib::{db::raw_def::*, is_sorted, AlgebraicType, ProductType, ScheduleAt};
    use spacetimedb_primitives::{ColId, ColList};
    use v9::{RawIndexAlgorithm, RawModuleDefV9Builder, TableAccess};
    use validate::tests::expect_identifier;

    #[test]
    fn successful_auto_migration() {
        let mut old_builder = RawModuleDefV9Builder::new();
        let old_schedule_at = old_builder.add_type::<ScheduleAt>();
        old_builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                ]),
                true,
            )
            .with_column_sequence(0)
            .with_unique_constraint(ColId(0))
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0]),
                },
                "id_index",
            )
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0, 1]),
                },
                "id_name_index",
            )
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
            .with_access(TableAccess::Public)
            .finish();

        let old_deliveries_type = old_builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", old_schedule_at.clone()),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_schedule("check_deliveries", 1)
            .finish();
        old_builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", AlgebraicType::Ref(old_deliveries_type))]),
            None,
        );

        old_builder
            .build_table_with_new_type(
                "Inspections",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", old_schedule_at.clone()),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .finish();

        old_builder.add_row_level_security("SELECT * FROM Apples");

        let old_def: ModuleDef = old_builder
            .finish()
            .try_into()
            .expect("old_def should be a valid database definition");

        let mut new_builder = RawModuleDefV9Builder::new();
        let _ = new_builder.add_type::<u32>(); // reposition ScheduleAt in the typespace, should have no effect.
        let new_schedule_at = new_builder.add_type::<ScheduleAt>();
        new_builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                ]),
                true,
            )
            // remove sequence
            // remove unique constraint
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0]),
                },
                "id_index",
            )
            // remove ["id", "name"] index
            // add ["id", "count"] index
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0, 2]),
                },
                "id_count_index",
            )
            .finish();

        new_builder
            .build_table_with_new_type(
                "Bananas",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                ]),
                true,
            )
            // add column sequence
            .with_column_sequence(0)
            // change access
            .with_access(TableAccess::Private)
            .finish();

        let new_deliveries_type = new_builder
            .build_table_with_new_type(
                "Deliveries",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", new_schedule_at.clone()),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            // remove schedule def
            .finish();

        new_builder.add_reducer(
            "check_deliveries",
            ProductType::from([("a", AlgebraicType::Ref(new_deliveries_type))]),
            None,
        );

        let new_inspections_type = new_builder
            .build_table_with_new_type(
                "Inspections",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", new_schedule_at.clone()),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            // add schedule def
            .with_schedule("perform_inspection", 1)
            .finish();

        // add reducer.
        new_builder.add_reducer(
            "perform_inspection",
            ProductType::from([("a", AlgebraicType::Ref(new_inspections_type))]),
            None,
        );

        // Add new table
        new_builder
            .build_table_with_new_type("Oranges", ProductType::from([("id", AlgebraicType::U32)]), true)
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0]),
                },
                "id_index",
            )
            .with_column_sequence(0)
            .with_unique_constraint(0)
            .with_primary_key(0)
            .finish();

        new_builder.add_row_level_security("SELECT * FROM Bananas");

        let new_def: ModuleDef = new_builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition");

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");

        let bananas = expect_identifier("Bananas");
        let oranges = expect_identifier("Oranges");

        let bananas_sequence = "Bananas_id_seq";
        let apples_unique_constraint = "Apples_id_key";
        let apples_sequence = "Apples_id_seq";
        let apples_id_name_index = "Apples_id_name_idx_btree";
        let apples_id_count_index = "Apples_id_count_idx_btree";
        let deliveries_schedule = "Deliveries_sched";
        let inspections_schedule = "Inspections_sched";

        assert!(is_sorted(plan.prechecks.iter()));

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

        assert!(is_sorted(steps.iter()));

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
    }

    #[test]
    fn auto_migration_errors() {
        let mut old_builder = RawModuleDefV9Builder::new();

        old_builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U16),
                ]),
                true,
            )
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0]),
                },
                "id_index",
            )
            .with_unique_constraint(ColList::from_iter([1, 2]))
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

        let mut new_builder = RawModuleDefV9Builder::new();

        new_builder
            .build_table_with_new_type(
                "Apples",
                ProductType::from([
                    ("name", AlgebraicType::U32), // change type of `name`
                    ("id", AlgebraicType::U64),   // change order
                    // remove count
                    ("weight", AlgebraicType::U16), // add weight
                ]),
                true,
            )
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([1]),
                },
                "id_index_new_accessor", // change accessor name
            )
            .with_unique_constraint(ColList::from_iter([1, 0]))
            .with_unique_constraint(ColId(0)) // add unique constraint
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

        let result = ponder_auto_migrate(&old_def, &new_def);

        let apples = expect_identifier("Apples");
        let bananas = expect_identifier("Bananas");

        let apples_name_unique_constraint = "Apples_name_key";

        let weight = expect_identifier("weight");
        let count = expect_identifier("count");
        let name = expect_identifier("name");

        expect_error_matching!(
            result,
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
            AutoMigrateError::ChangeColumnType {
                table,
                column,
                type1,
                type2
            } => table == &apples && column == &name && type1.0 == AlgebraicType::String && type2.0 == AlgebraicType::U32
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
}
