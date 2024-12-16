//! This module provides a function [`pretty_print`](pretty_print) that renders an automatic migration plan to a string.

use super::{IndexAlgorithm, MigratePlan, TableDef};
use crate::{auto_migrate::AutoMigrateStep, def::ConstraintData};
use colored::{self, ColoredString, Colorize};
use lazy_static::lazy_static;
use regex::Regex;
use spacetimedb_lib::db::raw_def::v9::{TableAccess, TableType};
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::{algebraic_type::fmt::fmt_algebraic_type, WithTypespace};
use std::fmt::{self, Write};

lazy_static! {
    // https://superuser.com/questions/380772/removing-ansi-color-codes-from-text-stream
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

/// Strip ANSI escape sequences from a string.
/// This is needed when printing in a terminal without support for these sequences,
/// such as `CMD.exe`.
pub fn strip_ansi_escape_codes(s: &str) -> String {
    ANSI_ESCAPE_SEQUENCE.replace_all(s, "").into_owned()
}

/// Pretty print a migration plan, resulting in a string (containing ANSI escape codes).
/// If you are printing
pub fn pretty_print(plan: &MigratePlan) -> Result<String, fmt::Error> {
    let plan = match plan {
        MigratePlan::Auto(plan) => plan,
    };
    let mut out = String::new();
    let outr = &mut out;

    writeln!(outr, "{}", "Migration plan".blue())?;
    writeln!(outr, "{}", "--------------")?;
    writeln!(outr, "")?;

    let added = "+".green().bold();
    let removed = "-".red().bold();

    for step in &plan.steps {
        match step {
            AutoMigrateStep::AddTable(t) => {
                let table = plan.new.table(*t).ok_or(fmt::Error)?;

                write!(outr, "{} table: {}", added, table_name(&*table.name))?;
                if table.table_type == TableType::System {
                    write!(outr, " (system)")?;
                }
                match table.table_access {
                    TableAccess::Private => write!(outr, "(private)")?,
                    TableAccess::Public => write!(outr, "(public)")?,
                }
                writeln!(outr)?;
                for column in &table.columns {
                    let resolved = WithTypespace::new(plan.new.typespace(), &column.ty)
                        .resolve_refs()
                        .map_err(|_| fmt::Error)?;

                    writeln!(
                        outr,
                        "  {} column: {}: {}",
                        added,
                        column.name,
                        fmt_algebraic_type(&resolved)
                    )?;
                }
                for constraint in table.constraints.values() {
                    match &constraint.data {
                        ConstraintData::Unique(unique) => {
                            write!(outr, "  {} unique constraint on ", added)?;
                            write_col_list(outr, &unique.columns.clone().into(), table)?;
                            writeln!(outr)?;
                        }
                    }
                }
                for index in table.indexes.values() {
                    match &index.algorithm {
                        IndexAlgorithm::BTree(btree) => {
                            write!(outr, "  {} btree index on ", added)?;
                            write_col_list(outr, &btree.columns, table)?;
                            writeln!(outr)?;
                        }
                    }
                }
                for sequence in table.sequences.values() {
                    let column = column_name(table, sequence.column);
                    writeln!(outr, "  {} sequence on {}", added, column)?;
                }
                if let Some(schedule) = &table.schedule {
                    let reducer = reducer_name(&*schedule.reducer_name);
                    writeln!(outr, "  {} schedule calling {}", added, reducer)?;
                }
            }
            AutoMigrateStep::AddIndex(index) => {
                let table_def = plan.new.stored_in_table_def(*index).ok_or(fmt::Error)?;
                let index_def = table_def.indexes.get(*index).ok_or(fmt::Error)?;

                write!(
                    outr,
                    "{} index on table {} columns ",
                    added,
                    table_name(&*table_def.name)
                )?;
                write_col_list(outr, index_def.algorithm.columns(), table_def)?;
                writeln!(outr)?;
            }
            AutoMigrateStep::RemoveIndex(index) => {
                let table_def = plan.old.stored_in_table_def(*index).ok_or(fmt::Error)?;
                let index_def = table_def.indexes.get(*index).ok_or(fmt::Error)?;

                write!(
                    outr,
                    "{} index on table {} columns ",
                    removed,
                    table_name(&*table_def.name)
                )?;
                write_col_list(outr, index_def.algorithm.columns(), table_def)?;
                writeln!(outr)?;
            }
            AutoMigrateStep::RemoveConstraint(constraint) => {
                let table_def = plan.old.stored_in_table_def(constraint).ok_or(fmt::Error)?;
                let constraint_def = table_def.constraints.get(*constraint).ok_or(fmt::Error)?;
                match &constraint_def.data {
                    ConstraintData::Unique(unique_constraint_data) => {
                        write!(
                            outr,
                            "{} unique constraint on table {} columns ",
                            removed,
                            table_name(&*table_def.name)
                        )?;
                        write_col_list(outr, &unique_constraint_data.columns.clone().into(), table_def)?;
                        writeln!(outr)?;
                    }
                }
            }
            AutoMigrateStep::AddSequence(sequence) => {
                let table_def = plan.new.stored_in_table_def(*sequence).ok_or(fmt::Error)?;
                let sequence_def = table_def.sequences.get(*sequence).ok_or(fmt::Error)?;

                write!(
                    outr,
                    "{} sequence on table {} column {}",
                    added,
                    table_name(&*table_def.name),
                    column_name(table_def, sequence_def.column)
                )?;
                writeln!(outr)?;
            }
            AutoMigrateStep::RemoveSequence(sequence) => {
                let table_def = plan.old.stored_in_table_def(*sequence).ok_or(fmt::Error)?;
                let sequence_def = table_def.sequences.get(*sequence).ok_or(fmt::Error)?;

                write!(
                    outr,
                    "{} sequence on table {} column {}",
                    removed,
                    table_name(&*table_def.name),
                    column_name(table_def, sequence_def.column)
                )?;
                writeln!(outr)?;
            }
            AutoMigrateStep::ChangeAccess(table) => {
                let table_def = plan.new.table(*table).ok_or(fmt::Error)?;

                write!(
                    outr,
                    "{} table access for table {}",
                    added,
                    table_name(&*table_def.name)
                )?;
                match table_def.table_access {
                    TableAccess::Private => write!(outr, " (public -> private)")?,
                    TableAccess::Public => write!(outr, " (private -> public)")?,
                }
                writeln!(outr)?;
            }
            AutoMigrateStep::AddSchedule(schedule) => {
                let table_def = plan.new.table(*schedule).ok_or(fmt::Error)?;
                let schedule_def = table_def.schedule.as_ref().ok_or(fmt::Error)?;

                let reducer = reducer_name(&*schedule_def.reducer_name);
                write!(
                    outr,
                    "{} schedule for table {} calling {}",
                    added,
                    table_name(&*table_def.name),
                    reducer
                )?;
                writeln!(outr)?;
            }
            AutoMigrateStep::RemoveSchedule(schedule) => {
                let table_def = plan.old.table(*schedule).ok_or(fmt::Error)?;
                let schedule_def = table_def.schedule.as_ref().ok_or(fmt::Error)?;

                let reducer = reducer_name(&*schedule_def.reducer_name);
                write!(
                    outr,
                    "{} schedule for table {} calling {}",
                    removed,
                    table_name(&*table_def.name),
                    reducer
                )?;
                writeln!(outr)?;
            }
            AutoMigrateStep::AddRowLevelSecurity(rls) => {
                writeln!(outr, "{} row level security policy:", added)?;
                writeln!(outr, "=============================")?;
                writeln!(outr, "{}", rls)?;
                writeln!(outr, "=============================")?;
            }
            AutoMigrateStep::RemoveRowLevelSecurity(rls) => {
                writeln!(outr, "{} row level security policy:", removed)?;
                writeln!(outr, "=============================")?;
                writeln!(outr, "{}", rls)?;
                writeln!(outr, "=============================")?;
            }
        }
    }

    Ok(out)
}

fn column_name(table_def: &TableDef, col_id: ColId) -> ColoredString {
    table_def
        .columns
        .get(col_id.idx())
        .map(|def| &*def.name)
        .unwrap_or("unknown_column")
        .magenta()
}

fn reducer_name(name: &str) -> ColoredString {
    name.blue()
}

fn table_name(name: &str) -> ColoredString {
    name.green()
}

fn write_col_list(out: &mut String, col_list: &ColList, table_def: &TableDef) -> Result<(), fmt::Error> {
    write!(out, "[")?;
    for (i, col) in col_list.iter().enumerate() {
        let join = if i == 0 { "" } else { ", " };
        write!(out, "{}{}", join, column_name(table_def, col))?;
    }
    write!(out, "]")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_strip_ansi_escape_sequences() {
        let input = "\x1b[31mHello, \x1b[32mworld!\x1b[0m";
        let expected = "Hello, world!";
        assert_eq!(super::strip_ansi_escape_codes(input), expected);
    }
}
