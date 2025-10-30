//! This module provides enhanced functionality for rendering automatic migration plans to strings.

use std::io;

use super::{AutoMigratePlan, IndexAlgorithm, ModuleDefLookup, TableDef};
use crate::{
    auto_migrate::AutoMigrateStep,
    def::{ConstraintData, FunctionKind, ModuleDef, ScheduleDef, ViewDef},
    identifier::Identifier,
};
use itertools::Itertools;
use spacetimedb_lib::{
    db::raw_def::v9::{RawRowLevelSecurityDefV9, TableAccess, TableType},
    AlgebraicType, AlgebraicValue,
};
use spacetimedb_sats::WithTypespace;
use thiserror::Error;

pub fn format_plan<F: MigrationFormatter>(f: &mut F, plan: &AutoMigratePlan) -> Result<(), FormattingErrors> {
    f.format_header()?;

    for step in &plan.steps {
        format_step(f, step, plan)?;
    }

    Ok(())
}

fn format_step<F: MigrationFormatter>(
    f: &mut F,
    step: &AutoMigrateStep,
    plan: &super::AutoMigratePlan,
) -> Result<(), FormattingErrors> {
    match step {
        AutoMigrateStep::AddView(view) => {
            let view_info = extract_view_info(*view, plan.new)?;
            f.format_view(&view_info, Action::Created)
        }
        AutoMigrateStep::RemoveView(view) => {
            let view_info = extract_view_info(*view, plan.old)?;
            f.format_view(&view_info, Action::Removed)
        }
        // This means the body of the view may have been updated.
        // So we must recompute it and send any updates to clients.
        // No need to include this step in the formatted plan.
        AutoMigrateStep::UpdateView(_) => Ok(()),
        AutoMigrateStep::AddTable(t) => {
            let table_info = extract_table_info(*t, plan)?;
            f.format_add_table(&table_info)
        }
        AutoMigrateStep::AddIndex(index) => {
            let index_info = extract_index_info(*index, plan.new)?;
            f.format_index(&index_info, Action::Created)
        }
        AutoMigrateStep::RemoveIndex(index) => {
            let index_info = extract_index_info(*index, plan.old)?;
            f.format_index(&index_info, Action::Removed)
        }
        AutoMigrateStep::RemoveConstraint(constraint) => {
            let constraint_info = extract_constraint_info(*constraint, plan.old)?;
            f.format_constraint(&constraint_info, Action::Removed)
        }
        AutoMigrateStep::AddSequence(sequence) => {
            let sequence_info = extract_sequence_info(*sequence, plan.new)?;
            f.format_sequence(&sequence_info, Action::Created)
        }
        AutoMigrateStep::RemoveSequence(sequence) => {
            let sequence_info = extract_sequence_info(*sequence, plan.old)?;
            f.format_sequence(&sequence_info, Action::Removed)
        }
        AutoMigrateStep::ChangeAccess(table) => {
            let access_info = extract_access_change_info(*table, plan)?;
            f.format_change_access(&access_info)
        }
        AutoMigrateStep::AddSchedule(schedule) => {
            let schedule_info = extract_schedule_info(*schedule, plan.new)?;
            f.format_schedule(&schedule_info, Action::Created)
        }
        AutoMigrateStep::RemoveSchedule(schedule) => {
            let schedule_info = extract_schedule_info(*schedule, plan.old)?;
            f.format_schedule(&schedule_info, Action::Removed)
        }
        AutoMigrateStep::AddRowLevelSecurity(rls) => {
            if let Some(rls_info) = extract_rls_info(*rls, plan)? {
                f.format_rls(&rls_info, Action::Created)?;
            }
            Ok(())
        }
        AutoMigrateStep::RemoveRowLevelSecurity(rls) => {
            if let Some(rls_info) = extract_rls_info(*rls, plan)? {
                f.format_rls(&rls_info, Action::Removed)?;
            }
            Ok(())
        }
        AutoMigrateStep::ChangeColumns(table) => {
            let column_changes = extract_column_changes(*table, plan)?;
            f.format_change_columns(&column_changes)
        }
        AutoMigrateStep::AddColumns(table) => {
            let new_columns = extract_new_columns(*table, plan)?;
            f.format_add_columns(&new_columns)
        }
        AutoMigrateStep::DisconnectAllUsers => f.format_disconnect_warning(),
    }?;

    Ok(())
}

#[derive(Error, Debug)]
pub enum FormattingErrors {
    #[error("Table not found: {table}")]
    TableNotFound { table: Box<str> },
    #[error("View not found: {view}")]
    ViewNotFound { view: Box<str> },
    #[error("Index not found")]
    IndexNotFound,
    #[error("Constraint not found")]
    ConstraintNotFound,
    #[error("Sequence not found")]
    SequenceNotFound,
    #[error("Schedule not found")]
    ScheduleNotFound,
    #[error("Type resolution failed")]
    TypeResolution,
    #[error("Column not found")]
    ColumnNotFound,
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
}

/// Action types for database operations
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Created,
    Removed,
    Changed,
}

/// Trait for formatting migration steps
/// This trait defines methods for formatting various components of a migration plan.
/// It allows for different implementations, such as ANSI formatting or plain text formatting.
pub trait MigrationFormatter {
    fn format_header(&mut self) -> io::Result<()>;
    fn format_add_table(&mut self, table_info: &TableInfo) -> io::Result<()>;
    fn format_view(&mut self, view_info: &ViewInfo, action: Action) -> io::Result<()>;
    fn format_index(&mut self, index_info: &IndexInfo, action: Action) -> io::Result<()>;
    fn format_constraint(&mut self, constraint_info: &ConstraintInfo, action: Action) -> io::Result<()>;
    fn format_sequence(&mut self, sequence_info: &SequenceInfo, action: Action) -> io::Result<()>;
    fn format_change_access(&mut self, access_info: &AccessChangeInfo) -> io::Result<()>;
    fn format_schedule(&mut self, schedule_info: &ScheduleInfo, action: Action) -> io::Result<()>;
    fn format_rls(&mut self, rls_info: &RlsInfo, action: Action) -> io::Result<()>;
    fn format_change_columns(&mut self, column_changes: &ColumnChanges) -> io::Result<()>;
    fn format_add_columns(&mut self, new_columns: &NewColumns) -> io::Result<()>;
    fn format_disconnect_warning(&mut self) -> io::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableInfo {
    pub name: String,
    pub is_system: bool,
    pub access: TableAccess,
    pub columns: Vec<ColumnInfo>,
    pub constraints: Vec<ConstraintInfo>,
    pub indexes: Vec<IndexInfo>,
    pub sequences: Vec<SequenceInfo>,
    pub schedule: Option<ScheduleInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewInfo {
    pub name: String,
    pub params: Vec<ViewParamInfo>,
    pub columns: Vec<ViewColumnInfo>,
    pub is_anonymous: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewColumnInfo {
    pub name: Identifier,
    pub type_name: AlgebraicType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewParamInfo {
    pub name: Identifier,
    pub type_name: AlgebraicType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnInfo {
    pub name: Identifier,
    pub type_name: AlgebraicType,
    pub default_value: Option<AlgebraicValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintInfo {
    pub name: String,
    pub columns: Vec<Identifier>,
    pub table_name: Identifier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<Identifier>,
    pub table_name: Identifier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SequenceInfo {
    pub name: String,
    pub column_name: Identifier,
    pub table_name: Identifier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccessChangeInfo {
    pub table_name: Identifier,
    pub new_access: TableAccess,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleInfo {
    pub table_name: String,
    pub function_name: Identifier,
    pub function_kind: FunctionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RlsInfo {
    pub policy: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnChanges {
    pub table_name: Identifier,
    pub changes: Vec<ColumnChange>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColumnChange {
    Renamed {
        old_name: Identifier,
        new_name: Identifier,
    },
    TypeChanged {
        name: Identifier,
        old_type: AlgebraicType,
        new_type: AlgebraicType,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewColumns {
    pub table_name: Identifier,
    pub columns: Vec<ColumnInfo>,
}

// Data extraction functions (these replace the original print functions' data gathering logic)
fn extract_table_info(
    table: <TableDef as crate::def::ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<TableInfo, FormattingErrors> {
    let table_def = plan.new.table(table).ok_or_else(|| FormattingErrors::TableNotFound {
        table: table.to_string().into(),
    })?;

    let columns = table_def
        .columns
        .iter()
        .map(|column| {
            let type_name = WithTypespace::new(plan.new.typespace(), &column.ty)
                .resolve_refs()
                .map_err(|_| FormattingErrors::TypeResolution)?;
            Ok(ColumnInfo {
                name: column.name.clone(),
                type_name,
                default_value: column.default_value.clone(),
            })
        })
        .collect::<Result<Vec<_>, FormattingErrors>>()?;

    let constraints = table_def
        .constraints
        .values()
        .sorted_by_key(|c| c.name.clone())
        .map(|constraint| {
            let ConstraintData::Unique(unique) = &constraint.data;
            Ok(ConstraintInfo {
                name: constraint.name.to_string(),
                columns: unique
                    .columns
                    .iter()
                    .map(|col_id| {
                        let column = table_def.get_column(col_id).ok_or(FormattingErrors::ColumnNotFound)?;
                        Ok(column.name.clone())
                    })
                    .collect::<Result<Vec<_>, FormattingErrors>>()?,
                table_name: table_def.name.clone(),
            })
        })
        .collect::<Result<Vec<_>, FormattingErrors>>()?;

    let indexes = table_def
        .indexes
        .values()
        .sorted_by_key(|c| c.name.clone())
        .map(|index| {
            let columns = match &index.algorithm {
                IndexAlgorithm::BTree(btree) => btree
                    .columns
                    .iter()
                    .map(|col_id| {
                        let column = table_def.get_column(col_id).ok_or(FormattingErrors::ColumnNotFound)?;
                        Ok(column.name.clone())
                    })
                    .collect::<Result<Vec<_>, FormattingErrors>>()?,
                IndexAlgorithm::Direct(direct) => {
                    let column = table_def
                        .get_column(direct.column)
                        .ok_or(FormattingErrors::ColumnNotFound)?;
                    vec![column.name.clone()]
                }
            };

            Ok(IndexInfo {
                name: index.name.to_string(),
                columns,
                table_name: table_def.name.clone(),
            })
        })
        .collect::<Result<Vec<_>, FormattingErrors>>()?;

    let sequences = table_def
        .sequences
        .values()
        .sorted_by_key(|c| c.name.clone())
        .map(|sequence| {
            let column = table_def
                .get_column(sequence.column)
                .ok_or(FormattingErrors::ColumnNotFound)?;
            Ok(SequenceInfo {
                name: sequence.name.to_string(),
                column_name: column.name.clone(),
                table_name: table_def.name.clone(),
            })
        })
        .collect::<Result<Vec<_>, FormattingErrors>>()?;

    let schedule = table_def.schedule.as_ref().map(|schedule| ScheduleInfo {
        table_name: table_def.name.to_string().clone(),
        function_name: schedule.function_name.clone(),
        function_kind: schedule.function_kind,
    });

    Ok(TableInfo {
        name: table_def.name.to_string(),
        is_system: table_def.table_type == TableType::System,
        access: table_def.table_access,
        columns,
        constraints,
        indexes,
        sequences,
        schedule,
    })
}

fn extract_view_info(
    view: <ViewDef as crate::def::ModuleDefLookup>::Key<'_>,
    module_def: &ModuleDef,
) -> Result<ViewInfo, FormattingErrors> {
    let view_def = module_def.view(view).ok_or_else(|| FormattingErrors::ViewNotFound {
        view: view.to_string().into(),
    })?;

    let name = view_def.name.to_string();
    let is_anonymous = view_def.is_anonymous;

    let params = view_def
        .param_columns
        .iter()
        .map(|column| {
            let type_name = WithTypespace::new(module_def.typespace(), &column.ty)
                .resolve_refs()
                .map_err(|_| FormattingErrors::TypeResolution)?;
            Ok(ViewParamInfo {
                name: column.name.clone(),
                type_name,
            })
        })
        .collect::<Result<Vec<_>, FormattingErrors>>()?;

    let columns = view_def
        .return_columns
        .iter()
        .map(|column| {
            let type_name = WithTypespace::new(module_def.typespace(), &column.ty)
                .resolve_refs()
                .map_err(|_| FormattingErrors::TypeResolution)?;
            Ok(ViewColumnInfo {
                name: column.name.clone(),
                type_name,
            })
        })
        .collect::<Result<Vec<_>, FormattingErrors>>()?;

    Ok(ViewInfo {
        name,
        params,
        columns,
        is_anonymous,
    })
}

fn extract_index_info(
    index: <crate::def::IndexDef as ModuleDefLookup>::Key<'_>,
    module_def: &ModuleDef,
) -> Result<IndexInfo, FormattingErrors> {
    let table_def = module_def
        .stored_in_table_def(index)
        .ok_or(FormattingErrors::IndexNotFound)?;
    let index_def = table_def.indexes.get(index).ok_or(FormattingErrors::IndexNotFound)?;

    let columns = match &index_def.algorithm {
        IndexAlgorithm::BTree(btree) => btree
            .columns
            .iter()
            .map(|col_id| {
                let column = table_def.get_column(col_id).ok_or(FormattingErrors::ColumnNotFound)?;
                Ok(column.name.clone())
            })
            .collect::<Result<Vec<_>, FormattingErrors>>()?,
        IndexAlgorithm::Direct(direct) => {
            let column = table_def
                .get_column(direct.column)
                .ok_or(FormattingErrors::ColumnNotFound)?;
            vec![column.name.clone()]
        }
    };

    Ok(IndexInfo {
        name: index_def.name.to_string(),
        columns,
        table_name: table_def.name.clone(),
    })
}

fn extract_constraint_info(
    constraint: <crate::def::ConstraintDef as ModuleDefLookup>::Key<'_>,
    module_def: &ModuleDef,
) -> Result<ConstraintInfo, FormattingErrors> {
    let table_def = module_def
        .stored_in_table_def(constraint)
        .ok_or(FormattingErrors::ConstraintNotFound)?;
    let constraint_def = table_def
        .constraints
        .get(constraint)
        .ok_or(FormattingErrors::ConstraintNotFound)?;

    let ConstraintData::Unique(unique_constraint_data) = &constraint_def.data;
    let columns = unique_constraint_data
        .columns
        .iter()
        .map(|col_id| {
            let column = table_def.get_column(col_id).ok_or(FormattingErrors::ColumnNotFound)?;
            Ok(column.name.clone())
        })
        .collect::<Result<Vec<_>, FormattingErrors>>()?;

    Ok(ConstraintInfo {
        name: constraint_def.name.to_string(),
        columns,
        table_name: table_def.name.clone(),
    })
}

fn extract_sequence_info(
    sequence: <crate::def::SequenceDef as ModuleDefLookup>::Key<'_>,
    module_def: &ModuleDef,
) -> Result<SequenceInfo, FormattingErrors> {
    let table_def = module_def
        .stored_in_table_def(sequence)
        .ok_or(FormattingErrors::SequenceNotFound)?;
    let sequence_def = table_def
        .sequences
        .get(sequence)
        .ok_or(FormattingErrors::SequenceNotFound)?;

    let column = table_def
        .get_column(sequence_def.column)
        .ok_or(FormattingErrors::ColumnNotFound)?;

    Ok(SequenceInfo {
        name: sequence_def.name.to_string(),
        column_name: column.name.clone(),
        table_name: table_def.name.clone(),
    })
}

fn extract_access_change_info(
    table: <TableDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<AccessChangeInfo, FormattingErrors> {
    let table_def = plan.new.table(table).ok_or_else(|| FormattingErrors::TableNotFound {
        table: table.to_string().into(),
    })?;

    Ok(AccessChangeInfo {
        table_name: table_def.name.clone(),
        new_access: table_def.table_access,
    })
}

fn extract_schedule_info(
    schedule_table: <ScheduleDef as ModuleDefLookup>::Key<'_>,
    module_def: &ModuleDef,
) -> Result<ScheduleInfo, FormattingErrors> {
    let schedule_def: &ScheduleDef = module_def
        .lookup(schedule_table)
        .ok_or(FormattingErrors::ScheduleNotFound)?;

    Ok(ScheduleInfo {
        table_name: schedule_def.name.to_string().clone(),
        function_name: schedule_def.function_name.clone(),
        function_kind: schedule_def.function_kind,
    })
}

fn extract_rls_info(
    rls: <RawRowLevelSecurityDefV9 as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<Option<RlsInfo>, FormattingErrors> {
    // Skip if policy unchanged (implementation detail workaround)
    if plan.old.lookup::<RawRowLevelSecurityDefV9>(rls) == plan.new.lookup::<RawRowLevelSecurityDefV9>(rls) {
        return Ok(None);
    }

    Ok(Some(RlsInfo {
        policy: rls.to_string(),
    }))
}

fn extract_column_changes(
    table: <TableDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<ColumnChanges, FormattingErrors> {
    let old_table = plan.old.table(table).ok_or_else(|| FormattingErrors::TableNotFound {
        table: table.to_string().into(),
    })?;
    let new_table = plan.new.table(table).ok_or_else(|| FormattingErrors::TableNotFound {
        table: table.to_string().into(),
    })?;

    let mut changes = Vec::new();

    // Find modified columns
    for new_col in &new_table.columns {
        if let Some(old_col) = old_table.columns.iter().find(|c| c.col_id == new_col.col_id) {
            if old_col.name != new_col.name {
                changes.push(ColumnChange::Renamed {
                    old_name: old_col.name.clone(),
                    new_name: new_col.name.clone(),
                });
            }
            if old_col.ty != new_col.ty {
                let old_type = WithTypespace::new(plan.old.typespace(), &old_col.ty)
                    .resolve_refs()
                    .map_err(|_| FormattingErrors::TypeResolution)?;
                let new_type = WithTypespace::new(plan.new.typespace(), &new_col.ty)
                    .resolve_refs()
                    .map_err(|_| FormattingErrors::TypeResolution)?;
                changes.push(ColumnChange::TypeChanged {
                    name: new_col.name.clone(),
                    old_type,
                    new_type,
                });
            }
        }
    }

    Ok(ColumnChanges {
        table_name: new_table.name.clone(),
        changes,
    })
}

fn extract_new_columns(
    table: <TableDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<NewColumns, FormattingErrors> {
    let table_def = plan.new.table(table).ok_or_else(|| FormattingErrors::TableNotFound {
        table: table.to_string().into(),
    })?;
    let old_table_def = plan.old.table(table).ok_or_else(|| FormattingErrors::TableNotFound {
        table: table.to_string().into(),
    })?;

    let mut new_columns = Vec::new();
    for column in &table_def.columns {
        if !old_table_def.columns.iter().any(|c| c.col_id == column.col_id) {
            let type_name = WithTypespace::new(plan.new.typespace(), &column.ty)
                .resolve_refs()
                .map_err(|_| FormattingErrors::TypeResolution)?;
            new_columns.push(ColumnInfo {
                name: column.name.clone(),
                type_name,
                default_value: column.default_value.clone(),
            });
        }
    }

    Ok(NewColumns {
        table_name: table_def.name.clone(),
        columns: new_columns,
    })
}
