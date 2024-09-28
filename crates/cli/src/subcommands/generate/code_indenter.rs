use std::fmt;
use std::ops::{Deref, DerefMut};

pub(super) type Indenter = CodeIndenter<String>;

#[macro_export]
macro_rules! indent_scope {
    ($x:ident) => {
        let mut $x = $x.indented(1);
    };
}

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

    /// Invoke `f` while indenting one level greater than `self` currently does.
    pub fn with_indent<Res>(&mut self, f: impl FnOnce(&mut Self) -> Res) -> Res {
        let mut indenter = self.indented(1);
        f(&mut indenter)
    }

    // Writes a newline without setting the `needs_indenting` flag.
    // TODO(cloutiertyler): I think it should set the flag, but I don't know
    // if anyone is relying on the current behavior.
    pub fn newline(&mut self) {
        self.writer.write_char('\n').unwrap();
    }

    /// Print an indented block delimited by `before` and `after`, with body written by `f`.
    pub fn delimited_block<Res>(&mut self, before: &str, f: impl FnOnce(&mut Self) -> Res, after: &str) -> Res {
        self.writer.write_str(before).unwrap();
        let res = self.with_indent(|out| {
            out.newline();
            // Need an explicit `write_indent` call here because calling `out.newline`
            // will not cause the subsequent line to be indented, as `write_str` thinks
            // it's an empty line.
            out.write_indent().unwrap();
            f(out)
        });
        self.writer.write_str(after).unwrap();
        res
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
impl CodeIndenter<String> {
    // This method allows `write!` and `writeln!` to be used directly on `CodeIndenter`.
    // It does the same thing as using it via the `fmt::Write` trait but in a non-fallible manner.
    // This is only allowed on `String` because it's the only type that can't fail to write.
    pub fn write_fmt(&mut self, args: fmt::Arguments) {
        fmt::Write::write_fmt(self, args).unwrap()
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
