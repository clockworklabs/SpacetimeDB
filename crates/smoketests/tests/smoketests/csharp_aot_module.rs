#![allow(clippy::disallowed_macros)]
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{require_dotnet, workspace_root};
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
    let emscripten_check = Command::new("emcc").arg("--version").output();
    if emscripten_check.is_err() || !emscripten_check.unwrap().status.success() {
        panic!(
            "NativeAOT-LLVM test requires emscripten but it was not found.\n\
             Install from: https://emscripten.org/docs/getting_started/downloads.html\n\
             Or ensure `emcc` is in your PATH."
        );
    }

    let workspace = workspace_root();
    let _cli_path = ensure_binaries_built();

    // Set EXPERIMENTAL_WASM_AOT=1 for this specific build
    // Build sdk-test-cs with NativeAOT-LLVM
    let mut cmd = Command::new("dotnet");
    cmd.arg("publish")
        .arg("-c")
        .arg("Release")
        .current_dir(workspace.join("modules/sdk-test-cs"))
        .env("EXPERIMENTAL_WASM_AOT", "1");

    let output = cmd.output().expect("Failed to run dotnet publish");
    assert!(
        output.status.success(),
        "NativeAOT-LLVM publish failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify StdbModule.wasm was produced
    let wasm_path = workspace.join("modules/sdk-test-cs/bin/Release/net8.0/wasi-wasm/publish/StdbModule.wasm");
    assert!(wasm_path.exists(), "StdbModule.wasm not found at {:?}", wasm_path);
}
