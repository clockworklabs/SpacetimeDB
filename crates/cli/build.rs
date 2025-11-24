use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Value;

fn main() {
    let git_hash = find_git_hash();
    println!("cargo:rustc-env=GIT_HASH={git_hash}");

    generate_template_files();
}

fn nix_injected_commit_hash() -> Option<String> {
    use std::env::VarError;
    // Our flake.nix sets this environment variable to be our git commit hash during the build.
    // This is important because git metadata is otherwise not available within the nix build sandbox,
    // and we don't install the git command-line tool in our build.
    match std::env::var("SPACETIMEDB_NIX_BUILD_GIT_COMMIT") {
        Ok(commit_sha) => {
            // Var is set, we're building under Nix.
            Some(commit_sha)
        }

        Err(VarError::NotPresent) => {
            // Var is not set, we're not in Nix.
            None
        }
        Err(VarError::NotUnicode(gross)) => {
            // Var is set but is invalid unicode, something is very wrong.
            panic!("Injected commit hash is not valid unicode: {gross:?}")
        }
    }
}

fn is_nix_build() -> bool {
    nix_injected_commit_hash().is_some()
}

fn find_git_hash() -> String {
    nix_injected_commit_hash().unwrap_or_else(|| {
        // When we're *not* building in Nix, we can assume that git metadata is still present in the filesystem,
        // and that the git command-line tool is installed.
        let output = Command::new("git").args(["rev-parse", "HEAD"]).output().unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    })
}

fn get_manifest_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
}

// This method generates functions with data used in `spacetime init`:
//
//   * `get_templates_json` - returns contents of the JSON file with the list of templates
//   * `get_template_files` - returns a HashMap with templates contents based on the
//                            templates list at crates/cli/templates/templates-list.json
//   * `get_cursorrules` - returns contents of a cursorrules file
fn generate_template_files() {
    let manifest_dir = get_manifest_dir();
    let manifest_path = Path::new(&manifest_dir);
    let templates_json_path = manifest_path.join("templates/templates-list.json");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("embedded_templates.rs");

    println!("cargo:rerun-if-changed=templates/templates-list.json");

    let templates_json =
        fs::read_to_string(&templates_json_path).expect("Failed to read templates/templates-list.json");

    let templates: serde_json::Value =
        serde_json::from_str(&templates_json).expect("Failed to parse templates/templates-list.json");

    let mut generated_code = String::new();
    generated_code.push_str("use std::collections::HashMap;\n\n");

    generated_code.push_str("pub fn get_templates_json() -> &'static str {\n");
    generated_code
        .push_str("    include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/templates/templates-list.json\"))\n");
    generated_code.push_str("}\n\n");

    generated_code
        .push_str("pub fn get_template_files() -> HashMap<&'static str, HashMap<&'static str, &'static str>> {\n");
    generated_code.push_str("    let mut templates = HashMap::new();\n\n");

    if let Some(template_list) = templates["templates"].as_array() {
        for template in template_list {
            let server_source = template["server_source"].as_str().unwrap();
            let client_source = template["client_source"].as_str().unwrap();

            let server_path = PathBuf::from(server_source);
            let client_path = PathBuf::from(client_source);

            let server_full_path = Path::new(&manifest_dir).join(&server_path);
            let client_full_path = Path::new(&manifest_dir).join(&client_path);

            if server_full_path.exists() {
                generate_template_entry(&mut generated_code, &server_path, server_source, &manifest_dir);
            }

            if client_full_path.exists() {
                generate_template_entry(&mut generated_code, &client_path, client_source, &manifest_dir);
            }
        }
    }

    generated_code.push_str("    templates\n");
    generated_code.push_str("}\n\n");

    let repo_root = get_repo_root();
    let workspace_cargo = repo_root.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", workspace_cargo.display());

    let (workspace_edition, workspace_versions) =
        extract_workspace_metadata(&workspace_cargo).expect("Failed to extract workspace metadata");

    let ts_bindings_package = repo_root.join("crates/bindings-typescript/package.json");
    println!("cargo:rerun-if-changed={}", ts_bindings_package.display());
    let ts_bindings_version =
        extract_ts_bindings_version(&ts_bindings_package).expect("Failed to read TypeScript bindings version");

    let cursorrules_path = repo_root.join("docs/.cursor/rules/spacetimedb.mdc");
    if cursorrules_path.exists() {
        generated_code.push_str("pub fn get_cursorrules() -> &'static str {\n");
        generated_code.push_str("    include_str!(\"");
        generated_code.push_str(&cursorrules_path.to_str().unwrap().replace("\\", "\\\\"));
        generated_code.push_str("\")\n");
        generated_code.push_str("}\n");

        println!("cargo:rerun-if-changed={}", cursorrules_path.display());
    } else {
        panic!("Could not find \"docs/.cursor/rules/spacetimedb.mdc\" file.");
    }

    // Expose workspace metadata so `spacetime init` can rewrite template manifests without hardcoding versions.
    generated_code.push_str("pub fn get_workspace_edition() -> &'static str {\n");
    generated_code.push_str(&format!("    \"{}\"\n", workspace_edition.escape_default()));
    generated_code.push_str("}\n\n");

    generated_code.push_str("pub fn get_workspace_dependency_version(name: &str) -> Option<&'static str> {\n");
    generated_code.push_str("    match name {\n");
    for (name, version) in &workspace_versions {
        generated_code.push_str(&format!(
            "        \"{}\" => Some(\"{}\"),\n",
            name.escape_default(),
            version.escape_default()
        ));
    }
    generated_code.push_str("        _ => None,\n");
    generated_code.push_str("    }\n");
    generated_code.push_str("}\n");

    generated_code.push('\n');
    generated_code.push_str("pub fn get_typescript_bindings_version() -> &'static str {\n");
    generated_code.push_str(&format!("    \"{}\"\n", ts_bindings_version.escape_default()));
    generated_code.push_str("}\n");

    write_if_changed(&dest_path, generated_code.as_bytes()).expect("Failed to write embedded_templates.rs");
}

fn generate_template_entry(code: &mut String, template_path: &Path, source: &str, manifest_dir: &Path) {
    let (git_files, resolved_base) = get_git_tracked_files(template_path, manifest_dir);

    if git_files.is_empty() {
        panic!("Template '{}' has no git-tracked files! Check that the directory exists and contains files tracked by git.", source);
    }

    // Example: /Users/user/SpacetimeDB
    let repo_root = get_repo_root();
    let repo_root_canonical = std::fs::canonicalize(&repo_root).unwrap();
    // Example: /Users/user/SpacetimeDB/crates/cli
    let manifest_canonical = Path::new(manifest_dir).canonicalize().unwrap();
    // Example: crates/cli
    let manifest_rel = manifest_canonical.strip_prefix(&repo_root_canonical).unwrap();

    // Example for inside crate: /Users/user/SpacetimeDB/crates/cli/templates/basic-rust/server
    // Example for outside crate: /Users/user/SpacetimeDB/modules/quickstart-chat
    let resolved_canonical = repo_root.join(&resolved_base).canonicalize().unwrap();

    // If the files are outside of the cli crate we need to copy them to the crate directory,
    // so they're included properly even when the crate is published
    let local_copy_dir = if resolved_canonical.strip_prefix(&manifest_canonical).is_err() {
        // Example source: "../../modules/quickstart-chat"
        // Sanitized: "parent_parent_modules_quickstart-chat"
        let sanitized_source = source.replace("/", "_").replace("\\", "_").replace("..", "parent");
        // Example: /Users/user/SpacetimeDB/crates/cli/.templates/parent_parent_modules_quickstart-chat
        let copy_dir = Path::new(manifest_dir).join(".templates").join(&sanitized_source);
        fs::create_dir_all(&copy_dir).expect("Failed to create .templates directory");

        Some(copy_dir)
    } else {
        None
    };

    code.push_str("    {\n");
    code.push_str("        let mut files = HashMap::new();\n");

    for file_path in git_files {
        // Example file_path: modules/quickstart-chat/src/lib.rs (relative to repo root)
        // Example resolved_base: modules/quickstart-chat
        // Example relative_path: src/lib.rs
        let relative_path = match file_path.strip_prefix(&resolved_base) {
            Ok(p) => p,
            Err(_) => {
                eprintln!(
                    "Warning: Could not strip prefix '{}' from '{}' for source '{}'",
                    resolved_base.display(),
                    file_path.display(),
                    source
                );
                continue;
            }
        };
        // Example: "src/lib.rs"
        let relative_str = relative_path.to_str().unwrap().replace("\\", "/");

        // Example: /Users/user/SpacetimeDB/modules/quickstart-chat/src/lib.rs
        let full_path = repo_root.join(&file_path);
        if full_path.exists() && full_path.is_file() {
            let include_path = if let Some(ref copy_dir) = local_copy_dir {
                // Outside crate: copy to .templates
                // Example dest_file: /Users/user/SpacetimeDB/crates/cli/.templates/parent_parent_modules_quickstart-chat/src/lib.rs
                let dest_file = copy_dir.join(relative_path);
                fs::create_dir_all(dest_file.parent().unwrap()).expect("Failed to create parent directory");
                copy_if_changed(&full_path, &dest_file)
                    .unwrap_or_else(|_| panic!("Failed to copy file {:?} to {:?}", full_path, dest_file));

                // Example relative_to_manifest: .templates/parent_parent_modules_quickstart-chat/src/lib.rs
                let relative_to_manifest = dest_file.strip_prefix(manifest_dir).unwrap();
                let path_str = relative_to_manifest.to_str().unwrap().replace("\\", "/");
                // Watch the original file for changes
                // Example: modules/quickstart-chat/src/lib.rs
                println!("cargo:rerun-if-changed={}", full_path.display());
                path_str
            } else {
                // Inside crate: use path relative to CARGO_MANIFEST_DIR
                // Example file_path: crates/cli/templates/basic-rust/server/src/lib.rs
                // Example manifest_rel: crates/cli
                // Result: templates/basic-rust/server/src/lib.rs
                let relative_to_manifest = file_path.strip_prefix(manifest_rel).unwrap();
                let path_str = relative_to_manifest.to_str().unwrap().replace("\\", "/");
                // Example: crates/cli/templates/basic-rust/server/src/lib.rs
                println!("cargo:rerun-if-changed={}", full_path.display());
                path_str
            };

            // Example include_path (inside crate): "templates/basic-rust/server/src/lib.rs"
            // Example include_path (outside crate): ".templates/parent_parent_modules_quickstart-chat/src/lib.rs"
            // Example relative_str: "src/lib.rs"
            code.push_str(&format!(
                "        files.insert(\"{}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{}\")));\n",
                relative_str, include_path
            ));
        }
    }

    code.push_str(&format!("        templates.insert(\"{}\", files);\n", source));
    code.push_str("    }\n\n");
}

/// Get a list of files tracked by git from a given directory
fn get_git_tracked_files(path: &Path, manifest_dir: &Path) -> (Vec<PathBuf>, PathBuf) {
    if is_nix_build() {
        // When building in Nix, we already know that there are no untracked files in our source tree,
        // so we just list all of the files.
        list_all_files(path, manifest_dir)
    } else {
        // When building outside of Nix, we invoke `git` to list all the tracked files.
        get_git_tracked_files_via_cli(path, manifest_dir)
    }
}

fn list_all_files(path: &Path, manifest_dir: &Path) -> (Vec<PathBuf>, PathBuf) {
    let manifest_dir = manifest_dir.canonicalize().unwrap_or_else(|err| {
        panic!(
            "Failed to canonicalize manifest_dir path {}: {err:#?}",
            manifest_dir.display()
        )
    });

    let template_root_absolute = get_full_path_within_manifest_dir(path, &manifest_dir);

    let repo_root = get_repo_root();

    let mut files = Vec::new();
    ls_recursively(&template_root_absolute, &repo_root, &mut files);

    (files, make_repo_root_relative(&template_root_absolute, &repo_root))
}

/// Get all the paths of files within `root_dir`,
/// transform them into paths relative to `repo_root`,
/// and insert them into `out`.
fn ls_recursively(root_dir: &Path, repo_root: &Path, out: &mut Vec<PathBuf>) {
    for dir_ent in std::fs::read_dir(root_dir).unwrap_or_else(|err| {
        panic!(
            "Failed to read_dir from template directory {}: {err:#?}",
            root_dir.display()
        )
    }) {
        let dir_ent = dir_ent.unwrap_or_else(|err| {
            panic!(
                "Got error during read_dir from template directory {}: {err:#?}",
                root_dir.display(),
            )
        });
        let file_path = dir_ent.path();
        let file_type = dir_ent.file_type().unwrap_or_else(|err| {
            panic!(
                "Failed to get file_type for template file {}: {err:#?}",
                file_path.display(),
            )
        });
        if file_type.is_dir() {
            ls_recursively(&file_path, repo_root, out);
        } else {
            out.push(make_repo_root_relative(&file_path, repo_root));
        }
    }
}

/// Treat `relative_path` as a relative path within `manifest_dir`
/// and transform it into an absolute, canonical path.
fn get_full_path_within_manifest_dir(relative_path: &Path, manifest_dir: &Path) -> PathBuf {
    let full_path = manifest_dir.join(relative_path);

    full_path.canonicalize().unwrap_or_else(|e| {
        panic!("Failed to canonicalize path {}: {}", full_path.display(), e);
    })
}

/// Transform `full_path` into a relative path within `repo_root`.
///
/// `full_path` and `repo_root` should both be canonical paths, as by [`Path::canonicalize`].
fn make_repo_root_relative(full_path: &Path, repo_root: &Path) -> PathBuf {
    full_path
        .strip_prefix(repo_root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| {
            panic!(
                "Path {} is outside repo root {}",
                full_path.display(),
                repo_root.display()
            )
        })
}

fn get_git_tracked_files_via_cli(path: &Path, manifest_dir: &Path) -> (Vec<PathBuf>, PathBuf) {
    let repo_root = get_repo_root();
    let repo_root = repo_root.canonicalize().unwrap_or_else(|err| {
        panic!(
            "Failed to canonicalize repo_root path {}: {err:#?}",
            repo_root.display(),
        )
    });

    let resolved_path = make_repo_root_relative(&get_full_path_within_manifest_dir(path, manifest_dir), &repo_root);

    let output = Command::new("git")
        .args(["ls-files", resolved_path.to_str().unwrap()])
        .current_dir(repo_root)
        .output()
        .expect("Failed to execute git ls-files");

    if !output.status.success() {
        return (Vec::new(), resolved_path);
    }

    let stdout = String::from_utf8(output.stdout).unwrap();
    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();

    (files, resolved_path)
}

fn get_repo_root() -> PathBuf {
    let manifest_dir = get_manifest_dir();
    // Cargo doesn't expose a way to get the workspace root, AFAICT (pgoldman 2025-10-31).
    // We don't want to query git metadata for this, as that will break in Nix builds.
    // We happen to know our own directory structure, so we can just walk the tree to get to the root.
    let repo_root = manifest_dir.join("..").join("..");
    repo_root.canonicalize().unwrap_or_else(|err| {
        panic!(
            "Failed to canonicalize repo_root path {}: {err:#?}",
            repo_root.display()
        )
    })
}

fn extract_workspace_metadata(path: &Path) -> io::Result<(String, BTreeMap<String, String>)> {
    let content = fs::read_to_string(path)?;
    let parsed: Value = content
        .parse()
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    let table = parsed
        .as_table()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "workspace manifest is not a table"))?;

    let workspace = table
        .get("workspace")
        .and_then(Value::as_table)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "workspace section missing"))?;

    let edition = workspace
        .get("package")
        .and_then(Value::as_table)
        .and_then(|pkg| pkg.get("edition"))
        .and_then(Value::as_str)
        .unwrap_or("2021")
        .to_string();

    let mut versions = BTreeMap::new();
    if let Some(deps) = workspace.get("dependencies").and_then(Value::as_table) {
        for (name, value) in deps {
            let version_opt = match value {
                Value::String(s) => Some(normalize_version(s)),
                Value::Table(table) => table.get("version").and_then(Value::as_str).map(normalize_version),
                _ => None,
            };

            if let Some(version) = version_opt {
                versions.insert(name.clone(), version);
            }
        }
    }

    Ok((edition, versions))
}

fn extract_ts_bindings_version(path: &Path) -> io::Result<String> {
    let content = fs::read_to_string(path)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&content).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    parsed
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing \"version\" field in TypeScript bindings package.json",
            )
        })
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('=').to_string()
}

fn write_if_changed(path: &Path, contents: &[u8]) -> io::Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == contents => Ok(()),
        _ => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = fs::File::create(path)?;
            file.write_all(contents)
        }
    }
}

fn copy_if_changed(src: &Path, dst: &Path) -> io::Result<()> {
    let src_bytes = fs::read(src)?;
    if let Ok(existing) = fs::read(dst) {
        if existing == src_bytes {
            return Ok(());
        }
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::File::create(dst)?;
    file.write_all(&src_bytes)
}
