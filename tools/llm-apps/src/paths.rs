//! Locations inside the SpacetimeDB checkout this binary was built from.

use std::path::{Path, PathBuf};

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `crate_dir()/relative`, with `..` resolved so printed paths read cleanly.
fn resolve(relative: &str) -> PathBuf {
    let path = crate_dir().join(relative);
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// The root of the SpacetimeDB checkout.
pub fn repo_root() -> PathBuf {
    resolve("../..")
}

/// `tools/llm-oneshot`: prompts, Cursor rules and the gitignored staging area.
pub fn oneshot_dir() -> PathBuf {
    resolve("../llm-oneshot")
}

/// `tools/llm-sequential-upgrade`: backend specs, templates and the gitignored run output.
pub fn sequential_dir() -> PathBuf {
    resolve("../llm-sequential-upgrade")
}

/// The chat-app prompts shared by both benchmarks.
pub fn chat_app_prompts_dir() -> PathBuf {
    oneshot_dir().join("apps/chat-app/prompts")
}

/// The SpacetimeDB agent skills that the sequential benchmark inlines into its prompts.
pub fn skills_dir() -> PathBuf {
    repo_root().join("skills")
}
