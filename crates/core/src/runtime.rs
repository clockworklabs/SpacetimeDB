//! Runtime boundary re-exported for core call sites.

pub use spacetimedb_runtime::{current_handle_or_new_runtime, TokioHandle, TokioRuntime};
pub use spacetimedb_runtime::{Runtime, RuntimeTimeout};
