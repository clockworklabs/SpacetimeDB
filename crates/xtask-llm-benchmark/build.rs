use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let benches_root = Path::new("src/benchmarks");
    let out_path = Path::new("src/bench/generated/registry.rs");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let mut mods = String::new();
    let mut arms = String::new();

    for cat in read_dirs_sorted(benches_root) {
        if !cat.is_dir() {
            continue;
        }
        let cat_name = fname(&cat);
        mods.push_str(&format!("pub mod {} {{\n", cat_name));

        for task in read_dirs_sorted(&cat) {
            if !task.is_dir() {
                continue;
            }
            let task_name = fname(&task);
            let spec_abs = manifest_dir
                .join(task.join("spec.rs"))
                .to_string_lossy()
                .replace('\\', "/");

            // include the spec.rs with an absolute, normalized path
            mods.push_str(&format!(
                "    pub mod {} {{ include!(r#\"{}\"#); }}\n",
                task_name, spec_abs
            ));

            // resolver arm points to the modules we just declared in this file
            arms.push_str(&format!(
                "        (\"{}\",\"{}\") => self::{}::{}::spec,\n",
                cat_name, task_name, cat_name, task_name
            ));
        }
        mods.push_str("}\n");
    }

    let code = format!(
        "{mods}\
use crate::eval::BenchmarkSpec;
use anyhow::{{anyhow, Result}};
use std::path::Path;

pub fn resolve_by_path(task_root: &Path) -> Result<fn() -> BenchmarkSpec> {{
    let task = task_root.file_name().and_then(|s| s.to_str()).ok_or_else(|| anyhow!(\"missing task name\"))?;
    let category = task_root.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).ok_or_else(|| anyhow!(\"missing category name\"))?;
    let ctor = match (category, task) {{
{arms}        _ => return Err(anyhow!(\"no spec registered for {{}}/{{}} (need spec.rs)\", category, task)),
    }};
    Ok(ctor)
}}
",
        mods = mods,
        arms = arms
    );

    fs::create_dir_all(out_path.parent().unwrap()).unwrap();
    fs::write(out_path, code).unwrap();
    println!("cargo:rerun-if-changed=src/benchmarks");
}

fn read_dirs_sorted(root: &Path) -> Vec<PathBuf> {
    let mut v: Vec<_> = fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())))
        .collect();
    v.sort();
    v
}

fn fname(p: &Path) -> String {
    p.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string()
}
