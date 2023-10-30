use std::hint::black_box;
use std::sync::atomic::{AtomicU32, Ordering};

/// This module allows temporarily enabling callgrind in async SpacetimeDB code, guarded by a global enable flag.
///
/// Our basic problem is that callgrind loses track of control flow when work is sent through a channel across threads.
/// Callgrind has a "toggle-collect" option that allows toggling event recording at a function boundary.
/// So, as long as we wrap code we're interested in with a marker function, we can record across threads.
///
/// However, there's a problems with using this: it will track ANY time that function is called, including during warmup!
/// We want to run our functions twice: once to warm up, and once to measure.
///
/// Our solution is to wrap the code of interest in a function that is only called when the global flag is set.
///
/// See: documentation on valgrind/callgrind/iai-callgrind's `toggle-collect` option (ctrl-f on these pages):
/// - https://github.com/iai-callgrind/iai-callgrind/
/// - https://valgrind.org/docs/manual/cl-manual.html
///
/// We do NOT use the valgrind macros (or the crate https://github.com/2dav/crabgrind) because they are a pain to build.
/// (Hours wasted here: 9.)
/// Instead, we have a wrapper function which is only called when a global flag is set.
///
/// To use this module, you need to do several things:
/// 1. Wrap your target code of interest with `invoke_allowing_callgrind`.
/// 2. In code that invokes that (possibly from another thread), wrap the code of interest with `enable_callgrind_globally`.
/// 3. Invoke callgrind on your executable with the flags:
///    `--collect-atstart=no --toggle-collect=`spacetimedb::callgrind_flag::flag*`
///    Or, if using our fork of iai callgrind (https://github.com/clockworklabs/iai-callgrind), use:
///    `LibraryBenchmarkConfig::default().with_custom_entry_point("spacetimedb::callgrind_flag::flag")`;

static CALLGRIND_ENABLED: AtomicU32 = AtomicU32::new(0);

/// Invoke a function, enabling callgrind on all threads.
/// If not running under valgrind, this simply invokes the function.
pub fn enable_callgrind_globally<T, F: FnOnce() -> T>(f: F) -> T {
    CALLGRIND_ENABLED.fetch_add(1, Ordering::Release);
    let result = black_box(flag(black_box(f)));
    CALLGRIND_ENABLED.fetch_sub(1, Ordering::Release);
    result
}

/// Invoke a function, allowing callgrind instrumentation.
/// For instrumentation to be active, callgrind must be enabled globally, and
/// the executable must be running under callgrind.
/// If not running under callgrind, this just invokes the passed function.
pub fn invoke_allowing_callgrind<T, F: FnOnce() -> T>(f: F) -> T {
    if CALLGRIND_ENABLED.load(Ordering::Acquire) > 0 {
        black_box(flag(f))
    } else {
        black_box(f())
    }
}

/// The magic juice.
/// This is what you tell callgrind to look for.
/// Don't change the name of this function (or the name of this module!) without updating
/// the benchmarks crate.
#[inline(never)]
fn flag<T, F: FnOnce() -> T>(f: F) -> T {
    black_box(f())
}
