//! Snapshot testing for the dependency tree of the `bindings` crate - we want
//! to make sure we don't unknowningly add a bunch of dependencies here,
//! slowing down compilation for every spacetime module.

// We need to remove the `cpufeatures` and `libc` dependencies from the output, it added on `macOS` with `arm` architecture:
// https://github.com/RustCrypto/sponges/blob/master/keccak/Cargo.toml#L24-L25, breaking local testing.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn hack_keccack(cmd: String) -> String {
    let mut found = false;
    let mut lines = cmd.lines().peekable();
    let mut output = String::new();

    while let Some(line) = lines.next() {
        // Check we only match keccak/cpufeatures/libc
        if line.contains("keccak") {
            found = true;
        }
        if found && line.contains("cpufeatures") {
            if let Some(next_line) = lines.peek() {
                if next_line.contains("libc") {
                    found = false;
                    // Skip libc
                    lines.next();
                    continue;
                }
            }
        }
        output.push_str(line);
        output.push('\n');
    }

    output
}
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn hack_keccack(cmd: String) -> String {
    cmd
}

#[test]
fn deptree_snapshot() -> std::io::Result<()> {
    let cmd = "cargo tree -p spacetimedb -f {lib} -e no-dev";
    let deps_tree = hack_keccack(run_cmd(cmd));
    let all_deps = hack_keccack(run_cmd("cargo tree -p spacetimedb -e no-dev --prefix none --no-dedupe"));
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
