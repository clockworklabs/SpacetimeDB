//! CLI server command tests

use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::Smoketest;
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

#[test]
fn cli_can_ping_spacetimedb_server() {
    let spacetime = Smoketest::builder().autopublish(false).build();
    let output = cli_cmd()
        .args(["server", "ping", &spacetime.server_url])
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "ping failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
