use crate::bench::utils::sanitize_db_name;
use anyhow::{bail, Result};
use regex::Regex;
use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

/// Strip ANSI escape codes (color codes) from a string
fn strip_ansi_codes(s: &str) -> Cow<'_, str> {
    static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| {
        // Matches ANSI escape sequences like \x1b[31m, \x1b[0m, etc.
        Regex::new(r"\x1b\[[0-9;]*m").unwrap()
    });
    ANSI_RE.replace_all(s, "")
}

/* -------------------------------------------------------------------------- */
/* Shared                                                                     */
/* -------------------------------------------------------------------------- */

pub trait Publisher: Send + Sync {
    fn publish(&self, host_url: &str, source: &Path, module_name: &str) -> Result<()>;
}

/// Check if the process was killed by a signal (e.g., SIGSEGV = 11)
#[cfg(unix)]
fn was_signal_killed(status: &std::process::ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    status.signal().is_some()
}

#[cfg(not(unix))]
fn was_signal_killed(_status: &std::process::ExitStatus) -> bool {
    false
}

/// Check if the failure is a transient error that should be retried.
/// These are resource contention issues in the dotnet WASI SDK.
fn is_transient_build_error(stderr: &str, stdout: &str) -> bool {
    let combined = format!("{stderr}{stdout}");
    // "Pipe is broken" errors from WASI SDK parallel builds
    combined.contains("Pipe is broken")
        || combined.contains("EmitBundleObjectFiles")
        // Other transient resource errors
        || combined.contains("Unable to read data from the transport connection")
        // WASI SDK tar extraction race condition - multiple parallel builds
        // trying to extract the same tarball simultaneously
        || (combined.contains("wasi-sdk") && combined.contains("tar"))
        || (combined.contains("MSB3073") && combined.contains("exited with code 2"))
}

fn run(cmd: &mut Command, label: &str) -> Result<()> {
    run_with_retry(cmd, label, 3)
}

fn run_with_retry(cmd: &mut Command, label: &str, max_retries: u32) -> Result<()> {
    use std::hash::{Hash, Hasher};
    let mut last_error = None;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            eprintln!(
                "⚠️ {label}: retrying after transient failure (attempt {}/{})",
                attempt + 1,
                max_retries + 1
            );
            // Add jitter to desynchronize parallel builds that fail simultaneously.
            // Use a simple hash of the label + attempt to get pseudo-random delay.
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            label.hash(&mut hasher);
            attempt.hash(&mut hasher);
            std::process::id().hash(&mut hasher);
            let jitter_ms = hasher.finish() % 2000; // 0-2 seconds of jitter
            std::thread::sleep(std::time::Duration::from_millis(1000 + jitter_ms));
        }

        eprintln!("==> {label}: {:?}", cmd);
        let out = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                last_error = Some(format!("{label}: spawn failed: {e}"));
                continue;
            }
        };

        if out.status.success() {
            return Ok(());
        }

        let code = out.status.code().unwrap_or(-1);
        let stderr_raw = String::from_utf8_lossy(&out.stderr);
        let stdout_raw = String::from_utf8_lossy(&out.stdout);
        let stderr = strip_ansi_codes(&stderr_raw);
        let stdout = strip_ansi_codes(&stdout_raw);

        // Retry on signal kills (like SIGSEGV) or transient build errors
        let should_retry = was_signal_killed(&out.status) || is_transient_build_error(&stderr, &stdout);
        if should_retry && attempt < max_retries {
            let reason = if was_signal_killed(&out.status) {
                "signal kill"
            } else {
                "transient build error"
            };
            eprintln!("⚠️ {label}: {reason} detected, will retry...");
            last_error = Some(format!(
                "{label} failed (exit={code})\n--- stderr ---\n{stderr}\n--- stdout ---\n{stdout}"
            ));
            continue;
        }

        bail!("{label} failed (exit={code})\n--- stderr ---\n{stderr}\n--- stdout ---\n{stdout}");
    }

    bail!(last_error.unwrap_or_else(|| format!("{label}: unknown error after retries")))
}

/* -------------------------------------------------------------------------- */
/* C# Publisher                                                               */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy)]
pub struct DotnetPublisher;

impl DotnetPublisher {
    fn ensure_csproj(root: &Path) -> Result<()> {
        let mut has = false;
        for ent in fs::read_dir(root)? {
            let ent = ent?;
            if ent.path().extension().map(|e| e == "csproj").unwrap_or(false) {
                has = true;
                break;
            }
        }
        if !has {
            bail!("expected a C# project in {}", root.display());
        }
        Ok(())
    }
}

impl Publisher for DotnetPublisher {
    fn publish(&self, host_url: &str, source: &Path, module_name: &str) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish csharp module {}", module_name);

        Self::ensure_csproj(source)?;

        let db = sanitize_db_name(module_name);

        let mut cmd = Command::new("spacetime");
        cmd.arg("build")
            .current_dir(source)
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .env("DOTNET_NOLOGO", "1")
            // Prevent MSBuild node reuse issues that cause "Pipe is broken" errors
            // when running multiple dotnet builds in parallel.
            .env("MSBUILDDISABLENODEREUSE", "1")
            .env("DOTNET_CLI_USE_MSBUILD_SERVER", "0");
        run(&mut cmd, "spacetime build (csharp)")?;

        let mut pubcmd = Command::new("spacetime");
        pubcmd
            .arg("publish")
            .arg("-c")
            .arg("-y")
            .arg("--server")
            .arg(host_url)
            .arg(&db)
            .current_dir(source);
        run(&mut pubcmd, "spacetime publish (csharp)")?;

        Ok(())
    }
}
/* -------------------------------------------------------------------------- */
/* Rust Publisher                                                             */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy)]
pub struct SpacetimeRustPublisher;

impl SpacetimeRustPublisher {
    fn ensure_standalone_manifest(dst: &Path) -> Result<()> {
        if !dst.join("Cargo.toml").exists() {
            bail!("no Cargo.toml in {}", dst.display());
        }
        Ok(())
    }
}

impl Publisher for SpacetimeRustPublisher {
    fn publish(&self, host_url: &str, source: &Path, module_name: &str) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish rust module {}", module_name);

        // Build/publish directly from `source`
        Self::ensure_standalone_manifest(source)?;

        // sanitize db + server
        let db = sanitize_db_name(module_name);

        // 2) Publish
        run(
            Command::new("spacetime")
                .arg("publish")
                .arg("-c")
                .arg("-y")
                .arg("--server")
                .arg(host_url)
                .arg(&db)
                .current_dir(source),
            "spacetime publish",
        )?;

        Ok(())
    }
}

/* -------------------------------------------------------------------------- */
/* TypeScript Publisher                                                       */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy)]
pub struct TypeScriptPublisher;

impl TypeScriptPublisher {
    fn ensure_package_json(root: &Path) -> Result<()> {
        if !root.join("package.json").exists() {
            bail!("no package.json in {}", root.display());
        }
        Ok(())
    }
}

impl Publisher for TypeScriptPublisher {
    fn publish(&self, host_url: &str, source: &Path, module_name: &str) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish typescript module {}", module_name);

        Self::ensure_package_json(source)?;
        let db = sanitize_db_name(module_name);

        // Install dependencies (--ignore-workspace to avoid parent workspace interference)
        run(
            Command::new("pnpm")
                .arg("install")
                .arg("--ignore-workspace")
                .current_dir(source),
            "pnpm install (typescript)",
        )?;

        // Publish (spacetime CLI handles TypeScript compilation internally)
        run(
            Command::new("spacetime")
                .arg("publish")
                .arg("-c")
                .arg("-y")
                .arg("--server")
                .arg(host_url)
                .arg(&db)
                .current_dir(source),
            "spacetime publish (typescript)",
        )?;

        Ok(())
    }
}
