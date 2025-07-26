//! This module provides enhanced functionality for rendering automatic migration plans to strings.

use super::{AutoMigratePlan, IndexAlgorithm, ModuleDefLookup, TableDef};
use crate::{auto_migrate::AutoMigrateStep, def::ConstraintData};
use colored::{self, Colorize};
use lazy_static::lazy_static;
use regex::Regex;
use spacetimedb_lib::{
    db::raw_def::v9::{RawRowLevelSecurityDefV9, TableAccess, TableType},
    AlgebraicType,
};
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::{algebraic_type::fmt::fmt_algebraic_type, WithTypespace};
use std::fmt::{self, Write};
use thiserror::Error;

lazy_static! {
    static ref ANSI_ESCAPE_SEQUENCE: Regex = Regex::new(
        r#"(?x) # verbose mode
        (?:
            \x1b \[ [\x30-\x3f]* [\x20-\x2f]* [\x40-\x7e]   # CSI sequences (start with "ESC [")
            | \x1b [PX^_] .*? \x1b \\                       # String Terminator sequences (end with "ESC \")
            | \x1b \] [^\x07]* (?: \x07 | \x1b \\ )         # Sequences ending in BEL ("\x07")
            | \x1b [\x40-\x5f] 
        )"#
    )
    .unwrap();
}

/// Custom error type for migration pretty printing
#[derive(Error, Debug)]
pub enum PrettyPrintError {
    #[error("Formatting error: {0}")]
    Format(#[from] fmt::Error),
    #[error("Table not found")]
    TableNotFound,
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
}

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi_escape_codes(s: &str) -> String {
    ANSI_ESCAPE_SEQUENCE.replace_all(s, "").into_owned()
}

/// Pretty print a migration plan, resulting in a string (containing ANSI escape codes).
pub fn pretty_print(plan: &AutoMigratePlan) -> Result<String, PrettyPrintError> {
    let mut out = String::with_capacity(512);

    writeln!(out, "--------------")?;
    writeln!(out, "{}", "Performed automatic migration".blue())?;
    writeln!(out, "--------------")?;

    for step in &plan.steps {
        print_step(&mut out, step, plan)?;
    }

    Ok(out)
}

fn print_step(out: &mut String, step: &AutoMigrateStep, plan: &super::AutoMigratePlan) -> Result<(), PrettyPrintError> {
    match step {
        AutoMigrateStep::AddTable(t) => print_add_table(out, t, plan),
        AutoMigrateStep::AddIndex(index) => print_add_index(out, *index, plan),
        AutoMigrateStep::RemoveIndex(index) => print_remove_index(out, *index, plan),
        AutoMigrateStep::RemoveConstraint(constraint) => print_remove_constraint(out, *constraint, plan),
        AutoMigrateStep::AddSequence(sequence) => print_add_sequence(out, *sequence, plan),
        AutoMigrateStep::RemoveSequence(sequence) => print_remove_sequence(out, *sequence, plan),
        AutoMigrateStep::ChangeAccess(table) => print_change_access(out, *table, plan),
        AutoMigrateStep::AddSchedule(schedule) => print_add_schedule(out, *schedule, plan),
        AutoMigrateStep::RemoveSchedule(schedule) => print_remove_schedule(out, *schedule, plan),
        AutoMigrateStep::AddRowLevelSecurity(rls) => print_add_rls(out, *rls, plan),
        AutoMigrateStep::RemoveRowLevelSecurity(rls) => print_remove_rls(out, *rls, plan),
        AutoMigrateStep::ChangeColumns(table) => print_change_columns(out, *table, plan),
        AutoMigrateStep::AddColumns(table) => print_add_columns(out, *table, plan),
        AutoMigrateStep::DisconnectAllUsers => print_disconnect_all_users(out),
    }
}

fn print_add_table(
    out: &mut String,
    table: <TableDef as crate::def::ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table = plan.new.table(table).ok_or(PrettyPrintError::TableNotFound)?;

    write!(out, "- {} table: {}", "Created".green().bold(), table_name(&table.name))?;

    if table.table_type == TableType::System {
        write!(out, " (system)")?;
    }
    match table.table_access {
        TableAccess::Private => write!(out, " (private)")?,
        TableAccess::Public => write!(out, " (public)")?,
    }
    writeln!(out)?;

    // Columns
    writeln!(out, "    - Columns:")?;
    for column in &table.columns {
        let resolved = WithTypespace::new(plan.new.typespace(), &column.ty)
            .resolve_refs()
            .map_err(|_| PrettyPrintError::TypeResolution)?;
        writeln!(out, "        - {}: {}", column_name(&column.name), type_name(&resolved))?;
    }

    // Constraints
    if !table.constraints.is_empty() {
        writeln!(out, "    - Unique constraints:")?;
        for constraint in table.constraints.values() {
            let ConstraintData::Unique(unique) = &constraint.data;
            writeln!(
                out,
                "        - {} on {}",
                constraint_name(&constraint.name),
                format_col_list(&unique.columns.clone().into(), table)?
            )?;
        }
    }

    // Indexes
    if !table.indexes.is_empty() {
        writeln!(out, "    - Indexes:")?;
        for index in table.indexes.values() {
            if let IndexAlgorithm::BTree(btree) = &index.algorithm {
                writeln!(
                    out,
                    "        - {} on {}",
                    index_name(&index.name),
                    format_col_list(&btree.columns, table)?
                )?;
            }
        }
    }

    // Sequences
    if !table.sequences.is_empty() {
        writeln!(out, "    - Auto-increment constraints:")?;
        for sequence in table.sequences.values() {
            let column = column_name_from_id(table, sequence.column)?;
            writeln!(out, "        - {} on {}", sequence_name(&sequence.name), column)?;
        }
    }

    // Schedule
    if let Some(schedule) = &table.schedule {
        writeln!(
            out,
            "        - Scheduled, calling reducer: {}",
            reducer_name(&schedule.reducer_name)
        )?;
    }

    Ok(())
}

fn print_add_index(
    out: &mut String,
    index: <crate::def::IndexDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan
        .new
        .stored_in_table_def(&index)
        .ok_or(PrettyPrintError::IndexNotFound)?;
    let index_def = table_def.indexes.get(index).ok_or(PrettyPrintError::IndexNotFound)?;

    writeln!(
        out,
        "- {} index {} on columns {} of table {}",
        "Created".green().bold(),
        index_name(&index_def.name),
        format_col_list(&index_def.algorithm.columns().to_owned(), table_def)?,
        table_name(&table_def.name),
    )?;
    Ok(())
}

fn print_remove_index(
    out: &mut String,
    index: <crate::def::IndexDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan
        .old
        .stored_in_table_def(&index)
        .ok_or(PrettyPrintError::IndexNotFound)?;
    let index_def = table_def.indexes.get(index).ok_or(PrettyPrintError::IndexNotFound)?;

    let col_list = &index_def.algorithm.columns().to_owned();
    writeln!(
        out,
        "- {} index {} on columns {} of table {}",
        "Removed".red().bold(),
        index_name(&index_def.name),
        format_col_list(col_list, table_def)?,
        table_name(&table_def.name)
    )?;
    Ok(())
}

fn print_remove_constraint(
    out: &mut String,
    constraint: <crate::def::ConstraintDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan
        .old
        .stored_in_table_def(&constraint)
        .ok_or(PrettyPrintError::ConstraintNotFound)?;
    let constraint_def = table_def
        .constraints
        .get(constraint)
        .ok_or(PrettyPrintError::ConstraintNotFound)?;

    let ConstraintData::Unique(unique_constraint_data) = &constraint_def.data;
    writeln!(
        out,
        "- {} unique constraint {} on columns {} of table {}",
        "Removed".red().bold(),
        constraint_name(&constraint_def.name),
        format_col_list(&unique_constraint_data.columns, table_def)?,
        table_name(&table_def.name)
    )?;
    Ok(())
}

fn print_add_sequence(
    out: &mut String,
    sequence: <crate::def::SequenceDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan
        .new
        .stored_in_table_def(&sequence)
        .ok_or(PrettyPrintError::SequenceNotFound)?;
    let sequence_def = table_def
        .sequences
        .get(sequence)
        .ok_or(PrettyPrintError::SequenceNotFound)?;

    writeln!(
        out,
        "- {} auto-increment constraint {} on column {} of table {}",
        "Created".green().bold(),
        constraint_name(&sequence_def.name),
        column_name_from_id(table_def, sequence_def.column)?,
        table_name(&table_def.name),
    )?;
    Ok(())
}

fn print_remove_sequence(
    out: &mut String,
    sequence: <crate::def::SequenceDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan
        .old
        .stored_in_table_def(&sequence)
        .ok_or(PrettyPrintError::SequenceNotFound)?;
    let sequence_def = table_def
        .sequences
        .get(sequence)
        .ok_or(PrettyPrintError::SequenceNotFound)?;

    writeln!(
        out,
        "- {} auto-increment constraint {} on column {} of table {}",
        "Removed".red().bold(),
        constraint_name(&sequence_def.name),
        column_name_from_id(table_def, sequence_def.column)?,
        table_name(&table_def.name),
    )?;
    Ok(())
}

fn print_change_access(
    out: &mut String,
    table: <TableDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan.new.table(table).ok_or(PrettyPrintError::TableNotFound)?;

    write!(
        out,
        "- {} access for table {}",
        "Changed".yellow().bold(),
        table_name(&table_def.name)
    )?;
    match table_def.table_access {
        TableAccess::Private => write!(out, " (public -> private)")?,
        TableAccess::Public => write!(out, " (private -> public)")?,
    }
    writeln!(out)?;
    Ok(())
}

fn print_add_schedule(
    out: &mut String,
    schedule_table: <crate::def::ScheduleDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan.new.table(schedule_table).ok_or(PrettyPrintError::TableNotFound)?;
    let schedule_def = table_def.schedule.as_ref().ok_or(PrettyPrintError::ScheduleNotFound)?;

    writeln!(
        out,
        "- {} schedule for table {} calling reducer {}",
        "Created".green().bold(),
        table_name(&table_def.name),
        reducer_name(&schedule_def.reducer_name)
    )?;
    Ok(())
}

fn print_remove_schedule(
    out: &mut String,
    schedule_table: <crate::def::ScheduleDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan.old.table(schedule_table).ok_or(PrettyPrintError::TableNotFound)?;
    let schedule_def = table_def.schedule.as_ref().ok_or(PrettyPrintError::ScheduleNotFound)?;

    writeln!(
        out,
        "- {} schedule for table {} calling reducer {}",
        "Removed".red().bold(),
        table_name(&table_def.name),
        reducer_name(&schedule_def.reducer_name)
    )?;
    Ok(())
}

fn print_add_rls(
    out: &mut String,
    rls: <RawRowLevelSecurityDefV9 as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    // Skip if policy unchanged (implementation detail workaround)
    if plan.old.lookup::<RawRowLevelSecurityDefV9>(rls) == plan.new.lookup::<RawRowLevelSecurityDefV9>(rls) {
        return Ok(());
    }
    writeln!(out, "- {} row level security policy:", "Created".green().bold())?;
    writeln!(out, "    `{}`", rls.blue())?;
    Ok(())
}

fn print_remove_rls(
    out: &mut String,
    rls: <RawRowLevelSecurityDefV9 as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    // Skip if policy unchanged (implementation detail workaround)
    if plan.old.lookup::<RawRowLevelSecurityDefV9>(rls) == plan.new.lookup::<RawRowLevelSecurityDefV9>(rls) {
        return Ok(());
    }
    writeln!(out, "- {} row level security policy:", "Removed".red().bold())?;
    writeln!(out, "    `{}`", rls.blue())?;
    Ok(())
}

fn print_change_columns(
    out: &mut String,
    table: <TableDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let old_table = plan.old.table(table).ok_or(PrettyPrintError::TableNotFound)?;
    let new_table = plan.new.table(table).ok_or(PrettyPrintError::TableNotFound)?;

    writeln!(
        out,
        "- {} columns for table {}",
        "Changed".yellow().bold(),
        table_name(&new_table.name)
    )?;

    // Modified columns
    for new_col in &new_table.columns {
        if let Some(old_col) = old_table.columns.iter().find(|c| c.col_id == new_col.col_id) {
            if old_col.name != new_col.name {
                writeln!(
                    out,
                    "    ~ Renamed: {} -> {}",
                    column_name(&old_col.name),
                    column_name(&new_col.name)
                )?;
            }
            if old_col.ty != new_col.ty {
                let old_resolved = WithTypespace::new(plan.old.typespace(), &old_col.ty)
                    .resolve_refs()
                    .map_err(|_| PrettyPrintError::TypeResolution)?;
                let new_resolved = WithTypespace::new(plan.new.typespace(), &new_col.ty)
                    .resolve_refs()
                    .map_err(|_| PrettyPrintError::TypeResolution)?;
                writeln!(
                    out,
                    "    ~ Modified: {} ({} -> {})",
                    column_name(&new_col.name),
                    type_name(&old_resolved),
                    type_name(&new_resolved)
                )?;
            }
        }
    }
    Ok(())
}

fn print_add_columns(
    out: &mut String,
    table: <TableDef as ModuleDefLookup>::Key<'_>,
    plan: &super::AutoMigratePlan,
) -> Result<(), PrettyPrintError> {
    let table_def = plan.new.table(table).ok_or(PrettyPrintError::TableNotFound)?;
    let old_table_def = plan.old.table(table).ok_or(PrettyPrintError::TableNotFound)?;

    writeln!(
        out,
        "- {} column in table {}",
        "Added".green().bold(),
        table_name(&table_def.name)
    )?;
    for column in &table_def.columns {
        if !old_table_def.columns.iter().any(|c| c.col_id == column.col_id) {
            let resolved = WithTypespace::new(plan.new.typespace(), &column.ty)
                .resolve_refs()
                .map_err(|_| PrettyPrintError::TypeResolution)?;
            writeln!(
                out,
                "    + {}: {} with default value: {:?}",
                column_name(&column.name),
                type_name(&resolved),
                column.default_value
            )?;
        }
    }
    Ok(())
}

fn print_disconnect_all_users(out: &mut String) -> Result<(), PrettyPrintError> {
    writeln!(
        out,
        "- {} all clients will be {} due to migration",
        "Warning".yellow().bold(),
        "disconnected".red().bold()
    )?;
    Ok(())
}

// Helper functions for consistent formatting
fn reducer_name(name: &str) -> String {
    format!("`{}`", name.yellow())
}

fn table_name(name: &str) -> String {
    format!("`{}`", name.cyan())
}

fn column_name(name: &str) -> String {
    table_name(name)
}

fn column_name_from_id(table_def: &TableDef, col_id: ColId) -> Result<String, PrettyPrintError> {
    let column = table_def
        .columns
        .get(col_id.idx())
        .ok_or(PrettyPrintError::ColumnNotFound)?;
    Ok(column_name(&column.name))
}

fn index_name(name: &str) -> String {
    format!("`{}`", name.purple())
}

fn constraint_name(name: &str) -> String {
    index_name(name)
}

fn sequence_name(name: &str) -> String {
    index_name(name)
}

fn type_name(type_: &AlgebraicType) -> String {
    format!("{}", fmt_algebraic_type(type_).to_string().green())
}

fn format_col_list(col_list: &ColList, table_def: &TableDef) -> Result<String, PrettyPrintError> {
    let mut out = String::new();
    write!(&mut out, "[")?;
    for (i, col) in col_list.iter().enumerate() {
        let join = if i == 0 { "" } else { ", " };
        write!(&mut out, "{}{}", join, column_name_from_id(table_def, col)?)?;
    }
    write!(&mut out, "]")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_escape_sequences() {
        let input = "\x1b[31mHello, \x1b[32mworld!\x1b[0m";
        let expected = "Hello, world!";
        assert_eq!(strip_ansi_escape_codes(input), expected);
    }

    #[test]
    fn test_strip_complex_ansi_sequences() {
        let input = "\x1b[1;31;40mBold Red on Black\x1b[0m \x1b]0;Title\x07 Normal text";
        let expected = "Bold Red on Black  Normal text";
        assert_eq!(strip_ansi_escape_codes(input), expected);
    }
}
