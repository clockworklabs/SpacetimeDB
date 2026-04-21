#![allow(clippy::disallowed_macros)]
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{have_emscripten, require_dotnet, workspace_root};
use std::process::Command;

/// Test NativeAOT-LLVM build path for C# modules.
/// Requires emscripten to be installed.
/// Only runs on Windows since runtime.linux-x64.Microsoft.DotNet.ILCompiler.LLVM
/// is not available on the dotnet-experimental NuGet feed.
#[test]
fn test_build_csharp_module_aot() {
    require_dotnet!();

    // NativeAOT-LLVM is only available on Windows
    if std::env::consts::OS != "windows" {
        eprintln!("Skipping AOT test - NativeAOT-LLVM for .NET 8 only available on Windows");
        return;
    }

    // Check for emscripten - fail with helpful message if not available
    // Uses have_emscripten() which checks for both `emcc` and `emcc.bat` on Windows
    if !have_emscripten() {
        panic!(
            "NativeAOT-LLVM test requires emscripten but it was not found.\n\
             Install from: https://emscripten.org/docs/getting_started/downloads.html\n\
             Or ensure `emcc` is in your PATH."
        );
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

    // Verify StdbModule.wasm was produced
    let wasm_path = workspace.join("modules/sdk-test-cs/bin/Release/net8.0/wasi-wasm/publish/StdbModule.wasm");
    assert!(wasm_path.exists(), "StdbModule.wasm not found at {:?}", wasm_path);
}
