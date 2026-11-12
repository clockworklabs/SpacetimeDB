use std::io::{self, Write};

use termcolor::{Buffer, Color, ColorSpec, WriteColor};

use crate::auto_migrate::PrettyPrintStyle;

/// Color scheme for consistent formatting
#[derive(Debug, Clone)]
pub(crate) struct ColorScheme {
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

/// An indent-aware, optionally-coloured text buffer.
///
/// Shared by the migration-plan formatter and the describe output, so both render
/// with the same colour scheme and the same colour/no-colour handling.
#[derive(Debug)]
pub(crate) struct StyledWriter {
    buffer: Buffer,
    colors: ColorScheme,
    indent_level: usize,
    indent_width: usize,
}

impl StyledWriter {
    pub(crate) fn new(style: PrettyPrintStyle, indent_width: usize) -> Self {
        Self {
            buffer: match style {
                PrettyPrintStyle::NoColor => Buffer::no_color(),
                PrettyPrintStyle::AnsiColor => Buffer::ansi(),
            },
            colors: ColorScheme::default(),
            indent_level: 0,
            indent_width,
        }
    }

    pub(crate) fn colors(&self) -> &ColorScheme {
        &self.colors
    }

    pub(crate) fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub(crate) fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    pub(crate) fn write_indent(&mut self) -> io::Result<()> {
        let indent = " ".repeat(self.indent_width * self.indent_level);
        self.buffer.write_all(indent.as_bytes())
    }

    pub(crate) fn write_plain(&mut self, text: &str) -> io::Result<()> {
        self.buffer.write_all(text.as_bytes())
    }

    pub(crate) fn write_line(&mut self, text: impl AsRef<str>) -> io::Result<()> {
        self.write_indent()?;
        self.buffer.write_all(text.as_ref().as_bytes())?;
        self.buffer.write_all(b"\n")
    }

    pub(crate) fn write_colored(&mut self, text: &str, color: Option<Color>, bold: bool) -> io::Result<()> {
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

    pub(crate) fn write_colored_line(&mut self, text: &str, color: Option<Color>, bold: bool) -> io::Result<()> {
        self.write_indent()?;
        self.write_colored(text, color, bold)?;
        self.buffer.write_all(b"\n")
    }

    pub(crate) fn write_with_background(&mut self, text: &str, bg: Color, bold: bool) -> io::Result<()> {
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

    pub(crate) fn into_string(self) -> String {
        String::from_utf8(self.buffer.into_inner()).expect("StyledWriter only writes &str, so output is UTF-8")
    }
}
