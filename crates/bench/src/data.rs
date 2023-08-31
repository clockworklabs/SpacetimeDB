use crate::utils::{encode, ResultBench, START_B};
use clap::ValueEnum;
use spacetimedb_lib::sats::SatsString;
use std::marker::PhantomData;
use std::ops::Range;

pub trait BuildDb {
    fn build(prefill: bool, fsync: bool) -> ResultBench<Self>
    where
        Self: Sized;
}

pub struct Pool<T> {
    instance: u8,
    pub(crate) prefill: bool,
    pub(crate) fsync: bool,
    _x: PhantomData<T>,
}

impl<T: BuildDb> Pool<T> {
    pub fn new(prefill: bool, fsync: bool) -> ResultBench<Self> {
        Ok(Self {
            instance: 0,
            prefill,
            fsync,
            _x: Default::default(),
        })
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> ResultBench<T> {
        self.instance += 1;
        T::build(self.prefill, self.fsync)
    }
}

#[derive(Debug)]
pub struct Data {
    pub(crate) a: i32,
    pub(crate) b: u64,
    pub(crate) c: SatsString,
}

impl Data {
    pub fn new(a: i32) -> Self {
        let b = (a as u64) + START_B;
        Self {
            a,
            b,
            c: SatsString::from_string(encode(b)),
        }
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
