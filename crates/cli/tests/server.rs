mod util;

use assert_cmd::cargo::cargo_bin_cmd;
use crate::util::SpacetimeDbGuard;

#[test]
fn cli_can_ping_spacetimedb_on_disk() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "server",
        "ping",
        &spacetime.host_url.to_string(),
    ])
    .assert()
    .success();
}