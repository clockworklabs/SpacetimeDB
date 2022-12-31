pub mod args;
pub mod buffer;
pub mod data_key;
pub mod error;
pub mod hash;
pub mod primary_key;
mod serde_mapping;
pub mod type_def;
pub mod type_value;
pub mod version;

pub use data_key::DataKey;
pub use hash::Hash;
pub use primary_key::PrimaryKey;
pub use type_def::*;
pub use type_value::{TupleValue, TypeValue};

pub const SCHEMA_FORMAT_VERSION: u16 = 0;

use std::fmt;
fn fmt_fn(f: impl Fn(&mut fmt::Formatter) -> fmt::Result) -> impl fmt::Display + fmt::Debug + serde::de::Expected {
    struct FDisplay<F>(F);
    impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Display for FDisplay<F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }
    impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Debug for FDisplay<F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }
    impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> serde::de::Expected for FDisplay<F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }
    FDisplay(f)
}
