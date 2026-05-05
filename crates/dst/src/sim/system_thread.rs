//! Guard against creating OS threads from inside the simulator.

use std::{cell::Cell, sync::OnceLock};

thread_local! {
    static IN_SIMULATION: Cell<bool> = const { Cell::new(false) };
}

pub(crate) struct SimulationThreadGuard {
    previous: bool,
}

pub(crate) fn enter_simulation_thread() -> SimulationThreadGuard {
    let previous = IN_SIMULATION.with(|state| state.replace(true));
    SimulationThreadGuard { previous }
}

impl Drop for SimulationThreadGuard {
    fn drop(&mut self) {
        IN_SIMULATION.with(|state| {
            state.set(self.previous);
        });
    }
}

fn in_simulation() -> bool {
    IN_SIMULATION.with(Cell::get)
}

/// Forbid creating system threads in simulation.
#[cfg(unix)]
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn pthread_attr_init(attr: *mut libc::pthread_attr_t) -> libc::c_int {
    if in_simulation() {
        eprintln!("attempt to spawn a system thread in simulation.");
        eprintln!("note: use simulator tasks instead.");
        return -1;
    }

    type PthreadAttrInit = unsafe extern "C" fn(*mut libc::pthread_attr_t) -> libc::c_int;
    static PTHREAD_ATTR_INIT: OnceLock<PthreadAttrInit> = OnceLock::new();
    let original = PTHREAD_ATTR_INIT.get_or_init(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, c"pthread_attr_init".as_ptr().cast());
        assert!(!ptr.is_null(), "failed to resolve original pthread_attr_init");
        std::mem::transmute(ptr)
    });
    unsafe { original(attr) }
}

#[cfg(test)]
mod tests {
    use crate::{seed::DstSeed, sim};

    #[test]
    #[cfg(unix)]
    fn runtime_forbids_system_thread_spawn() {
        let mut runtime = sim::Runtime::new(DstSeed(200)).unwrap();
        runtime.block_on(async {
            let result = std::panic::catch_unwind(|| std::thread::Builder::new().spawn(|| {}));
            assert!(result.is_err());
        });
    }
}
