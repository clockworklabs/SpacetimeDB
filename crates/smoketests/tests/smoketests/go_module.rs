#![allow(clippy::disallowed_macros)]
use spacetimedb_smoketests::require_go;

/// Ensure that go is detected correctly.
/// Full Go module build test will be added when Go modules are integrated.
#[test]
fn test_build_go_module() {
    require_go!();
    assert!(spacetimedb_smoketests::have_go());
}
