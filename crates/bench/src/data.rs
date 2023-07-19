use crate::utils::{encode, START_B};
use clap::ValueEnum;
use std::ops::Range;

#[derive(Debug)]
pub struct Data {
    pub(crate) a: i32,
    pub(crate) b: u64,
    pub(crate) c: String,
}

impl Data {
    pub fn new(a: i32) -> Self {
        let b = (a as u64) + START_B;
        Self { a, b, c: encode(b) }
    }
}

/// Database engine to use
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum DbEngine {
    Sqlite,
    Spacetime,
}

/// # of Rows to use in the benchmark
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Runs {
    /// Tiny = 100
    Tiny = 100,
    /// Small = 1000
    Small = 1000,
    /// Medium = 5000
    Medium = 5000,
    /// Large = 25000
    Large = 25000,
}

impl Runs {
    pub fn range(self) -> Range<u16> {
        let x = self as u16;
        0..x
    }

    pub fn data(self) -> impl Iterator<Item = Data> {
        let x = self as u16;
        (0..x).map(|x| Data::new(x as i32))
    }
}
