#![allow(clippy::disallowed_macros)]
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use spacetimedb_smoketests::{gradlew_path, patch_module_cargo_to_local_bindings, require_gradle, workspace_root};
use std::fs;
use std::process::Command;
use std::sync::Mutex;

/// Gradle builds sharing the same project directory cannot run in parallel.
/// This mutex serializes all Kotlin smoketests that invoke gradlew on sdks/kotlin/.
static GRADLE_LOCK: Mutex<()> = Mutex::new(());

/// Run the Kotlin SDK unit tests (BSATN codec, type round-trips, query builder, etc.).
/// Does not require a running SpacetimeDB server.
/// Skips if gradle is not available or disabled via SMOKETESTS_GRADLE=0.
#[test]
fn test_kotlin_sdk_unit_tests() {
    require_gradle!();
    let _lock = GRADLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let workspace = workspace_root();
    let cli_path = ensure_binaries_built();
    let kotlin_sdk_path = workspace.join("sdks/kotlin");
    let gradlew = gradlew_path().expect("gradlew not found");

    // The spacetimedb Gradle plugin auto-generates bindings during compilation.
    // Pass the CLI path via SPACETIMEDB_CLI so the plugin uses the freshly-built binary.
    let output = Command::new(&gradlew)
        .args([
            ":spacetimedb-sdk:jvmTest",
            ":codegen-tests:test",
            "--no-daemon",
            "--no-configuration-cache",
        ])
        .env("SPACETIMEDB_CLI", &cli_path)
        .current_dir(&kotlin_sdk_path)
        .output()
        .expect("Failed to run gradlew :spacetimedb-sdk:allTests :codegen-tests:test");

    if !output.status.success() {
        panic!(
            "Kotlin SDK unit tests failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    eprintln!("Kotlin SDK unit tests passed");
}

/// Run Kotlin SDK integration tests against a live SpacetimeDB server.
/// Spawns a local server, builds + publishes the integration test module,
/// then runs the Gradle integration tests with SPACETIMEDB_HOST set.
/// Skips if gradle is not available or disabled via SMOKETESTS_GRADLE=0.
#[test]
fn test_kotlin_integration() {
    require_gradle!();
    let _lock = GRADLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let workspace = workspace_root();
    let cli_path = ensure_binaries_built();
    let kotlin_sdk_path = workspace.join("sdks/kotlin");
    let module_path = kotlin_sdk_path.join("integration-tests/spacetimedb");

    // Isolate CLI config so we don't reuse stale tokens from the user's home config.
    // This mirrors what Smoketest.spacetime_cmd() does via --config-path.
    let config_dir = tempfile::tempdir().expect("Failed to create temp config dir");
    let config_path = config_dir.path().join("config.toml");

    // Helper: build a Command with --config-path already set.
    let cli = |extra_args: &[&str]| -> std::process::Output {
        Command::new(&cli_path)
            .arg("--config-path")
            .arg(&config_path)
            .args(extra_args)
            .output()
            .expect("Failed to run spacetime CLI command")
    };

    // Step 1: Spawn a local SpacetimeDB server
    let guard = SpacetimeDbGuard::spawn_in_temp_data_dir_with_pg_port(None);
    let server_url = &guard.host_url;
    eprintln!("[KOTLIN-INTEGRATION] Server running at {server_url}");

    // Step 2: Patch the module to use local bindings and build it
    patch_module_cargo_to_local_bindings(&module_path).expect("Failed to patch module Cargo.toml");

    let toolchain_src = workspace.join("rust-toolchain.toml");
    if toolchain_src.exists() {
        fs::copy(&toolchain_src, module_path.join("rust-toolchain.toml")).expect("Failed to copy rust-toolchain.toml");
    }

    let output = cli(&["build", "--module-path", module_path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "spacetime build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Step 4: Publish the module
    let db_name = "kotlin-integration-test";
    let output = cli(&[
        "publish",
        "--server",
        server_url,
        "--module-path",
        module_path.to_str().unwrap(),
        "--no-config",
        "-y",
        db_name,
    ]);
    assert!(
        output.status.success(),
        "spacetime publish failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("[KOTLIN-INTEGRATION] Module published as '{db_name}'");

    // Step 5: Run Gradle integration tests
    let gradlew = gradlew_path().expect("gradlew not found");
    let ws_url = server_url.replace("http://", "ws://").replace("https://", "wss://");

    let output = Command::new(&gradlew)
        .args([
            ":integration-tests:clean",
            ":integration-tests:test",
            "-PintegrationTests",
            "--no-daemon",
            "--no-configuration-cache",
            "--stacktrace",
        ])
        .env("SPACETIMEDB_CLI", &cli_path)
        .env("SPACETIMEDB_HOST", &ws_url)
        .env("SPACETIMEDB_DB_NAME", db_name)
        .current_dir(&kotlin_sdk_path)
        .output()
        .expect("Failed to run gradle integration tests");

    if !output.status.success() {
        panic!(
            "Kotlin integration tests failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    eprintln!("[KOTLIN-INTEGRATION] All integration tests passed");
    drop(guard);
}
