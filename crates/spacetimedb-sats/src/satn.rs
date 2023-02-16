use std::fmt::{self, Write as _};

use crate::fmt_fn;

pub trait Satn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;

    fn to_satn(&self) -> String {
        Wrapper::from_ref(self).to_string()
    }
    fn to_satn_pretty(&self) -> String {
        format!("{:#}", Wrapper::from_ref(self))
    }
}

#[repr(transparent)]
pub struct Wrapper<T: ?Sized>(pub T);

impl<T: ?Sized> Wrapper<T> {
    pub fn from_ref(t: &T) -> &Self {
        unsafe { &*(t as *const T as *const Wrapper<T>) }
    }
}

impl<T: Satn + ?Sized> fmt::Display for Wrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl<T: Satn + ?Sized> fmt::Debug for Wrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub(crate) struct EntryWrapper<'a, 'b: 'a, const SEP: char> {
    fmt: &'a mut fmt::Formatter<'b>,
    has_fields: bool,
}

impl<'a, 'b: 'a, const SEP: char> EntryWrapper<'a, 'b, SEP> {
    pub fn new(fmt: &'a mut fmt::Formatter<'b>) -> Self {
        Self { fmt, has_fields: false }
    }
    pub fn entry<F: Fn(&mut fmt::Formatter) -> fmt::Result>(&mut self, entry: F) -> fmt::Result {
        let res = (|| {
            if self.is_pretty() {
                if !self.has_fields {
                    self.fmt.write_str("\n")?;
                }
                let state = &mut Default::default();
                let mut writer = PadAdapter { fmt: self.fmt, state };
                write!(writer, "{:#}", fmt_fn(entry))?;
                writer.write_char(SEP)?;
                writer.write_char('\n')
            } else {
                if self.has_fields {
                    self.fmt.write_char(SEP)?;
                    self.fmt.write_char(' ')?;
                }
                entry(self.fmt)
            }
        })();
        self.has_fields = true;
        res
    }

    pub fn entries<F: Fn(&mut fmt::Formatter) -> fmt::Result>(
        &mut self,
        it: impl IntoIterator<Item = F>,
    ) -> fmt::Result {
        it.into_iter().try_for_each(|e| self.entry(e))
    }

    fn is_pretty(&self) -> bool {
        self.fmt.alternate()
    }
}

pub(crate) struct PadAdapter<'a, 'b, 'state> {
    fmt: &'a mut fmt::Formatter<'b>,
    state: &'state mut PadAdapterState,
}

struct PadAdapterState {
    on_newline: bool,
}

impl Default for PadAdapterState {
    fn default() -> Self {
        PadAdapterState { on_newline: true }
    }
}

impl fmt::Write for PadAdapter<'_, '_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for s in s.split_inclusive('\n') {
            if self.state.on_newline {
                self.fmt.write_str("    ")?;
            }

            self.state.on_newline = s.ends_with('\n');
            self.fmt.write_str(s)?;
        }
        Ok(())
    }
}
