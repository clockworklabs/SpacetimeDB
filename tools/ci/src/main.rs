#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs};

const README_PATH: &str = "tools/ci/README.md";

mod ci_docs;
mod smoketest;

/// SpacetimeDB CI tasks
///
/// This tool provides several subcommands for automating CI workflows in SpacetimeDB.
///
/// It may be invoked via `cargo ci <subcommand>`, or simply `cargo ci` to run all subcommands in
/// sequence. It is mostly designed to be run in CI environments via the github workflows, but can
/// also be run locally
#[derive(Parser)]
#[command(name = "cargo ci", subcommand_required = false, arg_required_else_help = false)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<CiCmd>,

    /// Skip specified subcommands when running all
    ///
    /// When no subcommand is specified, all subcommands are run in sequence. This option allows
    /// specifying subcommands to skip when running all. For example, to skip the `unreal-tests`
    /// subcommand, use `--skip unreal-tests`.
    #[arg(long)]
    skip: Vec<String>,
}

fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}

fn find_all_global_json(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let name = entry.file_name();
            if name == "target" || name == "node_modules" || name == ".git" || name == ".templates" {
                continue;
            }
            out.extend(find_all_global_json(&path)?);
        } else if path.file_name() == Some(OsStr::new("global.json")) {
            out.push(path);
        }
    }
    Ok(out)
}

/// On Windows with `core.symlinks=false`, git stores symlinks as plain text files
/// containing the relative path. These aren't valid JSON, so dotnet commands fail.
/// This function detects such broken "symlinks" and temporarily replaces them with
/// the real root global.json content. Returns the list of patched paths so they can
/// be restored later via `git checkout`.
fn fix_global_json_symlinks() -> Result<Vec<PathBuf>> {
    let root_content = fs::read_to_string("global.json")?;
    let globals = find_all_global_json(Path::new("."))?;
    let mut patched = Vec::new();
    for p in &globals {
        // Skip the root global.json itself.
        if p == Path::new("./global.json") {
            continue;
        }
        let content = fs::read_to_string(p)?;
        // If the file content looks like a relative path (not JSON), it's a broken symlink.
        let trimmed = content.trim();
        if !trimmed.starts_with('{') {
            log::info!(
                "Patching broken global.json symlink: {} (was: {:?})",
                p.display(),
                trimmed
            );
            fs::write(p, &root_content)?;
            patched.push(p.clone());
        }
    }
    Ok(patched)
}

/// Restore global.json files that were patched by `fix_global_json_symlinks`.
fn restore_global_json_symlinks(patched: &[PathBuf]) -> Result<()> {
    if patched.is_empty() {
        return Ok(());
    }
    log::info!("Restoring {} patched global.json symlink(s)", patched.len());
    // Normalize paths: strip leading "./" or ".\" and convert backslashes to forward slashes
    // so git recognizes them on Windows.
    let path_strs: Vec<String> = patched
        .iter()
        .map(|p| {
            let s = p.display().to_string().replace('\\', "/");
            s.strip_prefix("./").unwrap_or(&s).to_string()
        })
        .collect();
    // Restore each file individually so one failure doesn't block the rest.
    for path in &path_strs {
        let result = cmd("git", &["checkout", "HEAD", "--", path]).run();
        if let Err(e) = result {
            log::warn!("Failed to restore {}: {}", path, e);
        }
    }
    Ok(())
}

fn check_global_json_policy() -> Result<()> {
    ensure_repo_root()?;

    let root_json = Path::new("global.json");
    let root_real = fs::canonicalize(root_json)?;

    let globals = find_all_global_json(Path::new("."))?;

    let mut ok = true;
    for p in globals {
        let resolved = fs::canonicalize(&p)?;

        // The root global.json itself is allowed.
        if resolved == root_real {
            println!("OK: {}", p.display());
            continue;
        }

        let meta = fs::symlink_metadata(&p)?;
        if !meta.file_type().is_symlink() {
            eprintln!("Error: {} is not a symlink to root global.json", p.display());
            ok = false;
            continue;
        }

        eprintln!("Error: {} does not resolve to root global.json", p.display());
        eprintln!("  resolved: {}", resolved.display());
        eprintln!("  expected: {}", root_real.display());
        ok = false;
    }

    if !ok {
        bail!("global.json policy check failed");
    }

    Ok(())
}

fn overlay_unity_meta_skeleton(pkg_id: &str) -> Result<()> {
    let skeleton_base = Path::new("sdks/csharp/unity-meta-skeleton~");
    let skeleton_root = skeleton_base.join(pkg_id);
    if !skeleton_root.exists() {
        return Ok(());
    }

    let pkg_root = Path::new("sdks/csharp/packages").join(pkg_id);
    if !pkg_root.exists() {
        return Ok(());
    }

    // Copy spacetimedb.<pkg>.meta
    let pkg_root_meta = skeleton_base.join(format!("{pkg_id}.meta"));
    if pkg_root_meta.exists() {
        if let Some(parent) = pkg_root.parent() {
            let pkg_meta_dst = parent.join(format!("{pkg_id}.meta"));
            fs::copy(&pkg_root_meta, &pkg_meta_dst)?;
        }
    }

    let versioned_dir = match find_only_subdir(&pkg_root) {
        Ok(dir) => dir,
        Err(err) => {
            log::info!("Skipping Unity meta overlay for {pkg_id}: could not locate restored version dir: {err}");
            return Ok(());
        }
    };

    // If version.meta exists under the skeleton package, rename it to match the restored version dir.
    let version_meta_template = skeleton_root.join("version.meta");
    if version_meta_template.exists() {
        if let Some(parent) = versioned_dir.parent() {
            let version_name = versioned_dir
                .file_name()
                .expect("versioned directory should have a file name");
            let version_meta_dst = parent.join(format!("{}.meta", version_name.to_string_lossy()));
            fs::copy(&version_meta_template, &version_meta_dst)?;
        }
    }

    copy_overlay_dir(&skeleton_root, &versioned_dir)
}

fn clear_restored_package_dirs(pkg_id: &str) -> Result<()> {
    let pkg_root = Path::new("sdks/csharp/packages").join(pkg_id);
    if !pkg_root.exists() {
        return Ok(());
    }

    fs::remove_dir_all(&pkg_root)?;

    Ok(())
}

fn find_only_subdir(dir: &Path) -> Result<PathBuf> {
    let mut subdirs: Vec<PathBuf> = vec![];

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            subdirs.push(entry.path());
        }
    }

    match subdirs.as_slice() {
        [] => Err(anyhow::anyhow!(
            "Could not find a restored versioned directory under {}",
            dir.display()
        )),
        [only] => Ok(only.clone()),
        _ => Err(anyhow::anyhow!(
            "Expected exactly one restored versioned directory under {}, found {}",
            dir.display(),
            subdirs.len()
        )),
    }
}

fn copy_overlay_dir(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        bail!("Skeleton directory does not exist: {}", src.display());
    }
    if !dst.exists() {
        bail!("Destination directory does not exist: {}", dst.display());
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            if dst_path.exists() {
                copy_overlay_dir(&src_path, &dst_path)?;
            }
        } else {
            if src_path.extension() == Some(OsStr::new("meta")) {
                let asset_path = dst_path
                    .parent()
                    .expect("dst_path should have a parent")
                    .join(dst_path.file_stem().expect(".meta file should have a file stem"));

                if asset_path.exists() {
                    fs::copy(&src_path, &dst_path)?;
                } else if dst_path.exists() {
                    fs::remove_file(&dst_path)?;
                }
                continue;
            }

            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[derive(Subcommand)]
enum CiCmd {
    /// Runs tests
    ///
    /// Runs rust tests, codegens csharp sdk and runs csharp tests.
    /// This does not include Unreal tests.
    /// This expects to run in a clean git state.
    Test,
    /// Lints the codebase
    ///
    /// Runs rustfmt, clippy, csharpier and generates rust docs to ensure there are no warnings.
    Lint,
    /// Tests Wasm bindings
    ///
    /// Runs tests for the codegen crate and builds a test module with the wasm bindings.
    WasmBindings,
    /// Builds and packs C# DLLs and NuGet packages for local Unity workflows
    ///
    /// Packs the in-repo C# NuGet packages and restores the C# SDK to populate `sdks/csharp/packages/**`.
    /// Then overlays Unity `.meta` skeleton files from `sdks/csharp/unity-meta-skeleton~/**` onto the restored
    /// versioned package directory, so Unity can associate stable meta files with the most recently built package.
    Dlls,
    /// Runs smoketests
    ///
    /// Executes the smoketests suite with some default exclusions.
    Smoketests(smoketest::SmoketestsArgs),
    /// Tests the update flow
    ///
    /// Tests the self-update flow by building the spacetimedb-update binary for the specified
    /// target, by default the current target, and performing a self-install into a temporary
    /// directory.
    UpdateFlow {
        #[arg(
            long,
            long_help = "Target triple to build for, by default the current target. Used by github workflows to check the update flow on multiple platforms."
        )]
        target: Option<String>,
        #[arg(
            long,
            default_value = "false",
            long_help = "Whether to enable github token authentication feature when building the update binary. By default this is disabled."
        )]
        github_token_auth: bool,
    },
    /// Generates CLI documentation and checks for changes
    CliDocs {
        #[arg(
            long,
            long_help = "specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is located)"
        )]
        spacetime_path: Option<String>,
    },
    SelfDocs {
        #[arg(
            long,
            default_value_t = false,
            long_help = "Only check for changes, do not generate the docs"
        )]
        check: bool,
    },

    /// Verify that any non-root global.json files are symlinks to the root global.json.
    GlobalJsonPolicy,

    /// Regenerate all committed codegen outputs in the repo.
    ///
    /// Run this after changing codegen, table schemas, or module definitions to keep
    /// committed bindings in sync. Finishes with `cargo fmt --all`.
    ///
    /// ## What this regenerates
    ///
    /// ### 1. SDK test bindings (`regen_sdk_test_bindings`)
    ///
    /// Rust (5 module/client pairs via `spacetime generate --lang rust`):
    /// - modules/sdk-test                    -> sdks/rust/tests/test-client/src/module_bindings/
    /// - modules/sdk-test-connect-disconnect  -> sdks/rust/tests/connect_disconnect_client/src/module_bindings/
    /// - modules/sdk-test-procedure           -> sdks/rust/tests/procedure-client/src/module_bindings/
    /// - modules/sdk-test-view                -> sdks/rust/tests/view-client/src/module_bindings/
    /// - modules/sdk-test-event-table         -> sdks/rust/tests/event-table-client/src/module_bindings/
    ///
    /// C# regression tests (3 pairs via `spacetime generate --lang csharp`):
    /// - sdks/csharp/examples~/regression-tests/client/module_bindings/
    /// - sdks/csharp/examples~/regression-tests/republishing/client/module_bindings/
    /// - sdks/csharp/examples~/regression-tests/procedure-client/module_bindings/
    ///
    /// Unreal (2 pairs via `spacetime generate --lang unrealcpp`):
    /// - modules/sdk-test        -> sdks/unreal/tests/TestClient/
    /// - modules/sdk-test-procedure -> sdks/unreal/tests/TestProcClient/
    ///
    /// ### 2. Demo bindings (`regen_demo_bindings`)
    ///
    /// - Blackholio Unity C#: demo/Blackholio/client-unity/Assets/Scripts/autogen/
    /// - Blackholio Unreal C++: demo/Blackholio/client-unreal/
    ///
    /// ### 3. Template bindings (`regen_template_bindings`)
    ///
    /// - C# quickstart-chat: templates/chat-console-cs/module_bindings/
    /// - Rust chat-console-rs: templates/chat-console-rs/src/module_bindings/
    /// - TS templates: auto-discovered via `pnpm -r --filter ./templates/** run generate`
    /// - deno-ts: templates/deno-ts/src/module_bindings/ (explicit, not pnpm-discoverable)
    /// - Unreal QuickstartChat: sdks/unreal/examples/QuickstartChat/
    ///
    /// ### 4. SDK internal bindings (`regen_sdk_internal_bindings`)
    ///
    /// - TS client-api: crates/bindings-typescript/src/sdk/client_api/
    /// - C# ClientApi: sdks/csharp/src/SpacetimeDB/ClientApi/
    /// - TS test-app: crates/bindings-typescript/test-app/src/module_bindings/
    /// - TS test-react-router-app: crates/bindings-typescript/test-react-router-app/src/module_bindings/
    ///
    /// ### 5. Moduledef type bindings (`regen_moduledef_bindings`)
    ///
    /// - C#:  crates/bindings-csharp/Runtime/Internal/Autogen/
    /// - TS:  crates/bindings-typescript/src/lib/autogen/
    /// - C++: crates/bindings-cpp/include/spacetimedb/internal/autogen/
    /// - Unreal SDK: sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings/
    ///
    /// ## Other codegen not covered by this command
    ///
    /// - C++ WASM module (modules/sdk-test-procedure-cpp/client/) — requires Emscripten toolchain
    /// - CLI reference docs: `cargo ci cli-docs`
    /// - Codegen snapshot tests: `cargo test -p spacetimedb-codegen` (uses insta snapshots,
    ///   update with `cargo insta review`)
    Regen {
        /// Only check if bindings are up-to-date, without modifying files.
        ///
        /// Regenerates all bindings into a temporary state and then runs
        /// `tools/check-diff.sh` to verify nothing changed. Exits with
        /// an error if any bindings are stale. Also runs `check_autogen_coverage`
        /// to verify all committed autogen directories are covered by the regen script.
        #[arg(long, default_value_t = false)]
        check: bool,
    },
}

/// All directories that `cargo ci regen` is known to regenerate.
/// Used by `check_autogen_coverage` to verify no committed autogen is missed.
const REGEN_DIRS: &[&str] = &[
    // 1. SDK test bindings — Rust
    "sdks/rust/tests/test-client/src/module_bindings",
    "sdks/rust/tests/connect_disconnect_client/src/module_bindings",
    "sdks/rust/tests/procedure-client/src/module_bindings",
    "sdks/rust/tests/view-client/src/module_bindings",
    "sdks/rust/tests/event-table-client/src/module_bindings",
    // 1. SDK test bindings — C# regression tests
    "sdks/csharp/examples~/regression-tests/client/module_bindings",
    "sdks/csharp/examples~/regression-tests/republishing/client/module_bindings",
    "sdks/csharp/examples~/regression-tests/procedure-client/module_bindings",
    // 1. SDK test bindings — Unreal
    "sdks/unreal/tests/TestClient/Source/TestClient/Public/ModuleBindings",
    "sdks/unreal/tests/TestClient/Source/TestClient/Private/ModuleBindings",
    "sdks/unreal/tests/TestProcClient/Source/TestProcClient/Public/ModuleBindings",
    "sdks/unreal/tests/TestProcClient/Source/TestProcClient/Private/ModuleBindings",
    // 2. Demo bindings
    "demo/Blackholio/client-unity/Assets/Scripts/autogen",
    "demo/Blackholio/client-unreal/Source/client_unreal/Public/ModuleBindings",
    "demo/Blackholio/client-unreal/Source/client_unreal/Private/ModuleBindings",
    // 3. Template bindings — C#
    "templates/chat-console-cs/module_bindings",
    // 3. Template bindings — Rust
    "templates/chat-console-rs/src/module_bindings",
    // 3. Template bindings — TS (pnpm-discovered)
    "templates/basic-ts/src/module_bindings",
    "templates/browser-ts/src/module_bindings",
    "templates/bun-ts/src/module_bindings",
    "templates/chat-react-ts/src/module_bindings",
    "templates/keynote-2/module_bindings",
    "templates/nextjs-ts/src/module_bindings",
    "templates/nodejs-ts/src/module_bindings",
    "templates/nuxt-ts/module_bindings",
    "templates/react-ts/src/module_bindings",
    "templates/remix-ts/src/module_bindings",
    "templates/svelte-ts/src/module_bindings",
    "templates/tanstack-ts/src/module_bindings",
    "templates/vue-ts/src/module_bindings",
    // 3. Template bindings — deno-ts (explicit)
    "templates/deno-ts/src/module_bindings",
    // 3. Template bindings — Unreal QuickstartChat
    "sdks/unreal/examples/QuickstartChat/Source/QuickstartChat/Public/ModuleBindings",
    "sdks/unreal/examples/QuickstartChat/Source/QuickstartChat/Private/ModuleBindings",
    // 4. SDK internal bindings — TS client-api
    "crates/bindings-typescript/src/sdk/client_api",
    // 4. SDK internal bindings — C# ClientApi
    "sdks/csharp/src/SpacetimeDB/ClientApi",
    // 4. SDK internal bindings — TS test-app
    "crates/bindings-typescript/test-app/src/module_bindings",
    // 4. SDK internal bindings — TS test-react-router-app
    "crates/bindings-typescript/test-react-router-app/src/module_bindings",
    // 5. Moduledef type bindings
    "crates/bindings-csharp/Runtime/Internal/Autogen",
    "crates/bindings-typescript/src/lib/autogen",
    "crates/bindings-cpp/include/spacetimedb/internal/autogen",
    // 5. Moduledef type bindings — Unreal SDK
    "sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings",
];

/// Templates with committed bindings that are NOT pnpm-discoverable (handled explicitly).
/// Used by the template generate-script coverage check.
const TEMPLATES_WITHOUT_PNPM_GENERATE: &[&str] = &[
    "templates/chat-console-cs",
    "templates/chat-console-rs",
    "templates/deno-ts",
];

/// Capture the output of `get_ws_schema_v2` and write it to a temp file.
/// Returns the temp file handle (caller must keep it alive so the path stays valid).
fn get_ws_schema_tempfile() -> Result<tempfile::NamedTempFile> {
    log::info!("Capturing WS schema v2 to temp file");
    let schema_json = cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-client-api-messages",
        "--example",
        "get_ws_schema_v2"
    )
    .read()?;
    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(schema_json.as_bytes())?;
    tmp.flush()?;
    Ok(tmp)
}

// ---------------------------------------------------------------------------
// 1. SDK test bindings
// ---------------------------------------------------------------------------

/// Regenerate all SDK test bindings: Rust, C#, and Unreal.
fn regen_sdk_test_bindings() -> Result<()> {
    // --- Rust SDK test bindings ---
    // (module_dir, client_dir, include_private)
    let rust_bindings: &[(&str, &str, bool)] = &[
        ("sdk-test", "test-client", true),
        ("sdk-test-connect-disconnect", "connect_disconnect_client", true),
        ("sdk-test-procedure", "procedure-client", true),
        ("sdk-test-view", "view-client", true),
        ("sdk-test-event-table", "event-table-client", false),
    ];

    for &(module, client, include_private) in rust_bindings {
        let module_path = format!("modules/{module}");
        let out_dir = format!("sdks/rust/tests/{client}/src/module_bindings");
        log::info!("Generating Rust bindings: {module} -> {client}");

        let mut args = vec![
            "run",
            "-p",
            "spacetimedb-cli",
            "--",
            "generate",
            "--lang",
            "rust",
            "--project-path",
            &module_path,
            "--out-dir",
            &out_dir,
            "-y",
        ];
        if include_private {
            args.push("--include-private");
        }
        cmd("cargo", &args).run()?;
    }

    // --- C# regression test bindings ---
    let regression_tests: &[(&str, &str)] = &[
        (
            "sdks/csharp/examples~/regression-tests/server",
            "sdks/csharp/examples~/regression-tests/client/module_bindings",
        ),
        (
            "sdks/csharp/examples~/regression-tests/republishing/server-republish",
            "sdks/csharp/examples~/regression-tests/republishing/client/module_bindings",
        ),
        (
            "modules/sdk-test-procedure",
            "sdks/csharp/examples~/regression-tests/procedure-client/module_bindings",
        ),
    ];

    for &(project_path, out_dir) in regression_tests {
        log::info!("Generating C# regression test bindings: {out_dir}");
        cmd!(
            "cargo",
            "run",
            "-p",
            "spacetimedb-cli",
            "--",
            "generate",
            "-y",
            "-l",
            "csharp",
            "-o",
            out_dir,
            "--project-path",
            project_path
        )
        .run()?;
    }

    // --- Unreal SDK test bindings ---
    // (module, uproject_dir, module_name)
    let unreal_tests: &[(&str, &str, &str)] = &[
        ("sdk-test", "sdks/unreal/tests/TestClient", "TestClient"),
        (
            "sdk-test-procedure",
            "sdks/unreal/tests/TestProcClient",
            "TestProcClient",
        ),
    ];

    for &(module, uproject_dir, module_name) in unreal_tests {
        let module_path = format!("modules/{module}");
        log::info!("Generating Unreal bindings: {module} -> {module_name}");
        cmd!(
            "cargo",
            "run",
            "-p",
            "spacetimedb-cli",
            "--",
            "generate",
            "--lang",
            "unrealcpp",
            "--uproject-dir",
            uproject_dir,
            "--project-path",
            &module_path,
            "--module-name",
            module_name,
            "-y"
        )
        .run()?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Demo bindings
// ---------------------------------------------------------------------------

/// Regenerate demo project bindings: Blackholio Unity C# and Unreal C++.
fn regen_demo_bindings() -> Result<()> {
    log::info!("Generating Blackholio C# bindings");
    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "--lang",
        "csharp",
        "--project-path",
        "demo/Blackholio/server-rust",
        "--out-dir",
        "demo/Blackholio/client-unity/Assets/Scripts/autogen",
        "-y"
    )
    .run()?;

    log::info!("Generating Blackholio Unreal C++ bindings");
    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "--lang",
        "unrealcpp",
        "--uproject-dir",
        "demo/Blackholio/client-unreal",
        "--project-path",
        "demo/Blackholio/server-rust",
        "--module-name",
        "client_unreal",
        "-y"
    )
    .run()?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Template bindings
// ---------------------------------------------------------------------------

/// Regenerate starter-template bindings: C#, Rust, TS (via pnpm), deno-ts, Unreal QuickstartChat.
fn regen_template_bindings() -> Result<()> {
    // C# quickstart chat
    log::info!("Generating C# quickstart-chat template bindings");
    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "-y",
        "-l",
        "csharp",
        "-o",
        "templates/chat-console-cs/module_bindings",
        "--project-path",
        "templates/chat-console-cs/spacetimedb"
    )
    .run()?;

    // Rust chat-console-rs
    log::info!("Generating Rust chat-console-rs template bindings");
    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "-y",
        "-l",
        "rust",
        "-o",
        "templates/chat-console-rs/src/module_bindings",
        "--project-path",
        "templates/chat-console-rs/spacetimedb"
    )
    .run()?;

    // TS templates — auto-discover all under templates/ with a `generate` script
    log::info!("Generating TypeScript template bindings (pnpm)");
    cmd!(
        "pnpm",
        "-r",
        "--workspace-concurrency",
        "1",
        "--filter",
        "./templates/**",
        "--if-present",
        "run",
        "generate"
    )
    .run()?;

    // deno-ts — not pnpm-discoverable; install deps in its JS module dir first
    log::info!("Generating deno-ts template bindings");
    cmd!("pnpm", "install", "--ignore-workspace")
        .dir("templates/deno-ts/spacetimedb")
        .run()?;
    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "--lang",
        "typescript",
        "--out-dir",
        "templates/deno-ts/src/module_bindings",
        "--project-path",
        "templates/deno-ts/spacetimedb",
        "-y"
    )
    .run()?;
    // prettier may fail on Windows due to file locking; retry once after a brief pause.
    if cmd!("pnpm", "prettier", "--write", "templates/deno-ts/src/module_bindings")
        .run()
        .is_err()
    {
        std::thread::sleep(std::time::Duration::from_secs(1));
        cmd!("pnpm", "prettier", "--write", "templates/deno-ts/src/module_bindings").run()?;
    }

    // Unreal QuickstartChat example (same source module as C# quickstart-chat)
    log::info!("Generating Unreal QuickstartChat template bindings");
    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "--lang",
        "unrealcpp",
        "--uproject-dir",
        "sdks/unreal/examples/QuickstartChat",
        "--project-path",
        "templates/chat-console-cs/spacetimedb",
        "--module-name",
        "QuickstartChat",
        "-y"
    )
    .run()?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 4. SDK internal bindings
// ---------------------------------------------------------------------------

/// Regenerate SDK internal codegen: TS client-api, C# ClientApi, TS test-app, TS test-react-router-app.
fn regen_sdk_internal_bindings() -> Result<()> {
    // TS client-api bindings
    log::info!("Generating TypeScript client-api bindings");
    cmd!("cargo", "run", "-p", "generate-client-api").run()?;
    // prettier may fail on Windows due to file locking; retry once after a brief pause.
    if cmd!(
        "pnpm",
        "prettier",
        "--write",
        "crates/bindings-typescript/src/sdk/client_api"
    )
    .run()
    .is_err()
    {
        std::thread::sleep(std::time::Duration::from_secs(1));
        cmd!(
            "pnpm",
            "prettier",
            "--write",
            "crates/bindings-typescript/src/sdk/client_api"
        )
        .run()?;
    }

    // C# ClientApi bindings
    regen_csharp_client_api()?;

    // TS test-app bindings
    log::info!("Generating TypeScript test-app bindings");
    cmd!(
        "pnpm",
        "-r",
        "--filter",
        "./crates/bindings-typescript/test-app",
        "run",
        "generate"
    )
    .run()?;

    // TS test-react-router-app bindings (not in pnpm workspace)
    log::info!("Generating TypeScript test-react-router-app bindings");
    cmd!(
        "cargo",
        "run",
        "-p",
        "gen-bindings",
        "--",
        "--replacement",
        "../../../src/index"
    )
    .dir("crates/bindings-typescript/test-react-router-app")
    .run()?;
    // prettier may fail on Windows due to file locking; retry once after a brief pause.
    if cmd!(
        "pnpm",
        "prettier",
        "--write",
        "crates/bindings-typescript/test-react-router-app/src/module_bindings"
    )
    .run()
    .is_err()
    {
        std::thread::sleep(std::time::Duration::from_secs(1));
        cmd!(
            "pnpm",
            "prettier",
            "--write",
            "crates/bindings-typescript/test-react-router-app/src/module_bindings"
        )
        .run()?;
    }

    Ok(())
}

/// Regenerate C# ClientApi bindings from the WS v2 schema.
fn regen_csharp_client_api() -> Result<()> {
    log::info!("Generating C# ClientApi bindings");
    let out_dir = "sdks/csharp/src/SpacetimeDB/ClientApi";
    let output_staging = format!("{out_dir}/.output");

    let schema_tmp = get_ws_schema_tempfile()?;
    let schema_path = schema_tmp.path().to_string_lossy().to_string();

    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "-l",
        "csharp",
        "--namespace",
        "SpacetimeDB.ClientApi",
        "--module-def",
        &schema_path,
        "-o",
        &output_staging,
        "-y"
    )
    .run()?;

    // Move Types/* -> out_dir/
    let types_dir = format!("{output_staging}/Types");
    if Path::new(&types_dir).exists() {
        for entry in fs::read_dir(&types_dir)? {
            let entry = entry?;
            let src = entry.path();
            let dst = Path::new(out_dir).join(entry.file_name());
            fs::copy(&src, &dst)?;
        }
    }

    // Clean up staging directory
    if Path::new(&output_staging).exists() {
        fs::remove_dir_all(&output_staging)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Moduledef type bindings
// ---------------------------------------------------------------------------

/// Regenerate moduledef type bindings: C#, TypeScript, C++, and Unreal SDK.
fn regen_moduledef_bindings() -> Result<()> {
    let moduledef_examples: &[(&str, &str)] = &[
        ("regen-csharp-moduledef", "C#"),
        ("regen-typescript-moduledef", "TypeScript"),
        ("regen-cpp-moduledef", "C++"),
    ];

    for &(example, lang) in moduledef_examples {
        log::info!("Regenerating {lang} moduledef bindings");
        cmd!("cargo", "run", "-p", "spacetimedb-codegen", "--example", example).run()?;
    }

    // Unreal SDK moduledef
    regen_unreal_sdk_moduledef()?;

    Ok(())
}

/// Regenerate the Unreal SDK moduledef bindings from the WS v2 schema.
fn regen_unreal_sdk_moduledef() -> Result<()> {
    log::info!("Regenerating Unreal SDK moduledef bindings");
    let sdk_base = "sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk";

    let schema_tmp = get_ws_schema_tempfile()?;
    let schema_path = schema_tmp.path().to_string_lossy().to_string();

    cmd!(
        "cargo",
        "run",
        "-p",
        "spacetimedb-cli",
        "--",
        "generate",
        "--lang",
        "unrealcpp",
        "--uproject-dir",
        "sdks/unreal/src/SpacetimeDbSdk",
        "--module-name",
        "SpacetimeDbSdk",
        "--module-def",
        &schema_path,
        "-y"
    )
    .run()?;

    // Cleanup per DEVELOP.md: delete files that should not be checked in
    let private_mb = format!("{sdk_base}/Private/ModuleBindings");
    if Path::new(&private_mb).exists() {
        log::info!("Cleaning up {private_mb}");
        fs::remove_dir_all(&private_mb)?;
    }
    let reducer_base = format!("{sdk_base}/Public/ModuleBindings/ReducerBase.g.h");
    if Path::new(&reducer_base).exists() {
        fs::remove_file(&reducer_base)?;
    }
    let client_header = format!("{sdk_base}/Public/ModuleBindings/SpacetimeDBClient.g.h");
    if Path::new(&client_header).exists() {
        fs::remove_file(&client_header)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Version-only change cleanup
// ---------------------------------------------------------------------------

const VERSION_LINE_PREFIX: &str = "// This was generated using spacetimedb";

/// Strip version comment lines from file content for comparison purposes.
/// Returns a new string with those lines replaced by a stable placeholder.
fn strip_version_lines(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            if line.starts_with(VERSION_LINE_PREFIX) {
                "// VERSION_LINE_STRIPPED"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// After regeneration, revert files whose only change is the version/commit comment.
/// This prevents noisy diffs when running `cargo ci regen` on a different commit.
///
/// The version comment only lives in one "root" file per output directory (e.g.
/// `mod.rs`, `index.ts`, `SpacetimeDBClient.g.cs`). We must not revert that file
/// if sibling files in the same output directory have real changes — otherwise
/// you'd get a stale version comment next to actually-updated code.
///
/// Strategy: group modified files by their nearest REGEN_DIRS parent. For each
/// group, only revert version-only files if *every* file in the group is either
/// unchanged or version-only.
fn revert_version_only_changes() -> Result<()> {
    let modified = cmd!("git", "diff", "--name-only").read()?;
    if modified.is_empty() {
        return Ok(());
    }

    // Classify each modified file as version-only or real-change.
    // version_only: files where stripping the version line makes old == new
    // real_change:  files with substantive differences
    let mut version_only: Vec<String> = Vec::new();
    let mut real_change: BTreeSet<String> = BTreeSet::new();

    for file in modified.lines() {
        let file = file.trim();
        if file.is_empty() {
            continue;
        }
        let path = Path::new(file);
        if !path.exists() {
            continue;
        }

        let old_content = match cmd!("git", "show", format!("HEAD:{file}")).read() {
            Ok(c) => c,
            Err(_) => {
                real_change.insert(file.to_string());
                continue;
            }
        };

        let new_content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                real_change.insert(file.to_string());
                continue;
            }
        };

        if strip_version_lines(&old_content) == strip_version_lines(&new_content) {
            version_only.push(file.to_string());
        } else {
            real_change.insert(file.to_string());
        }
    }

    if version_only.is_empty() {
        return Ok(());
    }

    // For each version-only file, find its REGEN_DIRS parent directory.
    // Only revert if no sibling file in the same regen dir has real changes.
    let mut reverted = 0u32;
    for file in &version_only {
        let normalized = file.replace('\\', "/");
        // Find the REGEN_DIRS entry that is a prefix of this file
        let regen_dir = REGEN_DIRS.iter().find(|dir| normalized.starts_with(&format!("{dir}/")));

        let dominated_by_real_change = match regen_dir {
            Some(dir) => {
                // Check if any real_change file shares this regen dir
                real_change
                    .iter()
                    .any(|rc| rc.replace('\\', "/").starts_with(&format!("{dir}/")))
            }
            None => {
                // File is not under any REGEN_DIRS entry; safe to revert individually
                false
            }
        };

        if !dominated_by_real_change {
            cmd!("git", "checkout", "HEAD", "--", file).run()?;
            reverted += 1;
        }
    }

    if reverted > 0 {
        log::info!("Reverted {reverted} file(s) with version-comment-only changes");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Autogen coverage check
// ---------------------------------------------------------------------------

/// Walk a directory tree, collecting all files (not dirs). Skips symlinks.
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip hidden directories (.git, etc.) and common large dirs
        if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk_files(&entry.path(), out)?;
        } else if ft.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// Check that all committed autogen directories are covered by `REGEN_DIRS`.
/// Also checks that templates with committed bindings have a `generate` script.
fn check_autogen_coverage() -> Result<()> {
    log::info!("Checking autogen coverage");

    let markers = &[
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB",
        "// This was generated using spacetimedb cli version",
    ];

    // Directories to skip entirely.
    // - node_modules/target: build artifacts
    // - .templates: CLI codegen test snapshots
    // - crates/codegen: contains the marker string as source constants, not actual generated files
    // - modules/sdk-test-procedure-cpp/client: C++ WASM module requiring Emscripten, not buildable in CI
    let skip_components = &[
        "node_modules",
        "target",
        ".templates",
        "crates/codegen",
        "modules/sdk-test-procedure-cpp/client",
    ];

    // Collect all files in the repo
    let mut all_files = Vec::new();
    walk_files(Path::new("."), &mut all_files)?;

    // Find parent directories of files containing autogen markers
    let mut autogen_dirs: BTreeSet<String> = BTreeSet::new();

    for file_path in &all_files {
        // Skip ignored directories
        let path_str = file_path.to_string_lossy();
        // Normalize to forward slashes for matching
        let normalized = path_str.replace('\\', "/");
        if skip_components
            .iter()
            .any(|skip| normalized.contains(&format!("/{skip}/")) || normalized.starts_with(&format!("{skip}/")))
        {
            continue;
        }

        // Only check text-ish files
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "cs" | "ts" | "tsx" | "h" | "hpp" | "cpp" | "js") {
            continue;
        }

        // Read the first few lines to check for markers
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Only check the first 5 lines for the marker
        let header: String = content.lines().take(5).collect::<Vec<_>>().join("\n");
        if markers.iter().any(|m| header.contains(m)) {
            if let Some(parent) = file_path.parent() {
                // Normalize to forward slashes and strip leading ./
                let dir_str = parent
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .to_string();
                autogen_dirs.insert(dir_str);
            }
        }
    }

    // Build the allowlist set (including subdirectories of allowed dirs)
    let regen_dirs_set: BTreeSet<&str> = REGEN_DIRS.iter().copied().collect();

    // Check each discovered autogen dir against the allowlist.
    // A dir is covered if it equals or is a subdirectory of any REGEN_DIRS entry.
    let mut uncovered = Vec::new();
    for dir in &autogen_dirs {
        let covered = regen_dirs_set
            .iter()
            .any(|allowed| dir == allowed || dir.starts_with(&format!("{allowed}/")));
        if !covered {
            uncovered.push(dir.clone());
        }
    }

    if !uncovered.is_empty() {
        let list = uncovered
            .iter()
            .map(|d| format!("  - {d}/"))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "Found committed autogen files not covered by `cargo ci regen`:\n{list}\n\
             Add these directories to the regen script and to the REGEN_DIRS allowlist in main.rs."
        );
    }

    // Template-specific check: templates with committed bindings should have a `generate` script
    check_template_generate_coverage()?;

    log::info!("Autogen coverage check passed");
    Ok(())
}

/// For template directories with committed bindings, verify they have a `generate` script
/// in their package.json (unless explicitly handled outside pnpm).
fn check_template_generate_coverage() -> Result<()> {
    let templates_dir = Path::new("templates");
    if !templates_dir.exists() {
        return Ok(());
    }

    let mut missing = Vec::new();

    for entry in fs::read_dir(templates_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let template_path = entry.path();
        let template_name = format!("templates/{}", template_path.file_name().unwrap().to_string_lossy());

        // Skip templates that are explicitly handled without pnpm
        if TEMPLATES_WITHOUT_PNPM_GENERATE.iter().any(|t| *t == template_name) {
            continue;
        }

        // Check if this template has any committed bindings (module_bindings dir)
        let has_bindings =
            template_path.join("src/module_bindings").exists() || template_path.join("module_bindings").exists();

        if !has_bindings {
            continue;
        }

        // Check if package.json has a "generate" script
        let pkg_json_path = template_path.join("package.json");
        if !pkg_json_path.exists() {
            missing.push(template_name);
            continue;
        }

        let content = fs::read_to_string(&pkg_json_path)?;
        if !content.contains("\"generate\"") {
            missing.push(template_name);
        }
    }

    if !missing.is_empty() {
        let list = missing
            .iter()
            .map(|t| format!("  - {t}"))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "Templates with committed bindings but no `generate` script in package.json:\n{list}\n\
             Add a `generate` script or add the template to TEMPLATES_WITHOUT_PNPM_GENERATE."
        );
    }

    Ok(())
}

fn run_all_clap_subcommands(skips: &[String]) -> Result<()> {
    let subcmds = Cli::command()
        .get_subcommands()
        .map(|sc| sc.get_name().to_string())
        .collect::<Vec<_>>();

    for subcmd in subcmds {
        if skips.contains(&subcmd) {
            log::info!("skipping {subcmd} as requested");
            continue;
        }
        log::info!("executing cargo ci {subcmd}");
        cmd!("cargo", "ci", &subcmd).run()?;
    }
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.cmd {
        Some(CiCmd::Test) => {
            // TODO: This doesn't work on at least user Linux machines, because something here apparently uses `sudo`?

            // Exclude smoketests from `cargo test --all` since they require pre-built binaries.
            // Smoketests have their own dedicated command: `cargo ci smoketests`
            cmd!(
                "cargo",
                "test",
                "--all",
                "--exclude",
                "spacetimedb-smoketests",
                "--exclude",
                "spacetimedb-sdk",
                "--",
                "--test-threads=2",
                "--skip",
                "unreal"
            )
            .run()?;
            // SDK procedure tests intentionally make localhost HTTP requests.
            cmd!(
                "cargo",
                "test",
                "-p",
                "spacetimedb-sdk",
                "--features",
                "allow_loopback_http_for_tests",
                "--",
                "--test-threads=2",
                "--skip",
                "unreal"
            )
            .run()?;
            // TODO: This should check for a diff at the start. If there is one, we should alert the user
            // that we're disabling diff checks because they have a dirty git repo, and to re-run in a clean one
            // if they want those checks.

            // The fallocate tests have been flakely when running in parallel
            cmd!(
                "cargo",
                "test",
                "-p",
                "spacetimedb-durability",
                "--features",
                "fallocate",
                "--",
                "--test-threads=1",
            )
            .run()?;
            cmd!("bash", "tools/check-diff.sh").run()?;
            cmd!(
                "cargo",
                "run",
                "-p",
                "spacetimedb-codegen",
                "--example",
                "regen-csharp-moduledef",
            )
            .run()?;
            cmd!("bash", "tools/check-diff.sh", "crates/bindings-csharp").run()?;
            cmd!("dotnet", "test", "-warnaserror")
                .dir("crates/bindings-csharp")
                .run()?;
        }

        Some(CiCmd::Lint) => {
            cmd!("cargo", "fmt", "--all", "--", "--check").run()?;
            cmd!(
                "cargo",
                "clippy",
                "--all",
                "--tests",
                "--benches",
                "--",
                "-D",
                "warnings",
            )
            .run()?;
            cmd!("dotnet", "tool", "restore").dir("crates/bindings-csharp").run()?;
            cmd!("dotnet", "csharpier", "--check", ".")
                .dir("crates/bindings-csharp")
                .run()?;
            // `bindings` is the only crate we care strongly about documenting,
            // since we link to its docs.rs from our website.
            // We won't pass `--no-deps`, though,
            // since we want everything reachable through it to also work.
            // This includes `sats` and `lib`.
            cmd!("cargo", "doc")
                .dir("crates/bindings")
                // Make `cargo doc` exit with error on warnings, most notably broken links
                .env("RUSTDOCFLAGS", "--deny warnings")
                .run()?;
        }

        Some(CiCmd::WasmBindings) => {
            cmd!("cargo", "test", "-p", "spacetimedb-codegen").run()?;
            // Make sure the `Cargo.lock` file reflects the latest available versions.
            // This is what users would end up with on a fresh module, so we want to
            // catch any compile errors arising from a different transitive closure
            // of dependencies than what is in the workspace lock file.
            //
            // For context see also: https://github.com/clockworklabs/SpacetimeDB/pull/2714
            cmd!("cargo", "update").run()?;
            cmd!(
                "cargo",
                "run",
                "-p",
                "spacetimedb-cli",
                "--",
                "build",
                "--project-path",
                "modules/module-test",
            )
            .run()?;
        }

        Some(CiCmd::Dlls) => {
            ensure_repo_root()?;

            cmd!(
                "dotnet",
                "pack",
                "crates/bindings-csharp/BSATN.Runtime",
                "-c",
                "Release"
            )
            .run()?;
            cmd!("dotnet", "pack", "crates/bindings-csharp/Runtime", "-c", "Release").run()?;

            let repo_root = env::current_dir()?;
            let bsatn_source = repo_root.join("crates/bindings-csharp/BSATN.Runtime/bin/Release");
            let runtime_source = repo_root.join("crates/bindings-csharp/Runtime/bin/Release");

            let nuget_config_dir = tempfile::tempdir()?;
            let nuget_config_path = nuget_config_dir.path().join("nuget.config");
            let nuget_config_contents = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
            <configuration>
              <packageSources>
                <clear />
                <add key="Local SpacetimeDB.BSATN.Runtime" value="{}" />
                <add key="Local SpacetimeDB.Runtime" value="{}" />
                <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
              </packageSources>
              <packageSourceMapping>
                <packageSource key="Local SpacetimeDB.BSATN.Runtime">
                  <package pattern="SpacetimeDB.BSATN.Runtime" />
                </packageSource>
                <packageSource key="Local SpacetimeDB.Runtime">
                  <package pattern="SpacetimeDB.Runtime" />
                </packageSource>
                <packageSource key="nuget.org">
                  <package pattern="*" />
                </packageSource>
              </packageSourceMapping>
            </configuration>
            "#,
                bsatn_source.display(),
                runtime_source.display(),
            );
            fs::write(&nuget_config_path, nuget_config_contents)?;

            let nuget_config_path_str = nuget_config_path.to_string_lossy().to_string();

            clear_restored_package_dirs("spacetimedb.bsatn.runtime")?;
            clear_restored_package_dirs("spacetimedb.runtime")?;

            cmd!(
                "dotnet",
                "restore",
                "SpacetimeDB.ClientSDK.csproj",
                "--configfile",
                &nuget_config_path_str,
            )
            .dir("sdks/csharp")
            .run()?;

            overlay_unity_meta_skeleton("spacetimedb.bsatn.runtime")?;
            overlay_unity_meta_skeleton("spacetimedb.runtime")?;

            cmd!(
                "dotnet",
                "pack",
                "SpacetimeDB.ClientSDK.csproj",
                "-c",
                "Release",
                "--no-restore"
            )
            .dir("sdks/csharp")
            .run()?;
        }

        Some(CiCmd::Smoketests(args)) => {
            smoketest::run(args)?;
        }

        Some(CiCmd::UpdateFlow {
            target,
            github_token_auth,
        }) => {
            let mut common_args = vec![];
            if let Some(target) = target.as_ref() {
                common_args.push("--target");
                common_args.push(target);
                log::info!("checking update flow for target: {target}");
            } else {
                log::info!("checking update flow");
            }
            if github_token_auth {
                common_args.push("--features");
                common_args.push("github-token-auth");
            }

            cmd(
                "cargo",
                ["build", "-p", "spacetimedb-update"]
                    .into_iter()
                    .chain(common_args.clone()),
            )
            .run()?;
            // NOTE(bfops): We need the `github-token-auth` feature because we otherwise tend to get ratelimited when we try to fetch `/releases/latest`.
            // My best guess is that, on the GitHub runners, the "anonymous" ratelimit is shared by *all* users of that runner (I think this because it
            // happens very frequently on the `macos-runner`, but we haven't seen it on any others).
            let root_dir = tempfile::tempdir()?;
            let root_dir_string = root_dir.path().to_string_lossy().to_string();
            let root_arg = format!("--root-dir={}", root_dir_string);
            cmd(
                "cargo",
                ["run", "-p", "spacetimedb-update"]
                    .into_iter()
                    .chain(common_args.clone())
                    .chain(["--", "self-install", &root_arg, "--yes"].into_iter()),
            )
            .run()?;
            cmd!(format!("{}/spacetime", root_dir_string), &root_arg, "help",).run()?;
        }

        Some(CiCmd::CliDocs { spacetime_path }) => {
            if let Some(path) = spacetime_path {
                env::set_current_dir(path).ok();
            }
            let current_dir = env::current_dir().expect("No current directory!");
            let dir_name = current_dir.file_name().expect("No current directory!");
            if dir_name != "SpacetimeDB" && dir_name != "public" {
                anyhow::bail!(
                    "You must execute this binary from inside of the SpacetimeDB directory, or use --spacetime-path"
                );
            }

            cmd!("pnpm", "install", "--recursive").run()?;
            cmd!("pnpm", "generate-cli-docs").dir("docs").run()?;
            let out = cmd!("git", "status", "--porcelain", "--", "docs").read()?;
            if out.is_empty() {
                log::info!("No docs changes detected");
            } else {
                anyhow::bail!("CLI docs are out of date:\n{out}");
            }
        }

        Some(CiCmd::SelfDocs { check }) => {
            let readme_content = ci_docs::generate_cli_docs();
            let path = Path::new(README_PATH);

            if check {
                let existing = fs::read_to_string(path).unwrap_or_default();
                if existing != readme_content {
                    bail!("README.md is out of date. Please run `cargo ci self-docs` to update it.");
                } else {
                    log::info!("README.md is up to date.");
                }
            } else {
                fs::write(path, readme_content)?;
                log::info!("Wrote CLI docs to {}", path.display());
            }
        }

        Some(CiCmd::GlobalJsonPolicy) => {
            check_global_json_policy()?;
        }

        Some(CiCmd::Regen { check }) => {
            ensure_repo_root()?;

            // On Windows with core.symlinks=false, global.json "symlinks" are text
            // files containing relative paths. Patch them with real content so dotnet
            // commands work, then restore after regen.
            let patched_globals = fix_global_json_symlinks()?;

            let regen_result = (|| -> Result<()> {
                check_template_generate_coverage()?;
                regen_sdk_test_bindings()?;
                regen_demo_bindings()?;
                regen_template_bindings()?;
                regen_sdk_internal_bindings()?;
                regen_moduledef_bindings()?;

                log::info!("Running cargo fmt");
                cmd!("cargo", "fmt", "--all").run()?;

                revert_version_only_changes()?;
                Ok(())
            })();

            // Always restore global.json symlinks, even if regen failed.
            restore_global_json_symlinks(&patched_globals)?;

            // Propagate any regen error after restoring.
            regen_result?;

            if check {
                check_autogen_coverage()?;

                log::info!("Checking for stale bindings (expects clean git state)");
                cmd!("bash", "tools/check-diff.sh").run().map_err(|_| {
                    anyhow::anyhow!(
                        "Bindings are stale. Run `cargo ci regen` to update them.\n\
                         Note: --check expects a clean git state. Commit or stash unrelated \
                         changes first."
                    )
                })?;
                log::info!("All bindings are up-to-date.");
            }
        }

        None => run_all_clap_subcommands(&cli.skip)?,
    }

    Ok(())
}
