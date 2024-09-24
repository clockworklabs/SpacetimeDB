use crate::{def::*, error::PrettyAlgebraicType, identifier::Identifier};
use spacetimedb_data_structures::error_stream::{CollectAllErrors, CombineErrors, ErrorStream};
use spacetimedb_lib::db::raw_def::v9::TableType;
use spacetimedb_sats::WithTypespace;

pub type Result<T> = std::result::Result<T, ErrorStream<AutoMigrateError>>;

/// A plan for an automatic migration.
#[derive(Debug)]
pub struct AutoMigratePlan<'def> {
    /// The old database definition.
    pub old: &'def ModuleDef,
    /// The new database definition.
    pub new: &'def ModuleDef,
    /// The checks to perform before the automatic migration.
    pub prechecks: Vec<AutoMigratePrecheck<'def>>,
    /// The migration steps to perform.
    /// Order should not matter, as the steps are independent.
    pub steps: Vec<AutoMigrateStep<'def>>,
}

/// Checks that must be performed before performing an automatic migration.
/// These checks can access table contents and other database state.
#[derive(PartialEq, Eq, Debug)]
pub enum AutoMigratePrecheck<'def> {
    /// Perform a check that adding a sequence is valid (the relevant column contains no values
    /// greater than the sequence's start value).
    CheckAddSequenceRangeValid(<SequenceDef as ModuleDefLookup>::Key<'def>),
}

/// A step in an automatic migration.
#[derive(PartialEq, Eq, Debug)]
pub enum AutoMigrateStep<'def> {
    /// Add a table, including all indexes, constraints, and sequences.
    /// There will NOT be separate steps in the plan for adding indexes, constraints, and sequences.
    AddTable(<TableDef as ModuleDefLookup>::Key<'def>),
    /// Add an index.
    AddIndex(<IndexDef as ModuleDefLookup>::Key<'def>),
    /// Remove an index.
    RemoveIndex(<IndexDef as ModuleDefLookup>::Key<'def>),
    /// Remove a constraint.
    RemoveConstraint(<ConstraintDef as ModuleDefLookup>::Key<'def>),
    /// Add a sequence.
    AddSequence(<SequenceDef as ModuleDefLookup>::Key<'def>),
    /// Remove a sequence.
    RemoveSequence(<SequenceDef as ModuleDefLookup>::Key<'def>),
    /// Change the access of a table.
    ChangeAccess(<TableDef as ModuleDefLookup>::Key<'def>),
    /// Add a schedule annotation to a table.
    AddSchedule(<ScheduleDef as ModuleDefLookup>::Key<'def>),
    /// Remove a schedule annotation from a table.
    RemoveSchedule(<ScheduleDef as ModuleDefLookup>::Key<'def>),
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
    AddUniqueConstraint { constraint: Identifier },

    #[error("Changing a unique constraint {constraint} requires a manual migration")]
    ChangeUniqueConstraint { constraint: Identifier },

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
        index: Identifier,
        old_accessor: Option<Identifier>,
        new_accessor: Option<Identifier>,
    },
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
    let indexes_ok = auto_migrate_indexes(&mut plan);
    let sequences_ok = auto_migrate_sequences(&mut plan);
    let constraints_ok = auto_migrate_constraints(&mut plan);

    let ((), (), (), ()) = (tables_ok, indexes_ok, sequences_ok, constraints_ok).combine_errors()?;

    Ok(plan)
}

/// A diff between two items.
/// `Add` means the item is present in the new `ModuleDef` but not the old.
/// `Remove` means the item is present in the old `ModuleDef` but not the new.
/// `MaybeChange` indicates the item is present in both.
enum Diff<'def, T> {
    Add { new: &'def T },
    Remove { old: &'def T },
    MaybeChange { old: &'def T, new: &'def T },
}

/// Diff a collection of items, looking them up in both the old and new `ModuleDef` by their `ModuleDefLookup::Key`.
/// Keys are required to be stable across migrations, which mak
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

fn auto_migrate_indexes(plan: &mut AutoMigratePlan<'_>) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::indexes)
        .map(|index_diff| -> Result<()> {
            match index_diff {
                Diff::Add { new } => {
                    plan.steps.push(AutoMigrateStep::AddIndex(new.key()));
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

fn auto_migrate_sequences(plan: &mut AutoMigratePlan) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::sequences)
        .map(|sequence_diff| -> Result<()> {
            match sequence_diff {
                Diff::Add { new } => {
                    plan.prechecks
                        .push(AutoMigratePrecheck::CheckAddSequenceRangeValid(new.key()));
                    plan.steps.push(AutoMigrateStep::AddSequence(new.key()));
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

fn auto_migrate_constraints(plan: &mut AutoMigratePlan) -> Result<()> {
    diff(plan.old, plan.new, ModuleDef::constraints)
        .map(|constraint_diff| -> Result<()> {
            match constraint_diff {
                Diff::Add { new } => Err(AutoMigrateError::AddUniqueConstraint {
                    constraint: new.name.clone(),
                }
                .into()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_data_structures::expect_error_matching;
    use spacetimedb_lib::{db::raw_def::*, AlgebraicType, ProductType, ScheduleAt};
    use spacetimedb_primitives::ColList;
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
            .with_column_sequence(0, Some("Apples_sequence".into()))
            .with_unique_constraint(0.into(), Some("Apples_unique_constraint".into()))
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0]),
                },
                "id_index".into(),
                Some("Apples_id_index".into()),
            )
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0, 1]),
                },
                "id_name_index".into(),
                Some("Apples_id_name_index".into()),
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
            .with_schedule("check_deliveries", None)
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
            .finish();

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
                "id_index".into(),
                Some("Apples_id_index".into()),
            )
            // remove ["id", "name"] index
            // add ["id", "count"] index
            .with_index(
                RawIndexAlgorithm::BTree {
                    columns: ColList::from([0, 2]),
                },
                "id_count_index".into(),
                Some("Apples_id_count_index".into()),
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
            .with_column_sequence(0, Some("Bananas_sequence".into()))
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
            // add schedule def
            .with_schedule("perform_inspection", Some("Inspections_schedule".into()))
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
            .finish();

        let new_def: ModuleDef = new_builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition");

        let plan = ponder_auto_migrate(&old_def, &new_def).expect("auto migration should succeed");

        let bananas = expect_identifier("Bananas");
        let oranges = expect_identifier("Oranges");

        let bananas_sequence = expect_identifier("Bananas_sequence");
        let apples_unique_constraint = expect_identifier("Apples_unique_constraint");
        let apples_sequence = expect_identifier("Apples_sequence");
        let apples_id_name_index = expect_identifier("Apples_id_name_index");
        let apples_id_count_index = expect_identifier("Apples_id_count_index");
        let deliveries_schedule = expect_identifier("Deliveries_schedule");
        let inspections_schedule = expect_identifier("Inspections_schedule");

        assert_eq!(plan.prechecks.len(), 1);
        assert_eq!(
            plan.prechecks[0],
            AutoMigratePrecheck::CheckAddSequenceRangeValid(&bananas_sequence)
        );

        assert!(plan.steps.contains(&AutoMigrateStep::RemoveSequence(&apples_sequence)));
        assert!(plan
            .steps
            .contains(&AutoMigrateStep::RemoveConstraint(&apples_unique_constraint)));
        assert!(plan
            .steps
            .contains(&AutoMigrateStep::RemoveIndex(&apples_id_name_index)));
        assert!(plan.steps.contains(&AutoMigrateStep::AddIndex(&apples_id_count_index)));

        assert!(plan.steps.contains(&AutoMigrateStep::ChangeAccess(&bananas)));
        assert!(plan.steps.contains(&AutoMigrateStep::AddSequence(&bananas_sequence)));

        assert!(plan.steps.contains(&AutoMigrateStep::AddTable(&oranges)));

        assert!(plan
            .steps
            .contains(&AutoMigrateStep::RemoveSchedule(&deliveries_schedule)));
        assert!(plan
            .steps
            .contains(&AutoMigrateStep::AddSchedule(&inspections_schedule)));
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
                "id_index".into(),
                Some("Apples_id_index".into()),
            )
            .with_unique_constraint(ColList::from_iter([1, 2]), Some("Apples_changing_constraint".into()))
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
                    columns: ColList::from([0]),
                },
                "id_index_new_accessor".into(), // change accessor name
                Some("Apples_id_index".into()),
            )
            .with_unique_constraint(ColList::from_iter([0, 1]), Some("Apples_changing_constraint".into()))
            .with_unique_constraint(0.into(), Some("Apples_name_unique_constraint".into())) // add unique constraint
            .with_type(TableType::System) // change type
            .finish();

        // remove Bananas
        let new_def: ModuleDef = new_builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition");

        let result = ponder_auto_migrate(&old_def, &new_def);

        let apples = expect_identifier("Apples");
        let bananas = expect_identifier("Bananas");

        let apples_name_unique_constraint = expect_identifier("Apples_name_unique_constraint");
        let apples_changing_constraint = expect_identifier("Apples_changing_constraint");

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
            AutoMigrateError::AddUniqueConstraint { constraint } => constraint == &apples_name_unique_constraint
        );

        expect_error_matching!(
            result,
            AutoMigrateError::ChangeTableType { table, type1, type2 } => table == &apples && type1 == &TableType::User && type2 == &TableType::System
        );

        expect_error_matching!(
            result,
            AutoMigrateError::RemoveTable { table } => table == &bananas
        );

        let apples_id_index = expect_identifier("Apples_id_index");
        let accessor_old = expect_identifier("id_index");
        let accessor_new = expect_identifier("id_index_new_accessor");
        expect_error_matching!(
            result,
            AutoMigrateError::ChangeIndexAccessor {
                index,
                old_accessor,
                new_accessor
            } => index == &apples_id_index && old_accessor.as_ref() == Some(&accessor_old) && new_accessor.as_ref() == Some(&accessor_new)
        );

        expect_error_matching!(
            result,
            AutoMigrateError::ChangeUniqueConstraint { constraint } => constraint == &apples_changing_constraint
        );
    }
}
