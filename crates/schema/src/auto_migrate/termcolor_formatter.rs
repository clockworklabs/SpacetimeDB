use std::io::Write;
use std::{fmt, io};

use spacetimedb_lib::{db::raw_def::v9::TableAccess, AlgebraicType};
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use termcolor::{Buffer, Color, ColorChoice, ColorSpec, WriteColor};

use crate::auto_migrate::formatter::ViewInfo;

use super::formatter::{
    AccessChangeInfo, Action, ColumnChange, ColumnChanges, ConstraintInfo, IndexInfo, MigrationFormatter, NewColumns,
    RlsInfo, ScheduleInfo, SequenceInfo, TableInfo,
};

/// Color scheme for consistent formatting
#[derive(Debug, Clone)]
pub struct ColorScheme {
    pub created: Color,
    pub removed: Color,
    pub changed: Color,
    pub header: Color,
    pub table_name: Color,
    pub column_type: Color,
    pub section_header: Color,
    pub access: Color,
    pub warning: Color,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            created: Color::Green,
            removed: Color::Red,
            changed: Color::Yellow,
            header: Color::Blue,
            table_name: Color::Cyan,
            column_type: Color::Magenta,
            section_header: Color::Blue,
            access: Color::Green,
            warning: Color::Red,
        }
    }
}

#[derive(Debug)]
pub struct TermColorFormatter {
    buffer: Buffer,
    colors: ColorScheme,
    indent_level: usize,
}

impl fmt::Display for TermColorFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let buffer_content = std::str::from_utf8(self.buffer.as_slice()).map_err(|_| fmt::Error)?;
        write!(f, "{buffer_content}")
    }
}

impl TermColorFormatter {
    pub fn new(colors: ColorScheme, choice: ColorChoice) -> Self {
        Self {
            buffer: if choice == ColorChoice::Never {
                Buffer::no_color()
            } else {
                Buffer::ansi()
            },
            colors,
            indent_level: 0,
        }
    }

    fn write_indent(&mut self) -> io::Result<()> {
        let indent = "    ".repeat(self.indent_level);
        self.buffer.write_all(indent.as_bytes())
    }

    fn write_line(&mut self, text: impl AsRef<str>) -> io::Result<()> {
        self.write_indent()?;
        self.buffer.write_all(text.as_ref().as_bytes())?;
        self.buffer.write_all(b"\n")
    }

    fn write_colored(&mut self, text: &str, color: Option<Color>, bold: bool) -> io::Result<()> {
        let mut spec = ColorSpec::new();
        if let Some(c) = color {
            spec.set_fg(Some(c));
        }
        if bold {
            spec.set_bold(true);
        }
        self.buffer.set_color(&spec)?;
        self.buffer.write_all(text.as_bytes())?;
        self.buffer.reset()?;
        Ok(())
    }

    fn write_colored_line(&mut self, text: &str, color: Option<Color>, bold: bool) -> io::Result<()> {
        self.write_indent()?;
        self.write_colored(text, color, bold)?;
        self.buffer.write_all(b"\n")
    }

    fn write_with_background(&mut self, text: &str, bg: Color, bold: bool) -> io::Result<()> {
        let mut spec = ColorSpec::new();
        spec.set_bg(Some(bg));
        if bold {
            spec.set_bold(true);
        }
        self.buffer.set_color(&spec)?;
        self.buffer.write_all(text.as_bytes())?;
        self.buffer.reset()?;
        Ok(())
    }

    fn write_bullet(&mut self, text: &str) -> io::Result<()> {
        self.write_line(format!("• {text}"))
    }

    fn write_action_prefix(&mut self, action: &Action) -> io::Result<()> {
        self.write_indent()?;
        self.buffer.write_all("▸ ".to_string().as_bytes())?;
        action.write_with_color(&mut self.buffer, &self.colors)
    }

    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    fn format_type_name(&self, ty: &AlgebraicType) -> String {
        fmt_algebraic_type(ty).to_string()
    }

    fn write_type_name(&mut self, ty: &AlgebraicType) -> io::Result<()> {
        let s = self.format_type_name(ty);
        self.write_colored(&s, Some(self.colors.column_type), false)
    }

    fn format_access(&self, access: TableAccess) -> &'static str {
        match access {
            TableAccess::Private => "private",
            TableAccess::Public => "public",
        }
    }

    fn write_access(&mut self, access: TableAccess) -> io::Result<()> {
        let s = self.format_access(access);
        self.write_colored(s, Some(self.colors.access), false)
    }
}

impl MigrationFormatter for TermColorFormatter {
    fn format_header(&mut self) -> io::Result<()> {
        let line = "━".repeat(60);
        self.write_line(&line)?;
        self.write_colored_line("Database Migration Plan", Some(self.colors.header), true)?;
        self.write_line(&line)?;
        self.write_line("")
    }

    fn format_add_table(&mut self, table: &TableInfo) -> io::Result<()> {
        // Table header
        self.write_indent()?;
        self.buffer.write_all("▸ ".to_string().as_bytes())?;
        Action::Created.write_with_color(&mut self.buffer, &self.colors)?;
        let kind = if table.is_system { "system" } else { "user" };
        self.buffer.write_all(format!(" {kind} table: ").as_bytes())?;
        self.write_colored(&table.name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b" (")?;
        self.write_access(table.access)?;
        self.buffer.write_all(b")\n")?;

        self.indent();

        if !table.columns.is_empty() {
            self.write_colored_line("Columns:", Some(self.colors.section_header), true)?;
            self.indent();
            for col in &table.columns {
                self.write_indent()?;
                self.buffer.write_all(format!("• {}: ", col.name).as_bytes())?;
                self.write_type_name(&col.type_name)?;
                self.buffer.write_all(b"\n")?;
            }
            self.dedent();
        }

        if !table.constraints.is_empty() {
            self.write_colored_line("Unique constraints:", Some(self.colors.section_header), true)?;
            self.indent();
            for c in &table.constraints {
                let cols = c.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
                self.write_bullet(&format!("{} on [{}]", c.name, cols))?;
            }
            self.dedent();
        }

        if !table.indexes.is_empty() {
            self.write_colored_line("Indexes:", Some(self.colors.section_header), true)?;
            self.indent();
            for i in &table.indexes {
                let cols = i.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
                self.write_bullet(&format!("{} on [{}]", i.name, cols))?;
            }
            self.dedent();
        }

        if !table.sequences.is_empty() {
            self.write_colored_line("Auto-increment constraints:", Some(self.colors.section_header), true)?;
            self.indent();
            for s in &table.sequences {
                self.write_bullet(&format!("{} on {}", s.name, s.column_name))?;
            }
            self.dedent();
        }

        if let Some(s) = &table.schedule {
            self.write_colored_line("Schedule:", Some(self.colors.section_header), true)?;
            self.indent();
            self.write_bullet(&format!("Calls {}: {}", s.function_kind, s.function_name))?;
            self.dedent();
        }

        self.dedent();
        self.write_line("")
    }

    fn format_view(&mut self, view: &ViewInfo, action: Action) -> io::Result<()> {
        self.write_indent()?;
        self.buffer.write_all("▸ ".to_string().as_bytes())?;
        self.write_action_prefix(&action)?;
        self.buffer.write_all(if view.is_anonymous {
            b" anonymous view: "
        } else {
            b" view: "
        })?;
        self.write_colored(&view.name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b"\n")?;

        self.indent();

        if !view.params.is_empty() {
            self.write_colored_line("Parameters:", Some(self.colors.section_header), true)?;
            self.indent();
            for col in &view.params {
                self.write_indent()?;
                self.buffer.write_all(format!("• {}: ", col.name).as_bytes())?;
                self.write_type_name(&col.type_name)?;
                self.buffer.write_all(b"\n")?;
            }
            self.dedent();
        }

        if !view.columns.is_empty() {
            self.write_colored_line("Columns:", Some(self.colors.section_header), true)?;
            self.indent();
            for col in &view.columns {
                self.write_indent()?;
                self.buffer.write_all(format!("• {}: ", col.name).as_bytes())?;
                self.write_type_name(&col.type_name)?;
                self.buffer.write_all(b"\n")?;
            }
            self.dedent();
        }

        self.dedent();
        self.write_line("")
    }

    fn format_constraint(&mut self, c: &ConstraintInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        let cols = c.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
        self.buffer
            .write_all(format!(" unique constraint {} on [{}] of table ", c.name, cols).as_bytes())?;
        self.write_colored(&c.table_name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b"\n")
    }

    fn format_index(&mut self, i: &IndexInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        let cols = i.columns.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
        self.buffer
            .write_all(format!(" index {} on [{}] of table ", i.name, cols).as_bytes())?;
        self.write_colored(&i.table_name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b"\n")
    }

    fn format_sequence(&mut self, s: &SequenceInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        self.buffer.write_all(
            format!(
                " auto-increment constraint {} on column {} of table ",
                s.name, s.column_name
            )
            .as_bytes(),
        )?;
        self.write_colored(&s.table_name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b"\n")
    }

    fn format_change_access(&mut self, a: &AccessChangeInfo) -> io::Result<()> {
        let direction = match a.new_access {
            TableAccess::Private => "public → private",
            TableAccess::Public => "private → public",
        };
        self.write_action_prefix(&Action::Changed)?;
        self.buffer.write_all(b" access for table ")?;
        self.write_colored(&a.table_name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b" (")?;
        self.write_colored(direction, Some(self.colors.access), false)?;
        self.buffer.write_all(b")\n")
    }

    fn format_schedule(&mut self, s: &ScheduleInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        self.buffer.write_all(b" schedule for table ")?;
        self.write_colored(&s.table_name, Some(self.colors.table_name), true)?;
        self.buffer
            .write_all(format!(" calling {} {}\n", s.function_kind, s.function_name).as_bytes())
    }

    fn format_rls(&mut self, r: &RlsInfo, action: Action) -> io::Result<()> {
        self.write_action_prefix(&action)?;
        self.buffer.write_all(b" row level security policy:\n")?;
        self.indent();
        self.write_indent()?;
        self.buffer.write_all(b"`")?;
        self.write_colored(&r.policy, Some(self.colors.section_header), false)?;
        self.buffer.write_all(b"`\n")?;
        self.dedent();
        Ok(())
    }

    fn format_change_columns(&mut self, cs: &ColumnChanges) -> io::Result<()> {
        self.write_action_prefix(&Action::Changed)?;
        self.buffer.write_all(b" columns for table ")?;
        self.write_colored(&cs.table_name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b"\n")?;

        self.indent();
        for ch in &cs.changes {
            self.write_indent()?;
            match ch {
                ColumnChange::Renamed { old_name, new_name } => {
                    self.buffer
                        .write_all(format!("~ Renamed: {old_name} → {new_name}\n").as_bytes())?;
                }
                ColumnChange::TypeChanged {
                    name,
                    old_type,
                    new_type,
                } => {
                    self.buffer.write_all(format!("~ Modified: {name} (").as_bytes())?;
                    self.write_type_name(old_type)?;
                    self.buffer.write_all(" → ".to_string().as_bytes())?;
                    self.write_type_name(new_type)?;
                    self.buffer.write_all(b")\n")?;
                }
            }
        }
        self.dedent();
        Ok(())
    }

    fn format_add_columns(&mut self, nc: &NewColumns) -> io::Result<()> {
        let plural = if nc.columns.len() > 1 { "s" } else { "" };
        self.write_action_prefix(&Action::Created)?;
        self.buffer.write_all(format!(" column{plural} in table ").as_bytes())?;
        self.write_colored(&nc.table_name, Some(self.colors.table_name), true)?;
        self.buffer.write_all(b"\n")?;

        self.indent();
        for col in &nc.columns {
            let default = col
                .default_value
                .as_ref()
                .map(|v| format!(" (default: {v:#?})"))
                .unwrap_or_default();
            self.write_indent()?;
            self.buffer.write_all(format!("+ {}: ", col.name).as_bytes())?;
            self.write_type_name(&col.type_name)?;
            self.buffer.write_all(format!("{default}\n").as_bytes())?;
        }
        self.dedent();
        Ok(())
    }

    fn format_disconnect_warning(&mut self) -> io::Result<()> {
        self.write_indent()?;
        self.write_with_background(
            "!!! Warning: All clients will be disconnected due to breaking schema changes",
            self.colors.warning,
            true,
        )?;
        self.buffer.write_all(b"\n")
    }
}

trait ActionColorExt {
    fn write_with_color(&self, buffer: &mut Buffer, colors: &ColorScheme) -> io::Result<()>;
}

impl ActionColorExt for Action {
    fn write_with_color(&self, buffer: &mut Buffer, colors: &ColorScheme) -> io::Result<()> {
        let (text, color) = match self {
            Action::Created => ("Created", colors.created),
            Action::Removed => ("Removed", colors.removed),
            Action::Changed => ("Changed", colors.changed),
        };
        let mut spec = ColorSpec::new();
        spec.set_fg(Some(color)).set_bold(true);
        buffer.set_color(&spec)?;
        buffer.write_all(text.as_bytes())?;
        buffer.reset()?;
        Ok(())
    }
}
