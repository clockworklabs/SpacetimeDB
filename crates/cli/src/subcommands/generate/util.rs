//! Various utility functions that the generate modules have in common.

use std::fmt::{Display, Formatter, Result};

/// Turns a closure `f: Fn(&mut Formatter) -> Result` into `fmt::Display`.
pub(super) fn fmt_fn(f: impl Fn(&mut Formatter) -> Result) -> impl Display {
    struct FDisplay<F>(F);
    impl<F: Fn(&mut Formatter) -> Result> Display for FDisplay<F> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result {
            (self.0)(f)
        }
    }
    FDisplay(f)
}
