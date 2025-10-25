use std::{
    env, fs, io,
    path::{Component, Path, PathBuf},
};

fn main() {
    // crate root
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // where we read benchmark specs from
    let benches_root = manifest_dir.join("src/benchmarks");

    // where we write the generated code
    let gen_dir = manifest_dir.join("src/generated");
    let registry_rs = gen_dir.join("registry.rs");

    fs::create_dir_all(&gen_dir).unwrap();

    // chunks we build
    let mut mods_src = String::new();
    let mut arms_src = String::new();

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

            // module identifier e.g. basics_t_005_update
            let mod_ident = format_ident(&category, &task);

            // relative include path from src/generated/registry.rs → that spec.rs
            let rel_spec_path = relative_path(&registry_rs, &spec_path);

            // emit the inline module that includes that spec.rs file
            mods_src.push_str(&format!(
                "mod {mod_ident} {{\n    include!(\"{rel_spec_path}\");\n}}\n\n"
            ));

            // emit the match arm calling spec(), not build_spec()
            arms_src.push_str(&format!("        (\"{category}\", \"{task}\") => {mod_ident}::spec,\n"));
        }
    }

    // write src/generated/registry.rs
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

    fs::write(&registry_rs, file_contents).unwrap();
}

/// deterministic read_dir
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

/// last path segment as String
fn file_name_string(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .expect("utf8 dir name")
        .to_string()
}

/// make a valid Rust ident like basics_t_005_update
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

/// build a relative path string from `from` file to `to` file
fn relative_path(from: &Path, to: &Path) -> String {
    let base_dir = from.parent().expect("registry.rs must have a parent dir");
    let rel = diff_paths(to, base_dir).unwrap_or_else(|| to.to_path_buf());

    rel.to_string_lossy().replace('\\', "/")
}

/// minimal diff_paths so we don't add a dep
fn diff_paths(path: &Path, base: &Path) -> Option<PathBuf> {
    let path_comps: Vec<Component<'_>> = path.components().collect();
    let base_comps: Vec<Component<'_>> = base.components().collect();

    let common_len = path_comps.iter().zip(&base_comps).take_while(|(a, b)| a == b).count();

    let mut result = PathBuf::new();
    for _ in base_comps.iter().skip(common_len) {
        result.push("..");
    }

    for comp in path_comps.iter().skip(common_len) {
        match comp {
            Component::Normal(os) => result.push(os),
            Component::CurDir => result.push("."),
            Component::ParentDir => result.push(".."),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    Some(result)
}
