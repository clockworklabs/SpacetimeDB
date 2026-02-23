use crate::bench::utils::work_server_dir_scoped;
use anyhow::{bail, Context, Result};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub fn materialize_project(
    lang: &str,
    category: &str,
    task: &str,
    phase: &str,
    route_tag: &str,
    llm_code: &str,
) -> Result<PathBuf> {
    let out = work_server_dir_scoped(category, task, lang, phase, route_tag);
    let src = tmpl_root().join(match lang {
        "rust" => "rust/server",
        "csharp" => "csharp/server",
        "typescript" => "typescript/server",
        _ => bail!("unsupported lang `{}`", lang),
    });

    if out.exists() {
        let _ = fs::remove_dir_all(&out);
    }
    fs::create_dir_all(&out)?;
    copy_tree_with_templates(&src, &out)?;

    match lang {
        "rust" => inject_rust(&out, llm_code)?,
        "csharp" => inject_csharp(&out, llm_code)?,
        "typescript" => inject_typescript(&out, llm_code)?,
        _ => {}
    }

    Ok(out)
}

/* helpers */

fn tmpl_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join("templates")
}

/// Workspace root (public/) for local SDK paths.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask-llm-benchmark is under public/tools/xtask-llm-benchmark")
        .to_path_buf()
}

/// Relative path from materialized root to a workspace subpath (e.g. "crates/bindings").
/// Avoids Windows canonical paths (//?/D:/...) which can break Cargo/MSBuild/pnpm.
fn relative_to_workspace(root: &Path, ws_subpath: &str) -> Result<String> {
    let ws = workspace_root()
        .canonicalize()
        .with_context(|| "workspace root not found")?;
    let root_canon = root
        .canonicalize()
        .with_context(|| format!("materialized root not found: {}", root.display()))?;
    let root_rel = root_canon
        .strip_prefix(&ws)
        .with_context(|| format!("materialized dir {:?} not under workspace {:?}", root_canon, ws))?;
    let ups = root_rel.components().count();
    Ok(std::iter::repeat_n("..", ups).collect::<Vec<_>>().join("/") + "/" + ws_subpath)
}

fn copy_tree_with_templates(src: &Path, dst: &Path) -> Result<()> {
    fn recurse(from: &Path, to: &Path) -> Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let p = entry.path();
            let rel = p.strip_prefix(from)?;
            let out_path = to.join(rel);
            if entry.file_type()?.is_dir() {
                recurse(&p, &out_path)?;
            } else if out_path.extension().and_then(|e| e.to_str()) == Some("tmpl") {
                let rendered_path = out_path.with_extension("");
                let s = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
                let s = replace_placeholders(&s);
                if let Some(dir) = rendered_path.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(&rendered_path, s).with_context(|| format!("write {}", rendered_path.display()))?;
            } else {
                if let Some(dir) = out_path.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::copy(&p, &out_path)
                    .map(|_| ())
                    .with_context(|| format!("copy {} -> {}", p.display(), out_path.display()))?;
            }
        }
        Ok(())
    }
    if !src.exists() {
        bail!("missing template dir {}", src.display());
    }
    recurse(src, dst)
}

fn replace_placeholders(s: &str) -> String {
    let sdk = env::var("SPACETIME_SDK_VERSION").unwrap_or_else(|_| "1.5.0".into());
    s.replace("{SPACETIME_SDK_VERSION}", &sdk)
}

fn inject_rust(root: &Path, llm_code: &str) -> anyhow::Result<()> {
    let lib = root.join("src/lib.rs");
    ensure_parent(&lib)?;
    let mut contents = fs::read_to_string(&lib).unwrap_or_default();
    let marker = "/*__LLM_CODE__*/";
    let cleaned = normalize_source(llm_code);

    if let Some(idx) = contents.find(marker) {
        contents.replace_range(idx..idx + marker.len(), &cleaned);
    } else {
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(&cleaned);
    }
    fs::write(&lib, contents).with_context(|| format!("write {}", lib.display()))?;

    let relative = relative_to_workspace(root, "crates/bindings")?;
    let sdk_path = workspace_root().join("crates/bindings");
    if !sdk_path.is_dir() {
        bail!("local Rust SDK not found at {}", sdk_path.display());
    }
    let replacement = format!(r#"{{ path = "{}" }}"#, relative);
    let cargo_toml = root.join("Cargo.toml");
    let mut toml = fs::read_to_string(&cargo_toml).with_context(|| format!("read {}", cargo_toml.display()))?;
    toml = toml.replace("{SPACETIME_RUST_SDK_PATH}", &replacement);
    fs::write(&cargo_toml, toml).with_context(|| format!("write {}", cargo_toml.display()))?;
    Ok(())
}

fn inject_csharp(root: &Path, llm_code: &str) -> anyhow::Result<()> {
    let prog = root.join("Lib.cs");
    ensure_parent(&prog)?;
    let mut contents = fs::read_to_string(&prog).unwrap_or_default();
    let marker = "//__LLM_CODE__";
    let cleaned = normalize_source(llm_code);

    if let Some(idx) = contents.find(marker) {
        contents.replace_range(idx..idx + marker.len(), &cleaned);
    } else {
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(&cleaned);
    }
    fs::write(&prog, contents).with_context(|| format!("write {}", prog.display()))?;

    let base_rel = relative_to_workspace(root, "crates/bindings-csharp")?;
    let runtime_csproj = workspace_root().join("crates/bindings-csharp/Runtime/Runtime.csproj");
    if !runtime_csproj.is_file() {
        bail!("local C# Runtime not found at {}", runtime_csproj.display());
    }
    let runtime_ref = format!("{}/Runtime/Runtime.csproj", base_rel);
    let runtime_dir = format!("{}/Runtime", base_rel);
    let codegen_ref = format!("{}/Codegen/Codegen.csproj", base_rel);
    let csproj_path = root.join("StdbModule.csproj");
    let mut csproj = fs::read_to_string(&csproj_path).with_context(|| format!("read {}", csproj_path.display()))?;
    csproj = csproj.replace("{SPACETIME_CSHARP_RUNTIME_DIR}", &runtime_dir);
    csproj = csproj.replace("{SPACETIME_CSHARP_RUNTIME_REF}", &runtime_ref);
    csproj = csproj.replace("{SPACETIME_CSHARP_CODEGEN_REF}", &codegen_ref);
    fs::write(&csproj_path, csproj).with_context(|| format!("write {}", csproj_path.display()))?;
    Ok(())
}

fn inject_typescript(root: &Path, llm_code: &str) -> anyhow::Result<()> {
    let lib = root.join("src/index.ts");
    ensure_parent(&lib)?;
    let mut contents = fs::read_to_string(&lib).unwrap_or_default();
    let marker = "/*__LLM_CODE__*/";
    let cleaned = normalize_source(llm_code);

    if let Some(idx) = contents.find(marker) {
        contents.replace_range(idx..idx + marker.len(), &cleaned);
    } else {
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(&cleaned);
    }
    fs::write(&lib, contents).with_context(|| format!("write {}", lib.display()))?;

    let relative = relative_to_workspace(root, "crates/bindings-typescript")?;
    let sdk_path = workspace_root().join("crates/bindings-typescript");
    if !sdk_path.is_dir() {
        bail!("local TypeScript SDK not found at {}", sdk_path.display());
    }
    let dist_server = sdk_path.join("dist/server/index.mjs");
    if !dist_server.is_file() {
        bail!(
            "local TypeScript SDK at {} is not built (missing dist/server). Run: pnpm build (in crates/bindings-typescript)",
            sdk_path.display()
        );
    }
    let replacement = format!("file:{}", relative);
    let package_json = root.join("package.json");
    let mut pkg = fs::read_to_string(&package_json).with_context(|| format!("read {}", package_json.display()))?;
    pkg = pkg.replace("{SPACETIME_TS_SDK_REF}", &replacement);
    fs::write(&package_json, pkg).with_context(|| format!("write {}", package_json.display()))?;
    Ok(())
}

/// Remove leading/trailing Markdown fences like ```rust ... ``` or ~~~
/// Keeps the inner text intact. Always returns an owned String.
fn strip_code_fences(input: &str) -> String {
    let t = input.trim();
    if !(t.starts_with("```") || t.starts_with("~~~")) {
        return t.to_owned();
    }
    // split once on the first newline to skip the opening fence (and optional lang tag)
    let mut lines = t.lines();
    let _first = lines.next(); // opening fence
    let body = lines.collect::<Vec<_>>().join("\n");
    // trim a trailing closing fence if present
    let trimmed = body.trim_end();
    let trimmed = trimmed
        .strip_suffix("```")
        .or_else(|| trimmed.strip_suffix("~~~"))
        .unwrap_or(trimmed);
    trimmed.trim().to_owned()
}

fn normalize_source(input: &str) -> String {
    let mut out = strip_code_fences(input).replace("\r\n", "\n");
    out = out.trim_end().to_string();
    out.push('\n');
    out
}

fn ensure_parent(p: &Path) -> io::Result<()> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}
