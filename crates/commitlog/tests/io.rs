use std::path::Path;

mod dio;
mod random_payload;

/// Root directory to use for temporary files.
///
/// `$TMPDIR` is often a tmpfs, which behaves differently.
fn tempdir() -> &'static Path {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
}

fn enable_logging(level: log::LevelFilter) {
    let _ = env_logger::builder()
        .filter_level(level)
        .format_timestamp(None)
        .is_test(true)
        .try_init();
}
