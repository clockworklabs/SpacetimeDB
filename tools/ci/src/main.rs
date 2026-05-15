#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs};

const README_PATH: &str = "tools/ci/README.md";
const MINIMUM_PNPM_VERSION: (u64, u64, u64) = (10, 16, 0);
const MINIMUM_RELEASE_AGE_MINUTES: u64 = 2880;

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

fn parse_version(version: &str) -> Result<(u64, u64, u64)> {
    let mut parts = version
        .trim()
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty());
    let major = parts.next().unwrap_or_default().parse()?;
    let minor = parts.next().unwrap_or_default().parse()?;
    let patch = parts.next().unwrap_or_default().parse()?;
    Ok((major, minor, patch))
}

fn pnpm_version_is_supported(version: &str) -> Result<bool> {
    Ok(parse_version(version)? >= MINIMUM_PNPM_VERSION)
}

fn find_json_string_value(contents: &str, key: &str) -> Option<String> {
    let key = format!("\"{key}\"");
    let key_start = contents.find(&key)?;
    let after_key = &contents[key_start + key.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let end_quote = after_quote.find('"')?;
    Some(after_quote[..end_quote].to_string())
}

fn package_json_pnpm_version(package_manager: &str) -> Option<&str> {
    package_manager.strip_prefix("pnpm@")
}

fn check_pnpm_release_age_policy() -> Result<()> {
    ensure_repo_root()?;

    let package_json = fs::read_to_string("package.json")?;
    let package_manager = find_json_string_value(&package_json, "packageManager")
        .ok_or_else(|| anyhow::anyhow!("package.json is missing packageManager"))?;
    let package_manager_version = package_json_pnpm_version(&package_manager)
        .ok_or_else(|| anyhow::anyhow!("packageManager must be pnpm@<version>, found {package_manager:?}"))?;
    if !pnpm_version_is_supported(package_manager_version)? {
        bail!("packageManager must use pnpm >= 10.16.0 to support minimumReleaseAge");
    }

    let engine_pnpm = find_json_string_value(&package_json, "pnpm")
        .ok_or_else(|| anyhow::anyhow!("package.json engines is missing pnpm"))?;
    if !pnpm_version_is_supported(&engine_pnpm)? {
        bail!("engines.pnpm must require pnpm >= 10.16.0 to support minimumReleaseAge");
    }

    let workspace = fs::read_to_string("pnpm-workspace.yaml")?;
    let release_age = workspace
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let value = line.strip_prefix("minimumReleaseAge:")?.trim();
            value.parse::<u64>().ok()
        })
        .ok_or_else(|| anyhow::anyhow!("pnpm-workspace.yaml is missing minimumReleaseAge"))?;
    if release_age < MINIMUM_RELEASE_AGE_MINUTES {
        bail!("minimumReleaseAge must be at least {MINIMUM_RELEASE_AGE_MINUTES} minutes");
    }

    let pnpm_version = cmd!("pnpm", "--version").read()?;
    if !pnpm_version_is_supported(&pnpm_version)? {
        bail!("installed pnpm must be >= 10.16.0 to support minimumReleaseAge");
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
    /// Runs rustfmt, clippy, csharpier, TypeScript lint, and generates rust docs to ensure there
    /// are no warnings.
    Lint,
    /// Tests Wasm bindings
    ///
    /// Runs tests for the codegen crate and builds a test module with the wasm bindings.
    WasmBindings,
    /// Deprecated; use `cargo regen csharp dlls`.
    ///
    /// Builds and packs C# DLLs and NuGet packages for local Unity workflows.
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
    /// Verifies that the repository version upgrade tool still works.
    VersionUpgradeCheck,
    /// Builds the docs site.
    Docs,
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
    let diff_status = cmd!(
        "bash",
        "tools/check-diff.sh",
        "templates/chat-react-ts/src/module_bindings"
    )
    .run()?;
    if !diff_status.status.success() {
        bail!("Bindings are dirty. Please generate bindings again and commit them to this branch.");
    }
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
    cmd!("pnpm", "install", "--frozen-lockfile").dir("docs").run()?;
    cmd!("pnpm", "build").dir("docs").run()?;
    Ok(())
}

fn run_version_upgrade_check() -> Result<()> {
    cmd!(
        "cargo",
        "bump-versions",
        "123.456.789",
        "--rust-and-cli",
        "--csharp",
        "--typescript",
        "--cpp",
        "--accept-snapshots"
    )
    .run()?;
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.cmd {
        Some(CiCmd::Test) => {
            cmd!("pnpm", "build").dir("crates/bindings-typescript").run()?;

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
            check_pnpm_release_age_policy()?;
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
            cmd!("pnpm", "lint").run()?;
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
            eprintln!("warning: `cargo ci dlls` is deprecated; use `cargo regen csharp dlls` instead");
            cmd!("cargo", "regen", "csharp", "dlls").run()?;
        }

        Some(CiCmd::Smoketests(args)) => {
            ensure_repo_root()?;
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

            let mut spacetime_path = root_dir.path().join("spacetime");
            if !std::env::consts::EXE_EXTENSION.is_empty() {
                spacetime_path.set_extension(std::env::consts::EXE_EXTENSION);
            }
            cmd(spacetime_path, [&root_arg, "help"]).run()?;
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

            cmd!("pnpm", "install", "--recursive", "--frozen-lockfile").run()?;
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

        Some(CiCmd::VersionUpgradeCheck) => {
            run_version_upgrade_check()?;
        }

        Some(CiCmd::Docs) => {
            run_docs_build()?;
        }

        None => run_all_clap_subcommands(&cli.skip)?,
    }

    Ok(())
}
