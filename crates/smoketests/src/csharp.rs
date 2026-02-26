use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const PACKAGE_PROJECTS: [(&str, &str); 2] = [
    ("BSATN.Runtime", "SpacetimeDB.BSATN.Runtime"),
    ("Runtime", "SpacetimeDB.Runtime"),
];

const REQUIRED_RUNTIME_PACKAGES: [&str; 2] = ["SpacetimeDB.BSATN.Runtime", "SpacetimeDB.Runtime"];

#[derive(Debug)]
struct CsharpBuildEnv {
    local_feed_dir: PathBuf,
}

static CSHARP_WORKLOAD_READY: OnceLock<Result<(), anyhow::Error>> = OnceLock::new();
static CSHARP_BUILD_ENV: OnceLock<Result<CsharpBuildEnv, anyhow::Error>> = OnceLock::new();

/// Normalizes a filesystem path for string-based comparisons in NuGet artifacts.
///
/// NuGet and `project.assets.json` can emit paths with platform-specific separators
/// and optional trailing slashes; this keeps comparisons stable across hosts.
fn normalize_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn package_cache_contains_version(cache_root: &Path, package_id: &str, version: &str) -> bool {
    // NuGet usually stores package IDs lower-cased on disk.
    let expected = cache_root.join(package_id.to_ascii_lowercase()).join(version);
    if expected.exists() {
        return true;
    }
    let Ok(entries) = fs::read_dir(cache_root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false)
            && entry.file_name().to_string_lossy().eq_ignore_ascii_case(package_id)
            && entry.path().join(version).exists()
    })
}

/// Runs `dotnet` in a given working directory with error context suitable for tests.
///
/// This wrapper centralizes command construction so callers consistently include
/// command + cwd details in failures.
fn run_dotnet(args: &[&str], cwd: &Path) -> Result<String> {
    let mut cmd = Vec::with_capacity(args.len() + 1);
    cmd.push("dotnet");
    cmd.extend_from_slice(args);
    crate::run_cmd(&cmd, cwd).with_context(|| format!("dotnet {} failed in {}", args.join(" "), cwd.display()))
}

/// Ensures the WASI workload required by C# module publishing is present.
///
/// We do a best-effort install first, then assert by reading `dotnet workload list`.
/// Result is memoized for the process so repeated C# smoketests avoid redundant setup.
fn ensure_wasi_workload() -> Result<()> {
    let _ = CSHARP_WORKLOAD_READY
        .get_or_init(|| {
            let workspace = crate::workspace_root();
            let modules_dir = workspace.join("modules");
            let _ = run_dotnet(
                &[
                    "workload",
                    "install",
                    "wasi-experimental",
                    "--skip-manifest-update",
                ],
                &modules_dir,
            );
            let workloads = run_dotnet(&["workload", "list"], &modules_dir)?;
            if !workloads.contains("wasi-experimental") {
                bail!(
                    "dotnet wasi-experimental workload is required but not installed.\n`dotnet workload list` output:\n{}",
                    workloads
                );
            }
            Ok(())
        })
        .as_ref()
        .map_err(|err| anyhow!("{err:#}"))?;
    Ok(())
}

/// Builds (once per process) a local, source-built NuGet feed for runtime packages.
///
/// This is a guardrail against stale binaries. Tests consume packages packed from the
/// current checkout rather than whatever may exist in machine-global caches.
fn ensure_local_feed() -> Result<&'static CsharpBuildEnv> {
    CSHARP_BUILD_ENV
        .get_or_init(|| {
            ensure_wasi_workload()?;

            let workspace = crate::workspace_root();
            let bindings = workspace.join("crates/bindings-csharp");
            let local_feed_dir = workspace.join("target/smoketests-csharp/local-feed");
            if local_feed_dir.exists() {
                fs::remove_dir_all(&local_feed_dir)
                    .with_context(|| format!("Failed to clear {}", local_feed_dir.display()))?;
            }
            fs::create_dir_all(&local_feed_dir)
                .with_context(|| format!("Failed to create {}", local_feed_dir.display()))?;
            let local_feed_dir_str = local_feed_dir
                .to_str()
                .context("Local C# NuGet feed path is not valid UTF-8")?;

            for (project_dir, _) in PACKAGE_PROJECTS {
                run_dotnet(
                    &["pack", "-c", "Release", "-o", local_feed_dir_str],
                    &bindings.join(project_dir),
                )?;
            }

            let feed_files = fs::read_dir(&local_feed_dir)
                .with_context(|| format!("Failed to inspect {}", local_feed_dir.display()))?
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect::<Vec<_>>();

            for (_, package_id) in PACKAGE_PROJECTS {
                let package_prefix = format!("{package_id}.");
                if !feed_files
                    .iter()
                    .any(|name| name.starts_with(&package_prefix) && name.ends_with(".nupkg"))
                {
                    bail!(
                        "Local feed at {} is missing package {}. Found files: {:?}",
                        local_feed_dir.display(),
                        package_id,
                        feed_files
                    );
                }
            }

            Ok(CsharpBuildEnv { local_feed_dir })
        })
        .as_ref()
        .map_err(|err| anyhow!("{err:#}"))
}

/// Prepares a generated C# module directory for deterministic restore/publish.
///
/// It writes a module-local `nuget.config` that:
/// - isolates global package cache to `<module>/.nuget/packages`
/// - routes `SpacetimeDB.*` resolution to the source-built local feed
/// - still allows all other dependencies from nuget.org
pub(crate) fn prepare_csharp_module(module_path: &Path) -> Result<()> {
    let env = ensure_local_feed()?;

    let package_cache_dir = module_path.join(".nuget/packages");
    if package_cache_dir.exists() {
        fs::remove_dir_all(&package_cache_dir)
            .with_context(|| format!("Failed to clear {}", package_cache_dir.display()))?;
    }
    fs::create_dir_all(&package_cache_dir)
        .with_context(|| format!("Failed to create {}", package_cache_dir.display()))?;

    let nuget_config = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <config>
    <add key="globalPackagesFolder" value="{}" />
  </config>
  <packageSources>
    <clear />
    <add key="spacetimedb-local" value="{}" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <packageSource key="spacetimedb-local">
      <package pattern="SpacetimeDB.*" />
    </packageSource>
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
"#,
        normalize_path(&package_cache_dir),
        normalize_path(&env.local_feed_dir),
    );

    fs::write(module_path.join("nuget.config"), nuget_config)
        .with_context(|| format!("Failed to write {}", module_path.join("nuget.config").display()))?;
    Ok(())
}

/// Verifies a C# module restore/publish used the intended local bindings.
///
/// We assert two invariants:
/// - required `SpacetimeDB.*` runtime packages were resolved in `obj/project.assets.json`
/// - those resolved package versions are present in the module-local package cache
///
/// Failing any of these means the smoketest may have used stale or external packages.
pub(crate) fn verify_csharp_module_restore(module_path: &Path) -> Result<()> {
    let _ = ensure_local_feed()?;

    let assets_path = module_path.join("obj").join("project.assets.json");
    let assets_text =
        fs::read_to_string(&assets_path).with_context(|| format!("Failed to read {}", assets_path.display()))?;
    let assets: Value =
        serde_json::from_str(&assets_text).with_context(|| format!("Failed to parse {}", assets_path.display()))?;

    let libraries = assets
        .get("libraries")
        .and_then(Value::as_object)
        .context("project.assets.json missing libraries")?;
    let package_cache_dir = module_path.join(".nuget/packages");
    for package_id in REQUIRED_RUNTIME_PACKAGES {
        let package_key = libraries
            .keys()
            .find(|name| name.starts_with(&format!("{package_id}/")))
            .with_context(|| {
                format!(
                    "project.assets.json did not resolve expected package `{package_id}`.\nresolved SpacetimeDB packages: {:?}",
                    libraries
                        .keys()
                        .filter(|name| name.starts_with("SpacetimeDB."))
                        .collect::<Vec<_>>()
                )
            })?;
        let (_, version) = package_key
            .split_once('/')
            .with_context(|| format!("Unexpected package key format in project.assets.json: `{package_key}`"))?;
        if !package_cache_contains_version(&package_cache_dir, package_id, version) {
            bail!(
                "Resolved package `{package_id}/{version}` was not found in module-local package cache {}",
                normalize_path(&package_cache_dir),
            );
        }
    }

    Ok(())
}
