use anyhow::Context;
use itertools::Itertools;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_major_version(version: &str) -> Option<u8> {
    version.split('.').next()?.parse::<u8>().ok()
}

/// Describes which C# build path to use.
enum CsharpBuildPath {
    /// .NET 8 JIT via the `wasi-experimental` workload (Mono WASM).
    Net8Jit,
    /// .NET 8 NativeAOT-LLVM (opt-in via `--native-aot`).
    Net8Aot,
    /// .NET 10 NativeAOT-LLVM (auto-detected, only available path for .NET 10).
    Net10Aot,
}

pub(crate) fn build_csharp(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    // All `dotnet` commands must execute in the project directory, otherwise
    // global.json won't have any effect and wrong .NET SDK might be picked.
    macro_rules! dotnet {
        ($($arg:expr),*) => {
            duct::cmd!("dotnet", $($arg),*).dir(project_path)
        };
    }

    let native_aot_flag = std::env::var_os("EXPERIMENTAL_WASM_AOT").is_some_and(|v| v == "1");

    // Check for explicit dotnet version override from CLI (--dotnet-version flag)
    // This takes precedence over auto-detection.
    let dotnet_version_override = std::env::var("SPACETIMEDB_DOTNET_VERSION").ok();

    // Detect the .NET SDK version. Run from project directory only if global.json exists,
    // otherwise run from current directory. .NET 10 SDK crashes if global.json is missing.
    let global_json_exists = project_path.join("global.json").exists();
    let dotnet_version_str = match global_json_exists {
        true => dotnet!("--version").read(),
        false => duct::cmd!("dotnet", "--version").read(),
    } {
        Ok(v) => v,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("dotnet not found in PATH. Please install .NET SDK 8.0 or 10.0.")
        }
        Err(error) => anyhow::bail!("{error}"),
    };

    // Use explicit version if provided, otherwise auto-detect from dotnet --version
    let dotnet_major = dotnet_version_override
        .as_deref()
        .and_then(|v| v.parse().ok())
        .or_else(|| parse_major_version(&dotnet_version_str));

    // Determine the build path based on SDK version and --native-aot flag.
    let build_path = match (dotnet_major, native_aot_flag) {
        // .NET 10: always use NativeAOT-LLVM, no flag needed.
        (Some(10), _) => {
            if native_aot_flag {
                println!("Note: --native-aot is not needed with .NET 10 (NativeAOT-LLVM is used automatically).");
            }
            CsharpBuildPath::Net10Aot
        }
        // .NET 8 with --native-aot: use NativeAOT-LLVM with .NET 8 ILCompiler packages.
        (Some(8), true) => CsharpBuildPath::Net8Aot,
        // .NET 8 without flag: use the existing wasi-experimental JIT path.
        (Some(8), false) => CsharpBuildPath::Net8Jit,
        // Unsupported version.
        _ => {
            anyhow::bail!(
                "Unsupported .NET SDK version: {dotnet_version_str}. SpacetimeDB requires .NET SDK 8.0 or 10.0.\n\
                 If you have multiple versions installed, configure your project using \
                 https://learn.microsoft.com/en-us/dotnet/core/tools/global-json, \
                 or use --dotnet-version to specify the target version explicitly."
            );
        }
    };

    // For NativeAOT paths, ensure EXPERIMENTAL_WASM_AOT is set in the environment so MSBuild
    // conditionals in .csproj/.props/.targets files activate correctly.
    match &build_path {
        CsharpBuildPath::Net8Aot | CsharpBuildPath::Net10Aot => {
            // SAFETY: We are single-threaded at this point and no other code is reading
            // this environment variable concurrently.
            unsafe {
                std::env::set_var("EXPERIMENTAL_WASM_AOT", "1");
            }
        }
        CsharpBuildPath::Net8Jit => {}
    }

    // For the JIT path, ensure the wasi-experimental workload is installed.
    if matches!(build_path, CsharpBuildPath::Net8Jit) {
        // Check if the `wasi-experimental` workload is installed. Unfortunately, we
        // have to do this by inspecting the human-readable output. There is a
        // hidden `--machine-readable` flag but it also mixes in human-readable
        // output as well as unnecessarily updates various unrelated manifests.
        match dotnet!("workload", "list").read() {
            Ok(workloads) if workloads.contains("wasi-experimental") => {}
            Ok(_) => {
                // Finally, try to install the workload ourselves. On some systems
                // this might require elevated privileges, so print a nice error
                // message if it fails.
                dotnet!(
                    "workload",
                    "install",
                    "wasi-experimental",
                    "--skip-manifest-update"
                )
                .stderr_capture()
                .run()
                .context(concat!(
                    "Couldn't install the required wasi-experimental workload.\n",
                    "You might need to install it manually by running `dotnet workload install wasi-experimental` with privileged rights."
                ))?;
            }
            Err(error) => anyhow::bail!("{error}"),
        };
    }

    let config_name = if build_debug { "Debug" } else { "Release" };

    // Ensure the project path exists.
    fs::metadata(project_path).with_context(|| {
        format!(
            "The provided project path '{}' does not exist.",
            project_path.to_str().unwrap()
        )
    })?;

    // JIT and AOT builds use the same `dotnet publish` command.
    // Build-specific configuration (TFM, AOT settings, ILCompiler packages)
    // is handled by build_path detection and MSBuild props/targets.
    dotnet!("publish", "-c", config_name, "-v", "quiet").run()?;

    // Determine output path based on build path.
    // Both JIT and AOT builds produce StdbModule.wasm, but in different subdirectories:
    // - JIT (wasi-experimental): AppBundle/StdbModule.wasm
    // - AOT (NativeAOT-LLVM): publish/StdbModule.wasm
    let (target_framework, subdir) = match &build_path {
        CsharpBuildPath::Net10Aot => ("net10.0", "publish"),
        CsharpBuildPath::Net8Aot => ("net8.0", "publish"),
        CsharpBuildPath::Net8Jit => ("net8.0", "AppBundle"),
    };

    // check for the old .NET 7 path for projects that haven't migrated yet
    let bad_output_paths = [
        project_path.join(format!("bin/{config_name}/net7.0/StdbModule.wasm")),
        // for some reason there is sometimes a tilde here?
        project_path.join(format!("bin~/{config_name}/net7.0/StdbModule.wasm")),
    ];
    if bad_output_paths.iter().any(|p| p.exists()) {
        anyhow::bail!(concat!(
            "Looks like your project is using the deprecated .NET 7.0 WebAssembly bindings.\n",
            "Please migrate your project to the new .NET 8.0 template and delete the folders: bin, bin~, obj, obj~"
        ));
    }
    let possible_output_paths = [
        // Standard publish output paths (JIT and some AOT builds)
        project_path.join(format!(
            "bin/{config_name}/{target_framework}/wasi-wasm/{subdir}/StdbModule.wasm"
        )),
        project_path.join(format!(
            "bin~/{config_name}/{target_framework}/wasi-wasm/{subdir}/StdbModule.wasm"
        )),
        // NativeAOT-LLVM outputs to 'native' subdirectory instead of 'publish'
        project_path.join(format!(
            "bin/{config_name}/{target_framework}/wasi-wasm/native/StdbModule.wasm"
        )),
        project_path.join(format!(
            "bin~/{config_name}/{target_framework}/wasi-wasm/native/StdbModule.wasm"
        )),
        // Also check for raw wasm output without wasi-wasm RID folder (NativeAOT-LLVM sometimes does this)
        project_path.join(format!("bin/{config_name}/{target_framework}/native/StdbModule.wasm")),
        project_path.join(format!("bin~/{config_name}/{target_framework}/native/StdbModule.wasm")),
    ];
    // Check if both bin and bin~ variants exist for the same output path (indicates a conflict)
    for i in (0..possible_output_paths.len()).step_by(2) {
        if i + 1 < possible_output_paths.len() {
            let bin_path = &possible_output_paths[i];
            let bin_tilde_path = &possible_output_paths[i + 1];
            if bin_path.exists() && bin_tilde_path.exists() {
                anyhow::bail!(concat!(
                    "For some reason, your project has both a `bin` and a `bin~` folder.\n",
                    "I don't know which to use, so please delete both and rerun this command so that we can see which is up-to-date."
                ));
            }
        }
    }
    for output_path in possible_output_paths {
        if output_path.exists() {
            return Ok(output_path);
        }
    }
    anyhow::bail!("Built project successfully but couldn't find the output file.");
}

pub(crate) fn dotnet_format(project_dir: &Path, files: impl IntoIterator<Item = PathBuf>) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().expect("Failed to retrieve current directory");
    duct::cmd(
        "dotnet",
        itertools::chain(
            [
                "format",
                // We can't guarantee that the output lives inside a valid project or solution,
                // so to avoid crash we need to use the `dotnet whitespace --folder` mode instead
                // of a full style-aware formatter. Still better than nothing though.
                "whitespace",
                "--folder",
                project_dir.to_str().unwrap(),
                // Our files are marked with <auto-generated /> and will be skipped without this option.
                "--include-generated",
                "--include",
            ]
            .into_iter()
            .map_into::<OsString>(),
            // Resolve absolute paths for all of the files, because we receive them as relative paths to cwd, but
            // `dotnet format` will interpret those paths relative to `project_dir`.
            files
                .into_iter()
                .map(|f| {
                    let f = if f.is_absolute() { f } else { cwd.join(f) };
                    f.canonicalize().expect("Failed to canonicalize path: {f}")
                })
                .map_into(),
        ),
    )
    .run()?;
    Ok(())
}
