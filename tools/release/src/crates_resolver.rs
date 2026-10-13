use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT_CRATES: &[&str] = &["spacetimedb", "spacetimedb-sdk"];

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    version: String,
    manifest_path: PathBuf,
    publish: Option<Vec<String>>,
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    // Cargo reports the actual package name here, even for renamed dependencies.
    name: String,
    req: String,
    kind: Option<String>,
    path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct PublishCrate {
    pub name: String,
    pub version: String,
    pub manifest_path: PathBuf,
}

/// Finds the workspace root (public directory) by searching up the directory tree
pub fn find_workspace_root() -> Result<PathBuf> {
    // TODO(bfops): We can simplify this by doing `git rev-parse --show-toplevel`.
    // TODO(jdetter): We can probably just remove this. `cargo release` is almost exclusively going
    //   to be ran in the CI so it's fine to make an assumption about the directory
    //   where we're running `cargo release` from.
    std::env::current_dir()
        .context("Failed to get current directory")?
        .ancestors()
        .find_map(|path| {
            if path.join("Cargo.toml").exists() {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
        .context("Failed to find workspace root. Make sure you're running from within the SpacetimeDB repository.")
}

/// Returns the complete dependency-first publication plan, without resolving registry crates.
pub fn get_crates_to_publish() -> Result<Vec<PublishCrate>> {
    let workspace_root = find_workspace_root()?;
    let output = Command::new("cargo")
        .args(["metadata", "--format-version=1", "--no-deps"])
        .current_dir(workspace_root)
        .output()
        .context("Failed to execute cargo metadata")?;

    if !output.status.success() {
        bail!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata output")?;
    publication_plan(&metadata, ROOT_CRATES)
}

fn publication_plan(metadata: &CargoMetadata, roots: &[&str]) -> Result<Vec<PublishCrate>> {
    let by_name: HashMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();
    let by_path: HashMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| {
            Ok((
                package
                    .manifest_path
                    .parent()
                    .context("Manifest has no parent directory")?,
                package,
            ))
        })
        .collect::<Result<_>>()?;
    let mut visiting = Vec::new();
    let mut visited = HashSet::new();
    let mut plan = Vec::new();

    for root in roots {
        let package = by_name
            .get(root)
            .with_context(|| format!("Crate '{root}' not found in cargo metadata"))?;
        visit(package, &by_path, &mut visiting, &mut visited, &mut plan)?;
    }
    Ok(plan)
}

fn visit<'a>(
    package: &'a Package,
    by_path: &HashMap<&Path, &'a Package>,
    visiting: &mut Vec<&'a str>,
    visited: &mut HashSet<&'a str>,
    plan: &mut Vec<PublishCrate>,
) -> Result<()> {
    if visited.contains(package.name.as_str()) {
        return Ok(());
    }
    if let Some(start) = visiting.iter().position(|name| *name == package.name) {
        let mut cycle = visiting[start..].to_vec();
        cycle.push(&package.name);
        bail!("Publication dependency cycle: {}", cycle.join(" -> "));
    }
    if let Some(registries) = &package.publish
        && !registries.iter().any(|registry| registry == "crates-io")
    {
        bail!(
            "Crate '{}' is required for publication but does not allow publishing to crates.io",
            package.name
        );
    }
    visiting.push(&package.name);

    // Read declared dependencies, not resolve.nodes: inactive optional dependencies
    // must also be available when Cargo prepares the published package's lockfile.
    // Do not filter by target, optional status, or dependency-name prefix.
    for dependency in &package.dependencies {
        if dependency.kind.as_deref() == Some("dev") {
            continue;
        }
        let Some(path) = &dependency.path else {
            continue;
        };
        let dep_package = by_path.get(path.as_path()).with_context(|| {
            format!(
                "Local dependency '{}' of '{}' at {} is not a workspace package",
                dependency.name,
                package.name,
                path.display()
            )
        })?;
        let requirement = semver::VersionReq::parse(&dependency.req)?;
        let version = semver::Version::parse(&dep_package.version)?;
        if dependency.name != dep_package.name || !requirement.matches(&version) {
            bail!(
                "Dependency '{}' of '{}' requires {}, but local package is {}@{}",
                dependency.name,
                package.name,
                dependency.req,
                dep_package.name,
                dep_package.version
            );
        }
        visit(dep_package, by_path, visiting, visited, plan)?;
    }

    visiting.pop();
    visited.insert(&package.name);
    plan.push(PublishCrate {
        name: package.name.clone(),
        version: package.version.clone(),
        manifest_path: package.manifest_path.clone(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn package(name: &str, dependencies: Vec<Value>) -> Value {
        json!({
            "name": name,
            "version": "2.12.0",
            "manifest_path": format!("/workspace/{name}/Cargo.toml"),
            "publish": null,
            "dependencies": dependencies,
        })
    }

    fn dependency(name: &str) -> Value {
        json!({
            "name": name,
            "req": "=2.12.0",
            "kind": null,
            "path": format!("/workspace/{name}"),
        })
    }

    fn plan(packages: Vec<Value>) -> Result<Vec<PublishCrate>> {
        let metadata = serde_json::from_value(json!({ "packages": packages })).unwrap();
        publication_plan(&metadata, ROOT_CRATES)
    }

    #[test]
    fn includes_optional_target_and_renamed_dependencies_in_dependency_order() {
        let mut auth = dependency("spacetimedb-auth");
        auth["optional"] = json!(true);
        auth["target"] = json!("cfg(not(target_arch = \"wasm32\"))");
        let mut datastore = dependency("spacetimedb-test-datastore");
        datastore["optional"] = json!(true);
        datastore["target"] = auth["target"].clone();
        let mut core = dependency("spacetimedb-core");
        core["rename"] = json!("spacetimedb_core");
        let packages = vec![
            package("spacetimedb", vec![auth, datastore]),
            package("spacetimedb-sdk", vec![dependency("spacetimedb-auth")]),
            package("spacetimedb-auth", vec![]),
            package("spacetimedb-test-datastore", vec![core]),
            package("spacetimedb-core", vec![]),
        ];
        let names: Vec<_> = plan(packages)
            .unwrap()
            .into_iter()
            .map(|package| package.name)
            .collect();
        assert_eq!(
            names,
            [
                "spacetimedb-auth",
                "spacetimedb-core",
                "spacetimedb-test-datastore",
                "spacetimedb",
                "spacetimedb-sdk"
            ]
        );
    }

    #[test]
    fn includes_build_dependencies_without_name_prefix_and_excludes_dev_and_registry_dependencies() {
        let mut build = dependency("build-helper");
        build["kind"] = json!("build");
        let mut dev = dependency("unpublished-tests");
        dev["kind"] = json!("dev");
        let mut registry = dependency("spacetimedb-external");
        registry["path"] = Value::Null;
        let names: Vec<_> = plan(vec![
            package("spacetimedb", vec![build, dev, registry]),
            package("spacetimedb-sdk", vec![]),
            package("build-helper", vec![]),
        ])
        .unwrap()
        .into_iter()
        .map(|package| package.name)
        .collect();
        assert_eq!(names, ["build-helper", "spacetimedb", "spacetimedb-sdk"]);
    }

    #[test]
    fn rejects_cycles_with_dependency_chain() {
        let error = plan(vec![
            package("spacetimedb", vec![dependency("spacetimedb-auth")]),
            package("spacetimedb-auth", vec![dependency("spacetimedb")]),
            package("spacetimedb-sdk", vec![]),
        ])
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("spacetimedb -> spacetimedb-auth -> spacetimedb"),
            "{error}"
        );
    }

    #[test]
    fn rejects_dependencies_that_cannot_publish_to_crates_io() {
        for publish in [json!([]), json!(["private-registry"])] {
            let mut auth = package("spacetimedb-auth", vec![]);
            auth["publish"] = publish;
            let error = plan(vec![
                package("spacetimedb", vec![dependency("spacetimedb-auth")]),
                package("spacetimedb-sdk", vec![]),
                auth,
            ])
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("spacetimedb-auth") && error.contains("does not allow publishing"),
                "{error}"
            );
        }
    }

    #[test]
    fn rejects_mismatched_local_versions() {
        let mut auth = package("spacetimedb-auth", vec![]);
        auth["version"] = json!("2.11.0");
        let error = plan(vec![
            package("spacetimedb", vec![dependency("spacetimedb-auth")]),
            package("spacetimedb-sdk", vec![]),
            auth,
        ])
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("requires =2.12.0") && error.contains("spacetimedb-auth@2.11.0"),
            "{error}"
        );
    }

    #[test]
    fn rejects_local_dependencies_outside_metadata() {
        let error = plan(vec![
            package("spacetimedb", vec![dependency("missing")]),
            package("spacetimedb-sdk", vec![]),
        ])
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("Local dependency 'missing' of 'spacetimedb'") && error.contains("not a workspace package"),
            "{error}"
        );
    }
}
