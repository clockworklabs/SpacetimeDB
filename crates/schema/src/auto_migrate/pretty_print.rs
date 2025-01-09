//! This module provides a function [`pretty_print`](pretty_print) that renders an automatic migration plan to a string.

use super::{IndexAlgorithm, MigratePlan, TableDef};
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
/// If you are printing to a console without ANSI escape code support, call [`strip_ansi_escape_codes`] on the
/// resulting string.
pub fn pretty_print(plan: &MigratePlan) -> Result<String, fmt::Error> {
    let MigratePlan::Auto(plan) = plan;
    let mut out = String::new();
    let outr = &mut out;

    writeln!(outr, "--------------")?;
    writeln!(outr, "{}", "Performed automatic migration".blue())?;
    writeln!(outr, "--------------")?;

    let created = "Created".green().bold();
    let removed = "Removed".red().bold();

    for step in &plan.steps {
        match step {
            AutoMigrateStep::AddTable(t) => {
                let table = plan.new.table(*t).ok_or(fmt::Error)?;

                write!(outr, "- {} table: {}", created, table_name(&table.name))?;
                if table.table_type == TableType::System {
                    write!(outr, " (system)")?;
                }
                match table.table_access {
                    TableAccess::Private => write!(outr, " (private)")?,
                    TableAccess::Public => write!(outr, " (public)")?,
                }
                writeln!(outr)?;
                writeln!(outr, "    - Columns:")?;
                for column in &table.columns {
                    let resolved = WithTypespace::new(plan.new.typespace(), &column.ty)
                        .resolve_refs()
                        .map_err(|_| fmt::Error)?;

                    writeln!(
                        outr,
                        "        - {}: {}",
                        column_name(&column.name),
                        type_name(&resolved)
                    )?;
                }
                if !table.constraints.is_empty() {
                    writeln!(outr, "    - Unique constraints:")?;
                    for constraint in table.constraints.values() {
                        match &constraint.data {
                            ConstraintData::Unique(unique) => {
                                writeln!(
                                    outr,
                                    "        - {} on {}",
                                    constraint_name(&constraint.name),
                                    format_col_list(&unique.columns.clone().into(), table)?
                                )?;
                            }
                        }
                    }
                }
                if !table.indexes.is_empty() {
                    writeln!(outr, "    - Indexes:")?;
                    for index in table.indexes.values() {
                        match &index.algorithm {
                            IndexAlgorithm::BTree(btree) => {
                                write!(
                                    outr,
                                    "        - {} on {}",
                                    index_name(&index.name),
                                    format_col_list(&btree.columns, table)?
                                )?;
                                writeln!(outr)?;
                            }
                        }
                    }
                }
                if !table.sequences.is_empty() {
                    writeln!(outr, "    - Auto-increment constraints:")?;
                    for sequence in table.sequences.values() {
                        let column = column_name_from_id(table, sequence.column);
                        writeln!(outr, "        - {} on {}", sequence_name(&sequence.name), column)?;
                    }
                }
                if let Some(schedule) = &table.schedule {
                    let reducer = reducer_name(&schedule.reducer_name);
                    writeln!(outr, "        - Scheduled, calling reducer: {}", reducer)?;
                }
            }
            AutoMigrateStep::AddIndex(index) => {
                let table_def = plan.new.stored_in_table_def(index).ok_or(fmt::Error)?;
                let index_def = table_def.indexes.get(*index).ok_or(fmt::Error)?;

                writeln!(
                    outr,
                    "- {} index {} on columns {} of table {}",
                    created,
                    index_name(index),
                    format_col_list(index_def.algorithm.columns(), table_def)?,
                    table_name(&table_def.name),
                )?;
            }
            AutoMigrateStep::RemoveIndex(index) => {
                let table_def = plan.old.stored_in_table_def(index).ok_or(fmt::Error)?;
                let index_def = table_def.indexes.get(*index).ok_or(fmt::Error)?;

                let col_list = match &index_def.algorithm {
                    IndexAlgorithm::BTree(b) => &b.columns,
                };
                write!(
                    outr,
                    "- {} index {} on columns {} of table {}",
                    removed,
                    index_name(index),
                    format_col_list(col_list, table_def)?,
                    table_name(&table_def.name)
                )?;
                writeln!(outr)?;
            }
            AutoMigrateStep::RemoveConstraint(constraint) => {
                let table_def = plan.old.stored_in_table_def(constraint).ok_or(fmt::Error)?;
                let constraint_def = table_def.constraints.get(*constraint).ok_or(fmt::Error)?;
                match &constraint_def.data {
                    ConstraintData::Unique(unique_constraint_data) => {
                        write!(
                            outr,
                            "- {} unique constraint {} on columns {} of table {}",
                            removed,
                            constraint_name(constraint),
                            format_col_list(&unique_constraint_data.columns, table_def)?,
                            table_name(&table_def.name)
                        )?;
                        writeln!(outr)?;
                    }
                }
            }
            AutoMigrateStep::AddSequence(sequence) => {
                let table_def = plan.new.stored_in_table_def(sequence).ok_or(fmt::Error)?;
                let sequence_def = table_def.sequences.get(*sequence).ok_or(fmt::Error)?;

                write!(
                    outr,
                    "- {} auto-increment constraint {} on column {} of table {}",
                    created,
                    constraint_name(sequence),
                    column_name_from_id(table_def, sequence_def.column),
                    table_name(&table_def.name),
                )?;
                writeln!(outr)?;
            }
            AutoMigrateStep::RemoveSequence(sequence) => {
                let table_def = plan.old.stored_in_table_def(sequence).ok_or(fmt::Error)?;
                let sequence_def = table_def.sequences.get(*sequence).ok_or(fmt::Error)?;

                write!(
                    outr,
                    "- {} auto-increment constraint {} on column {} of table {}",
                    removed,
                    constraint_name(sequence),
                    column_name_from_id(table_def, sequence_def.column),
                    table_name(&table_def.name),
                )?;
                writeln!(outr)?;
            }
            AutoMigrateStep::ChangeAccess(table) => {
                let table_def = plan.new.table(*table).ok_or(fmt::Error)?;

                write!(outr, "- Changed access for table {}", table_name(&table_def.name))?;
                match table_def.table_access {
                    TableAccess::Private => write!(outr, " (public -> private)")?,
                    TableAccess::Public => write!(outr, " (private -> public)")?,
                }
                writeln!(outr)?;
            }
            AutoMigrateStep::AddSchedule(schedule) => {
                let table_def = plan.new.table(*schedule).ok_or(fmt::Error)?;
                let schedule_def = table_def.schedule.as_ref().ok_or(fmt::Error)?;

                let reducer = reducer_name(&schedule_def.reducer_name);
                writeln!(
                    outr,
                    "- {} schedule for table {} calling reducer {}",
                    created,
                    table_name(&table_def.name),
                    reducer
                )?;
            }
            AutoMigrateStep::RemoveSchedule(schedule) => {
                let table_def = plan.old.table(*schedule).ok_or(fmt::Error)?;
                let schedule_def = table_def.schedule.as_ref().ok_or(fmt::Error)?;

                let reducer = reducer_name(&schedule_def.reducer_name);
                writeln!(
                    outr,
                    "- {} schedule for table {} calling reducer {}",
                    removed,
                    table_name(&table_def.name),
                    reducer
                )?;
            }
            AutoMigrateStep::AddRowLevelSecurity(rls) => {
                // Implementation detail: Row-level-security policies are always removed and re-added
                // because the `core` crate needs to recompile some stuff.
                // We hide this from the user.
                if plan.old.lookup::<RawRowLevelSecurityDefV9>(*rls)
                    == plan.new.lookup::<RawRowLevelSecurityDefV9>(*rls)
                {
                    continue;
                }
                writeln!(outr, "- {} row level security policy:", created)?;
                writeln!(outr, "    `{}`", rls.blue())?;
            }
            AutoMigrateStep::RemoveRowLevelSecurity(rls) => {
                // Implementation detail: Row-level-security policies are always removed and re-added
                // because the `core` crate needs to recompile some stuff.
                // We hide this from the user.
                if plan.old.lookup::<RawRowLevelSecurityDefV9>(*rls)
                    == plan.new.lookup::<RawRowLevelSecurityDefV9>(*rls)
                {
                    continue;
                }
                writeln!(outr, "- {} row level security policy:", removed)?;
                writeln!(outr, "    `{}`", rls.blue())?;
            }
        }
    }

    Ok(out)
}

fn reducer_name(name: &str) -> String {
    format!("`{}`", name.yellow())
}

fn table_name(name: &str) -> String {
    format!("`{}`", name.cyan())
}

fn column_name(name: &str) -> String {
    table_name(name)
}

fn column_name_from_id(table_def: &TableDef, col_id: ColId) -> String {
    column_name(
        table_def
            .columns
            .get(col_id.idx())
            .map(|def| &*def.name)
            .unwrap_or("unknown_column"),
    )
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

fn format_col_list(col_list: &ColList, table_def: &TableDef) -> Result<String, fmt::Error> {
    let mut out = String::new();
    write!(&mut out, "[")?;
    for (i, col) in col_list.iter().enumerate() {
        let join = if i == 0 { "" } else { ", " };
        write!(&mut out, "{}{}", join, column_name_from_id(table_def, col))?;
    }
    write!(&mut out, "]")?;
    Ok(out)
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
