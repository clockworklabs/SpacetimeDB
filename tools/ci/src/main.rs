#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use std::ffi::OsStr;
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

fn check_global_json_policy() -> Result<()> {
    ensure_repo_root()?;

    let root_json = Path::new("global.json");
    let root_real = fs::canonicalize(root_json)?;
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
        let resolved = fs::canonicalize(&p)?;

        // The root global.json itself is allowed.
        if resolved == root_real {
            println!("OK: {}", p.display());
            continue;
        }

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

        None => run_all_clap_subcommands(&cli.skip)?,
    }

    Ok(())
}
