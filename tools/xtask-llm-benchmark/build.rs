#![allow(clippy::disallowed_macros)]

use std::{
    env, fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
};

fn main() {
    // === Paths ===
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let benches_root = manifest_dir.join("src/benchmarks");

    let gen_dir = manifest_dir.join("src/generated");
    let registry_rs = gen_dir.join("registry.rs");

    fs::create_dir_all(&gen_dir).unwrap();

    // We'll gather generated module blocks + match arms
    let mut mods_src = String::new();
    let mut arms_src = String::new();

    // Track whether we actually saw anything. If we saw nothing,
    // that's almost always a wrong-path / didn't put build.rs in right crate problem.
    let mut found_any = false;

    // Walk: src/benchmarks/<category>/<task>/spec.rs
    for cat_entry in read_dir_sorted(&benches_root) {
        let cat_entry = cat_entry.unwrap();
        let cat_path = cat_entry.path();
        if !cat_path.is_dir() {
            continue;
        }
        let category = file_name_string(&cat_path);

        for task_entry in read_dir_sorted(&cat_path) {
            let task_entry = task_entry.unwrap();
            let task_path = task_entry.path();
            if !task_path.is_dir() {
                continue;
            }
            let task = file_name_string(&task_path);

            let spec_path = task_path.join("spec.rs");
            if !spec_path.is_file() {
                continue;
            }

            found_any = true;

            // ex: basics_t_005_update
            let mod_ident = format_ident(&category, &task);

            // registry.rs (we are generating) → ../../benchmarks/.../spec.rs (relative include path)
            let rel_spec_path = relative_path(&registry_rs, &spec_path);

            // inline submodule
            mods_src.push_str(&format!(
                "#[allow(dead_code)]\n#[allow(clippy::all)]\nmod {mod_ident} {{\n    include!(\"{rel_spec_path}\");\n}}\n\n"
            ));

            // map ("category","task") → that module's spec() fn
            arms_src.push_str(&format!("        (\"{category}\", \"{task}\") => {mod_ident}::spec,\n"));
        }
    }

    if !found_any {
        // Fail fast instead of silently letting the stub compile.
        panic!(
            "build.rs: did not find any benchmark specs under {:?}.
This usually means one of two things:
1) The benchmarks actually live somewhere else (path mismatch).
2) build.rs is not in the same crate root as the code you're compiling, \
   so Cargo is not running this script for that crate.",
            benches_root
        );
    }

    // Build final file string
    let file_contents = format!(
        "use crate::eval::BenchmarkSpec;
use anyhow::{{anyhow, Result}};
use std::path::Path;

{mods_src}pub fn resolve_by_path(task_root: &Path) -> Result<fn() -> BenchmarkSpec> {{
    let task = task_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!(\"missing task name\"))?;
    let category = task_root
        .parent()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()))
        .ok_or_else(|| anyhow!(\"missing category name\"))?;

    let ctor = match (category, task) {{
{arms_src}        _ => return Err(anyhow!(
            \"no spec registered for {{}}/{{}} (need spec.rs)\",
            category,
            task
        )),
    }};

    Ok(ctor)
}}
"
    );

    // Write unformatted first
    fs::write(&registry_rs, file_contents).unwrap();

    // Best-effort: format it so CI/rustfmt is happy
    let _ = Command::new("rustup").args(["component", "add", "rustfmt"]).status();

    let _ = Command::new("rustfmt")
        .arg("--edition")
        .arg("2021")
        .arg(registry_rs.to_string_lossy().to_string())
        .status();
}

/// Deterministic read_dir so output order is stable.
fn read_dir_sorted(dir: &Path) -> Vec<io::Result<fs::DirEntry>> {
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap().collect();
    entries.sort_by_key(|res| {
        res.as_ref()
            .ok()
            .and_then(|e| e.file_name().into_string().ok())
            .unwrap_or_default()
    });
    entries
}

/// Get final path segment as String.
fn file_name_string(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .expect("utf8 dir name")
        .to_string()
}

/// Turn ("basics","t_005_update") into "basics_t_005_update"
/// - lowercase
/// - non [a-z0-9_] → '_'
/// - if first char is digit, prefix '_'
fn format_ident(category: &str, task: &str) -> String {
    fn sanitize(s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect()
    }

    let mut ident = format!("{}_{}", sanitize(category), sanitize(task));
    if ident.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        ident.insert(0, '_');
    }
    ident
}

/// Build a relative path string from `from` file to `to` file,
/// normalized to `/` for portability so `include!` is valid.
fn relative_path(from: &Path, to: &Path) -> String {
    let base_dir = from.parent().expect("registry.rs must have a parent dir");
    let rel = diff_paths(to, base_dir).unwrap_or_else(|| to.to_path_buf());
    rel.to_string_lossy().replace('\\', "/")
}

/// Minimal diff_paths (no extra crate).
fn diff_paths(path: &Path, base: &Path) -> Option<PathBuf> {
    let path_comps: Vec<Component<'_>> = path.components().collect();
    let base_comps: Vec<Component<'_>> = base.components().collect();

    // find shared prefix
    let common_len = path_comps.iter().zip(&base_comps).take_while(|(a, b)| a == b).count();

    // walk back from base
    let mut out = PathBuf::new();
    for _ in base_comps.iter().skip(common_len) {
        out.push("..");
    }

    // then walk forward into path
    for comp in path_comps.iter().skip(common_len) {
        match comp {
            Component::Normal(os) => out.push(os),
            Component::CurDir => out.push("."),
            Component::ParentDir => out.push(".."),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    Some(out)
}
