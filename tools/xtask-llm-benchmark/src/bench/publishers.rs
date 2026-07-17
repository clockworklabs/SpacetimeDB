use crate::bench::utils::sanitize_db_name;
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask-llm-benchmark is under public/tools/xtask-llm-benchmark")
        .to_path_buf()
}

fn pnpm_minimum_release_age() -> Result<String> {
    let workspace = fs::read_to_string(workspace_root().join("pnpm-workspace.yaml"))?;
    workspace
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("minimumReleaseAge:")?
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map(|age| age.to_string())
        .ok_or_else(|| anyhow::anyhow!("pnpm-workspace.yaml is missing minimumReleaseAge"))
}

fn path_entries() -> Vec<PathBuf> {
    #[cfg(windows)]
    let path = env::var_os("Path").or_else(|| env::var_os("PATH"));
    #[cfg(not(windows))]
    let path = env::var_os("PATH");

    path.map(|path| env::split_paths(&path).collect()).unwrap_or_default()
}

fn command_path_candidates(name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let path = Path::new(name);
        if path.extension().is_some() {
            vec![name.to_string()]
        } else {
            vec![
                format!("{name}.cmd"),
                format!("{name}.exe"),
                format!("{name}.bat"),
                name.to_string(),
            ]
        }
    }
    #[cfg(not(windows))]
    {
        vec![name.to_string()]
    }
}

fn resolve_command_on_path(name: &str) -> Option<PathBuf> {
    for dir in path_entries() {
        for candidate in command_path_candidates(name) {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn configured_nodejs_dir() -> Option<PathBuf> {
    env::var("NODEJS_DIR")
        .ok()
        .map(|s| s.trim().trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn pnpm_in_dir(dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        for candidate in ["pnpm.cmd", "pnpm.exe", "pnpm.bat"] {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let path = dir.join("pnpm");
        path.is_file().then_some(path)
    }
}

fn node_in_dir(dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    let path = dir.join("node.exe");
    #[cfg(not(windows))]
    let path = dir.join("node");

    path.is_file().then_some(path)
}

fn resolve_node_exe(nodejs_dir: Option<&Path>) -> Option<PathBuf> {
    nodejs_dir
        .and_then(node_in_dir)
        .or_else(|| resolve_command_on_path("node"))
        .or_else(|| {
            env::var("NVM_SYMLINK")
                .ok()
                .map(PathBuf::from)
                .and_then(|dir| node_in_dir(&dir))
        })
}

struct CliRootDir {
    path: PathBuf,
}

impl CliRootDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

fn spacetime_cmd(cli_root: &CliRootDir) -> Command {
    let spacetime = env::var_os("LLM_BENCH_SPACETIME_BIN").unwrap_or_else(|| "spacetime".into());
    let mut cmd = Command::new(spacetime);
    cmd.arg("--root-dir").arg(cli_root.path());
    cmd
}

fn pnpm_cjs_for_cmd(pnpm: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let is_cmd = pnpm
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"));
        if !is_cmd {
            return None;
        }

        let cjs = pnpm
            .parent()?
            .join("node_modules")
            .join("pnpm")
            .join("bin")
            .join("pnpm.cjs");
        cjs.is_file().then_some(cjs)
    }
    #[cfg(not(windows))]
    {
        let _ = pnpm;
        None
    }
}

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
    fn publish(&self, host_url: &str, source: &Path, module_name: &str, clear_database: bool) -> Result<()>;
}

fn database_cli_root(module_name: &str) -> Result<CliRootDir> {
    let db = sanitize_db_name(module_name);
    let path = env::temp_dir().join(format!("stdb-llm-cli-{}-{db}", std::process::id()));
    fs::create_dir_all(&path)?;
    Ok(CliRootDir { path })
}

/// Check if the process was killed by a signal (e.g., SIGSEGV = 11)
#[cfg(unix)]
fn signal_killed_by(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn signal_killed_by(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

/// Check if the failure is a transient error that should be retried.
/// These are resource contention issues in the dotnet WASI SDK.
fn is_transient_build_error(stderr: &str, stdout: &str) -> bool {
    let combined = format!("{stderr}{stdout}");
    let diagnostic = combined.to_ascii_lowercase();
    // "Pipe is broken" errors from WASI SDK parallel builds
    combined.contains("Pipe is broken")
        || combined.contains("EmitBundleObjectFiles")
        // Other transient resource errors
        || combined.contains("Unable to read data from the transport connection")
        // WASI SDK tar extraction race condition - multiple parallel builds
        // trying to extract the same tarball simultaneously
        || (combined.contains("wasi-sdk") && combined.contains("tar"))
        || (combined.contains("MSB3073") && combined.contains("exited with code 2"))
        // dotnet can crash below spacetime while spacetime exits 1.
        || combined.contains("code <signal")
        // A child process can occasionally disappear on Windows after login but
        // before the CLI emits a compiler/MSBuild diagnostic. Retrying is safe:
        // benchmark publishes always use their own temporary CLI root and database.
        || (!diagnostic.contains("error")
            && !diagnostic.contains("failed")
            && !diagnostic.contains("exception"))
}

#[cfg(test)]
mod tests {
    use super::is_transient_build_error;

    #[test]
    fn retries_publish_exit_without_actionable_diagnostic() {
        assert!(is_transient_build_error(
            "Saving config to temporary-root/config/cli.toml.",
            "Logged in with identity c200"
        ));
    }

    #[test]
    fn does_not_retry_source_compile_error() {
        assert!(!is_transient_build_error(
            "error: could not compile `spacetime-module`",
            ""
        ));
    }
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

        // Retry on signal kills (like SIGSEGV) or transient build errors.
        let signal = signal_killed_by(&out.status);
        let should_retry = signal.is_some() || is_transient_build_error(&stderr, &stdout);
        if should_retry && attempt < max_retries {
            let reason = if let Some(signal) = signal {
                format!("signal {signal}")
            } else {
                "transient build error".to_string()
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

    fn configure_dotnet_env(cmd: &mut Command) -> &mut Command {
        cmd.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .env("DOTNET_NOLOGO", "1")
            // The CI runner's .NET install can crash while formatting localized
            // DateTime/TimeZoneInfo data before publish starts. Force invariant
            // globalization so generated C# module publish reaches MSBuild.
            .env("DOTNET_SYSTEM_GLOBALIZATION_INVARIANT", "1")
            // Prevent MSBuild node reuse issues that cause "Pipe is broken" errors
            // when running multiple dotnet builds in parallel.
            .env("MSBUILDDISABLENODEREUSE", "1")
            .env("DOTNET_CLI_USE_MSBUILD_SERVER", "0")
    }
}

impl Publisher for DotnetPublisher {
    fn publish(&self, host_url: &str, source: &Path, module_name: &str, clear_database: bool) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish csharp module {}", module_name);

        Self::ensure_csproj(source)?;

        let db = sanitize_db_name(module_name);
        let source = source
            .canonicalize()
            .with_context(|| format!("failed to resolve C# source path {}", source.display()))?;
        let cli_root = database_cli_root(module_name)?;

        let mut pubcmd = spacetime_cmd(&cli_root);
        pubcmd.arg("publish");
        if clear_database {
            pubcmd.arg("-c");
        }
        pubcmd
            .arg("-y")
            .arg("--server")
            .arg(host_url)
            .arg("--module-path")
            .arg(&source)
            .arg(&db)
            .current_dir(&source);
        Self::configure_dotnet_env(&mut pubcmd);
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
    fn publish(&self, host_url: &str, source: &Path, module_name: &str, clear_database: bool) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish rust module {}", module_name);

        // Build/publish directly from `source`
        Self::ensure_standalone_manifest(source)?;

        // sanitize db + server
        let db = sanitize_db_name(module_name);
        let cli_root = database_cli_root(module_name)?;

        // 2) Publish
        let mut pubcmd = spacetime_cmd(&cli_root);
        pubcmd.arg("publish");
        if clear_database {
            pubcmd.arg("-c");
        }
        pubcmd
            .arg("-y")
            .arg("--server")
            .arg(host_url)
            .arg(&db)
            .current_dir(source);
        if let Some(target_dir) = env::var_os("LLM_BENCH_RUST_TARGET_DIR") {
            pubcmd.env("CARGO_TARGET_DIR", target_dir);
        }
        run(&mut pubcmd, "spacetime publish")?;

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
    fn publish(&self, host_url: &str, source: &Path, module_name: &str, clear_database: bool) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish typescript module {}", module_name);

        Self::ensure_package_json(source)?;
        let db = sanitize_db_name(module_name);
        let cli_root = database_cli_root(module_name)?;

        // Install dependencies (--ignore-workspace to avoid parent workspace interference).
        let nodejs_dir = configured_nodejs_dir();
        let pnpm_exe = nodejs_dir
            .as_deref()
            .and_then(pnpm_in_dir)
            .or_else(|| resolve_command_on_path("pnpm"));
        if let Some(ref pnpm) = pnpm_exe {
            eprintln!("[pnpm] using {}", pnpm.display());
        } else if let Some(ref dir) = nodejs_dir {
            eprintln!(
                "[pnpm] NODEJS_DIR set to {} but pnpm not found there or on PATH",
                dir.display()
            );
        }
        let node_exe = resolve_node_exe(nodejs_dir.as_deref());
        let pnpm_cjs = pnpm_exe.as_deref().and_then(pnpm_cjs_for_cmd);
        let mut pnpm_cmd = if let (Some(node), Some(cjs)) = (&node_exe, pnpm_cjs) {
            eprintln!("[pnpm] invoking {} {}", node.display(), cjs.display());
            let mut cmd = Command::new(node);
            cmd.arg(cjs);
            cmd
        } else {
            match &pnpm_exe {
                Some(p) => Command::new(p),
                None => Command::new("pnpm"),
            }
        };
        pnpm_cmd
            .arg("install")
            .arg("--ignore-workspace")
            .current_dir(source)
            .env("CI", "true")
            // This install runs in a materialized project with workspace config
            // ignored, so pass the repo's pnpm package-age policy explicitly.
            .env("npm_config_minimum_release_age", pnpm_minimum_release_age()?);
        let mut prepend_paths = Vec::new();
        if let Some(dir) = nodejs_dir {
            prepend_paths.push(dir);
        }
        if let Some(ref pnpm) = pnpm_exe
            && let Some(parent) = pnpm.parent()
        {
            prepend_paths.push(parent.to_path_buf());
        }
        if let Some(node) = node_exe
            && let Some(parent) = node.parent()
        {
            prepend_paths.push(parent.to_path_buf());
        }
        let child_path = if !prepend_paths.is_empty() {
            let mut paths = path_entries();
            for path in prepend_paths.into_iter().rev() {
                if !paths.iter().any(|existing| existing == &path) {
                    paths.insert(0, path);
                }
            }
            env::join_paths(paths).ok()
        } else {
            None
        };
        if let Some(ref new_path) = child_path {
            #[cfg(windows)]
            {
                pnpm_cmd.env_remove("PATH");
                pnpm_cmd.env("Path", new_path);
            }
            #[cfg(not(windows))]
            pnpm_cmd.env("PATH", new_path);
        }
        run(&mut pnpm_cmd, "pnpm install (typescript)")?;

        // Publish (spacetime CLI handles TypeScript compilation internally)
        let mut publish_cmd = spacetime_cmd(&cli_root);
        publish_cmd.arg("publish");
        if clear_database {
            publish_cmd.arg("-c");
        }
        publish_cmd
            .arg("-y")
            .arg("--server")
            .arg(host_url)
            .arg(&db)
            .current_dir(source);
        if let Some(ref new_path) = child_path {
            #[cfg(windows)]
            {
                publish_cmd.env_remove("PATH");
                publish_cmd.env("Path", new_path);
            }
            #[cfg(not(windows))]
            publish_cmd.env("PATH", new_path);
        }
        run(&mut publish_cmd, "spacetime publish (typescript)")?;

        Ok(())
    }
}
