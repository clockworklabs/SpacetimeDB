//! A bare-bones version of `std::collections::BTreeMap`
//! providing only the API that [`crate::btree_index::MultiMap`] requires.
//!
//! The main difference,
//! and the reason why we fork std's map is that the `Borrow` trait is in the way.

#![allow(
    clippy::type_complexity,
    clippy::needless_borrow,
    clippy::unnecessary_mut_passed,
    clippy::clone_on_copy,
    clippy::drop_non_drop,
    clippy::mem_replace_option_with_none,
    unstable_name_collisions
)]

mod borrow;
mod entry;
mod map;
mod mem;
mod navigate;
mod node;
mod polyfill;
mod search;

pub use map::{BTreeMap, Range, Values};
