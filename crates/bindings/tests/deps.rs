//! Snapshot testing for the dependency tree of the `bindings` crate - we want
//! to make sure we don't unknowningly add a bunch of dependencies here,
//! slowing down compilation for every spacetime module.

#[test]
fn deptree_snapshot() -> std::io::Result<()> {
    let cmd = "cargo tree -p spacetimedb -f {lib} -e no-dev";
    let deps_tree = run_cmd(cmd);

    let all_deps = run_cmd("cargo tree -p spacetimedb -e no-dev --prefix none --no-dedupe");
    let mut all_deps = all_deps.lines().collect::<Vec<_>>();
    all_deps.sort();
    all_deps.dedup();
    let num_deps = all_deps.len();

    insta::assert_snapshot!(
        "spacetimedb_bindings_dependencies",
        format!("total crates: {num_deps}\n{deps_tree}"),
        cmd
    );

    let cmd = "cargo tree -p spacetimedb -e no-dev -d --depth 0";
    insta::assert_snapshot!("duplicate_deps", run_cmd(cmd), cmd);

    Ok(())
}

// runs a command string, split on spaces
#[track_caller]
fn run_cmd(cmd: &str) -> String {
    let mut args = cmd.split(' ');
    let output = std::process::Command::new(args.next().unwrap())
        .args(args)
        .stdout(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
