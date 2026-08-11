#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::{cmd, Expression};
use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs};

mod cla_assistant;
mod codeowners_check;
mod internal_tests;
mod keynote_bench;
mod smoketest;
mod util;
mod workflow_watch;

use util::ensure_repo_root;

/// On Windows, `pnpm` is installed as a `.cmd` shim which `CreateProcess` cannot
/// find without going through the shell.  Wrapping with `cmd /c` fixes this.
/// On Unix, we invoke `pnpm` directly.
fn pnpm<I, S>(args: I) -> Expression
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let args: Vec<std::ffi::OsString> = args.into_iter().map(|a| a.as_ref().to_os_string()).collect();
    if cfg!(windows) {
        let mut full: Vec<std::ffi::OsString> = vec!["/c".into(), "pnpm".into()];
        full.extend(args);
        cmd("cmd", full)
    } else {
        cmd("pnpm", args)
    }
}

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
    #[arg(long, default_value = "other-workflows")]
    skip: Vec<String>,
}

fn check_global_json_policy() -> Result<()> {
    ensure_repo_root()?;

    let root_json = Path::new("global.json");
    let root_contents = fs::read_to_string(root_json)?;

    let globals = git_tracked_files(":(glob)**/global.json")?;

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

fn package_json_pnpm_version(package_manager: &str) -> Option<&str> {
    package_manager.strip_prefix("pnpm@")
}

fn git_tracked_files(pathspec: &str) -> Result<Vec<PathBuf>> {
    let output = cmd!("git", "ls-files", pathspec).read()?;
    Ok(output.lines().map(PathBuf::from).collect())
}

fn package_json_string_value(package_json: &Value, key: &str) -> Option<String> {
    package_json.get(key)?.as_str().map(str::to_owned)
}

fn package_json_engines_pnpm(package_json: &Value) -> Option<String> {
    package_json.get("engines")?.get("pnpm")?.as_str().map(str::to_owned)
}

fn read_package_json(path: &Path) -> Result<Value> {
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

fn is_npm_package_json(package_json: &Value) -> bool {
    [
        "bin",
        "dependencies",
        "devDependencies",
        "exports",
        "main",
        "optionalDependencies",
        "packageManager",
        "peerDependencies",
        "scripts",
        "type",
    ]
    .iter()
    .any(|key| package_json.get(key).is_some())
}

fn is_template_path(path: &Path) -> bool {
    path.starts_with("templates")
}

fn minimum_release_age(path: &Path) -> Result<u64> {
    let workspace = fs::read_to_string(path)?;
    workspace
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let value = line.strip_prefix("minimumReleaseAge:")?.trim();
            value.parse::<u64>().ok()
        })
        .ok_or_else(|| anyhow::anyhow!("{} is missing minimumReleaseAge", path.display()))
}

fn npmrc_minimum_release_age(path: &Path, expected_minimum_release_age: u64) -> Result<u64> {
    let contents = fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(
                "{} is tracked but missing from the working tree. Restore it with:\nminimum-release-age={}",
                path.display(),
                expected_minimum_release_age
            )
        } else {
            anyhow::anyhow!(
                "failed to read {} while checking pnpm minimum package age: {err}",
                path.display()
            )
        }
    })?;
    contents
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let value = line.strip_prefix("minimum-release-age=")?.trim();
            value.parse::<u64>().ok()
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} must contain `minimum-release-age={}` to match root pnpm-workspace.yaml",
                path.display(),
                expected_minimum_release_age
            )
        })
}

fn check_pnpm_release_age_policy() -> Result<()> {
    ensure_repo_root()?;

    let root_package_json_path = Path::new("package.json");
    let root_package_json = read_package_json(root_package_json_path)?;
    let package_manager = package_json_string_value(&root_package_json, "packageManager")
        .ok_or_else(|| anyhow::anyhow!("package.json is missing packageManager"))?;
    let package_manager_version = package_json_pnpm_version(&package_manager)
        .ok_or_else(|| anyhow::anyhow!("packageManager must be pnpm@<version>, found {package_manager:?}"))?;

    let expected_engine_pnpm = format!(">={package_manager_version}");
    let engine_pnpm = package_json_engines_pnpm(&root_package_json)
        .ok_or_else(|| anyhow::anyhow!("package.json engines is missing pnpm"))?;
    if engine_pnpm != expected_engine_pnpm {
        bail!("package.json engines.pnpm must be {expected_engine_pnpm:?}, found {engine_pnpm:?}");
    }

    for package_json_path in git_tracked_files(":(glob)**/package.json")? {
        let package_json = read_package_json(&package_json_path)?;
        let Some(found_package_manager) = package_json_string_value(&package_json, "packageManager") else {
            continue;
        };
        if found_package_manager != package_manager {
            bail!(
                "{} packageManager must match root package.json: expected {:?}, found {:?}",
                package_json_path.display(),
                package_manager,
                found_package_manager
            );
        }
    }

    let root_workspace_path = Path::new("pnpm-workspace.yaml");
    let root_minimum_release_age = minimum_release_age(root_workspace_path)?;
    for workspace_path in git_tracked_files(":(glob)**/pnpm-workspace.yaml")? {
        let found_minimum_release_age = minimum_release_age(&workspace_path)?;
        if found_minimum_release_age != root_minimum_release_age {
            bail!(
                "{} minimumReleaseAge must match root pnpm-workspace.yaml: expected {}, found {}",
                workspace_path.display(),
                root_minimum_release_age,
                found_minimum_release_age
            );
        }
    }

    for npmrc_path in git_tracked_files(":(glob)**/.npmrc")? {
        // Template package roots are copied into projects created by `spacetime init`.
        // They must not embed this repo's package-age policy; smoketests enforce it
        // at the pnpm process boundary instead.
        if is_template_path(&npmrc_path) {
            continue;
        }
        let found_minimum_release_age = npmrc_minimum_release_age(&npmrc_path, root_minimum_release_age)?;
        if found_minimum_release_age != root_minimum_release_age {
            bail!(
                "{} minimum-release-age must match root pnpm-workspace.yaml: expected {}, found {}",
                npmrc_path.display(),
                root_minimum_release_age,
                found_minimum_release_age
            );
        }
    }

    for package_json_path in git_tracked_files(":(glob)**/package.json")? {
        // Template package roots are copied into projects created by `spacetime init`.
        // They must not require adjacent .npmrc files for this repo's package-age
        // policy; smoketests enforce it at the pnpm process boundary instead.
        if is_template_path(&package_json_path) {
            continue;
        }
        let package_json = read_package_json(&package_json_path)?;
        if !is_npm_package_json(&package_json) {
            continue;
        }
        let package_dir = package_json_path
            .parent()
            .expect("git-tracked package.json path should have a parent");
        let npmrc_path = package_dir.join(".npmrc");
        if !npmrc_path.is_file() {
            bail!(
                "{} is required because {} is an npm/pnpm package manifest.\nAdd {} containing:\nminimum-release-age={}",
                npmrc_path.display(),
                package_json_path.display(),
                npmrc_path.display(),
                root_minimum_release_age
            );
        }
        let found_minimum_release_age = npmrc_minimum_release_age(&npmrc_path, root_minimum_release_age)?;
        if found_minimum_release_age != root_minimum_release_age {
            bail!(
                "{} minimum-release-age must match root pnpm-workspace.yaml: expected {}, found {}",
                npmrc_path.display(),
                root_minimum_release_age,
                found_minimum_release_age
            );
        }
    }

    for workflow_path in git_tracked_files(".github/workflows/*")? {
        let contents = fs::read_to_string(&workflow_path)?;
        if contents.contains("pnpm/action-setup@v4") {
            bail!(
                "{} must use ./.github/actions/setup-pnpm instead of pnpm/action-setup@v4",
                workflow_path.display()
            );
        }
    }

    Ok(())
}

/// Codex plugin ships a copy of `skills/`, because plugin installers do not follow symlinks,
/// this checks if the copy is in sync
fn check_codex_plugin_skills_sync() -> Result<()> {
    let source = Path::new("skills");
    let copy = Path::new("codex-plugin/plugins/spacetimedb/skills");

    if fs::symlink_metadata(copy)
        .with_context(|| format!("reading {}", copy.display()))?
        .file_type()
        .is_symlink()
    {
        bail!(
            "{} must be a real directory, not a symlink, plugin installers do not follow symlinks",
            copy.display()
        );
    }

    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                walk(root, &entry.path(), out)?;
            } else {
                let rel = entry
                    .path()
                    .strip_prefix(root)
                    .expect("walked path should be under its root")
                    .to_path_buf();
                out.insert(rel);
            }
        }
        Ok(())
    }

    let mut source_files = BTreeSet::new();
    walk(source, source, &mut source_files)?;
    let mut copy_files = BTreeSet::new();
    walk(copy, copy, &mut copy_files)?;

    let mut drift = Vec::new();
    for rel in &source_files {
        if !copy_files.contains(rel) {
            drift.push(format!("missing from copy: {}", rel.display()));
        } else if fs::read(source.join(rel))? != fs::read(copy.join(rel))? {
            drift.push(format!("differs: {}", rel.display()));
        }
    }
    for rel in &copy_files {
        if !source_files.contains(rel) {
            drift.push(format!("extraneous in copy: {}", rel.display()));
        }
    }

    if !drift.is_empty() {
        bail!(
            "codex-plugin skills copy is out of sync with skills/:\n  {}\nRun: node codex-plugin/scripts/check-skills-sync.ts --fix",
            drift.join("\n  ")
        );
    }
    Ok(())
}

fn check_claude_marketplace_skills() -> Result<()> {
    // Claude catalog lists each skill explicitly, so a new skill under `skills/` is
    // invisible to Claude until it is added there
    let catalog_path = Path::new(".claude-plugin/marketplace.json");
    let contents = fs::read_to_string(catalog_path).with_context(|| format!("reading {}", catalog_path.display()))?;
    let catalog: Value =
        serde_json::from_str(&contents).with_context(|| format!("parsing {}", catalog_path.display()))?;
    let listed: BTreeSet<String> = catalog["plugins"][0]["skills"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{} plugins[0].skills must be an array", catalog_path.display()))?
        .iter()
        .filter_map(|value| value.as_str())
        .map(|entry| entry.trim_start_matches("./").to_owned())
        .collect();

    let mut present = BTreeSet::new();
    for entry in fs::read_dir("skills").with_context(|| "reading skills")? {
        let entry = entry?;
        if entry.path().join("SKILL.md").is_file() {
            present.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }

    if listed != present {
        let missing: Vec<_> = present.difference(&listed).cloned().collect();
        let extraneous: Vec<_> = listed.difference(&present).cloned().collect();
        bail!(
            "{} skills list does not match skills/:\n  missing: {:?}\n  extraneous: {:?}\nUpdate the skills list in {}",
            catalog_path.display(),
            missing,
            extraneous,
            catalog_path.display()
        );
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
    /// Runs the keynote benchmark as a CI performance regression gate.
    ///
    /// Assumes release SpacetimeDB binaries and the TypeScript SDK are already built, runs the
    /// keynote SpacetimeDB benchmark for 60 seconds against the TypeScript and Rust modules, and
    /// fails if throughput is below 275K TPS for TypeScript or 300K TPS for Rust.
    KeynoteBench,
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
    /// Workflows should leave here if they should not be run as part of a no-subcommand invocation of `cargo ci`.
    OtherWorkflows {
        #[command(subcommand)]
        cmd: OtherWorkflowsCmd,
    },
}

#[derive(Subcommand)]
enum OtherWorkflowsCmd {
    /// Selects or starts the private workflow for a public Internal Tests run.
    CoordinateInternalTests(internal_tests::CoordinateArgs),
    /// Checks that sensitive CODEOWNERS-controlled files have the required approvals.
    CodeownersCheck {
        /// Git ref to compare against, usually origin/<pull request base branch>.
        #[arg(long)]
        base_ref: String,
        /// Pull request number to inspect for approval state.
        #[arg(long)]
        pr_number: u64,
    },
    /// Interacts with CLA Assistant.
    ClaAssistant {
        #[command(subcommand)]
        cmd: cla_assistant::ClaAssistantCmd,
    },
    /// Waits for a GitHub Actions workflow run to complete.
    Watch {
        /// Repository containing the workflow run, in owner/repo form.
        #[arg(long)]
        repo: String,
        /// GitHub Actions workflow run ID.
        #[arg(long)]
        run_id: u64,
        /// Seconds to sleep between polls.
        #[arg(long, default_value_t = 30)]
        interval_seconds: u64,
        /// Maximum number of polls before timing out. Polls forever by default.
        #[arg(long)]
        max_attempts: Option<u64>,
    },
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
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
    pnpm(["test"]).dir("crates/bindings-typescript").run()?;
    pnpm(["generate"]).dir("templates/chat-react-ts").run()?;
    let diff_status = cmd!(
        "bash",
        "tools/check-diff.sh",
        "templates/chat-react-ts/src/module_bindings"
    )
    .run()?;
    if !diff_status.status.success() {
        bail!("Bindings are dirty. Please generate bindings again and commit them to this branch.");
    }
    pnpm(["build"]).dir("templates/chat-react-ts").run()?;
    pnpm(["-r", "--filter", "./**", "run", "build"])
        .dir("templates")
        .run()?;
    pnpm(["-r", "--filter", "./**", "run", "build"])
        .dir("crates/bindings-typescript")
        .run()?;
    Ok(())
}

fn run_docs_build() -> Result<()> {
    pnpm(["install"]).dir("docs").run()?;
    pnpm(["build"]).dir("docs").run()?;
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
            pnpm(["build"]).dir("crates/bindings-typescript").run()?;

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
                "--exclude",
                "spacetimedb",
                "--",
                "--test-threads=2",
                "--skip",
                "unreal"
            )
            .run()?;
            // Bindings snapshot tests rely on the unstable feature,
            // as they compile and test APIs which are gated behind that feature,
            // e.g. procedures, HTTP handlers.
            cmd!(
                "cargo",
                "test",
                "-p",
                "spacetimedb",
                "--features",
                "unstable",
                "--",
                "--test-threads=2",
            )
            .run()?;
            // The SDK test harness uses the same child-process server guard as smoketests,
            // which expects release CLI/standalone binaries to already exist.
            cmd!(
                "cargo",
                "build",
                "--release",
                "-p",
                "spacetimedb-cli",
                "-p",
                "spacetimedb-standalone",
                "--features",
                "spacetimedb-standalone/allow_loopback_http_for_tests",
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
            check_codex_plugin_skills_sync()?;
            check_claude_marketplace_skills()?;
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
                "--timings",
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
                "--timings",
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
            pnpm(["lint"]).run()?;
            cmd!("cargo", "test", "--doc", "--target", "wasm32-unknown-unknown")
                .dir("crates/bindings")
                .run()?;
            cmd!("cargo", "test", "--doc").dir("crates/bindings").run()?;
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
            pnpm([
                "install",
                "--filter",
                "./crates/bindings-typescript...",
                "--filter",
                "./modules/module-test-ts...",
            ])
            .run()?;
            pnpm(["build"]).dir("crates/bindings-typescript").run()?;
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

        Some(CiCmd::KeynoteBench) => {
            ensure_repo_root()?;
            keynote_bench::run()?;
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

            pnpm(["install", "--recursive"]).run()?;
            pnpm(["generate-cli-docs"]).dir("docs").run()?;
            let out = cmd!("git", "status", "--porcelain", "--", "docs").read()?;
            if out.is_empty() {
                log::info!("No docs changes detected");
            } else {
                anyhow::bail!("CLI docs are out of date:\n{out}");
            }
        }

        Some(CiCmd::GlobalJsonPolicy) => {
            check_global_json_policy()?;
        }

        Some(CiCmd::OtherWorkflows {
            cmd: OtherWorkflowsCmd::CoordinateInternalTests(args),
        }) => {
            internal_tests::coordinate(args)?;
        }
        Some(CiCmd::OtherWorkflows {
            cmd: OtherWorkflowsCmd::CodeownersCheck { base_ref, pr_number },
        }) => {
            codeowners_check::run(&base_ref, pr_number)?;
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

        Some(CiCmd::OtherWorkflows {
            cmd: OtherWorkflowsCmd::ClaAssistant { cmd },
        }) => {
            cla_assistant::run(cmd)?;
        }

        Some(CiCmd::OtherWorkflows {
            cmd:
                OtherWorkflowsCmd::Watch {
                    repo,
                    run_id,
                    interval_seconds,
                    max_attempts,
                },
        }) => {
            workflow_watch::watch_workflow_run(&repo, run_id, interval_seconds, max_attempts)?;
        }

        None => run_all_clap_subcommands(&cli.skip)?,
    }

    Ok(())
}
