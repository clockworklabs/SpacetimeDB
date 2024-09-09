use crate::parser::parser::SqlErrorWithLocation;
use ariadne::{ColorGenerator, Label, Report, ReportKind, Source};
use core::fmt;
use std::ops::Range;

pub struct UserError<'a> {
    pub error: SqlErrorWithLocation<'a>,
    pub report: Report<'a, (&'static str, Range<usize>)>,
}

impl<'a> UserError<'a> {
    pub(crate) fn print(&self) -> std::io::Result<()> {
        self.report.eprint(("SQL", Source::from(self.error.sql)))
    }
}

impl<'a> fmt::Display for UserError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut vec = Vec::new();
        self.report
            .write(("SQL", Source::from(self.error.sql)), &mut vec)
            .unwrap();
        write!(f, "{vec}", vec = String::from_utf8(vec).unwrap())
    }
}

pub fn to_fancy_error(error: SqlErrorWithLocation<'_>) -> UserError<'_> {
    let mut colors = ColorGenerator::new();
    let color = colors.next();
    let report = Report::build(ReportKind::Error, "SQL", 0)
        .with_message(error.error.to_string())
        .with_label(
            Label::new(("SQL", error.span.clone()))
                .with_message(error.label.clone())
                .with_color(color),
        );

    UserError {
        error,
        report: report.finish(),
    }
}
