#![no_std]

#[cfg(any(feature = "sim", feature = "alloc"))]
extern crate alloc;
#[cfg(test)]
extern crate std;

#[cfg(feature = "sim")]
pub mod sim;

pub mod io;
