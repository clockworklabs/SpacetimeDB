pub type TokioHandle = tokio::runtime::Handle;
pub type TokioRuntime = tokio::runtime::Runtime;

pub fn current_handle_or_new_runtime() -> std::io::Result<(TokioHandle, Option<TokioRuntime>)> {
    if let Ok(handle) = TokioHandle::try_current() {
        return Ok((handle, None));
    }

    let runtime = TokioRuntime::new()?;
    Ok((runtime.handle().clone(), Some(runtime)))
}
