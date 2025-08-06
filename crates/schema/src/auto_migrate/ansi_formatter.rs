use std::fmt;

use colored::{control::set_override, Color, Colorize as _};
use spacetimedb_lib::{db::raw_def::v9::TableAccess, AlgebraicType};
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;

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
            removed: Color::BrightRed,
            changed: Color::Yellow,
            header: Color::BrightBlue,
            table_name: Color::Cyan,
            column_type: Color::Magenta,
            section_header: Color::Blue,
            access: Color::BrightGreen,
            warning: Color::Red,
        }
    }
}

#[derive(Debug)]
pub struct AnsiFormatter {
    buffer: String,
    colors: ColorScheme,
    indent_level: usize,
}

impl fmt::Display for AnsiFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.buffer)
    }
}

impl AnsiFormatter {
    /// Create a new formatter with custom colors
    pub fn new(cap: usize, colors: ColorScheme) -> Self {
        // This overrides `NO_COLOR` as `ANSIFormatter` should always be colored.
        set_override(true);
        Self {
            buffer: String::with_capacity(cap),
            colors,
            indent_level: 0,
        }
    }

    /// Add a line with proper indentation
    fn add_line(&mut self, text: impl AsRef<str>) {
        let indent = "    ".repeat(self.indent_level);

        self.buffer.push_str(&format!("{}{}\n", indent, text.as_ref()));
    }

    /// Increase indentation level
    fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease indentation level
    fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Format a type name with consistent coloring
    fn format_type_name(&self, type_name: &AlgebraicType) -> String {
        fmt_algebraic_type(type_name)
            .to_string()
            .color(self.colors.column_type)
            .to_string()
    }

    /// Format table access with consistent coloring
    fn format_access(&self, access: TableAccess) -> String {
        match access {
            TableAccess::Private => "private",
            TableAccess::Public => "public",
        }
        .color(self.colors.access)
        .to_string()
    }

    /// Format a section header
    fn format_section_header(&self, text: &str) -> String {
        text.color(self.colors.section_header).bold().to_string()
    }

    /// Format an item bullet point
    fn format_item(&self, text: &str) -> String {
        format!("• {text}")
    }

    /// Format table header line
    fn format_table_header_line(&self, name: &str, is_system: bool, access: TableAccess, action: &Action) -> String {
        format!(
            "▸ {} {} table: {} ({})",
            action.format_with_color(&self.colors),
            if is_system { "system" } else { "user" },
            name.color(self.colors.table_name).bold(),
            self.format_access(access)
        )
    }
}

impl MigrationFormatter for AnsiFormatter {
    fn format_header(&mut self) {
        let header_line = "━".repeat(60);
        let title = "Database Migration Plan".color(self.colors.header).bold();
        self.add_line(&header_line);
        self.add_line(title.to_string());
        self.add_line(&header_line);
        self.add_line("");
    }

    fn format_add_table(&mut self, table_info: &TableInfo) {
        // Table header
        let header = self.format_table_header_line(
            &table_info.name,
            table_info.is_system,
            table_info.access,
            &Action::Created,
        );
        self.add_line(&header);

        self.indent();

        // Columns section
        if !table_info.columns.is_empty() {
            self.add_line(&*self.format_section_header("Columns:"));
            self.indent();
            for column in &table_info.columns {
                let column_text = format!("{}: {}", &column.name, self.format_type_name(&column.type_name));
                self.add_line(self.format_item(&column_text));
            }
            self.dedent();
        }

        // Constraints section
        if !table_info.constraints.is_empty() {
            self.add_line(&*self.format_section_header("Unique constraints:"));
            self.indent();
            for constraint in &table_info.constraints {
                let constraint_text = format!(
                    "{} on [{}]",
                    &constraint.name,
                    constraint
                        .columns
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.add_line(self.format_item(&constraint_text));
            }
            self.dedent();
        }

        // Indexes section
        if !table_info.indexes.is_empty() {
            self.add_line(&*self.format_section_header("Indexes:"));
            self.indent();
            for index in &table_info.indexes {
                let index_text = format!(
                    "{} on [{}]",
                    &index.name,
                    index
                        .columns
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.add_line(self.format_item(&index_text));
            }
            self.dedent();
        }

        // Sequences section
        if !table_info.sequences.is_empty() {
            self.add_line(&*self.format_section_header("Auto-increment constraints:"));
            self.indent();
            for sequence in &table_info.sequences {
                let sequence_text = format!("{} on {}", &sequence.name, &sequence.column_name);
                self.add_line(self.format_item(&sequence_text));
            }
            self.dedent();
        }

        // Schedule section
        if let Some(schedule) = &table_info.schedule {
            self.add_line(&*self.format_section_header("Schedule:"));
            self.indent();
            let schedule_text = format!("Calls reducer: {}", &schedule.reducer_name);
            self.add_line(self.format_item(&schedule_text));
            self.dedent();
        }

        self.dedent();
        self.add_line("");
    }

    fn format_constraint(&mut self, constraint_info: &ConstraintInfo, action: Action) {
        let text = format!(
            "▸ {} unique constraint {} on [{}] of table {}",
            action.format_with_color(&self.colors),
            &constraint_info.name,
            constraint_info
                .columns
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            constraint_info.table_name.color(self.colors.table_name).bold()
        );
        self.add_line(&text);
    }
    fn format_index(&mut self, index_info: &IndexInfo, action: Action) {
        let text = format!(
            "▸ {} index {} on [{}] of table {}",
            action.format_with_color(&self.colors),
            &index_info.name,
            index_info
                .columns
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            index_info.table_name.color(self.colors.table_name).bold()
        );
        self.add_line(&text);
    }

    fn format_sequence(&mut self, sequence_info: &SequenceInfo, action: Action) {
        let text = format!(
            "▸ {} auto-increment constraint {} on column {} of table {}",
            action.format_with_color(&self.colors),
            &sequence_info.name,
            &sequence_info.column_name,
            sequence_info.table_name.color(self.colors.table_name).bold()
        );
        self.add_line(&text);
    }

    fn format_change_access(&mut self, access_info: &AccessChangeInfo) {
        let direction = match access_info.new_access {
            TableAccess::Private => "public → private",
            TableAccess::Public => "private → public",
        };

        let text = format!(
            "▸ {} access for table {} ({})",
            Action::Changed.format_with_color(&self.colors),
            access_info.table_name.color(self.colors.table_name).bold(),
            direction.color(self.colors.access)
        );
        self.add_line(&text);
    }

    fn format_schedule(&mut self, schedule_info: &ScheduleInfo, action: Action) {
        let text = format!(
            "▸ {} schedule for table {} calling reducer {}",
            action.format_with_color(&self.colors),
            schedule_info.table_name.color(self.colors.table_name).bold(),
            &schedule_info.reducer_name
        );
        self.add_line(&text);
    }

    fn format_rls(&mut self, rls_info: &RlsInfo, action: Action) {
        self.add_line(format!(
            "▸ {} row level security policy:",
            action.format_with_color(&self.colors)
        ));
        self.indent();
        self.add_line(format!("`{}`", rls_info.policy.color(self.colors.section_header)));
        self.dedent();
    }

    fn format_change_columns(&mut self, column_changes: &ColumnChanges) {
        self.add_line(format!(
            "▸ {} columns for table {}",
            Action::Changed.format_with_color(&self.colors),
            column_changes.table_name.color(self.colors.table_name).bold()
        ));

        self.indent();
        for change in &column_changes.changes {
            match change {
                ColumnChange::Renamed { old_name, new_name } => {
                    self.add_line(format!("~ Renamed: {old_name} → {new_name}"));
                }
                ColumnChange::TypeChanged {
                    name,
                    old_type,
                    new_type,
                } => {
                    self.add_line(format!(
                        "~ Modified: {} ({} → {})",
                        name,
                        self.format_type_name(old_type),
                        self.format_type_name(new_type)
                    ));
                }
            }
        }
        self.dedent();
    }

    fn format_add_columns(&mut self, new_columns: &NewColumns) {
        let plural = if new_columns.columns.len() > 1 { "s" } else { "" };
        self.add_line(format!(
            "▸ {} column{} in table {}",
            Action::Created.format_with_color(&self.colors),
            plural,
            new_columns.table_name.color(self.colors.table_name).bold()
        ));

        self.indent();
        for column in &new_columns.columns {
            let default_text = if let Some(av) = &column.default_value {
                format!(" (default: {av:#?})")
            } else {
                String::new()
            };

            self.add_line(format!(
                "+ {}: {}{}",
                &column.name,
                self.format_type_name(&column.type_name),
                default_text
            ));
        }
        self.dedent();
    }

    fn format_disconnect_warning(&mut self) {
        self.add_line(
            "!!! Warning: All clients will be disconnected due to breaking schema changes"
                .bold()
                .on_color(self.colors.warning)
                .to_string(),
        );
    }
}
