pub mod prometheus_handle;

pub trait ResultInspectExt<T, E> {
    fn inspect_err_(self, f: impl FnOnce(&E)) -> Self;
}
impl<T, E> ResultInspectExt<T, E> for Result<T, E> {
    #[inline]
    fn inspect_err_(self, f: impl FnOnce(&E)) -> Self {
        if let Err(e) = &self {
            f(e)
        }
        self
    }
}
