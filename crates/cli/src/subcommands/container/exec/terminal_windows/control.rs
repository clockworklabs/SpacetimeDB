use super::{check, lock, ClientControl, Shared};
use anyhow::{ensure, Result};
use std::sync::{Arc, Mutex};
use windows_sys::Win32::{
    Foundation::{BOOL, FALSE, TRUE},
    System::{Console::*, Threading::INFINITE},
};

static ACTIVE: Mutex<Option<(Arc<Shared>, bool)>> = Mutex::new(None);
pub(super) struct Registration {
    shared: Arc<Shared>,
    sealed: bool,
}
impl Registration {
    pub fn new(shared: Arc<Shared>) -> Result<Self> {
        let mut active = lock(&ACTIVE);
        ensure!(active.is_none(), "another Windows terminal owner is active");
        unsafe {
            check(SetConsoleCtrlHandler(Some(handler), TRUE))?;
        }
        *active = Some((shared.clone(), true));
        Ok(Self { shared, sealed: false })
    }
    pub fn seal(&mut self) -> Result<()> {
        if self.sealed {
            return Ok(());
        }
        let mut active = lock(&ACTIVE);
        if let Some((shared, admitted)) = active.as_mut()
            && Arc::ptr_eq(shared, &self.shared)
        {
            *admitted = false;
        }
        self.sealed = true;
        unsafe { check(SetConsoleCtrlHandler(Some(handler), FALSE)) }
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        let _ = self.seal();
        let mut active = lock(&ACTIVE);
        if active
            .as_ref()
            .is_some_and(|(shared, _)| Arc::ptr_eq(shared, &self.shared))
        {
            *active = None;
        }
    }
}

struct Callback(Arc<Shared>);
impl Drop for Callback {
    fn drop(&mut self) {
        let mut count = lock(&self.0.callbacks);
        *count -= 1;
        self.0.callbacks_done.notify_all();
    }
}
unsafe extern "system" fn handler(event: u32) -> BOOL {
    let callback = {
        let active = lock(&ACTIVE);
        let Some((shared, true)) = active.as_ref() else {
            return FALSE;
        };
        *lock(&shared.callbacks) += 1;
        Callback(shared.clone())
    };
    // The OS owns this callback thread. Never unwind across the ABI boundary.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(event, &callback.0))).unwrap_or_else(|_| {
        callback.0.fail();
        TRUE
    })
}
fn dispatch(event: u32, shared: &Shared) -> BOOL {
    match event {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => {
            let signal = if event == CTRL_C_EVENT { 2 } else { 3 };
            if shared.controls.try_send(Ok(ClientControl::Signal(signal))).is_err() {
                shared.fail();
            }
            TRUE
        }
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            shared.fail();
            // Returning allows OS termination. Only acknowledge after joined I/O
            // and restoration; an OS forced timeout cannot count as completion.
            shared.restored.wait(INFINITE);
            TRUE
        }
        _ => FALSE,
    }
}

#[cfg(test)]
pub(super) fn invoke(event: u32) -> BOOL {
    unsafe { handler(event) }
}
