//! Experimental: detect thread parking inside the single-threaded simulator.
//!
//! This module installs a **seccomp BPF filter** that traps any
//! `futex(FUTEX_WAIT, …)` or `futex(FUTEX_WAIT_BITSET, …)` syscall while a
//! simulation is running.  Because the simulator only has one OS thread, a
//! blocking call such as `send_blocking` or `thread::park` would park that
//! thread and deadlock the runtime.  The trap delivers `SIGSYS`, the handler
//! prints a diagnostic, and the process aborts – giving a clear failure
//! instead of a silent hang.
//!
//! # Caveats (experimental)
//!
//! - **Linux + x86_64 only.**  The BPF instructions and `ucontext` layout are
//!   arch‑specific.  Building on other targets compiles this module away.
//! - **Process‑wide side effect.**  Once installed, the seccomp filter stays
//!   for the lifetime of the process.  Outside a simulation the handler
//!   silently skips the blocking instruction (returning 0), so normal code is
//!   not affected.
//! - **No false positive from mutex contention.**  The filter specifically
//!   targets `FUTEX_WAIT` / `FUTEX_WAIT_BITSET`.  Mutex lock operations
//!   use `FUTEX_LOCK_PI` or a different futex command and are allowed.
//!   Since the simulation is single‑threaded, internal `pthread_mutex_lock`
//!   calls never contend and never reach `FUTEX_WAIT`.
//! - **`std::process::abort` in the signal handler** is intentional – it is one
//!   of the few async‑signal‑safe operations available.  The panic machinery
//!   would re‑enter `futex` for lock acquisition and cause a recursive trap.

#![allow(clippy::disallowed_macros)]

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod imp {
    use core::sync::atomic::AtomicBool;
    use core::sync::atomic::Ordering;

    // ── constants from kernel headers ──────────────────────────────────
    // Most come from `libc` directly; a few are defined here because
    // `libc` does not export them (e.g. `AUDIT_ARCH_X86_64`).
    const AUDIT_ARCH_X86_64: u32 = 0xC000003E;          // <linux/audit.h> — EM_X86_64 | __AUDIT_ARCH_64BIT

    // ── BPF instruction builders ───────────────────────────────────────
    // Classic BPF instruction format used by seccomp.
    // Each instruction is a `sock_filter { code, jt, jf, k }`:
    //   code  — opcode (class | size | mode)
    //   jt    — jump offset if true
    //   jf    — jump offset if false
    //   k     — generic operand / immediate / offset
    //
    // Available opcode components from <linux/bpf_common.h>:
    //   class:  BPF_LD (0x00), BPF_LDX (0x01), BPF_ALU (0x04), BPF_JMP (0x05), BPF_RET (0x06)
    //   size:   BPF_W  (0x00), BPF_H  (0x08), BPF_B  (0x10)
    //   mode:   BPF_ABS(0x20), BPF_IND(0x40), BPF_MEM(0x60), BPF_LEN(0x80)
    //   jmp-op: BPF_JA (0x00), BPF_JEQ(0x10), BPF_JGT(0x20), BPF_JGE(0x30), BPF_JSET(0x40)
    //   alu-op: BPF_ADD(0x00), BPF_SUB(0x10), BPF_MUL(0x20), BPF_AND(0x50)
    //   src:    BPF_K  (0x00 — use k field), BPF_X  (0x08 — use X register)

    /// One BPF statement (no jump): reads data or returns a value.
    fn bpf_stmt(op: u32, k: u32) -> libc::sock_filter {
        libc::sock_filter { code: op as u16, jt: 0, jf: 0, k }
    }

    /// One BPF jump: compares A against `k` and branches.
    fn bpf_jmp(op: u32, jt: u8, jf: u8, k: u32) -> libc::sock_filter {
        libc::sock_filter { code: op as u16, jt, jf, k }
    }

    /// Install a seccomp BPF filter that traps `futex(FUTEX_WAIT)`.
    ///
    /// Everything (prctl + sigaction + BPF) is done once per process via
    /// an `AtomicBool`.  The first thread to enter simulation performs the
    /// syscalls; subsequent threads inherit the filter at creation time.
    pub fn install() {
        static INSTALLED: AtomicBool = AtomicBool::new(false);
        if INSTALLED.swap(true, Ordering::Relaxed) {
            return;
        }
        unsafe {
            // ── step 1: PR_SET_NO_NEW_PRIVS ─────────────────────────────
            // Lets unprivileged threads install a seccomp filter.
            let ret = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
            assert_eq!(ret, 0, "parking_detect: PR_SET_NO_NEW_PRIVS failed");

            // ── step 2: register SIGSYS handler ─────────────────────────
            // SA_NODEFER: allow re‑entering the handler if an abort‑time
            //             syscall also hits the filter.
            let mut sa: libc::sigaction = core::mem::zeroed();
            sa.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
            let ptr = sigsys_handler as extern "C" fn(i32, *mut libc::siginfo_t, *mut libc::c_void);
            // The sa_handler / sa_sigaction field is a union; write via raw
            // bytes to avoid fighting the libc type definitions.
            let dst: *mut libc::c_void = (&mut sa) as *mut _ as *mut libc::c_void;
            dst.cast::<usize>().write(ptr as usize);
            let ret = libc::sigaction(libc::SIGSYS, &sa, core::ptr::null_mut());
            assert_eq!(ret, 0, "parking_detect: sigaction(SIGSYS) failed");

            // ── step 3: install the BPF filter ──────────────────────────
            // Every syscall is checked by this 11-instruction seccomp
            // program.  The kernel provides `struct seccomp_data`:
            //
            //   offset  size  field
            //        0     4  nr        (syscall number, 202 = futex)
            //        4     4  arch      (AUDIT_ARCH_*)
            //       24     8  args[1]   (futex op | flags)
            //
            // We verify the architecture, then the syscall number,
            // then the futex operation (after masking the PRIVATE flag).
            let bpf: [libc::sock_filter; 11] = [
                // ── insn 0: ld [4] ─────────────────────────────────
                // Load the `arch` field of `seccomp_data` into A.
                bpf_stmt(
                    libc::BPF_LD | libc::BPF_W | libc::BPF_ABS,
                    4,
                ),

                // ── insn 1: jeq AUDIT_ARCH_X86_64, 0, 8 ──────────
                // If arch == x86_64 → continue (jt:0 → insn 2).
                // Otherwise → jump forward 8 (jf:8 → insn 10, KILL).
                // x86 compat syscalls have a different data layout
                // and must be rejected outright.
                bpf_jmp(
                    libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K,
                    0, 8,
                    AUDIT_ARCH_X86_64,
                ),

                // ── insn 2: ld [0] ─────────────────────────────────
                // Load the `nr` (syscall number) into A.
                bpf_stmt(
                    libc::BPF_LD | libc::BPF_W | libc::BPF_ABS,
                    0,
                ),

                // ── insn 3: jeq __NR_FUTEX, 0, 5 ─────────────────
                // If nr == FUTEX (202) → continue (jt:0 → insn 4).
                // Otherwise → jump forward 5 (jf:5 → insn 9, ALLOW).
                bpf_jmp(
                    libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K,
                    0, 5,
                    libc::SYS_futex as u32,
                ),

                // ── insn 4: ld [24] ────────────────────────────────
                // Load `args[1]` — the futex operation word (op | flags).
                //   e.g. FUTEX_WAIT (0), FUTEX_WAIT_BITSET (9),
                //        FUTEX_PRIVATE_FLAG (0x80)
                bpf_stmt(
                    libc::BPF_LD | libc::BPF_W | libc::BPF_ABS,
                    24,
                ),

                // ── insn 5: and 0x7F ──────────────────────────────
                // Strip the PRIVATE flag bit (0x80).
                // After masking:
                //   FUTEX_WAIT (0), FUTEX_WAIT|PRIVATE (0x80) → 0
                //   FUTEX_WAIT_BITSET (9), FUTEX_WAIT_BITSET|PRIVATE (0x89) → 9
                bpf_stmt(
                    libc::BPF_ALU | libc::BPF_AND | libc::BPF_K,
                    0x7F,
                ),

                // ── insn 6: jeq 0, 1, 0 ──────────────────────────
                // If masked op == FUTEX_WAIT (0) → jump forward 1
                // (jt:1 → insn 8, TRAP).
                // Otherwise → fall through (jf:0 → insn 7).
                bpf_jmp(
                    libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K,
                    1, 0,
                    0, // FUTEX_WAIT
                ),

                // ── insn 7: jeq 9, 0, 1 ──────────────────────────
                // If masked op == FUTEX_WAIT_BITSET (9) → fall
                // through (jt:0 → insn 8, TRAP).
                // Otherwise → jump forward 1 (jf:1 → insn 9, ALLOW).
                bpf_jmp(
                    libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K,
                    0, 1,
                    9, // FUTEX_WAIT_BITSET
                ),

                // ── insn 8: ret SECCOMP_RET_TRAP ────────────────
                // Deliver SIGSYS.  Our handler checks
                // `sim_std::in_simulation()` and aborts if inside a
                // simulation, or skips the instruction otherwise.
                bpf_stmt(
                    libc::BPF_RET | libc::BPF_K,
                    libc::SECCOMP_RET_TRAP,
                ),

                // ── insn 9: ret SECCOMP_RET_ALLOW ──────────────
                // Not a futex wait — let the syscall through.
                bpf_stmt(
                    libc::BPF_RET | libc::BPF_K,
                    libc::SECCOMP_RET_ALLOW,
                ),

                // ── insn 10: ret SECCOMP_RET_KILL ─────────────
                // Architecture mismatch — kill the process.
                bpf_stmt(
                    libc::BPF_RET | libc::BPF_K,
                    libc::SECCOMP_RET_KILL,
                ),
            ];

            let prog = libc::sock_fprog {
                len: bpf.len() as u16,
                filter: &bpf as *const libc::sock_filter as *mut libc::sock_filter,
            };
            let ret = libc::syscall(
                libc::SYS_seccomp,
                libc::SECCOMP_SET_MODE_FILTER,
                0,
                &prog,
            );
            assert_eq!(
                ret,
                0,
                "parking_detect: seccomp(SECCOMP_SET_MODE_FILTER) failed",
            );
        }
    }

    /// SIGSYS handler — traps a `futex_wait` inside simulation and aborts.
    ///
    /// Outside simulation the seccomp filter is also active (it is process‑wide),
    /// so non‑simulation futex waits are harmless — the handler skips the
    /// `syscall` instruction and returns 0 (spurious wakeup).  In the final
    /// simulation binary `in_simulation()` is always true, so this branch is
    /// dead code and the optimizer removes it.
    extern "C" fn sigsys_handler(
        _sig: i32,
        _info: *mut libc::siginfo_t,
        ctx: *mut libc::c_void,
    ) {
        if crate::sim_std::in_simulation() {
            const MSG: &[u8] = b"\
                blocking syscall (futex wait) detected inside deterministic simulation\n\
                \x20 note: use non-blocking alternatives or run with the tokio runtime\n\
            ";
            unsafe {
                libc::write(libc::STDERR_FILENO, MSG.as_ptr() as *const _, MSG.len());
                libc::abort();
            }
        }

        // Outside simulation: skip the `syscall` instruction and return 0.
        // The x86_64 `syscall` opcode is 2 bytes (0x0f 0x05).
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let uc = &mut *(ctx as *mut libc::ucontext_t);
            uc.uc_mcontext.gregs[libc::REG_RIP as usize] =
                uc.uc_mcontext.gregs[libc::REG_RIP as usize].wrapping_add(2);
            uc.uc_mcontext.gregs[libc::REG_RAX as usize] = 0;
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = ctx;
            unsafe { libc::abort(); }
        }
    }
}

// —— RTLD_NEXT interposition hooks -------------------------------------------

/// Hook Unix thread creation by interposing `pthread_attr_init`.
///
/// `std::thread::Builder::spawn` initializes pthread attributes before creating
/// the thread. Returning an error here while simulation is active makes hidden
/// OS thread creation fail early, before host scheduling can affect replay.
/// Outside simulation, this delegates to the real libc symbol through `RTLD_NEXT`.
#[cfg(unix)]
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn pthread_attr_init(attr: *mut libc::pthread_attr_t) -> libc::c_int {
    if crate::sim_std::in_simulation() {
        unsafe {
            let msg = b"attempt to spawn a system thread in simulation.\nnote: use simulator tasks instead.\n";
            libc::write(libc::STDERR_FILENO, msg.as_ptr() as *const _, msg.len());
        }
        return -1;
    }

    type PthreadAttrInit = unsafe extern "C" fn(*mut libc::pthread_attr_t) -> libc::c_int;
    static PTHREAD_ATTR_INIT: spin::once::Once<PthreadAttrInit> = spin::once::Once::new();
    let original = PTHREAD_ATTR_INIT.call_once(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, c"pthread_attr_init".as_ptr().cast());
        assert!(!ptr.is_null(), "failed to resolve original pthread_attr_init");
        core::mem::transmute(ptr)
    });
    unsafe { original(attr) }
}

/// Hook OS randomness by interposing `getrandom`.
///
/// This crate no longer tries to make host randomness deterministic. Any such
/// request is surfaced with a warning and then delegated to the host OS.
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
    unsafe {
        let msg = b"warning: randomness requested; delegating to host OS\n";
        libc::write(libc::STDERR_FILENO, msg.as_ptr() as *const _, msg.len());
    }
    eprintln!("{}", std::backtrace::Backtrace::force_capture());
    unsafe { real_getrandom()(buf, buflen, flags) }
}

#[cfg(target_os = "linux")]
fn real_getrandom() -> unsafe extern "C" fn(*mut u8, usize, u32) -> isize {
    type GetrandomFn = unsafe extern "C" fn(*mut u8, usize, u32) -> isize;
    static GETRANDOM: spin::once::Once<GetrandomFn> = spin::once::Once::new();
    *GETRANDOM.call_once(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, c"getrandom".as_ptr().cast());
        assert!(!ptr.is_null(), "failed to resolve original getrandom");
        core::mem::transmute(ptr)
    })
}

#[cfg(not(target_os = "linux"))]
fn real_getrandom() -> unsafe extern "C" fn(*mut u8, usize, u32) -> isize {
    compile_error!("unsupported OS for DST getrandom override");
}

/// Hook `getentropy` and route it through the same deterministic path as
/// `getrandom`.
///
/// The 256-byte limit is part of the getentropy contract. Keeping this wrapper
/// small means all entropy decisions stay centralized in `getrandom`.
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn getentropy(buf: *mut u8, buflen: usize) -> i32 {
    if buflen > 256 {
        return -1;
    }
    match unsafe { getrandom(buf, buflen, 0) } {
        -1 => -1,
        _ => 0,
    }
}

// —— public API --------------------------------------------------------------

/// Install the parking‑detection seccomp filter (if supported).
///
/// On non‑Linux or non‑x86_64 this is a no‑op.
#[allow(dead_code)]
pub fn install() {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    imp::install();
}

#[cfg(test)]
mod tests {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn assert_subprocess_aborts(test_name: &str, env_var: &str) {
        let exe = std::env::current_exe().expect("failed to get test binary path");
        let output = std::process::Command::new(&exe)
            .env(env_var, "1")
            .arg("--exact")
            .arg(test_name)
            .output()
            .expect("failed to run subprocess");

        assert!(!output.status.success(), "expected {test_name} to abort");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("blocking syscall (futex wait)"),
            "expected blocking message in stderr, got:\n{stderr}",
        );
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn futex_block_trigger() {
        if std::env::var("SPACETIMEDB_FUTEX_BLOCK").is_err() {
            return;
        }
        let mut runtime = crate::sim::Runtime::new(42);
        crate::sim_std::block_on(&mut runtime, async {
            let (_tx, rx) = std::sync::mpsc::channel::<()>();
            let _ = rx.recv();
        });
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn mpsc_recv_blocks_in_simulation() {
        assert_subprocess_aborts("hooks::tests::futex_block_trigger", "SPACETIMEDB_FUTEX_BLOCK");
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn contended_parking_lot_mutex_trigger() {
        if std::env::var("SPACETIMEDB_PARKING_LOT_CONTEND").is_err() {
            return;
        }
        let mut runtime = crate::sim::Runtime::new(42);
        crate::sim_std::block_on(&mut runtime, async {
            let lock = parking_lot::Mutex::new(42);
            let _guard = lock.lock();
            let _guard2 = lock.lock();
        });
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn parking_lot_contended_blocks_in_simulation() {
        assert_subprocess_aborts(
            "hooks::tests::contended_parking_lot_mutex_trigger",
            "SPACETIMEDB_PARKING_LOT_CONTEND",
        );
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn send_blocking_at_bound_trigger() {
        if std::env::var("SPACETIMEDB_SENDBLOCK_BOUND").is_err() {
            return;
        }
        let mut runtime = crate::sim::Runtime::new(42);
        crate::sim_std::block_on(&mut runtime, async {
            let (tx, _rx) = async_channel::bounded::<i32>(1);
            tx.send_blocking(1).expect("first send");
            tx.send_blocking(2).expect("full — never reaches");
        });
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn send_blocking_at_bound_blocks_in_simulation() {
        assert_subprocess_aborts(
            "hooks::tests::send_blocking_at_bound_trigger",
            "SPACETIMEDB_SENDBLOCK_BOUND",
        );
    }

    #[test]
    #[cfg(unix)]
    fn runtime_forbids_system_thread_spawn() {
        let mut runtime = crate::sim::Runtime::new(200);
        crate::sim_std::block_on(&mut runtime, async {
            let result = std::panic::catch_unwind(|| std::thread::Builder::new().spawn(|| {}));
            assert!(result.is_err());
        });
    }

    #[test]
    fn getentropy_delegates_to_host_randomness_outside_simulation() {
        let mut actual = [0u8; 24];
        unsafe {
            assert_eq!(super::getentropy(actual.as_mut_ptr(), actual.len()), 0);
        }
    }

    #[test]
    #[cfg(unix)]
    fn uncontended_parking_lot_mutex_works_in_simulation() {
        let mut runtime = crate::sim::Runtime::new(42);
        crate::sim_std::block_on(&mut runtime, async {
            let lock = parking_lot::Mutex::new(42);
            assert_eq!(*lock.lock(), 42);
        });
    }

    #[test]
    fn bounded_async_channel_send_blocking_not_full() {
        let mut runtime = crate::sim::Runtime::new(42);
        crate::sim_std::block_on(&mut runtime, async {
            let (tx, rx) = async_channel::bounded::<i32>(2);
            tx.send_blocking(1).expect("send within capacity");
            tx.send_blocking(2).expect("send within capacity");
            drop(rx);
        });
    }
}
