#![allow(clippy::disallowed_macros)]
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use spacetimedb_smoketests::{gradlew_path, patch_module_cargo_to_local_bindings, require_gradle, workspace_root};
use std::fs;
use std::process::Command;
use std::sync::Mutex;

/// Gradle builds sharing the same project directory cannot run in parallel.
/// This mutex serializes all Kotlin smoketests that invoke gradlew on sdks/kotlin/.
static GRADLE_LOCK: Mutex<()> = Mutex::new(());

/// Ensure that generated Kotlin bindings compile against the local Kotlin SDK.
/// This test does not depend on a running SpacetimeDB instance.
/// Skips if gradle is not available or disabled via SMOKETESTS_GRADLE=0.
#[test]
fn test_build_kotlin_client() {
    require_gradle!();
    let _lock = GRADLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let workspace = workspace_root();
    let cli_path = ensure_binaries_built();

    let tmpdir = tempfile::tempdir().expect("Failed to create temp directory");

    // Step 1: Initialize a Rust server module
    let output = Command::new(&cli_path)
        .args([
            "init",
            "--non-interactive",
            "--lang=rust",
            "--project-path",
            tmpdir.path().to_str().unwrap(),
            "kotlin-smoketest",
        ])
        .output()
        .expect("Failed to run spacetime init");
    assert!(
        output.status.success(),
        "spacetime init failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let module_path = tmpdir.path().join("spacetimedb");
    patch_module_cargo_to_local_bindings(&module_path).expect("Failed to patch module Cargo.toml");

    // Copy rust-toolchain.toml so the module builds with the right toolchain
    let toolchain_src = workspace.join("rust-toolchain.toml");
    if toolchain_src.exists() {
        fs::copy(&toolchain_src, module_path.join("rust-toolchain.toml")).expect("Failed to copy rust-toolchain.toml");
    }

    // Step 2: Build the server module (compiles to WASM)
    let output = Command::new(&cli_path)
        .args(["build", "--module-path", module_path.to_str().unwrap()])
        .output()
        .expect("Failed to run spacetime build");
    assert!(
        output.status.success(),
        "spacetime build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Step 3: Set up a Gradle project that uses the spacetimedb plugin
    let client_dir = tmpdir.path().join("client");
    fs::create_dir_all(client_dir.join("src/main/kotlin")).expect("Failed to create source directory");

    let kotlin_sdk_path = workspace.join("sdks/kotlin");
    let kotlin_sdk_path_str = kotlin_sdk_path.display().to_string().replace('\\', "/");
    let cli_path_str = cli_path.display().to_string().replace('\\', "/");
    let module_path_str = module_path.display().to_string().replace('\\', "/");

    // Read the version catalog from the SDK so we use the same Kotlin version
    let libs_toml = fs::read_to_string(kotlin_sdk_path.join("gradle/libs.versions.toml"))
        .expect("Failed to read SDK libs.versions.toml");

    let kotlin_version = libs_toml
        .lines()
        .find(|line| line.starts_with("kotlin = "))
        .and_then(|line| line.split('"').nth(1))
        .expect("Failed to parse kotlin version from libs.versions.toml");

    // settings.gradle.kts — use includeBuild for both plugin and SDK
    let settings_gradle = format!(
        r#"rootProject.name = "kotlin-smoketest-client"

pluginManagement {{
    includeBuild("{kotlin_sdk_path_str}/gradle-plugin")
    repositories {{
        mavenCentral()
        gradlePluginPortal()
    }}
}}

dependencyResolutionManagement {{
    repositories {{
        mavenCentral()
    }}
}}

plugins {{
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}}

includeBuild("{kotlin_sdk_path_str}")
"#
    );
    fs::write(client_dir.join("settings.gradle.kts"), settings_gradle).expect("Failed to write settings.gradle.kts");

    // build.gradle.kts — uses the spacetimedb plugin for codegen + explicit SDK dep
    let build_gradle = format!(
        r#"plugins {{
    id("org.jetbrains.kotlin.jvm") version "{kotlin_version}"
    id("com.clockworklabs.spacetimedb")
}}

kotlin {{
    jvmToolchain(21)
}}

spacetimedb {{
    modulePath.set(file("{module_path_str}"))
    cli.set(file("{cli_path_str}"))
}}

dependencies {{
    implementation("com.clockworklabs:spacetimedb-sdk")
}}
"#
    );
    fs::write(client_dir.join("build.gradle.kts"), build_gradle).expect("Failed to write build.gradle.kts");

    // Minimal Main.kt that imports generated types (compile check only)
    fs::write(
        client_dir.join("src/main/kotlin/Main.kt"),
        r#"import module_bindings.*

fun main() {
    // Compile-check: reference generated module type to ensure bindings are valid
    println(Module::class.simpleName)
}
"#,
    )
    .expect("Failed to write Main.kt");

    // Step 5: Copy Gradle wrapper from the Kotlin SDK into the temp project
    let gradlew = gradlew_path().expect("gradlew not found");
    let sdk_root = gradlew.parent().unwrap();
    fs::copy(&gradlew, client_dir.join("gradlew")).expect("Failed to copy gradlew");
    let wrapper_src = sdk_root.join("gradle/wrapper");
    let wrapper_dst = client_dir.join("gradle/wrapper");
    fs::create_dir_all(&wrapper_dst).expect("Failed to create gradle/wrapper dir");
    for entry in fs::read_dir(&wrapper_src)
        .expect("Failed to read gradle/wrapper")
        .flatten()
    {
        fs::copy(entry.path(), wrapper_dst.join(entry.file_name())).expect("Failed to copy gradle wrapper file");
    }

    // Run ./gradlew compileKotlin to validate the bindings compile
    let output = Command::new(client_dir.join("gradlew"))
        .args(["compileKotlin", "--no-daemon", "--stacktrace"])
        .current_dir(&client_dir)
        .output()
        .expect("Failed to run gradlew compileKotlin");

    if !output.status.success() {
        panic!(
            "gradle compileKotlin failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    eprintln!("Kotlin SDK smoketest passed: bindings compile successfully");
}

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
