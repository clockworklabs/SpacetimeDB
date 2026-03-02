use spacetimedb_guard::ensure_binaries_built;
use std::process::Command;
use tempfile::tempdir;

/// Ensure that the CLI is able to create a local project.
/// This test does not depend on a running spacetimedb instance.
#[test]
fn test_create_project() {
    let cli_path = ensure_binaries_built();
    let tmpdir = tempdir().expect("Failed to create temp dir");
    let tmpdir_path = tmpdir.path().to_str().unwrap();

    // Without --lang, init should fail
    let output = Command::new(&cli_path)
        .args(["init", "--non-interactive", "test-project"])
        .current_dir(tmpdir_path)
        .output()
        .expect("Failed to run spacetime init");
    assert!(!output.status.success(), "Expected init without --lang to fail");

    // Without --project-path to specify location, init should fail
    let output = Command::new(&cli_path)
        .args([
            "init",
            "--non-interactive",
            "--project-path",
            tmpdir_path,
            "test-project",
        ])
        .output()
        .expect("Failed to run spacetime init");
    assert!(
        !output.status.success(),
        "Expected init without --lang to fail even with --project-path"
    );

    // With all required args, init should succeed
    let output = Command::new(&cli_path)
        .args([
            "init",
            "--non-interactive",
            "--lang=rust",
            "--project-path",
            tmpdir_path,
            "test-project",
        ])
        .output()
        .expect("Failed to run spacetime init");
    assert!(
        output.status.success(),
        "Expected init to succeed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Running init again in the same directory should fail (already exists)
    let output = Command::new(&cli_path)
        .args([
            "init",
            "--non-interactive",
            "--lang=rust",
            "--project-path",
            tmpdir_path,
            "test-project",
        ])
        .output()
        .expect("Failed to run spacetime init");
    assert!(
        !output.status.success(),
        "Expected init to fail when project already exists"
    );
}
