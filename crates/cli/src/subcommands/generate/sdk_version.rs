//! Client SDK requirements for generated bindings. Each language supplies its
//! version detector; project discovery and diagnostics stay outside codegen.
use super::Language;
use anyhow::Context;
use colored::Colorize;
use semver::Version;
use serde_json::Value;
use std::path::{Path, PathBuf};

struct SdkRequirement {
    name: &'static str,
    minimum: Version,
    detect: fn(&Path) -> anyhow::Result<DetectedSdk>,
}

struct DetectedSdk {
    version: Option<Version>,
    detail: String,
}

fn requirement(language: Language) -> Option<SdkRequirement> {
    match language {
        Language::TypeScript => Some(SdkRequirement {
            name: "TypeScript spacetimedb",
            minimum: Version::new(2, 16, 0),
            detect: typescript_sdk,
        }),
        _ => None,
    }
}

pub(super) fn warn_if_incompatible(language: Language, output: &Path) {
    if let Some(message) = warning(language, output) {
        eprintln!(
            "\n{}\n{message}\n",
            "WARNING: Check your client SDK before using these bindings"
                .yellow()
                .bold()
        );
    }
}

fn warning(language: Language, output: &Path) -> Option<String> {
    let requirement = requirement(language)?;
    let detected = match (requirement.detect)(output) {
        Ok(detected) => detected,
        Err(error) => DetectedSdk {
            version: None,
            detail: format!("could not inspect the target project: {error:#}"),
        },
    };
    if detected.version.as_ref().is_some_and(|v| v >= &requirement.minimum) {
        return None;
    }
    let action = if detected.version.is_some() {
        "Upgrade your client project's SDK before using these bindings."
    } else {
        "Could not determine the SDK version in use. Install or upgrade your client project's SDK before using these bindings."
    };
    Some(format!(
        "Generated bindings in '{}' require {} SDK {} or later.\nDetected SDK: {}.\n{}",
        output.display(),
        requirement.name,
        requirement.minimum,
        detected.detail,
        action
    ))
}

fn read_package(path: &Path) -> anyhow::Result<Value> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn package_version(package: &Value, path: &Path) -> anyhow::Result<DetectedSdk> {
    let raw = package["version"].as_str().context("SDK package has no version")?;
    Ok(DetectedSdk {
        version: Some(Version::parse(raw).context("invalid SDK package version")?),
        detail: format!("found {raw} in '{}'", path.display()),
    })
}

fn typescript_sdk(output: &Path) -> anyhow::Result<DetectedSdk> {
    let absolute = if output.is_absolute() {
        output.to_owned()
    } else {
        std::env::current_dir()?.join(output)
    };
    // The output can be a directory that has not been created yet. Resolve the
    // client package from it, rather than inspecting the server's module path.
    let mut project: Option<(PathBuf, Value)> = None;
    for dir in absolute.ancestors() {
        let path = dir.join("package.json");
        if path.is_file() {
            let package = read_package(&path)?;
            if package["name"] == "spacetimedb" {
                return package_version(&package, &path);
            }
            project = Some((path, package));
            break;
        }
    }
    let Some((manifest, package)) = project else {
        return Ok(DetectedSdk {
            version: None,
            detail: "no client package.json found".into(),
        });
    };
    // npm, Yarn node_modules, and pnpm symlinks all resolve this way. Continue
    // through workspace ancestors for hoisted dependencies.
    for dir in manifest.parent().unwrap().ancestors() {
        let path = dir.join("node_modules/spacetimedb/package.json");
        if path.is_file() {
            return package_version(&read_package(&path)?, &path);
        }
    }
    let declared = [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .into_iter()
    .find_map(|section| package[section]["spacetimedb"].as_str());
    Ok(DetectedSdk {
        // A range, tag, workspace link, or git dependency does not tell us the
        // resolved version. Do not claim it is safe based on its lower bound.
        version: declared.and_then(|v| Version::parse(v).ok()),
        detail: match declared {
            Some(version) => format!(
                "'{}' declares spacetimedb '{version}', with no installed SDK found",
                manifest.display()
            ),
            None => format!("no spacetimedb dependency found in '{}'", manifest.display()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, value: Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value.to_string()).unwrap();
    }

    #[test]
    fn checks_target_project_and_hoisted_installed_version() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("clients/web");
        write(
            &target.join("package.json"),
            serde_json::json!({"dependencies":{"spacetimedb":"^2.15.0"}}),
        );
        let installed = root.path().join("node_modules/spacetimedb/package.json");
        for (version, warns) in [
            ("2.13.9", true),
            ("2.16.0-rc.1", true),
            ("2.16.0", false),
            ("3.0.0", false),
        ] {
            write(&installed, serde_json::json!({"name":"spacetimedb", "version":version}));
            let message = warning(Language::TypeScript, &target.join("src/not-created/bindings"));
            assert_eq!(message.is_some(), warns, "{version}");
            if let Some(message) = message {
                assert!(message.contains(version));
                assert!(message.contains("2.16.0 or later"));
                assert!(message.contains("Upgrade"));
            }
        }
        // Prefer the target's local dependency over a hoisted copy.
        write(
            &target.join("node_modules/spacetimedb/package.json"),
            serde_json::json!({"version":"2.15.0"}),
        );
        assert!(warning(Language::TypeScript, &target.join("src/bindings")).is_some());
    }

    #[test]
    fn warns_for_unknown_versions_without_blocking_generation() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("package.json");
        write(
            &manifest,
            serde_json::json!({"dependencies":{"spacetimedb":"workspace:*"}}),
        );
        let message = warning(Language::TypeScript, &root.path().join("src/bindings")).unwrap();
        assert!(message.contains("Could not determine"));
        assert!(message.contains("workspace:*"));
        std::fs::write(&manifest, "invalid json").unwrap();
        assert!(warning(Language::TypeScript, root.path())
            .unwrap()
            .contains("could not inspect"));
        std::fs::remove_file(&manifest).unwrap();
        assert!(warning(Language::TypeScript, root.path())
            .unwrap()
            .contains("no client package.json"));
    }

    #[test]
    fn handles_exact_manifest_versions_and_leaves_other_languages_alone() {
        let root = tempfile::tempdir().unwrap();
        for (version, warns) in [("2.15.0", true), ("2.16.0", false), ("^2.16.0", true)] {
            write(
                &root.path().join("package.json"),
                serde_json::json!({"devDependencies":{"spacetimedb":version}}),
            );
            assert_eq!(warning(Language::TypeScript, root.path()).is_some(), warns);
        }
        assert!(warning(Language::Rust, root.path()).is_none());
        assert!(warning(Language::Csharp, root.path()).is_none());
    }
}
