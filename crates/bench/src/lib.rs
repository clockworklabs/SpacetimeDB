pub mod data;
pub mod spacetime;
pub mod sqlite;
pub(crate) mod utils;

pub mod prelude {
    pub use crate::data::*;
    pub use crate::utils::{ResultBench, DB_POOL, SPACETIME, SQLITE, START_B};

    pub use crate::spacetime;
    pub use crate::sqlite;
}
