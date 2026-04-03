#[cfg(any(test, feature = "test"))]
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockEventKind {
    BeginReadRequested,
    BeginReadAcquired,
    BeginWriteRequested,
    BeginWriteAcquired,
    SequenceMutexRequested,
    SequenceMutexAcquired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LockEvent {
    pub kind: LockEventKind,
}

#[cfg(any(test, feature = "test"))]
type Hook = Arc<dyn Fn(LockEvent) + Send + Sync + 'static>;

#[cfg(any(test, feature = "test"))]
fn hook_cell() -> &'static Mutex<Option<Hook>> {
    static CELL: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(None))
}

#[cfg(any(test, feature = "test"))]
pub struct HookGuard;

#[cfg(any(test, feature = "test"))]
impl Drop for HookGuard {
    fn drop(&mut self) {
        *hook_cell().lock().expect("lock hook cell") = None;
    }
}

#[cfg(any(test, feature = "test"))]
pub fn install_lock_event_hook(hook: impl Fn(LockEvent) + Send + Sync + 'static) -> HookGuard {
    *hook_cell().lock().expect("lock hook cell") = Some(Arc::new(hook));
    HookGuard
}

#[cfg(not(any(test, feature = "test")))]
pub struct HookGuard;

#[cfg(not(any(test, feature = "test")))]
pub fn install_lock_event_hook(_hook: impl Fn(LockEvent) + Send + Sync + 'static) -> HookGuard {
    HookGuard
}

pub(super) fn emit(event: LockEvent) {
    #[cfg(any(test, feature = "test"))]
    if let Some(hook) = hook_cell().lock().expect("lock hook cell").clone() {
        hook(event);
    }
}
