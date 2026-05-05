//! Opaque runtime boundary for crates that should not depend on Tokio directly.

pub type Handle = tokio::runtime::Handle;
pub type Runtime = tokio::runtime::Runtime;

pub fn current_handle_or_new_runtime() -> anyhow::Result<(Handle, Option<Runtime>)> {
    if let Ok(handle) = Handle::try_current() {
        return Ok((handle, None));
    }

    let runtime = Runtime::new()?;
    Ok((runtime.handle().clone(), Some(runtime)))
}
