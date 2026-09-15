use std::io;

use spacetimedb_lib::{db::raw_def::v9::TableAccess, AlgebraicType};
use spacetimedb_primitives::ColId;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;

use crate::auto_migrate::formatter::ViewInfo;
use crate::auto_migrate::PrettyPrintStyle;
use crate::styled_writer::StyledWriter;

use super::formatter::{
    AccessChangeInfo, Action, ColumnChange, ColumnChanges, ConstraintInfo, IndexInfo, MigrationFormatter, NewColumns,
    RlsInfo, ScheduleInfo, SequenceInfo, TableInfo,
};
use crate::identifier::NamespacedIdentifier;

const MIGRATION_INDENT_WIDTH: usize = 4;

#[derive(Debug)]
pub struct TermColorFormatter {
    out: StyledWriter,
}

impl TermColorFormatter {
    pub fn new(style: PrettyPrintStyle) -> Self {
        Self {
            out: StyledWriter::new(style, MIGRATION_INDENT_WIDTH),
        }
    }

    pub fn into_string(self) -> String {
        self.out.into_string()
    }

    fn write_bullet(&mut self, text: &str) -> io::Result<()> {
        self.out.write_line(format!("• {text}"))
    }

    fn write_action_prefix(&mut self, action: &Action) -> io::Result<()> {
        self.out.write_indent()?;
        self.out.write_plain("▸ ")?;
        action.write_with_color(&mut self.out)
    }

    fn format_type_name(&self, ty: &AlgebraicType) -> String {
        fmt_algebraic_type(ty).to_string()
    }

    fn write_type_name(&mut self, ty: &AlgebraicType) -> io::Result<()> {
        let s = self.format_type_name(ty);
        self.out.write_colored(&s, Some(self.out.colors().column_type), false)
    }

    fn format_access(&self, access: TableAccess) -> &'static str {
        match access {
            TableAccess::Private => "private",
            TableAccess::Public => "public",
        }
    }

    fn write_access(&mut self, access: TableAccess) -> io::Result<()> {
        let s = self.format_access(access);
        self.out.write_colored(s, Some(self.out.colors().access), false)
    }
}

impl MigrationFormatter for TermColorFormatter {
    fn format_header(&mut self) -> io::Result<()> {
        let line = "━".repeat(60);
        self.out.write_line(&line)?;
        self.out
            .write_colored_line("Database Migration Plan", Some(self.out.colors().header), true)?;
        self.out.write_line(&line)?;
        self.out.write_line("")
    }

    fn format_add_table(&mut self, table: &TableInfo) -> io::Result<()> {
        // Table header
        self.out.write_indent()?;
        self.out.write_plain("▸ ")?;
        Action::Created.write_with_color(&mut self.out)?;
        let kind = if table.is_system { "system" } else { "user" };
        self.out.write_plain(&format!(" {kind} table: "))?;
        self.out
            .write_colored(&table.name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain(" (")?;
        self.write_access(table.access)?;
        self.out.write_plain(")\n")?;

        self.out.indent();

        if !table.columns.is_empty() {
            self.out
                .write_colored_line("Columns:", Some(self.out.colors().section_header), true)?;
            self.out.indent();
            for col in &table.columns {
                self.out.write_indent()?;
                self.out.write_plain(&format!("• {}: ", col.name))?;
                self.write_type_name(&col.type_name)?;
                self.out.write_plain("\n")?;
            }
            self.out.dedent();
        }

        if !table.constraints.is_empty() {
            self.out
                .write_colored_line("Unique constraints:", Some(self.out.colors().section_header), true)?;
            self.out.indent();
            for c in &table.constraints {
                let cols = c.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
                self.write_bullet(&format!("{} on [{}]", c.name, cols))?;
            }
            self.out.dedent();
        }

        if !table.indexes.is_empty() {
            self.out
                .write_colored_line("Indexes:", Some(self.out.colors().section_header), true)?;
            self.out.indent();
            for i in &table.indexes {
                let cols = i.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
                self.write_bullet(&format!("{} on [{}]", i.name, cols))?;
            }
            self.out.dedent();
        }

        if !table.sequences.is_empty() {
            self.out.write_colored_line(
                "Auto-increment constraints:",
                Some(self.out.colors().section_header),
                true,
            )?;
            self.out.indent();
            for s in &table.sequences {
                self.write_bullet(&format!("{} on {}", s.name, s.column_name))?;
            }
            self.out.dedent();
        }

        if let Some(s) = &table.schedule {
            self.out
                .write_colored_line("Schedule:", Some(self.out.colors().section_header), true)?;
            self.out.indent();
            self.write_bullet(&format!("Calls {}: {}", s.function_kind, s.function_name))?;
            self.out.dedent();
        }

        self.out.dedent();
        self.out.write_line("")
    }

    fn format_remove_table(&mut self, table_name: &NamespacedIdentifier) -> io::Result<()> {
        self.write_action_prefix(&Action::Removed)?;
        self.out.write_plain(" table: ")?;
        self.out
            .write_colored(table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")?;
        self.out.write_line("")
    }

    fn format_view(&mut self, view: &ViewInfo, action: Action) -> io::Result<()> {
        self.out.write_indent()?;
        self.out.write_plain("▸ ")?;
        self.write_action_prefix(&action)?;
        self.out.write_plain(if view.is_anonymous {
            " anonymous view: "
        } else {
            " view: "
        })?;
        self.out
            .write_colored(&view.name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")?;

        self.out.indent();

        if !view.params.is_empty() {
            self.out
                .write_colored_line("Parameters:", Some(self.out.colors().section_header), true)?;
            self.out.indent();
            for col in &view.params {
                self.out.write_indent()?;
                self.out.write_plain(&format!("• {}: ", col.name))?;
                self.write_type_name(&col.type_name)?;
                self.out.write_plain("\n")?;
            }
            self.out.dedent();
        }

        if !view.columns.is_empty() {
            self.out
                .write_colored_line("Columns:", Some(self.out.colors().section_header), true)?;
            self.out.indent();
            for col in &view.columns {
                self.out.write_indent()?;
                self.out.write_plain(&format!("• {}: ", col.name))?;
                self.write_type_name(&col.type_name)?;
                self.out.write_plain("\n")?;
            }
            self.out.dedent();
        }

        self.out.dedent();
        self.out.write_line("")
    }

    fn format_constraint(&mut self, c: &ConstraintInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        let cols = c.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
        self.out
            .write_plain(&format!(" unique constraint {} on [{}] of table ", c.name, cols))?;
        self.out
            .write_colored(&c.table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")
    }

    fn format_index(&mut self, i: &IndexInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        let cols = i.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
        self.out
            .write_plain(&format!(" index {} on [{}] of table ", i.name, cols))?;
        self.out
            .write_colored(&i.table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")
    }

    fn format_sequence(&mut self, s: &SequenceInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        self.out.write_plain(&format!(
            " auto-increment constraint {} on column {} of table ",
            s.name, s.column_name
        ))?;
        self.out
            .write_colored(&s.table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")
    }

    fn format_change_access(&mut self, a: &AccessChangeInfo) -> io::Result<()> {
        let direction = match a.new_access {
            TableAccess::Private => "public → private",
            TableAccess::Public => "private → public",
        };
        self.write_action_prefix(&Action::Changed)?;
        self.out.write_plain(" access for table ")?;
        self.out
            .write_colored(&a.table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain(" (")?;
        self.out
            .write_colored(direction, Some(self.out.colors().access), false)?;
        self.out.write_plain(")\n")
    }

    fn format_change_primary_key(
        &mut self,
        table_name: &NamespacedIdentifier,
        old_pk: Option<ColId>,
        new_pk: Option<ColId>,
    ) -> io::Result<()> {
        let description = match (old_pk, new_pk) {
            (Some(_), None) => "removed".to_string(),
            (None, Some(col)) => format!("added on column {col}"),
            (Some(_), Some(col)) => format!("changed to column {col}"),
            (None, None) => return Ok(()),
        };
        self.write_action_prefix(&Action::Changed)?;
        self.out.write_plain(" primary key on table ")?;
        self.out
            .write_colored(table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain(&format!(" ({description})\n"))
    }

    fn format_schedule(&mut self, s: &ScheduleInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        self.out.write_plain(" schedule for table ")?;
        self.out
            .write_colored(&s.table_name, Some(self.out.colors().table_name), true)?;
        self.out
            .write_plain(&format!(" calling {} {}\n", s.function_kind, s.function_name))
    }

    fn format_rls(&mut self, r: &RlsInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        self.out.write_plain(" row level security policy:\n")?;
        self.out.indent();
        self.out.write_indent()?;
        self.out.write_plain("`")?;
        self.out
            .write_colored(&r.policy, Some(self.out.colors().section_header), false)?;
        self.out.write_plain("`\n")?;
        self.out.dedent();
        Ok(())
    }

    fn format_change_columns(&mut self, cs: &ColumnChanges) -> io::Result<()> {
        self.write_action_prefix(&Action::Changed)?;
        self.out.write_plain(" columns for table ")?;
        self.out
            .write_colored(&cs.table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")?;

        self.out.indent();
        for ch in &cs.changes {
            self.out.write_indent()?;
            match ch {
                ColumnChange::Renamed { old_name, new_name } => {
                    self.out.write_plain(&format!("~ Renamed: {old_name} → {new_name}\n"))?;
                }
                ColumnChange::TypeChanged {
                    name,
                    old_type,
                    new_type,
                } => {
                    self.out.write_plain(&format!("~ Modified: {name} ("))?;
                    self.write_type_name(old_type)?;
                    self.out.write_plain(" → ")?;
                    self.write_type_name(new_type)?;
                    self.out.write_plain(")\n")?;
                }
            }
        }
        self.out.dedent();
        Ok(())
    }

    fn format_add_columns(&mut self, nc: &NewColumns) -> io::Result<()> {
        let plural = if nc.columns.len() > 1 { "s" } else { "" };
        self.write_action_prefix(&Action::Created)?;
        self.out.write_plain(&format!(" column{plural} in table "))?;
        self.out
            .write_colored(&nc.table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")?;

        self.out.indent();
        for col in &nc.columns {
            let default = col
                .default_value
                .as_ref()
                .map(|v| format!(" (default: {v:#?})"))
                .unwrap_or_default();
            self.out.write_indent()?;
            self.out.write_plain(&format!("+ {}: ", col.name))?;
            self.write_type_name(&col.type_name)?;
            self.out.write_plain(&format!("{default}\n"))?;
        }
        self.out.dedent();
        Ok(())
    }

    fn format_change_table_accessor_name(&mut self, table_name: &str) -> io::Result<()> {
        self.write_action_prefix(&Action::Changed)?;
        self.out.write_plain(" table accessor name for ")?;
        self.out
            .write_colored(table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")
    }

    fn format_change_column_accessor_name(
        &mut self,
        table_name: &NamespacedIdentifier,
        col_name: &str,
    ) -> io::Result<()> {
        self.write_action_prefix(&Action::Changed)?;
        self.out.write_plain(" column accessor name for ")?;
        self.out
            .write_colored(table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain(".")?;
        self.out
            .write_colored(col_name, Some(self.out.colors().column_type), true)?;
        self.out.write_plain("\n")
    }

    fn format_disconnect_warning(&mut self) -> io::Result<()> {
        self.out.write_indent()?;
        self.out.write_with_background(
            "!!! Warning: All clients will be disconnected due to breaking schema changes",
            self.out.colors().warning,
            true,
        )?;
        self.out.write_plain("\n")
    }

    fn format_event_table_reschema(&mut self, table_name: &NamespacedIdentifier) -> io::Result<()> {
        // TODO(format-event-table-reschema): I (pgoldman 2026-06-10) didn't have time to meaningfully format event table reschemas,
        // so for now we're just printing the table name.

        self.write_action_prefix(&Action::Changed)?;
        self.out.write_plain(" schema of event table ")?;
        self.out
            .write_colored(table_name, Some(self.out.colors().table_name), true)?;
        self.out.write_plain("\n")?;

        Ok(())
    }
}

trait ActionColorExt {
    fn write_with_color(&self, w: &mut StyledWriter) -> io::Result<()>;
}

impl ActionColorExt for Action {
    fn write_with_color(&self, w: &mut StyledWriter) -> io::Result<()> {
        let colors = w.colors();
        let (text, color) = match self {
            Action::Created => ("Created", colors.created),
            Action::Removed => ("Removed", colors.removed),
            Action::Changed => ("Changed", colors.changed),
        };
        w.write_colored(text, Some(color), true)
    }
}
