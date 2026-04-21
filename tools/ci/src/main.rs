#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use regex::Regex;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

const README_PATH: &str = "tools/ci/README.md";

mod ci_docs;
mod smoketest;
mod util;

use util::ensure_repo_root;

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

fn check_global_json_policy() -> Result<()> {
    ensure_repo_root()?;

    let root_json = Path::new("global.json");
    let root_contents = fs::read_to_string(root_json)?;

    fn find_all_global_json(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                out.extend(find_all_global_json(&path)?);
            } else if path.file_name() == Some(OsStr::new("global.json")) {
                out.push(path);
            }
        }
        Ok(out)
    }

    let globals = find_all_global_json(Path::new("."))?;

    let mut ok = true;
    for p in globals {
        let meta = fs::symlink_metadata(&p)?;
        let is_symlink = meta.file_type().is_symlink();
        let is_template_global_json = p.strip_prefix(".").unwrap_or(&p).starts_with(Path::new("templates"));
        if is_template_global_json && is_symlink {
            eprintln!(
                "Error: {} is a symlink. Template files must not be symlinks; they are copied literally and this will break if the CLI is built under Windows where symlinks are not supported.",
                p.display()
            );
            ok = false;
        }

        let contents = fs::read_to_string(&p)?;
        if contents != root_contents {
            eprintln!("Error: {} does not match the root global.json contents", p.display());
            ok = false;
        } else if !is_template_global_json || !is_symlink {
            println!("OK: {}", p.display());
        }
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
    if pkg_root_meta.exists()
        && let Some(parent) = pkg_root.parent()
    {
        let pkg_meta_dst = parent.join(format!("{pkg_id}.meta"));
        fs::copy(&pkg_root_meta, &pkg_meta_dst)?;
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
    if version_meta_template.exists()
        && let Some(parent) = versioned_dir.parent()
    {
        let version_name = versioned_dir
            .file_name()
            .expect("versioned directory should have a file name");
        let version_meta_dst = parent.join(format!("{}.meta", version_name.to_string_lossy()));
        fs::copy(&version_meta_template, &version_meta_dst)?;
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
    /// Checks that publishable crates satisfy publish constraints.
    PublishChecks,
    /// Runs TypeScript workspace tests and template build checks.
    TypescriptTest,
    /// Builds the docs site.
    Docs,
    /// Runs the C# SDK test suite and binding checks.
    CsharpTests,
    /// Prepares the Unity test workspace and publishes the local test module.
    UnityTests,
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

fn tracked_rs_files_under(path: &str) -> Result<Vec<PathBuf>> {
    let output = cmd!("git", "ls-files", "--", path).read()?;
    Ok(output
        .lines()
        .filter(|line| line.ends_with(".rs"))
        .map(PathBuf::from)
        .collect())
}

fn run_check_diff(path: &str, message: &str) -> Result<()> {
    let status = Command::new("bash").args(["tools/check-diff.sh", path]).status()?;
    if !status.success() {
        bail!("{message}");
    }
    Ok(())
}

fn prepare_csharp_sdk_solution() -> Result<()> {
    cmd!(
        "dotnet",
        "pack",
        "crates/bindings-csharp/BSATN.Runtime",
        "-c",
        "Release"
    )
    .run()?;
    cmd!("dotnet", "pack", "crates/bindings-csharp/Runtime", "-c", "Release").run()?;
    cmd!("bash", "./tools~/write-nuget-config.sh", "../..")
        .dir("sdks/csharp")
        .run()?;
    cmd!(
        "dotnet",
        "restore",
        "--configfile",
        "NuGet.Config",
        "SpacetimeDB.ClientSDK.sln"
    )
    .dir("sdks/csharp")
    .run()?;
    Ok(())
}

fn run_publish_checks() -> Result<()> {
    cmd!("bash", "-lc", "test -d venv || python3 -m venv venv").run()?;
    cmd!("venv/bin/pip3", "install", "argparse", "toml").run()?;

    let crates = cmd!(
        "venv/bin/python3",
        "tools/find-publish-list.py",
        "--recursive",
        "--directories",
        "--quiet",
        "spacetimedb",
        "spacetimedb-sdk"
    )
    .read()?;

    let mut failed = Vec::new();
    for crate_dir in crates.split_whitespace() {
        if let Err(err) = cmd!("venv/bin/python3", "tools/crate-publish-checks.py", crate_dir).run() {
            eprintln!("crate publish checks failed for {crate_dir}: {err}");
            failed.push(crate_dir.to_string());
        }
    }

    if !failed.is_empty() {
        bail!("crate publish checks failed for: {}", failed.join(", "));
    }

    Ok(())
}

fn run_typescript_tests() -> Result<()> {
    cmd!("pnpm", "build").dir("crates/bindings-typescript").run()?;
    cmd!("pnpm", "test").dir("crates/bindings-typescript").run()?;
    cmd!("pnpm", "generate").dir("templates/chat-react-ts").run()?;
    run_check_diff(
        "templates/chat-react-ts/src/module_bindings",
        "Bindings are dirty. Please generate bindings again and commit them to this branch.",
    )?;
    cmd!("pnpm", "build").dir("templates/chat-react-ts").run()?;
    cmd!("pnpm", "-r", "--filter", "./**", "run", "build")
        .dir("templates")
        .run()?;
    cmd!("pnpm", "-r", "--filter", "./**", "run", "build")
        .dir("crates/bindings-typescript")
        .run()?;
    Ok(())
}

fn run_docs_build() -> Result<()> {
    cmd!("pnpm", "install").dir("docs").run()?;
    cmd!("pnpm", "build").dir("docs").run()?;
    Ok(())
}

fn run_local_spacetime_script(script_name: &str, body: &str) -> Result<()> {
    let script = format!(
        r#"set -euo pipefail
spacetime start >"/tmp/{script_name}.log" 2>&1 &
STDB_PID=$!
trap 'kill "$STDB_PID" >/dev/null 2>&1 || true' EXIT
sleep 3
{body}
"#
    );
    cmd!("bash", "-lc", &script).run()?;
    Ok(())
}

fn run_csharp_tests() -> Result<()> {
    prepare_csharp_sdk_solution()?;

    cmd!("dotnet", "test", "-warnaserror", "--no-restore")
        .dir("sdks/csharp")
        .run()?;
    cmd!(
        "dotnet",
        "format",
        "--no-restore",
        "--verify-no-changes",
        "SpacetimeDB.ClientSDK.sln"
    )
    .dir("sdks/csharp")
    .run()?;

    cmd!("bash", "tools~/gen-quickstart.sh").dir("sdks/csharp").run()?;
    run_check_diff(
        "sdks/csharp/examples~/quickstart-chat",
        "quickstart-chat bindings have changed. Please run `sdks/csharp/tools~/gen-quickstart.sh`.",
    )?;

    run_local_spacetime_script(
        "spacetimedb-csharp-tests",
        r#"bash sdks/csharp/tools~/run-regression-tests.sh"#,
    )?;
    run_check_diff(
        "sdks/csharp/examples~/regression-tests",
        "Bindings are dirty. Please run `sdks/csharp/tools~/gen-regression-tests.sh`.",
    )?;

    Ok(())
}

fn patch_blackholio_server_dependency() -> Result<()> {
    let cargo_toml_path = Path::new("demo/Blackholio/server-rust/Cargo.toml");
    let existing = fs::read_to_string(cargo_toml_path)?;
    let dependency_line = Regex::new(r#"(?m)^spacetimedb\s*=.*$"#)?;
    let updated = dependency_line.replace(&existing, r#"spacetimedb = { path = "../../../crates/bindings" }"#);

    ensure!(
        updated.as_ref() != existing,
        "Failed to patch demo/Blackholio/server-rust/Cargo.toml with local spacetimedb dependency"
    );

    fs::write(cargo_toml_path, updated.as_ref())?;
    Ok(())
}

fn run_unity_tests() -> Result<()> {
    prepare_csharp_sdk_solution()?;
    patch_blackholio_server_dependency()?;

    cmd!("bash", "./generate.sh", "-y")
        .dir("demo/Blackholio/server-rust")
        .run()?;
    run_check_diff(
        "demo/Blackholio/client-unity/Assets/Scripts/autogen",
        "Bindings are dirty. Please run `demo/Blackholio/server-rust/generate.sh`.",
    )?;

    run_dlls()?;

    run_local_spacetime_script(
        "spacetimedb-unity-tests",
        r#"spacetime logout && spacetime login --server-issued-login local
cd demo/Blackholio/server-rust
bash ./publish.sh"#,
    )?;

    cmd!(
        "bash",
        "-lc",
        r#"cd demo/Blackholio/client-unity/Packages
yq e -i '.dependencies["com.clockworklabs.spacetimedbsdk"] = "file:../../../../sdks/csharp"' manifest.json
cat manifest.json"#
    )
    .run()?;

    Ok(())
}

fn run_dlls() -> Result<()> {
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

    Ok(())
}

fn run_update_flow(target: Option<String>, github_token_auth: bool) -> Result<()> {
    let mut common_args = vec![];
    if let Some(target) = target.as_ref() {
        common_args.push("--target");
        common_args.push(target.as_str());
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
            .chain(common_args.iter().copied()),
    )
    .run()?;

    let root_dir = tempfile::tempdir()?;
    let root_arg = format!("--root-dir={}", root_dir.path().display());
    cmd(
        "cargo",
        ["run", "-p", "spacetimedb-update"]
            .into_iter()
            .chain(common_args.iter().copied())
            .chain(["--", "self-install", &root_arg, "--yes"].into_iter()),
    )
    .run()?;

    let mut spacetime_path = root_dir.path().join("spacetime");
    if !std::env::consts::EXE_EXTENSION.is_empty() {
        spacetime_path.set_extension(std::env::consts::EXE_EXTENSION);
    }
    cmd(spacetime_path, [&root_arg, "help"]).run()?;

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
            // Run the same SDK suite against wasm/browser test clients.
            cmd!(
                "cargo",
                "test",
                "-p",
                "spacetimedb-sdk",
                "--features",
                "allow_loopback_http_for_tests,browser",
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
            ensure_repo_root()?;
            // `cargo fmt --all` only checks files that Cargo discovers through workspace/package targets.
            // However, we also keep Rust sources in a locations that are tracked but not part of our workspace,
            // so this approach properly catches all the files, where `cargo fmt` does not.
            let mut files = Vec::new();
            files.extend(tracked_rs_files_under(".")?);
            const RUSTFMT_BATCH_SIZE: usize = 200;
            for batch in files.chunks(RUSTFMT_BATCH_SIZE) {
                let mut args = Vec::<OsString>::with_capacity(batch.len() + 1);
                args.push("--check".into());
                args.extend(batch.iter().map(|path| path.as_os_str().to_os_string()));
                cmd("rustfmt", args).run()?;
            }
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
            cmd!(
                "cargo",
                "clippy",
                "--no-default-features",
                "--features=browser",
                "-pspacetimedb-sdk",
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
            // Pre-build the CLI so that it _doesn't_ get `cargo update`d, since that may break the build.
            cmd!("cargo", "build", "-p", "spacetimedb-cli").run()?;
            // Make sure the `Cargo.lock` file reflects the latest available versions.
            // This is what users would end up with on a fresh module, so we want to
            // catch any compile errors arising from a different transitive closure
            // of dependencies than what is in the workspace lock file.
            //
            // For context see also: https://github.com/clockworklabs/SpacetimeDB/pull/2714
            cmd!("cargo", "update").run()?;
            let cli_path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .unwrap()
                .join("target/debug/spacetimedb-cli")
                .with_extension(std::env::consts::EXE_EXTENSION);
            cmd!(cli_path, "build", "--module-path", "modules/module-test",).run()?;
        }

        Some(CiCmd::Dlls) => {
            run_dlls()?;
        }

        Some(CiCmd::Smoketests(args)) => {
            smoketest::run(args)?;
        }

        Some(CiCmd::UpdateFlow {
            target,
            github_token_auth,
        }) => {
            run_update_flow(target, github_token_auth)?;
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

        Some(CiCmd::PublishChecks) => {
            run_publish_checks()?;
        }

        Some(CiCmd::TypescriptTest) => {
            run_typescript_tests()?;
        }

        Some(CiCmd::Docs) => {
            run_docs_build()?;
        }

        Some(CiCmd::CsharpTests) => {
            run_csharp_tests()?;
        }

        Some(CiCmd::UnityTests) => {
            run_unity_tests()?;
        }

        None => run_all_clap_subcommands(&cli.skip)?,
    }

    Ok(())
}
