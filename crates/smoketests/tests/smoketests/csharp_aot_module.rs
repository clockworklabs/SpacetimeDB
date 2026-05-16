#![allow(clippy::disallowed_macros)]
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{require_dotnet, workspace_root};
use std::process::Command;

/// Detect the major version of the active .NET SDK.
fn dotnet_major_version() -> Option<u8> {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let v = String::from_utf8_lossy(&o.stdout);
            v.trim().split('.').next()?.parse::<u8>().ok()
        })
}

/// Test NativeAOT-LLVM build path for C# modules.
///
/// Platform support depends on the .NET SDK version:
/// - .NET 8 AOT: Windows-only (runtime.linux-x64.Microsoft.DotNet.ILCompiler.LLVM
///   8.0.0-* was never published to the dotnet-experimental NuGet feed).
/// - .NET 10 AOT: Windows and Linux (both runtime packages are available).
///
/// NativeAOT-LLVM targets WASI and uses WASI SDK (clang), not the wasi-experimental
/// workload or emscripten. WASI SDK is auto-downloaded by SpacetimeDB.Runtime.targets.
/// The user must set EXPERIMENTAL_WASM_AOT=1 to enable the AOT build path.
#[test]
fn test_build_csharp_module_aot() {
    require_dotnet!();

    let major = dotnet_major_version();
    let target_framework = match major {
        Some(v) if v >= 10 => "net10.0",
        Some(8) => "net8.0",
        _ => {
            eprintln!("Skipping AOT test - unsupported .NET SDK version: {:?}", major);
            return;
        }
    };

    // .NET 8 ILCompiler.LLVM packages are only available for Windows.
    // .NET 10+ ILCompiler.LLVM packages are available for Windows and Linux.
    if target_framework == "net8.0" && std::env::consts::OS != "windows" {
        eprintln!("Skipping .NET 8 AOT test - ILCompiler.LLVM 8.0.0-* only available on Windows");
        return;
    }
    if std::env::consts::OS != "windows" && std::env::consts::OS != "linux" {
        eprintln!("Skipping AOT test - NativeAOT-LLVM only available on Windows and Linux");
        return;
    }

    let workspace = workspace_root();
    let _cli_path = ensure_binaries_built();

    // Create isolated NuGet packages folder to avoid file lock conflicts
    // NativeAOT-LLVM packages contain DLLs that stay locked and interfere with other tests
    let nuget_packages_dir = tempfile::tempdir().expect("Failed to create temp directory for NuGet packages");

    // Set EXPERIMENTAL_WASM_AOT=1 for this specific build
    // Build sdk-test-cs with NativeAOT-LLVM
    let mut cmd = Command::new("dotnet");
    cmd.arg("publish")
        .arg("-c")
        .arg("Release")
        .current_dir(workspace.join("modules/sdk-test-cs"))
        .env("EXPERIMENTAL_WASM_AOT", "1")
        .env("NUGET_PACKAGES", nuget_packages_dir.path());

    let output = cmd.output().expect("Failed to run dotnet publish");

    assert!(
        output.status.success(),
        "NativeAOT-LLVM publish failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Clean up temp dir explicitly to verify no file locks remain
    // This ensures subsequent tests can clear NuGet locals without conflicts
    drop(nuget_packages_dir);

    // Verify StdbModule.wasm was produced at the correct TFM-specific output path
    let wasm_path = workspace.join(format!(
        "modules/sdk-test-cs/bin/Release/{target_framework}/wasi-wasm/publish/StdbModule.wasm"
    ));
    assert!(wasm_path.exists(), "StdbModule.wasm not found at {:?}", wasm_path);
}
