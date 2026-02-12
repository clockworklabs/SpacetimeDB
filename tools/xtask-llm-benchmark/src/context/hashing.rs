use anyhow::{bail, Context, Result};
use blake3::Hasher;
use std::borrow::Cow;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::context::combine::build_context;
use crate::context::constants::docs_dir;
use crate::context::{resolve_mode_paths_hashing, rustdoc_crate_root};
use crate::eval::Lang;

// --- compute: stable rel path + normalized file bytes ---
pub fn compute_context_hash(mode: &str) -> Result<String> {
    let mut hasher = Hasher::new();
    let mut files: Vec<PathBuf> = resolve_mode_paths_hashing(mode)?;
    files.sort();

    // stable base used to relativize paths
    let base = base_for_mode_hashing(mode)?;

    for path in files {
        // stable relative path with forward slashes
        let rel = stable_rel_for_hash(&base, &path);
        hasher.update(rel.as_bytes());

        // normalize line endings so Git's autocrlf doesn't flip the hash
        let mut f = fs::File::open(&path).with_context(|| format!("open {}", rel))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).with_context(|| format!("read {}", rel))?;
        let norm = normalize_lf(&buf);
        hasher.update(&norm);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Compute hash of the processed context (after language-specific tab filtering).
/// This ensures each lang/mode combination gets its own unique hash.
pub fn compute_processed_context_hash(mode: &str, lang: Lang) -> Result<String> {
    let context = build_context(mode, Some(lang))?;
    let mut hasher = Hasher::new();
    // Normalize line endings for deterministic hash across OS/checkouts
    let normalized = normalize_lf(context.as_bytes());
    hasher.update(&normalized);
    Ok(hasher.finalize().to_hex().to_string())
}

// --- stable base for stripping prefixes ---
fn base_for_mode_hashing(mode: &str) -> Result<PathBuf> {
    Ok(match mode {
        "docs" | "llms.md" | "cursor_rules" => docs_dir(),
        "rustdoc_json" => rustdoc_crate_root(),
        _ => bail!("unknown mode `{mode}`"),
    })
}

// produce a consistent, forward-slash relative path for hashing
fn stable_rel_for_hash(base: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(base).unwrap_or(p);
    let s = rel.to_string_lossy();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}

// normalize CRLF -> LF for deterministic bytes across OS/checkouts
fn normalize_lf(bytes: &[u8]) -> Cow<'_, [u8]> {
    // fast path: if there is no '\r', keep as-is
    if !bytes.contains(&b'\r') {
        return Cow::Borrowed(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            out.push(b'\n');
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Cow::Owned(out)
}

pub fn gather_docs_files() -> Result<Vec<PathBuf>> {
    let base = docs_dir().join("docs");
    let mut out = Vec::new();
    recurse_dir(&base, &mut out)?;
    out.retain(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("md" | "mdc")));
    out.sort();
    Ok(out)
}

fn recurse_dir(dir: &Path, acc: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            recurse_dir(&path, acc)?;
        } else if ft.is_file() {
            acc.push(path);
        }
    }
    Ok(())
}
