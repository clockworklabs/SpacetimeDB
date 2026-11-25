//! Assertions for rules and world hooks.
//!
//! Names and meaning follow the Antithesis SDK, so these can be forwarded
//! there later: `assert_always` must hold every time it is evaluated, and
//! `assert_sometimes` must hold at least once per campaign. They are plain
//! functions, so a failing check returns through a visible `?`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;

/// An assertion that did not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionFailed {
    pub name: &'static str,
    pub details: String,
}

impl fmt::Display for AssertionFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "assertion failed: {}", self.name)?;
        if !self.details.is_empty() {
            write!(formatter, ": {}", self.details)?;
        }
        Ok(())
    }
}

impl std::error::Error for AssertionFailed {}

/// Error for an assertion that failed unconditionally.
pub fn assertion_failed(name: &'static str, details: String) -> anyhow::Error {
    AssertionFailed { name, details }.into()
}

/// Fail unless `condition` holds. `details` runs only on failure.
pub fn assert_always(condition: bool, name: &'static str, details: impl FnOnce() -> String) -> anyhow::Result<()> {
    if condition {
        return Ok(());
    }
    Err(assertion_failed(name, details()))
}

/// Hit counts of one `assert_sometimes`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SometimesCount {
    pub evaluated: u64,
    pub held: u64,
}

thread_local! {
    static SOMETIMES: RefCell<BTreeMap<&'static str, SometimesCount>> = const { RefCell::new(BTreeMap::new()) };
}

/// Record whether `condition` held. Campaign reports list every
/// `assert_sometimes` with its hit count, so a path the workload stopped
/// reaching becomes visible.
pub fn assert_sometimes(condition: bool, name: &'static str) {
    SOMETIMES.with(|counts| {
        let mut counts = counts.borrow_mut();
        let count = counts.entry(name).or_default();
        count.evaluated += 1;
        count.held += condition as u64;
        assert!(count.held <= count.evaluated);
    });
}

/// Take and reset the `assert_sometimes` counts recorded on this thread.
pub(crate) fn take_sometimes() -> BTreeMap<&'static str, SometimesCount> {
    SOMETIMES.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(value: u64) -> anyhow::Result<()> {
        assert_always(value < 10, "value is small", || format!("value was {value}"))?;
        assert_sometimes(value == 3, "value is three");
        Ok(())
    }

    #[test]
    fn assert_always_reports_name_and_details() {
        let err = check(12).unwrap_err();
        let failed = err.downcast_ref::<AssertionFailed>().unwrap();
        assert_eq!(failed.name, "value is small");
        assert_eq!(failed.details, "value was 12");
    }

    #[test]
    fn assert_sometimes_counts_hits() {
        take_sometimes();
        for value in [1, 3, 3, 5] {
            check(value).unwrap();
        }
        let counts = take_sometimes();
        let count = counts["value is three"];
        assert_eq!((count.evaluated, count.held), (4, 2));
    }
}
