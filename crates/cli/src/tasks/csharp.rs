use anyhow::Context;
use itertools::Itertools;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_major_version(version: &str) -> Option<u8> {
    version.split('.').next()?.parse::<u8>().ok()
}

enum OriginalGlobalJson {
    Missing,
    File(String),
    Symlink(PathBuf),
}

struct TemporaryGlobalJson {
    path: PathBuf,
    original: OriginalGlobalJson,
}

impl Drop for TemporaryGlobalJson {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        match &self.original {
            OriginalGlobalJson::Missing => {}
            OriginalGlobalJson::File(content) => {
                let _ = fs::write(&self.path, content);
            }
            OriginalGlobalJson::Symlink(target) => {
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(target, &self.path);
                #[cfg(windows)]
                let _ = std::os::windows::fs::symlink_file(target, &self.path);
            }
        }
    }
}

fn dotnet_global_json(major: u8) -> anyhow::Result<String> {
    match major {
        8 => Ok(r#"{"sdk":{"version":"8.0.100","rollForward":"latestFeature"}}"#.to_string()),
        10 => Ok(r#"{"sdk":{"version":"10.0.100","rollForward":"latestMinor"}}"#.to_string()),
        _ => anyhow::bail!("Unsupported .NET SDK version: {major}. SpacetimeDB requires .NET SDK 8.0 or 10.0."),
    }
}

fn temporarily_pin_project_sdk(project_path: &Path, major: u8) -> anyhow::Result<TemporaryGlobalJson> {
    let path = project_path.join("global.json");
    let original = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => OriginalGlobalJson::Symlink(fs::read_link(&path)?),
        Ok(_) => OriginalGlobalJson::File(fs::read_to_string(&path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => OriginalGlobalJson::Missing,
        Err(error) => return Err(error.into()),
    };

    fs::write(&path, dotnet_global_json(major)?)?;
    Ok(TemporaryGlobalJson { path, original })
}

#[derive(Debug)]
struct CsprojTargetFrameworks {
    path: PathBuf,
    majors: Vec<u8>,
}

impl CsprojTargetFrameworks {
    fn single_major(&self) -> Option<u8> {
        (self.majors.len() == 1).then_some(self.majors[0])
    }
}

/// Read the target framework major versions directly from the project's `.csproj` file.
/// Returns `net8.0`, `net10.0`, etc. from both `<TargetFramework>` and `<TargetFrameworks>`.
/// Multi-target projects do not imply a single selected SDK version; callers should use a
/// higher-priority signal or default policy in that case.
fn read_tfms_from_csproj(project_path: &Path) -> Option<CsprojTargetFrameworks> {
    let entries: Vec<_> = match std::fs::read_dir(project_path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            eprintln!("read_tfm: read_dir({}) failed: {e}", project_path.display());
            return None;
        }
    };
    let csproj_entry = entries
        .iter()
        .find(|e| e.path().extension().and_then(|x| x.to_str()) == Some("csproj"));
    let csproj = match csproj_entry {
        Some(e) => e.path(),
        None => {
            let names: Vec<_> = entries.iter().map(|e| e.file_name()).collect();
            eprintln!(
                "read_tfm: no .csproj found in {}. Files: {:?}",
                project_path.display(),
                names
            );
            return None;
        }
    };
    eprintln!("read_tfm: found csproj at {}", csproj.display());
    let content = match std::fs::read_to_string(&csproj) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("read_tfm: failed to read {}: {e}", csproj.display());
            return None;
        }
    };
    let mut has_target_frameworks = false;
    let mut majors = Vec::new();
    for tag in ["TargetFramework", "TargetFrameworks"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let Some(start) = content.find(&open).map(|s| s + open.len()) else {
            continue;
        };
        let Some(end) = content[start..].find(&close).map(|e| start + e) else {
            continue;
        };

        has_target_frameworks |= tag == "TargetFrameworks";
        for tfm in content[start..end].split(';') {
            let Some(version) = tfm.trim().strip_prefix("net") else {
                continue;
            };
            if let Some(major) = version.split('.').next().and_then(|v| v.parse::<u8>().ok())
                && !majors.contains(&major)
            {
                majors.push(major);
            }
        }
    }

    majors.sort();
    eprintln!("read_tfm: parsed majors={majors:?}, has_target_frameworks={has_target_frameworks}");
    Some(CsprojTargetFrameworks { path: csproj, majors })
}

fn find_global_json(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|path| path.join("global.json"))
        .find(|path| path.exists())
}

fn remove_project_assets(project_dir: &Path) {
    for obj_dir in ["obj", "obj~"] {
        let assets = project_dir.join(obj_dir).join("project.assets.json");
        if assets.exists() {
            let _ = fs::remove_file(&assets);
        }
    }
}

fn project_reference_paths(csproj: &Path) -> Vec<PathBuf> {
    let Ok(content) = fs::read_to_string(csproj) else {
        return Vec::new();
    };
    let Some(project_dir) = csproj.parent() else {
        return Vec::new();
    };

    content
        .split("<ProjectReference")
        .skip(1)
        .filter_map(|entry| {
            let include = entry.split('>').next()?;
            let start = include.find("Include=\"")? + "Include=\"".len();
            let end = include[start..].find('"')? + start;
            Some(project_dir.join(&include[start..end]))
        })
        .collect()
}

fn remove_project_assets_recursive(csproj: &Path, visited: &mut HashSet<PathBuf>) {
    let key = fs::canonicalize(csproj).unwrap_or_else(|_| csproj.to_path_buf());
    if !visited.insert(key) {
        return;
    }

    if let Some(project_dir) = csproj.parent() {
        remove_project_assets(project_dir);
    }
    for reference in project_reference_paths(csproj) {
        remove_project_assets_recursive(&reference, visited);
    }
}

fn collect_project_references_recursive(csproj: &Path, visited: &mut HashSet<PathBuf>, references: &mut Vec<PathBuf>) {
    let key = fs::canonicalize(csproj).unwrap_or_else(|_| csproj.to_path_buf());
    if !visited.insert(key) {
        return;
    }

    for reference in project_reference_paths(csproj) {
        references.push(reference.clone());
        collect_project_references_recursive(&reference, visited, references);
    }
}

fn targets_netstandard20(csproj: &Path) -> bool {
    fs::read_to_string(csproj)
        .map(|content| {
            content.contains("<TargetFramework>netstandard2.0</TargetFramework>")
                || content.contains("<TargetFrameworks>netstandard2.0</TargetFrameworks>")
        })
        .unwrap_or(false)
}

/// Describes which C# build path to use.
#[derive(Debug)]
enum CsharpBuildPath {
    /// .NET 8 JIT via the `wasi-experimental` workload (Mono WASM).
    Net8Jit,
    /// .NET 8 NativeAOT-LLVM (opt-in via `--native-aot`).
    Net8Aot,
    /// .NET 10 NativeAOT-LLVM (auto-detected, only available path for .NET 10).
    Net10Aot,
}

pub(crate) fn build_csharp(
    project_path: &Path,
    build_debug: bool,
    native_aot: bool,
    dotnet_version_override: Option<u8>,
) -> anyhow::Result<PathBuf> {
    // All `dotnet` commands must execute in the project directory, otherwise
    // global.json won't have any effect and wrong .NET SDK might be picked.
    macro_rules! dotnet {
        ($($arg:expr),*) => {
            duct::cmd!("dotnet", $($arg),*).dir(project_path)
        };
    }

    let native_aot_flag = native_aot;

    let project_global_json = find_global_json(project_path);
    let cwd = std::env::current_dir()?;
    let cwd_global_json = find_global_json(&cwd);
    let dotnet_version_result = if project_global_json.is_some() {
        Some(dotnet!("--version").read())
    } else if cwd_global_json.is_some() {
        Some(duct::cmd!("dotnet", "--version").read())
    } else {
        None
    };
    let dotnet_version_str = match dotnet_version_result {
        Some(Ok(v)) => Some(v),
        Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("dotnet not found in PATH. Please install .NET SDK 8.0 or 10.0.")
        }
        Some(Err(error)) => anyhow::bail!("{error}"),
        None => None,
    };

    // Resolution order:
    //   1. --dotnet-version CLI flag (explicit user override)
    //   2. global.json-selected SDK, if one applies to the project or current directory
    //   3. Single <TargetFramework> in the project's .csproj
    //   4. .NET 10 default for missing or multi-target project context
    let csproj_tfms = read_tfms_from_csproj(project_path);
    eprintln!(
        "dotnet version detection: override={:?}, global_json={:?}, csproj_tfms={:?}, dotnet_version={:?}, project_path={}",
        dotnet_version_override,
        project_global_json.as_ref().or(cwd_global_json.as_ref()),
        csproj_tfms,
        dotnet_version_str,
        project_path.display()
    );
    let dotnet_major = dotnet_version_override
        .or_else(|| dotnet_version_str.as_deref().and_then(parse_major_version))
        .or_else(|| csproj_tfms.as_ref().and_then(CsprojTargetFrameworks::single_major))
        .or(Some(10));

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
            let selected_version = dotnet_major
                .map(|v| v.to_string())
                .or_else(|| dotnet_version_str.clone())
                .unwrap_or_else(|| "<unknown>".to_string());
            anyhow::bail!(
                "Unsupported .NET SDK version: {}. SpacetimeDB requires .NET SDK 8.0 or 10.0.\n\
                 If you have multiple versions installed, configure your project using \
                 https://learn.microsoft.com/en-us/dotnet/core/tools/global-json, \
                 or use --dotnet-version to specify the target version explicitly.",
                selected_version
            );
        }
    };

    let desired_sdk_major = match build_path {
        CsharpBuildPath::Net8Jit | CsharpBuildPath::Net8Aot => 8,
        CsharpBuildPath::Net10Aot => 10,
    };
    let active_sdk_major = dotnet_version_str.as_deref().and_then(parse_major_version);
    let temporary_global_json = if active_sdk_major != Some(desired_sdk_major) {
        let active = dotnet_version_str.as_deref().map(str::trim).unwrap_or("<unknown>");
        let global_json = temporarily_pin_project_sdk(project_path, desired_sdk_major)?;
        println!(
            "Note: temporarily pinned {} to the .NET {desired_sdk_major} SDK (active SDK was .NET {active}).",
            project_path.join("global.json").display()
        );
        Some(global_json)
    } else {
        None
    };

    // Manage the EXPERIMENTAL_WASM_AOT environment variable for MSBuild.
    // - Net8Aot / Net10Aot: must SET it — the ILCompiler.LLVM.targets import in
    //   SpacetimeDB.Runtime.targets is gated on this env var. Without it, the NativeAOT
    //   toolchain is not activated and dotnet produces managed DLLs instead of a .wasm.
    // - Net8Jit: must UNSET it — prevents MSBuild from incorrectly enabling NativeAOT mode
    //   when the env var is set globally (e.g., in CI).
    match &build_path {
        CsharpBuildPath::Net8Aot | CsharpBuildPath::Net10Aot => {
            // SAFETY: We are single-threaded at this point and no other code is reading
            // this environment variable concurrently.
            unsafe {
                std::env::set_var("EXPERIMENTAL_WASM_AOT", "1");
            }
        }
        CsharpBuildPath::Net8Jit => {
            // SAFETY: We are single-threaded at this point.
            unsafe {
                std::env::remove_var("EXPERIMENTAL_WASM_AOT");
            }
        }
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

    // For .NET 8 builds, force a re-restore by deleting any cached project.assets.json.
    // If a prior `dotnet restore` ran without EXPERIMENTAL_WASM_AOT=1 (e.g. as part of a
    // solution restore), the cached assets won't include ILCompiler.LLVM, causing
    // `dotnet publish` to silently fall back to the net8.0 Mono wasi-experimental path.
    // Conversely, if a prior restore ran with NativeAOT enabled, the JIT path can import
    // ILCompiler targets from stale assets before restore has a chance to correct them.
    // Deleting the file makes dotnet re-restore with the correct environment.
    //
    // Net10Aot does NOT need this: the ILCompiler.LLVM dependency is unconditional for
    // net10.0 in the Runtime.csproj, so the solution-level restore already resolves it.
    // Deleting the assets file here would force an implicit re-restore that uses the
    // project's local NuGet.Config, which may have stale/invalid package source paths
    // (e.g. Windows-only paths in CI on Linux), breaking the build.
    if matches!(build_path, CsharpBuildPath::Net8Aot | CsharpBuildPath::Net8Jit) {
        if let Some(csproj_path) = csproj_tfms.as_ref().map(|tfms| &tfms.path) {
            remove_project_assets_recursive(csproj_path, &mut HashSet::new());
        } else {
            remove_project_assets(project_path);
        }
    }

    // Ensure the project path exists.
    fs::metadata(project_path).with_context(|| {
        format!(
            "The provided project path '{}' does not exist.",
            project_path.to_str().unwrap()
        )
    })?;

    // Determine the target framework moniker and output subdirectory for this build path.
    // Both JIT and AOT builds produce StdbModule.wasm, but in different subdirectories:
    // - JIT (wasi-experimental): AppBundle/StdbModule.wasm
    // - AOT (NativeAOT-LLVM): publish/StdbModule.wasm
    let (target_framework, subdir) = match &build_path {
        CsharpBuildPath::Net10Aot => ("net10.0", "native"),
        CsharpBuildPath::Net8Aot => ("net8.0", "native"),
        CsharpBuildPath::Net8Jit => ("net8.0", "AppBundle"),
    };
    let target_frameworks_override = format!("TargetFrameworks={target_framework}");
    let csproj_file_name = csproj_tfms.as_ref().map(|tfms| {
        tfms.path
            .file_name()
            .map(OsString::from)
            .unwrap_or_else(|| tfms.path.as_os_str().to_os_string())
    });

    if matches!(build_path, CsharpBuildPath::Net8Aot | CsharpBuildPath::Net8Jit) {
        let mut restore_args: Vec<OsString> = vec!["restore".into()];
        if let Some(csproj_file_name) = &csproj_file_name {
            restore_args.push(csproj_file_name.clone());
        }
        restore_args.extend(["--force".into(), format!("-p:{target_frameworks_override}").into()]);
        duct::cmd("dotnet", restore_args).dir(project_path).run()?;

        if let Some(csproj_path) = csproj_tfms.as_ref().map(|tfms| &tfms.path) {
            let mut references = Vec::new();
            collect_project_references_recursive(csproj_path, &mut HashSet::new(), &mut references);
            for reference in references
                .into_iter()
                .unique_by(|path| fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
                .filter(|path| targets_netstandard20(path))
            {
                let reference = fs::canonicalize(&reference).unwrap_or(reference);
                duct::cmd("dotnet", ["restore".as_ref(), reference.as_os_str()])
                    .dir(project_path)
                    .run()?;
            }
        }
    }

    // JIT and AOT builds use the same `dotnet publish` command.
    // Build-specific configuration (TFM, AOT settings, ILCompiler packages)
    // is handled by build_path detection and MSBuild props/targets.
    // We pass -f {target_framework} explicitly so that the correct TFM is used
    // even when the system-default SDK version differs from the csproj TFM
    // (e.g. system is .NET 10 but csproj says net8.0 → must publish as net8.0).
    let mut publish_args: Vec<OsString> = vec!["publish".into()];
    if let Some(csproj_file_name) = &csproj_file_name {
        publish_args.push(csproj_file_name.clone());
    }
    publish_args.extend([
        "-c".into(),
        config_name.into(),
        "-f".into(),
        target_framework.into(),
        "-v".into(),
        "quiet".into(),
    ]);
    if matches!(build_path, CsharpBuildPath::Net8Aot | CsharpBuildPath::Net8Jit) {
        publish_args.push("--no-restore".into());
        publish_args.push("--force".into());
        publish_args.push(format!("-p:{target_frameworks_override}").into());
    }
    duct::cmd("dotnet", publish_args).dir(project_path).run()?;

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
            "bin/{config_name}/{target_framework}/wasi-wasm/{subdir}/StdbModule.wasm"
        )),
        project_path.join(format!(
            "bin~/{config_name}/{target_framework}/wasi-wasm/{subdir}/StdbModule.wasm"
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
    for output_path in &possible_output_paths {
        if output_path.exists() {
            drop(temporary_global_json);
            return Ok(output_path.clone());
        }
    }

    // Diagnostic: list what we expected and what actually exists in the output directories
    eprintln!("Build path: {build_path:?}, target_framework: {target_framework}, subdir: {subdir}");
    eprintln!("Expected output in one of:");
    for p in &possible_output_paths {
        eprintln!("  {}", p.display());
    }
    for bin_dir_name in ["bin", "bin~"] {
        let bin_dir = project_path.join(bin_dir_name);
        if bin_dir.exists() {
            eprintln!("Contents of {}:", bin_dir.display());
            fn list_recursive(dir: &std::path::Path, depth: usize) {
                if depth > 6 {
                    return;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        eprintln!(
                            "{}{}{}",
                            "  ".repeat(depth + 1),
                            name,
                            if path.is_dir() { "/" } else { "" }
                        );
                        if path.is_dir() {
                            list_recursive(&path, depth + 1);
                        }
                    }
                }
            }
            list_recursive(&bin_dir, 0);
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
