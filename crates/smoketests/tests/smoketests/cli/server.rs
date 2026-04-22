//! CLI server command tests

use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

#[test]
fn cli_can_ping_spacetimedb_on_disk() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let output = cli_cmd()
        .args(["server", "ping", &spacetime.host_url.to_string()])
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "ping failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
