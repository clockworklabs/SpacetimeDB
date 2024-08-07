//! Snapshot testing for the dependency tree of the `bindings` crate - we want
//! to make sure we don't unknowningly add a bunch of dependencies here,
//! slowing down compilation for every spacetime module.

#[cfg(target_os = "macos")]
/// On macOS, the `keccak` crate depends on `cpufeatures/libc`, which don't on Linux.
fn filter_keccak(input: String) -> String {
    let mut lines = input.lines();
    let mut found_keccak = false;
    let mut found_cpufeatures = false;
    let mut result = Vec::new();

    while let Some(line) = lines.next() {
        if line.contains("keccak") {
            found_keccak = true;
            result.push(line.to_string());
        } else if found_keccak && line.contains("cpufeatures") {
            found_cpufeatures = true;
        } else if found_cpufeatures && line.contains("libc") {
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

#[cfg(not(target_os = "macos"))]
fn filter_keccak(input: String) -> String {
    input
}

#[test]
fn deptree_snapshot() -> std::io::Result<()> {
    let cmd = "cargo tree -p spacetimedb -f {lib} -e no-dev";
    let deps_tree = filter_keccak(run_cmd(cmd));

    let all_deps = filter_keccak(run_cmd("cargo tree -p spacetimedb -e no-dev --prefix none --no-dedupe"));

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
