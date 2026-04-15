use std::path::PathBuf;

pub const DOCS_DIR_DEFAULT: &str = "../../docs";
pub const SKILLS_DIR_DEFAULT: &str = "../../skills";
pub const RUSTDOC_CRATE_ROOT_DEFAULT: &str = "../../crates/bindings";

pub const ALL_MODES: &[&str] = &[
    "docs",
    "llms.md",
    "guidelines",
    "rustdoc_json",
    "no_context",
    "none",          // alias for no_context (backward compat)
    "no_guidelines", // alias for no_context (backward compat)
    "search",        // no docs context but web search enabled via OpenRouter :online
];

/// Modes that produce an empty context string (no documentation is injected).
#[inline]
pub fn is_empty_context_mode(mode: &str) -> bool {
    matches!(mode, "no_context" | "none" | "no_guidelines" | "search")
}

#[inline]
pub fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOCS_DIR_DEFAULT)
}

#[inline]
pub fn skills_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SKILLS_DIR_DEFAULT)
}

#[inline]
pub fn rustdoc_crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RUSTDOC_CRATE_ROOT_DEFAULT)
}

#[inline]
pub fn rustdoc_readme_path() -> Option<PathBuf> {
    let root = rustdoc_crate_root();
    for name in ["README.md", "Readme.md", "readme.md"] {
        let p = root.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}
