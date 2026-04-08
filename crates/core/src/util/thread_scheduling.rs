use core_affinity::CoreId;

/// Apply the current platform's preferred scheduler hint for compute-heavy worker threads.
///
/// On Linux and other non-macOS platforms, this uses CPU affinity when a core is provided.
/// On macOS, scheduler hints are intentionally disabled.
pub(crate) fn apply_compute_thread_hint(core_id: Option<CoreId>) {
    #[cfg(target_os = "macos")]
    {
        let _ = core_id;
    }

    #[cfg(not(target_os = "macos"))]
    if let Some(core_id) = core_id {
        core_affinity::set_for_current(core_id);
    }
}
