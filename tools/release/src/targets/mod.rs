pub mod cpp;
pub mod crates;
pub mod csharp;
pub mod docker;
pub mod npm;
pub mod util;

/// Common trait for all release targets
pub trait ReleaseTarget {
    fn release(&self) -> Result<(), String>;
    fn name(&self) -> &'static str;
}
