use anyhow::{anyhow, Context};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::detect::find_executable;

/// Execute a command in a working directory and verify it succeeds.
///
/// On Windows, commands are wrapped in `cmd /C` to support `.bat` and `.cmd` files
/// (e.g., `emcc.bat`, `emcmake.cmd`), which don't work with direct execution.
///
/// Returns an error if the command fails (non-zero exit code).
fn run_command<S: AsRef<str>>(prog: &str, args: &[S], cwd: &Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let mut pieces: Vec<String> = Vec::with_capacity(1 + args.len());
        pieces.push(prog.to_string());
        pieces.extend(args.iter().map(|s| s.as_ref().to_string()));
        duct::cmd("cmd", std::iter::once("/C").chain(pieces.iter().map(String::as_str)))
            .dir(cwd)
            .run()
            .with_context(|| format!("failed running `{prog}`"))?;
    }
    #[cfg(not(windows))]
    {
        duct::cmd(prog, args.iter().map(|s| s.as_ref()))
            .dir(cwd)
            .run()
            .with_context(|| format!("failed running `{prog}`"))?;
    }
    Ok(())
}

pub(crate) fn build_cpp(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    // Verify required tools are in PATH
    #[cfg(windows)]
    let emcc_found = find_executable("emcc.bat").is_some();
    #[cfg(not(windows))]
    let emcc_found = find_executable("emcc").is_some();

    if !emcc_found {
        return Err(anyhow!("`emcc` not found in PATH. Activate Emscripten (emsdk_env)."));
    }

    #[cfg(windows)]
    let cmake_found = find_executable("cmake.exe").is_some();
    #[cfg(not(windows))]
    let cmake_found = find_executable("cmake").is_some();

    if !cmake_found {
        return Err(anyhow!("`cmake` not found in PATH."));
    }

    #[cfg(windows)]
    let emcmake_found = find_executable("emcmake.bat").is_some();
    #[cfg(not(windows))]
    let emcmake_found = find_executable("emcmake").is_some();

    if !emcmake_found {
        return Err(anyhow!("`emcmake` not found in PATH. Is Emscripten env active?"));
    }

    let build_type = if build_debug { "Debug" } else { "Release" };
    let build_dir = project_path.join("build");

    // === Configure (no generator flags; let emcmake/cmake decide or reuse existing) ===
    // This matches: emcmake cmake -B build .
    // We keep -S/-B so `project_path` can be anywhere, and pass CMAKE_BUILD_TYPE (ignored by multi-config).
    let cfg_args = [
        "cmake",
        "-S",
        ".",
        "-B",
        "build",
        &format!("-DCMAKE_BUILD_TYPE={}", build_type),
    ];
    run_command("emcmake", &cfg_args, project_path).context("Failed to configure C++ project with emcmake/cmake")?;

    // === Build (matches: cmake --build build) ===
    // Always pass --config; it's required for multi-config and ignored for single-config.
    let build_args = ["--build", "build", "--config", build_type, "--parallel"];
    run_command("cmake", &build_args, project_path).context("Failed to build C++ project")?;

    // Find the most recently modified .wasm under build/ directory
    // This ensures we get the latest build output when rebuilding, instead of potentially
    // picking up an older cached wasm file from a previous build
    let wasm = WalkDir::new(&build_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("wasm") {
                e.metadata().ok().and_then(|m| m.modified().ok()).map(|mtime| (p.to_path_buf(), mtime))
            } else {
                None
            }
        })
        .max_by_key(|(_, mtime)| *mtime)
        .map(|(path, _)| path)
        .ok_or_else(|| {
            anyhow!(
                "Built successfully but couldn't find a .wasm under {}",
                build_dir.display()
            )
        })?;

    Ok(wasm)
}
