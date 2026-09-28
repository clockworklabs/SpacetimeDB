#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use duct::{cmd, Expression};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::tempdir;

#[derive(Parser)]
#[command(about = "Warms the Rust caches used by the Linux CI runner image")]
struct Args {}

struct WarmRunner {
    pass: &'static str,
    failures: Vec<String>,
}

impl WarmRunner {
    fn new() -> Self {
        Self {
            pass: "setup",
            failures: Vec::new(),
        }
    }

    fn set_pass(&mut self, pass: &'static str) {
        self.pass = pass;
    }

    fn required<F>(&mut self, description: &str, action: F)
    where
        F: FnOnce() -> Result<()>,
    {
        eprintln!("[{}] {description}", self.pass);
        if let Err(error) = action() {
            let failure = format!("{}: {description}: {error:#}", self.pass);
            eprintln!("FAILED: {failure}");
            self.failures.push(failure);
        }
    }

    fn finish(self) -> Result<()> {
        if self.failures.is_empty() {
            return Ok(());
        }

        bail!(
            "cache warming failed in {} required step(s):\n  - {}",
            self.failures.len(),
            self.failures.join("\n  - ")
        )
    }
}

fn run(expression: Expression) -> Result<()> {
    expression.run()?;
    Ok(())
}

fn cargo<I, S>(args: I) -> Expression
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    cmd("cargo", args)
}

fn expected_target_dir() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("actions-runner/_work/target"))
}

fn checked_target_dir() -> Result<PathBuf> {
    ensure!(cfg!(target_os = "linux"), "cache warming is supported only on Linux");
    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .context("CARGO_TARGET_DIR is not set")?;
    let expected = expected_target_dir()?;
    ensure!(
        target == expected,
        "refusing to reset unexpected Cargo target {}; expected {}",
        target.display(),
        expected.display()
    );
    Ok(target)
}

fn reset_cargo_target(target: &Path) -> Result<()> {
    if target.exists() {
        fs::remove_dir_all(target).with_context(|| format!("failed to remove {}", target.display()))?;
    }
    fs::create_dir_all(target).with_context(|| format!("failed to create {}", target.display()))?;
    Ok(())
}

fn warm_runtime_builds(runner: &mut WarmRunner) {
    runner.required("Build cargo-ci dispatcher", || run(cargo(["build", "-p", "ci"])));
    runner.required("Build debug CLI and standalone with loopback support", || {
        run(cargo([
            "build",
            "-p",
            "spacetimedb-cli",
            "-p",
            "spacetimedb-standalone",
            "--features",
            "spacetimedb-standalone/allow_loopback_http_for_tests",
        ]))
    });
    runner.required("Build debug CLI", || run(cargo(["build", "-p", "spacetimedb-cli"])));
    runner.required("Build release CLI", || {
        run(cargo(["build", "--release", "-p", "spacetimedb-cli"]))
    });
    runner.required("Build release standalone", || {
        run(cargo(["build", "--release", "-p", "spacetimedb-standalone"]))
    });
    runner.required("Build release CLI and standalone with loopback support", || {
        run(cargo([
            "build",
            "--release",
            "-p",
            "spacetimedb-cli",
            "-p",
            "spacetimedb-standalone",
            "--features",
            "spacetimedb-standalone/allow_loopback_http_for_tests",
        ]))
    });
    runner.required("Build Linux updater", || {
        run(cargo([
            "build",
            "-p",
            "spacetimedb-update",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--features",
            "github-token-auth",
        ]))
    });
}

fn warm_test_builds(runner: &mut WarmRunner) {
    runner.required("Compile primary test workspace", || {
        run(cargo([
            "test",
            "--no-run",
            "--all",
            "--exclude",
            "spacetimedb-smoketests",
            "--exclude",
            "spacetimedb-sdk",
            "--exclude",
            "spacetimedb",
        ]))
    });
    runner.required("Compile unstable bindings tests", || {
        run(cargo([
            "test",
            "--no-run",
            "-p",
            "spacetimedb",
            "--features",
            "unstable",
        ]))
    });
    runner.required("Compile loopback SDK tests", || {
        run(cargo([
            "test",
            "--no-run",
            "-p",
            "spacetimedb-sdk",
            "--features",
            "allow_loopback_http_for_tests",
        ]))
    });
    runner.required("Compile browser SDK tests", || {
        run(cargo([
            "test",
            "--no-run",
            "-p",
            "spacetimedb-sdk",
            "--features",
            "allow_loopback_http_for_tests,browser",
        ]))
    });
    runner.required("Compile fallocate durability tests", || {
        run(cargo([
            "test",
            "--no-run",
            "-p",
            "spacetimedb-durability",
            "--features",
            "fallocate",
        ]))
    });
    runner.required("Compile C# module-definition generator", || {
        run(cargo([
            "build",
            "-p",
            "spacetimedb-codegen",
            "--example",
            "regen-csharp-moduledef",
        ]))
    });
}

fn warm_lint_builds(runner: &mut WarmRunner) {
    runner.required("Compile primary Clippy workload", || {
        run(cargo([
            "clippy",
            "--timings",
            "--all",
            "--tests",
            "--benches",
            "--",
            "-D",
            "warnings",
        ]))
    });
    runner.required("Compile browser SDK Clippy workload", || {
        run(cargo([
            "clippy",
            "--timings",
            "--no-default-features",
            "--features=browser",
            "-p",
            "spacetimedb-sdk",
            "--tests",
            "--benches",
            "--",
            "-D",
            "warnings",
        ]))
    });
    runner.required("Compile wasm bindings documentation tests", || {
        run(cargo([
            "test",
            "--manifest-path",
            "crates/bindings/Cargo.toml",
            "--doc",
            "--target",
            "wasm32-unknown-unknown",
        ]))
    });
    runner.required("Compile native bindings documentation tests", || {
        run(cargo([
            "test",
            "--manifest-path",
            "crates/bindings/Cargo.toml",
            "--doc",
        ]))
    });
    runner.required("Build bindings documentation", || {
        run(cargo(["doc", "--manifest-path", "crates/bindings/Cargo.toml"]).env("RUSTDOCFLAGS", "--deny warnings"))
    });
}

fn warm_local_installs(runner: &mut WarmRunner) {
    runner.required("Install CLI from its local path", || {
        run(cargo([
            "install",
            "--force",
            "--path",
            "crates/cli",
            "--locked",
            "--message-format=short",
        ]))
    });
    runner.required("Install standalone from its local path", || {
        run(cargo([
            "install",
            "--force",
            "--path",
            "crates/standalone",
            "--locked",
            "--message-format=short",
        ]))
    });
    runner.required("Install loopback standalone from its local path", || {
        run(cargo([
            "install",
            "--force",
            "--path",
            "crates/standalone",
            "--features",
            "allow_loopback_http_for_tests",
            "--locked",
            "--message-format=short",
        ]))
    });
}

fn warm_ci_tools(runner: &mut WarmRunner) {
    const PACKAGES: &[&str] = &[
        "ci",
        "ci-test",
        "ci-lint",
        "ci-module-latest-deps",
        "ci-smoketests",
        "ci-smoketest-checks",
        "ci-keynote-bench",
        "ci-update-flow",
        "ci-cli-docs",
        "ci-global-json-policy",
        "ci-publish-checks",
        "ci-typescript-test",
        "ci-version-upgrade-check",
        "ci-docs-build",
        "ci-codeowners-check",
        "ci-cla-assistant",
        "ci-workflow-coordinator",
        "ci-workflow-watch",
        "ci-run-spacetime",
    ];

    for package in PACKAGES {
        runner.required(&format!("Build {package}"), || run(cargo(["build", "-p", package])));
    }
    runner.required("Build TypeScript binding generator", || {
        run(cargo(["build", "-p", "gen-bindings"]))
    });
    runner.required("Build client API generator", || {
        run(cargo(["build", "-p", "generate-client-api"]))
    });
    runner.required("Build SDK regeneration tool", || run(cargo(["build", "-p", "regen"])));
}

fn cli_binary() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    let path = PathBuf::from(home).join(".cargo/bin/spacetimedb-cli");
    ensure!(path.is_file(), "SpacetimeDB CLI does not exist at {}", path.display());
    Ok(path)
}

fn build_latest_deps_module() -> Result<()> {
    ensure!(
        cmd!("git", "diff", "--quiet", "HEAD", "--", "Cargo.lock")
            .unchecked()
            .run()?
            .status
            .success(),
        "Cargo.lock has pre-existing changes"
    );
    let module_lock = Path::new("modules/module-test/Cargo.lock");
    ensure!(!module_lock.exists(), "{} already exists", module_lock.display());

    let build_result = run(cargo(["run", "-p", "ci-module-latest-deps"]).env("SPACETIME_BIN", cli_binary()?));
    let restore_result = run(cmd!("git", "restore", "--worktree", "--", "Cargo.lock"));
    let remove_result = if module_lock.exists() {
        fs::remove_file(module_lock).context("failed to remove generated module-test Cargo.lock")
    } else {
        Ok(())
    };

    restore_result.context("failed to restore Cargo.lock")?;
    remove_result?;
    build_result
}

fn patched_blackholio_manifest(original: &str) -> Result<String> {
    let mut replaced = false;
    let mut patched = String::with_capacity(original.len());
    for line in original.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed
            .strip_prefix("spacetimedb")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
        {
            let indent = &line[..line.len() - trimmed.len()];
            patched.push_str(indent);
            patched.push_str("spacetimedb = { path = \"../../../crates/bindings\" }");
            if line.ends_with('\n') {
                patched.push('\n');
            }
            replaced = true;
        } else {
            patched.push_str(line);
        }
    }
    ensure!(replaced, "Blackholio manifest has no spacetimedb dependency to patch");
    Ok(patched)
}

fn build_blackholio_module() -> Result<()> {
    let manifest = Path::new("demo/Blackholio/server-rust/Cargo.toml");
    let original = fs::read_to_string(manifest).context("failed to read Blackholio manifest")?;
    let patched = patched_blackholio_manifest(&original)?;
    fs::write(manifest, patched).context("failed to patch Blackholio manifest")?;

    let build_result = run(cmd(
        cli_binary()?,
        ["build", "--module-path", "demo/Blackholio/server-rust"],
    ));
    let restore_result = fs::write(manifest, original).context("failed to restore Blackholio manifest");
    restore_result?;
    build_result
}

fn warm_independent_modules(runner: &mut WarmRunner) {
    runner.required(
        "Build module with latest compatible dependencies",
        build_latest_deps_module,
    );
    runner.required("Build patched Blackholio Rust module", build_blackholio_module);
}

type WarmFamily = fn(&mut WarmRunner);

const FAMILIES: &[(&str, WarmFamily)] = &[
    ("runtime builds", warm_runtime_builds),
    ("test builds", warm_test_builds),
    ("lint and docs builds", warm_lint_builds),
    ("local installs", warm_local_installs),
    ("CI tools", warm_ci_tools),
    ("independent modules", warm_independent_modules),
];

fn populate_sccache(runner: &mut WarmRunner, target: &Path) -> Result<()> {
    for &(name, family) in FAMILIES {
        let started = Instant::now();
        reset_cargo_target(target)?;
        runner.set_pass(match name {
            "runtime builds" => "sccache population: runtime builds",
            "test builds" => "sccache population: test builds",
            "lint and docs builds" => "sccache population: lint and docs builds",
            "local installs" => "sccache population: local installs",
            "CI tools" => "sccache population: CI tools",
            "independent modules" => "sccache population: independent modules",
            _ => unreachable!(),
        });
        family(runner);
        eprintln!("[{}] completed in {}s", runner.pass, started.elapsed().as_secs());
    }
    Ok(())
}

fn seed_target(runner: &mut WarmRunner, target: &Path) -> Result<()> {
    reset_cargo_target(target)?;
    for &(name, family) in FAMILIES {
        let started = Instant::now();
        runner.set_pass(match name {
            "runtime builds" => "target seed: runtime builds",
            "test builds" => "target seed: test builds",
            "lint and docs builds" => "target seed: lint and docs builds",
            "local installs" => "target seed: local installs",
            "CI tools" => "target seed: CI tools",
            "independent modules" => "target seed: independent modules",
            _ => unreachable!(),
        });
        family(runner);
        eprintln!("[{}] completed in {}s", runner.pass, started.elapsed().as_secs());
    }
    Ok(())
}

fn ensure_cargo_nextest() -> Result<()> {
    if cargo(["nextest", "--version"]).unchecked().run()?.status.success() {
        return Ok(());
    }
    run(cargo(["install", "--locked", "cargo-nextest"]).env_remove("CARGO_TARGET_DIR"))
}

fn warm_smoketest_archive() -> Result<()> {
    let temp = tempdir().context("failed to create smoketest archive directory")?;
    let archive = temp.path().join("smoketest-nextest.tar.zst");
    let args = vec![
        OsString::from("run"),
        OsString::from("-p"),
        OsString::from("ci-smoketests"),
        OsString::from("--"),
        OsString::from("--suite"),
        OsString::from("standalone"),
        OsString::from("archive"),
        OsString::from("--archive-file"),
        archive.into_os_string(),
    ];
    run(cargo(args))
}

fn main() -> Result<()> {
    Args::parse();
    ensure!(
        Path::new("Cargo.toml").is_file(),
        "run this command from the repository root"
    );
    let target = checked_target_dir()?;
    let mut runner = WarmRunner::new();

    runner.set_pass("dependency fetch");
    runner.required("Fetch root workspace dependencies", || {
        run(cargo(["fetch"]).env("CARGO_NET_GIT_FETCH_WITH_CLI", "true"))
    });
    runner.required("Fetch smoketest module dependencies", || {
        run(
            cargo(["fetch", "--manifest-path", "crates/smoketests/modules/Cargo.toml"])
                .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true"),
        )
    });

    populate_sccache(&mut runner, &target)?;
    seed_target(&mut runner, &target)?;

    runner.set_pass("target seed: smoketests");
    runner.required("Install cargo-nextest", ensure_cargo_nextest);
    runner.required("Build standalone smoketest archive", warm_smoketest_archive);
    runner.finish()
}

#[cfg(test)]
mod tests {
    use super::patched_blackholio_manifest;

    #[test]
    fn patches_blackholio_dependency() {
        let input = "[dependencies]\nspacetimedb = { version = \"1\" }\nlog = \"0.4\"\n";
        let output = patched_blackholio_manifest(input).unwrap();
        assert_eq!(
            output,
            "[dependencies]\nspacetimedb = { path = \"../../../crates/bindings\" }\nlog = \"0.4\"\n"
        );
    }
}
