use std::fmt;
use std::iter::successors;

pub const DB_POOL: u8 = 10;
pub const SQLITE: &str = "Sqlite";
pub const SPACETIME: &str = "SpacetimeDB";
//TODO: This should be 100 and in the prefill steps run with Large, but that take too much time with spacetimedb
pub const START_B: u64 = 10;

/// A wrapper for using on test so the error display nicely
pub struct TestError {
    pub error: Box<dyn std::error::Error>,
}

impl fmt::Debug for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Format the error in yellow
        write!(f, "\x1b[93m{}\x1b[0m", self.error)
    }
}

impl<E: std::error::Error + 'static> From<E> for TestError {
    fn from(e: E) -> Self {
        Self { error: Box::new(e) }
    }
}

/// A wrapper for using [Result] in tests, so it display nicely
pub type ResultBench<T> = Result<T, TestError>;

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "zero", "ten", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const ORDERS: [&str; 7] = [
    "zero",
    "thousand",
    "million",
    "billion",
    "trillion",
    "quadrillion",
    "quintillion", // enough for u64::MAX
];

fn format_num(num: u64, div: u64, order: &str) -> String {
    match (num / div, num % div) {
        (upper, 0) => format!("{} {}", encode(upper), order),
        (upper, lower) => {
            format!("{} {} {}", encode(upper), order, encode(lower))
        }
    }
}

pub fn encode(num: u64) -> String {
    match num {
        0..=19 => ONES[num as usize].to_string(),
        20..=99 => {
            let upper = (num / 10) as usize;
            match num % 10 {
                0 => TENS[upper].to_string(),
                lower => format!("{}-{}", TENS[upper], encode(lower)),
            }
        }
        100..=999 => format_num(num, 100, "hundred"),
        _ => {
            let (div, order) = successors(Some(1u64), |v| v.checked_mul(1000))
                .zip(ORDERS.iter())
                .find(|&(e, _)| e > num / 1000)
                .unwrap();

            format_num(num, div, order)
        }
    }
}
