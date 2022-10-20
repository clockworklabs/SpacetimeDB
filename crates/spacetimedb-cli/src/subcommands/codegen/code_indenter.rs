use std::fmt;
use std::ops::{Deref, DerefMut};

pub struct CodeIndenter<W: fmt::Write> {
    writer: W,
    level: u32,
    needs_indenting: bool,
}
impl<W: fmt::Write> CodeIndenter<W> {
    pub fn new(writer: W) -> Self {
        CodeIndenter {
            writer,
            level: 0,
            needs_indenting: true,
        }
    }
    // pub fn get_ref(&self) -> &W {
    //     &self.writer
    // }
    // pub fn get_mut(&mut self) -> &mut W {
    //     &mut self.writer
    // }
    pub fn into_inner(self) -> W {
        self.writer
    }
    pub fn indent(&mut self, n: u32) {
        self.level = self.level.saturating_add(n);
    }
    pub fn dedent(&mut self, n: u32) {
        self.level = self.level.saturating_sub(n);
    }
    pub fn indented(&mut self, n: u32) -> IndentScope<'_, W> {
        self.indent(n);
        IndentScope { fmt: self }
    }
    fn write_indent(&mut self) -> fmt::Result {
        for _ in 0..self.level {
            self.writer.write_str(super::INDENT)?;
        }
        Ok(())
    }
}
impl<W: fmt::Write> fmt::Write for CodeIndenter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for (i, line) in s.split_inclusive('\n').enumerate() {
            let write_indent = i != 0 || std::mem::take(&mut self.needs_indenting);
            // skip the indent if it's an empty line
            if write_indent && line != "\n" {
                self.write_indent()?;
            }
            self.writer.write_str(line)?;
        }
        self.needs_indenting = s.ends_with('\n');
        Ok(())
    }
}
pub struct IndentScope<'a, W: fmt::Write> {
    fmt: &'a mut CodeIndenter<W>,
}
impl<W: fmt::Write> Drop for IndentScope<'_, W> {
    fn drop(&mut self) {
        self.fmt.dedent(1);
    }
}
impl<T: fmt::Write> Deref for IndentScope<'_, T> {
    type Target = CodeIndenter<T>;
    fn deref(&self) -> &Self::Target {
        self.fmt
    }
}
impl<T: fmt::Write> DerefMut for IndentScope<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.fmt
    }
}
