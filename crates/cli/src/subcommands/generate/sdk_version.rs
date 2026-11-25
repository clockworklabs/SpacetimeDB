//! SDK requirements apply to the client project containing the output directory.
use super::Language;
use colored::Colorize;
use semver::Version;
use serde_json::Value;
use std::path::{Path, PathBuf};

const CSHARP_MINIMUM: Version = Version::new(2, 17, 0);
const UNITY_PACKAGE: &str = "com.clockworklabs.spacetimedbsdk";

pub(super) fn warn_if_incompatible(language: Language, output: &Path) {
    let warning = match language {
        Language::Csharp => csharp_warning(output, &CSHARP_MINIMUM),
        _ => None,
    };
    if let Some(warning) = warning {
        eprintln!(
            "\n{}\n{warning}\n",
            "WARNING: Check your client SDK before using these bindings"
                .yellow()
                .bold()
        );
    }
}

struct ClientProject {
    path: PathBuf,
    sdk: String,
    version: Option<Version>,
    unity: bool,
}

fn json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn unity_project(dir: &Path) -> Option<ClientProject> {
    let packages = dir.join("Packages");
    let path = packages.join("manifest.json");
    let manifest = json(&path)?;
    let requested = manifest["dependencies"][UNITY_PACKAGE].as_str();
    let local = requested
        .and_then(|s| s.strip_prefix("file:"))
        .map(|s| packages.join(s));
    let package_version = |p: PathBuf| {
        let package = json(&p.join("package.json"))?;
        (package["name"] == UNITY_PACKAGE)
            .then(|| Version::parse(package["version"].as_str()?).ok())
            .flatten()
    };
    let version = package_version(packages.join(UNITY_PACKAGE))
        .or_else(|| local.and_then(package_version))
        .or_else(|| requested.and_then(|s| Version::parse(s).ok()))
        .or_else(|| {
            let lock = json(&packages.join("packages-lock.json"))?;
            let entry = &lock["dependencies"][UNITY_PACKAGE];
            let resolved = entry["version"].as_str()?;
            if let Ok(version) = Version::parse(resolved) {
                return Some(version);
            }
            // A Git hash or branch in the manifest does not identify the package version.
            // Read the version Unity installed for that revision.
            let cached = std::fs::read_dir(dir.join("Library/PackageCache")).ok()?;
            let mut versions = cached
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(&format!("{UNITY_PACKAGE}@"))
                })
                .filter_map(|e| package_version(e.path()));
            let first = versions.next()?;
            versions.all(|v| v == first).then_some(first)
        });
    Some(ClientProject {
        path,
        sdk: UNITY_PACKAGE.into(),
        version,
        unity: true,
    })
}

fn nuget_project(path: &Path) -> Option<ClientProject> {
    let doc = xmltree::Element::parse(std::fs::File::open(path).ok()?).ok()?;
    let references = doc
        .children
        .iter()
        .filter_map(|n| n.as_element())
        .filter(|e| e.name == "ItemGroup")
        .flat_map(|group| group.children.iter().filter_map(|n| n.as_element()))
        .filter(|e| e.name == "PackageReference");
    for item in references {
        let Some(sdk) = item.attributes.get("Include") else {
            continue;
        };
        if !["SpacetimeDB.ClientSDK", "SpacetimeDB.ClientSDK.Godot"]
            .iter()
            .any(|name| sdk.eq_ignore_ascii_case(name))
        {
            continue;
        }
        let declared = item
            .attributes
            .get("Version")
            .cloned()
            .or_else(|| item.get_child("Version")?.get_text().map(|v| v.into_owned()));
        let assets = json(&path.parent()?.join("obj/project.assets.json"));
        let installed = assets.as_ref().and_then(|assets| {
            let owner = assets.pointer("/project/restore/projectPath")?.as_str()?;
            if Path::new(owner).file_name() != path.file_name() {
                return None;
            }
            assets["libraries"].as_object()?.keys().find_map(|key| {
                let (name, version) = key.split_once('/')?;
                name.eq_ignore_ascii_case(sdk)
                    .then(|| Version::parse(version).ok())
                    .flatten()
            })
        });
        return Some(ClientProject {
            path: path.to_owned(),
            sdk: sdk.clone(),
            version: installed.or_else(|| Version::parse(declared.as_deref()?).ok()),
            unity: false,
        });
    }
    None
}

fn csharp_warning(output: &Path, minimum: &Version) -> Option<String> {
    let output = std::env::current_dir().ok()?.join(output);
    for dir in output.ancestors().filter(|p| p.is_dir()) {
        let projects: Vec<_> = if let Some(unity) = unity_project(dir) {
            vec![unity]
        } else {
            let mut paths: Vec<_> = std::fs::read_dir(dir)
                .ok()?
                .flatten()
                .map(|p| p.path())
                .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("csproj")))
                .collect();
            paths.sort();
            paths.iter().filter_map(|p| nuget_project(p)).collect()
        };
        if projects.is_empty() {
            continue;
        }
        let warnings: Vec<_> = projects.into_iter().filter(|p| p.version.as_ref().is_none_or(|v| v < minimum)).map(|p| {
            let project = p.path.display();
            let detected = p.version.map_or_else(|| "unknown".to_owned(), |v| v.to_string());
            let action = if p.unity {
                format!("Update {} in '{}' to an SDK release at {minimum} or later", p.sdk, project)
            } else {
                format!("Run `dotnet add \"{project}\" package {} --version {minimum}`", p.sdk)
            };
            format!("C# bindings require SDK {minimum} or later. Detected version in '{project}': {detected}.\n{action}, then rerun `spacetime generate`.")
        }).collect();
        return (!warnings.is_empty()).then(|| warnings.join("\n"));
    }
    Some(format!("C# bindings require SDK {minimum} or later. Could not find the client SDK for '{}'. Check the SpacetimeDB SDK reference in the client .csproj or Unity Packages/manifest.json, then rerun `spacetime generate`.", output.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warns_for_old_nuget_and_recognizes_restored_version_for_a_range() {
        let root = tempfile::tempdir().unwrap();
        let csproj = root.path().join("Game.csproj");
        let output = root.path().join("not-yet-created/bindings");
        std::fs::write(&csproj, r#"<Project><ItemGroup><PackageReference Include="SpacetimeDB.ClientSDK.Godot" Version="2.16.0"/></ItemGroup></Project>"#).unwrap();
        let warning = csharp_warning(&output, &CSHARP_MINIMUM).unwrap();
        assert!(
            warning.contains("2.16.0")
                && warning.contains("dotnet add")
                && warning.contains("package SpacetimeDB.ClientSDK.Godot")
        );
        std::fs::write(&csproj, r#"<Project><ItemGroup><PackageReference Include="SpacetimeDB.ClientSDK.Godot" Version="2.17.*"/></ItemGroup></Project>"#).unwrap();
        assert!(csharp_warning(&output, &CSHARP_MINIMUM).unwrap().contains("unknown"));
        std::fs::create_dir(root.path().join("obj")).unwrap();
        let assets = serde_json::json!({"project":{"restore":{"projectPath":csproj}}, "libraries":{"SpacetimeDB.ClientSDK.Godot/2.17.1":{}}});
        std::fs::write(root.path().join("obj/project.assets.json"), assets.to_string()).unwrap();
        assert!(csharp_warning(&output, &CSHARP_MINIMUM).is_none());
    }
    #[test]
    fn finds_the_sdk_in_the_unity_manifest() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Packages");
        std::fs::create_dir(&dir).unwrap();
        let manifest = serde_json::json!({"dependencies": { (UNITY_PACKAGE): "2.16.0" }});
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        let warning = csharp_warning(&root.path().join("Assets/Scripts/Bindings"), &CSHARP_MINIMUM).unwrap();
        assert!(warning.contains("manifest.json") && warning.contains(UNITY_PACKAGE) && warning.contains("2.16.0"));
    }
}
