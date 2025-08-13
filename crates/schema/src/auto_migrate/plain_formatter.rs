use super::{
    ansi_formatter::{AnsiFormatter, ColorScheme},
    formatter::MigrationFormatter,
};
use lazy_static::lazy_static;
use regex::Regex;
pub struct PlainFormatter(AnsiFormatter);

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

impl PlainFormatter {
    pub fn new(cap: usize) -> Self {
        Self(AnsiFormatter::new(cap, ColorScheme::default()))
    }
}

impl std::fmt::Display for PlainFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted = self.0.to_string();
        let cleaned = ANSI_ESCAPE_SEQUENCE.replace_all(&formatted, "");
        write!(f, "{cleaned}")
    }
}

impl MigrationFormatter for PlainFormatter {
    fn format_header(&mut self) {
        self.0.format_header();
    }

    fn format_add_table(&mut self, table_info: &super::formatter::TableInfo) {
        self.0.format_add_table(table_info);
    }

    fn format_index(&mut self, index_info: &super::formatter::IndexInfo, action: super::formatter::Action) {
        self.0.format_index(index_info, action);
    }

    fn format_constraint(
        &mut self,
        constraint_info: &super::formatter::ConstraintInfo,
        action: super::formatter::Action,
    ) {
        self.0.format_constraint(constraint_info, action);
    }

    fn format_sequence(&mut self, sequence_info: &super::formatter::SequenceInfo, action: super::formatter::Action) {
        self.0.format_sequence(sequence_info, action);
    }

    fn format_change_access(&mut self, access_info: &super::formatter::AccessChangeInfo) {
        self.0.format_change_access(access_info);
    }

    fn format_schedule(&mut self, schedule_info: &super::formatter::ScheduleInfo, action: super::formatter::Action) {
        self.0.format_schedule(schedule_info, action);
    }

    fn format_rls(&mut self, rls_info: &super::formatter::RlsInfo, action: super::formatter::Action) {
        self.0.format_rls(rls_info, action);
    }

    fn format_change_columns(&mut self, column_changes: &super::formatter::ColumnChanges) {
        self.0.format_change_columns(column_changes);
    }

    fn format_add_columns(&mut self, new_columns: &super::formatter::NewColumns) {
        self.0.format_add_columns(new_columns);
    }

    fn format_disconnect_warning(&mut self) {
        self.0.format_disconnect_warning();
    }
}
