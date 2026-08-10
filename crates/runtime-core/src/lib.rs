#![no_std]

#[cfg(feature = "sim")]
extern crate alloc;
#[cfg(any(feature = "std", test))]
extern crate std;

#[cfg(feature = "sim")]
pub mod sim;
