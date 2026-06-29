use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct CargoToml {
    dependencies: Option<toml::Table>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
}

/// Gets cargo metadata and returns a mapping of crate names to their manifest paths
pub fn get_crate_manifest_map(workspace_root: &Path) -> Result<HashMap<String, PathBuf>> {
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

    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata output")?;

    let mut map = HashMap::new();
    for package in metadata.packages {
        map.insert(package.name, package.manifest_path);
    }

    Ok(map)
}

/// Finds SpacetimeDB dependencies in a Cargo.toml file
pub fn find_spacetimedb_dependencies(cargo_toml_path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(cargo_toml_path)
        .with_context(|| format!("Failed to read Cargo.toml at {}", cargo_toml_path.display()))?;

    // NOTE(bfops): I prefer what find-publish-list.py does; it reads the deps from the cargo metadata output.
    let cargo_data: CargoToml = toml::from_str(&content)
        .with_context(|| format!("Failed to parse Cargo.toml at {}", cargo_toml_path.display()))?;

    let mut deps = Vec::new();
    if let Some(dependencies) = cargo_data.dependencies {
        for (dep_name, _) in dependencies {
            if dep_name.starts_with("spacetimedb-") {
                deps.push(dep_name);
            }
        }
    }

    Ok(deps)
}

/// Processes a crate to find its SpacetimeDB dependencies
pub fn get_crate_deps(crate_name: &String, manifest_map: &HashMap<String, PathBuf>) -> Result<Vec<String>> {
    // Look up the crate in the manifest map
    let cargo_toml_path = manifest_map
        .get(crate_name)
        .with_context(|| format!("Crate '{}' not found in cargo metadata", &crate_name))?;

    println!("\nChecking crate '{}'...", &crate_name);

    let deps = find_spacetimedb_dependencies(cargo_toml_path)?;
    if !deps.is_empty() {
        for name in &deps {
            println!("  {}", name);
        }
    } else {
        println!("  No spacetimedb-* dependencies found.");
    }

    let mut all_deps = deps.clone();

    for dep_name in deps {
        let sub_deps = get_crate_deps(&dep_name, manifest_map)?;
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
pub fn get_crates_to_publish() -> Result<Vec<String>> {
    // Find the workspace root (public directory)
    let workspace_root = find_workspace_root()?;

    // Get the manifest map from cargo metadata
    let manifest_map = get_crate_manifest_map(&workspace_root)?;

    // We must publish the bindings + sdk at a minimum
    // Note: "bindings" corresponds to the "spacetimedb" crate
    // and "sdk" corresponds to the "spacetimedb-sdk" crate
    let root_crates = vec!["spacetimedb".to_string(), "spacetimedb-sdk".to_string()];
    let mut all_crates = Vec::new();
    all_crates.extend(root_crates.iter().cloned());

    // Add all dependencies of the root crates
    for crate_name in &root_crates {
        all_crates.extend(get_crate_deps(crate_name, &manifest_map)?);
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
