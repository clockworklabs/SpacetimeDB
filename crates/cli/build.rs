use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output().unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
    println!("cargo:rustc-env=GIT_HASH={git_hash}");

    generate_template_files();
}

// This method generates functions with data used in `spacetime init`:
//
//   * `get_templates_json` - returns contents of the JSON file with the list of templates
//   * `get_template_files` - returns a HashMap with templates contents based on the
//                            templates list at crates/cli/templates/templates-list.json
//   * `get_cursorrules` - returns contents of a cursorrules file
fn generate_template_files() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let templates_json_path = Path::new(&manifest_dir).join("templates/templates-list.json");
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
    let cursorrules_path = repo_root.join("docs/.cursor/rules/spacetimedb.md");
    if cursorrules_path.exists() {
        generated_code.push_str("pub fn get_cursorrules() -> &'static str {\n");
        generated_code.push_str("    include_str!(\"");
        generated_code.push_str(&cursorrules_path.to_str().unwrap().replace("\\", "\\\\"));
        generated_code.push_str("\")\n");
        generated_code.push_str("}\n");

        let cursorrules_relative = cursorrules_path.strip_prefix(&repo_root).unwrap();
        println!("cargo:rerun-if-changed={}", cursorrules_relative.display());
    } else {
        panic!("Could not find \"docs/.cursor/rules/spacetimedb.md\" file.");
    }

    fs::write(dest_path, generated_code).expect("Failed to write embedded_templates.rs");
}

fn generate_template_entry(code: &mut String, template_path: &Path, source: &str, manifest_dir: &str) {
    let (git_files, resolved_base) = get_git_tracked_files(template_path, manifest_dir);

    if git_files.is_empty() {
        panic!("Template '{}' has no git-tracked files! Check that the directory exists and contains files tracked by git.", source);
    }

    let repo_root = get_repo_root();

    code.push_str("    {\n");
    code.push_str("        let mut files = HashMap::new();\n");

    for file_path in git_files {
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
        let relative_str = relative_path.to_str().unwrap().replace("\\", "/");

        let full_path = repo_root.join(&file_path);
        if full_path.exists() && full_path.is_file() {
            let file_path_str = file_path.to_str().unwrap().replace("\\", "/");
            code.push_str(&format!(
                "        files.insert(\"{}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../{}\")));\n",
                relative_str, file_path_str
            ));

            println!("cargo:rerun-if-changed=../../{}", file_path_str);
        }
    }

    code.push_str(&format!("        templates.insert(\"{}\", files);\n", source));
    code.push_str("    }\n\n");
}

// Get a list of files tracked by git from a given directory
fn get_git_tracked_files(path: &Path, manifest_dir: &str) -> (Vec<PathBuf>, PathBuf) {
    let full_path = Path::new(manifest_dir).join(path);

    let repo_root = get_repo_root();
    let manifest_canonical = Path::new(manifest_dir).canonicalize().unwrap();
    let repo_canonical = repo_root.canonicalize().unwrap();
    let manifest_rel = manifest_canonical.strip_prefix(&repo_canonical).unwrap();

    let resolved_path = if full_path.is_symlink() {
        match fs::read_link(&full_path) {
            Ok(target) => {
                let abs_target = if target.is_absolute() {
                    target
                } else {
                    // we need to resolve the symlink path, full_path.parent()
                    // is a directory containing the symlink
                    full_path.parent().unwrap().join(target)
                };
                let canonical = abs_target.canonicalize().unwrap_or(abs_target);
                canonical
                    .strip_prefix(&repo_canonical)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|_| {
                        panic!(
                            "Symlink target {} is outside repo root {}",
                            canonical.display(),
                            repo_canonical.display()
                        )
                    })
            }
            Err(e) => {
                panic!("Failed to read symlink {}: {}", full_path.display(), e);
            }
        }
    } else {
        manifest_rel.join(path)
    };

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
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .expect("Failed to get git repo root");
    let path = String::from_utf8(output.stdout).unwrap().trim().to_string();
    PathBuf::from(path)
}
