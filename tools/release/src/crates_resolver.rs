use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    version: String,
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    // This is the actual package name, even if the dependency is renamed.
    name: String,
    kind: Option<String>,
    path: Option<PathBuf>,
}

fn get_cargo_metadata(workspace_root: &Path) -> Result<CargoMetadata> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--no-deps")
        .current_dir(workspace_root)
        .output()
        .context("Failed to execute cargo metadata")?;

    if !output.status.success() {
        anyhow::bail!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata output")
}

#[derive(Debug, PartialEq, Eq)]
pub struct CrateToPublish {
    pub name: String,
    pub version: String,
}

/// Processes a crate to find its local publication dependencies
fn get_crate_deps(crate_name: &String, packages: &HashMap<String, &Package>) -> Result<Vec<String>> {
    let package = packages
        .get(crate_name)
        .with_context(|| format!("Crate '{}' not found in cargo metadata", crate_name))?;

    println!("\nChecking crate '{}'...", crate_name);

    // Include all local normal and build dependencies, including optional dependencies
    // and dependencies for any target. Exclude dev dependencies and registry dependencies.
    let deps: Vec<_> = package
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind.as_deref() != Some("dev") && dependency.path.is_some())
        .map(|dependency| {
            assert!(
                dependency.name.starts_with("spacetimedb-"),
                "Local publication dependency '{}' of '{}' must start with 'spacetimedb-'",
                dependency.name,
                crate_name
            );
            dependency.name.clone()
        })
        .collect();
    if !deps.is_empty() {
        for name in &deps {
            println!("  {}", name);
        }
    } else {
        println!("  No local publication dependencies found.");
    }

    let mut all_deps = deps.clone();

    for dep_name in deps {
        let sub_deps = get_crate_deps(&dep_name, packages)?;
        all_deps.extend(sub_deps);
    }

    Ok(all_deps)
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

/// Returns a list of crates to publish in the correct order
pub fn get_crates_to_publish() -> Result<Vec<CrateToPublish>> {
    let workspace_root = find_workspace_root()?;
    let metadata = get_cargo_metadata(&workspace_root)?;
    let order = get_publish_order(&metadata)?;
    let mut versions: HashMap<_, _> = metadata
        .packages
        .into_iter()
        .map(|package| (package.name, package.version))
        .collect();
    order
        .into_iter()
        .map(|name| {
            let version = versions
                .remove(&name)
                .context("Publication package missing from metadata")?;
            Ok(CrateToPublish { name, version })
        })
        .collect()
}

fn get_publish_order(metadata: &CargoMetadata) -> Result<Vec<String>> {
    let packages = metadata
        .packages
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect();

    // We must publish the bindings + sdk at a minimum
    // Note: "bindings" corresponds to the "spacetimedb" crate
    // and "sdk" corresponds to the "spacetimedb-sdk" crate
    let root_crates = vec!["spacetimedb".to_string(), "spacetimedb-sdk".to_string()];
    let mut all_crates = Vec::new();
    all_crates.extend(root_crates.iter().cloned());

    // Add all dependencies of the root crates
    for crate_name in &root_crates {
        all_crates.extend(get_crate_deps(crate_name, &packages)?);
    }

    // It takes a bit of reasoning to conclude that this is, in fact, going to be a legitimate
    // dependency-order of all of these crates. Because of how the list is constructed, once it's reversed,
    // every crate will be mentioned before any of the crates that use it. Because of that, it's safe to
    // deduplicate the list in a way that preserves the _first_ occurrence of every crate name, without
    // violating the "mentioned before it's used" property of the list.
    let mut seen = HashSet::new();
    let mut publish_order = Vec::new();

    for crate_name in all_crates.into_iter().rev() {
        if seen.insert(crate_name.clone()) {
            publish_order.push(crate_name);
        }
    }

    Ok(publish_order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn package(name: &str, dependencies: Vec<Value>) -> Value {
        json!({
            "name": name,
            "version": "1.2.3",
            "dependencies": dependencies,
        })
    }

    fn dependency(name: &str) -> Value {
        json!({ "name": name, "kind": null, "path": format!("/workspace/{name}") })
    }

    fn order(packages: Vec<Value>) -> Vec<String> {
        let metadata = serde_json::from_value(json!({ "packages": packages })).unwrap();
        get_publish_order(&metadata).unwrap()
    }

    #[test]
    fn includes_optional_target_and_renamed_dependencies() {
        let mut auth = dependency("spacetimedb-auth");
        auth["optional"] = json!(true);
        auth["target"] = json!("cfg(not(target_arch = \"wasm32\"))");
        let mut datastore = dependency("spacetimedb-test-datastore");
        datastore["optional"] = json!(true);
        datastore["target"] = auth["target"].clone();
        let mut core = dependency("spacetimedb-core");
        core["rename"] = json!("spacetimedb_core");
        let names = order(vec![
            package("spacetimedb", vec![auth, datastore]),
            package("spacetimedb-sdk", vec![dependency("spacetimedb-auth")]),
            package("spacetimedb-auth", vec![]),
            package("spacetimedb-test-datastore", vec![core]),
            package("spacetimedb-core", vec![]),
        ]);
        assert_eq!(names.len(), 5);
        for (dependency, consumer) in [
            ("spacetimedb-auth", "spacetimedb"),
            ("spacetimedb-auth", "spacetimedb-sdk"),
            ("spacetimedb-core", "spacetimedb-test-datastore"),
            ("spacetimedb-test-datastore", "spacetimedb"),
        ] {
            let dep = names.iter().position(|name| name == dependency).unwrap();
            let parent = names.iter().position(|name| name == consumer).unwrap();
            assert!(dep < parent, "{dependency} must precede {consumer}: {names:?}");
        }
    }

    #[test]
    fn includes_local_build_dependencies_and_excludes_dev_and_registry_dependencies() {
        let mut build = dependency("spacetimedb-build-helper");
        build["kind"] = json!("build");
        let mut dev = dependency("unpublished-tests");
        dev["kind"] = json!("dev");
        let mut registry = dependency("spacetimedb-external");
        registry["path"] = Value::Null;
        assert_eq!(
            order(vec![
                package("spacetimedb", vec![build, dev, registry]),
                package("spacetimedb-sdk", vec![]),
                package("spacetimedb-build-helper", vec![]),
            ]),
            ["spacetimedb-build-helper", "spacetimedb-sdk", "spacetimedb"]
        );
    }

    #[test]
    #[should_panic(
        expected = "Local publication dependency 'build-helper' of 'spacetimedb' must start with 'spacetimedb-'"
    )]
    fn panics_for_local_dependencies_without_spacetimedb_prefix() {
        order(vec![
            package("spacetimedb", vec![dependency("build-helper")]),
            package("spacetimedb-sdk", vec![]),
            package("build-helper", vec![]),
        ]);
    }
}
