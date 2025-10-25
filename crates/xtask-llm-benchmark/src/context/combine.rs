use crate::context::paths::resolve_mode_paths;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub fn build_context(mode: &str) -> Result<String> {
    if mode == "rustdoc_json" {
        return build_context_from_rustdoc_json();
    }

    let files = resolve_mode_paths(mode)?;
    let mut out = String::with_capacity(1024 * 1024);
    for p in files {
        let rel = rel_display(&p);
        let contents = fs::read_to_string(&p).with_context(|| format!("read {}", rel))?;
        out.push_str("\n\n---\n");
        out.push_str(&format!("// file: {}\n\n", rel));
        out.push_str(&contents);
    }
    Ok(out)
}

fn build_context_from_rustdoc_json() -> Result<String> {
    let files = resolve_mode_paths("rustdoc_json")?;
    let json_path = files
        .iter()
        .find(|p| p.file_name().map(|n| n.to_string_lossy()) == Some("spacetimedb.json".into()))
        .cloned()
        .or_else(|| files.first().cloned())
        .ok_or_else(|| anyhow!("rustdoc_json: no files found"))?;

    let rel = rel_display(&json_path);
    let v: Value = serde_json::from_str(&fs::read_to_string(&json_path).with_context(|| format!("read {}", rel))?)?;

    let index = v
        .get("index")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("missing index"))?;
    let paths = v
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("missing paths"))?;
    let root = v.get("root").map(|x| x.to_string()).unwrap_or_default();
    let crate_name = index
        .get(&root)
        .and_then(|it| it.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("spacetimedb")
        .to_string();

    let crate_version = v.get("crate_version").and_then(Value::as_str).unwrap_or("unknown");

    let mut rows = Vec::<Row>::new();
    for (id, item) in index {
        let id_s = id.as_str();
        if !belongs_to_crate(paths, id_s, &crate_name) {
            continue;
        }
        let docs = item_docs(item);
        if docs.trim().is_empty() {
            continue;
        }
        let kind = item_kind(item);
        let path = full_path(paths, index, id_s, &crate_name);
        // let name = item.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        rows.push(Row {
            path,
            kind,
            docs: collapse_blanks(docs.trim()),
        });
    }

    rows.sort_by(|a, b| (order_key(&a.kind), a.path.to_lowercase()).cmp(&(order_key(&b.kind), b.path.to_lowercase())));

    let mut out = String::with_capacity(1024 * 1024);
    out.push_str(&format!(
        "# {} — Flattened Docs (rustdoc JSON -> Markdown)\n",
        crate_name
    ));
    out.push_str(&format!(
        "_Items with docs only. Source: rustdoc JSON crate_version={}_\n",
        crate_version
    ));

    let mut current_section: Option<&'static str> = None;
    for r in rows {
        let sec = section_for(&r.kind);
        if current_section != Some(sec) {
            current_section = Some(sec);
            out.push_str(&format!("\n## {}\n", sec));
        }
        out.push_str(&format!("### `{}`  — _{}_\n\n", r.path, r.kind));
        out.push_str(&r.docs);
        out.push_str("\n\n");
    }

    Ok(out)
}

#[derive(Debug)]
struct Row {
    path: String,
    kind: String,
    docs: String,
}

fn belongs_to_crate(paths: &serde_json::Map<String, Value>, id: &str, crate_name: &str) -> bool {
    if let Some(p) = paths.get(id).and_then(Value::as_object) {
        if let Some(arr) = p.get("path").and_then(Value::as_array) {
            return arr.get(0).and_then(Value::as_str) == Some(crate_name);
        }
    }
    false
}

fn full_path(
    paths: &serde_json::Map<String, Value>,
    index: &serde_json::Map<String, Value>,
    id: &str,
    crate_name: &str,
) -> String {
    if let Some(p) = paths.get(id).and_then(Value::as_object) {
        if let Some(arr) = p.get("path").and_then(Value::as_array) {
            let mut segs: Vec<String> = arr.iter().filter_map(Value::as_str).map(|s| s.to_string()).collect();
            if let Some(name) = index.get(id).and_then(|it| it.get("name")).and_then(Value::as_str) {
                segs.push(name.to_string());
            }
            return segs.join("::");
        }
    }
    let nm = index
        .get(id)
        .and_then(|it| it.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("<unnamed>");
    format!("{}::{}", crate_name, nm)
}

fn item_kind(item: &Value) -> String {
    if let Some(k) = item.get("kind").and_then(Value::as_str) {
        return k.to_string();
    }
    if let Some(inner) = item.get("inner").and_then(Value::as_object) {
        if let Some((k, _)) = inner.iter().next() {
            return k.to_string();
        }
    }
    "unknown".to_string()
}

fn item_docs(item: &Value) -> String {
    if let Some(d) = item.get("docs").and_then(Value::as_str) {
        return d.to_string();
    }
    if let Some(inner) = item.get("inner").and_then(Value::as_object) {
        for (_k, v) in inner {
            if let Some(m) = v.as_object() {
                if let Some(d) = m.get("docs").and_then(Value::as_str) {
                    return d.to_string();
                }
            }
        }
    }
    String::new()
}

fn section_for(kind: &str) -> &'static str {
    match kind {
        "module" => "Modules",
        "struct" | "enum" | "union" | "type_alias" => "Types",
        "trait" | "trait_alias" => "Traits",
        "function" | "method" => "Functions",
        "macro" => "Macros",
        "constant" | "static" => "Constants",
        _ => "Other",
    }
}

fn order_key(kind: &str) -> i32 {
    match kind {
        "module" => 0,
        "struct" | "enum" | "union" | "type_alias" => 1,
        "trait" | "trait_alias" => 2,
        "function" | "method" => 3,
        "macro" => 4,
        "constant" | "static" => 5,
        _ => 99,
    }
}

fn rel_display(p: &PathBuf) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}

fn collapse_blanks(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_blank = false;
    for line in s.lines() {
        let is_blank = line.trim().is_empty();
        if is_blank && prev_blank {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        prev_blank = is_blank;
    }
    out.trim_end().to_string()
}
