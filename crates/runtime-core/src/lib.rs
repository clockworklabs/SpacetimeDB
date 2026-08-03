#![no_std]

#[cfg(feature = "sim")]
extern crate alloc;
#[cfg(test)]
extern crate std;

#[cfg(feature = "sim")]
pub mod sim;

pub mod io;
