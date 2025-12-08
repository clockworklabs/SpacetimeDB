use anyhow::{anyhow, Context};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Run a program cross-platform. On Windows, go via `cmd /C` so .bat/.cmd work.
fn run_status<S: AsRef<str>>(prog: &str, args: &[S], cwd: &Path) -> anyhow::Result<()> {
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

fn run_read<S: AsRef<str>>(prog: &str, args: &[S]) -> std::io::Result<String> {
    #[cfg(windows)]
    {
        let mut pieces: Vec<String> = Vec::with_capacity(1 + args.len());
        pieces.push(prog.to_string());
        pieces.extend(args.iter().map(|s| s.as_ref().to_string()));
        duct::cmd("cmd", std::iter::once("/C").chain(pieces.iter().map(String::as_str)))
            .stderr_capture()
            .stdout_capture()
            .read()
    }
    #[cfg(not(windows))]
    {
        duct::cmd(prog, args.iter().map(|s| s.as_ref()))
            .stderr_capture()
            .stdout_capture()
            .read()
    }
}

pub(crate) fn build_cpp(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    // Tool sanity checks (shell-wrapped so .bat shims work on Windows)
    run_read("emcc", &["--version"])
        .map_err(|_| anyhow!("`emcc` not found/runnable. Activate Emscripten (emsdk_env)."))?;
    run_read("cmake", &["--version"]).map_err(|_| anyhow!("`cmake` not found in PATH."))?;
    // Just ensure emcmake runs; some versions lack `--version`.
    if run_read("emcmake", &["cmake", "--version"]).is_err() {
        return Err(anyhow!("`emcmake` not found/runnable. Is Emscripten env active?"));
    }

    let build_type = if build_debug { "Debug" } else { "Release" };
    let build_dir = project_path.join("build");

    // === Configure (no generator flags; let emcmake/cmake decide or reuse existing) ===
    // This matches: emcmake cmake -B build .
    // We keep -S/-B so `project_path` can be anywhere, and pass CMAKE_BUILD_TYPE (ignored by multi-config).
    let cfg_args = [
        "cmake",
        "-S",
        &project_path.to_string_lossy(),
        "-B",
        &build_dir.to_string_lossy(),
        &format!("-DCMAKE_BUILD_TYPE={}", build_type),
    ];
    run_status("emcmake", &cfg_args, project_path).context("Failed to configure C++ project with emcmake/cmake")?;

    // === Build (matches: cmake --build build) ===
    // Always pass --config; it’s required for multi-config and ignored for single-config.
    let build_args = [
        "--build",
        &build_dir.to_string_lossy(),
        "--config",
        build_type,
        "--parallel",
    ];
    run_status("cmake", &build_args, project_path).context("Failed to build C++ project")?;

    // Find the first .wasm under build/ (covers Debug/Release or target subdirs)
    let wasm = WalkDir::new(&build_dir)
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|e| {
            let p = e.path();
            (p.extension().and_then(|s| s.to_str()) == Some("wasm")).then(|| p.to_path_buf())
        })
        .ok_or_else(|| {
            anyhow!(
                "Built successfully but couldn't find a .wasm under {}",
                build_dir.display()
            )
        })?;

    Ok(wasm)
}
