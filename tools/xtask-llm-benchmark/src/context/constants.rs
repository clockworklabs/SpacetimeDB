use std::path::PathBuf;

pub const DOCS_DIR_DEFAULT: &str = "../../docs";
pub const RUSTDOC_CRATE_ROOT_DEFAULT: &str = "../../crates/bindings";

// Docs benchmark files (CI - single "best" model to test documentation quality)
pub const DOCS_BENCHMARK_DETAILS_DEFAULT: &str = "../../docs/llms/docs-benchmark-details.json";
pub const DOCS_BENCHMARK_SUMMARY_DEFAULT: &str = "../../docs/llms/docs-benchmark-summary.json";
pub const DOCS_BENCHMARK_COMMENT_DEFAULT: &str = "../../docs/llms/docs-benchmark-comment.md";

// LLM comparison files (manual runs - all models to compare LLM performance)
pub const LLM_COMPARISON_DETAILS_DEFAULT: &str = "../../docs/llms/llm-comparison-details.json";
pub const LLM_COMPARISON_SUMMARY_DEFAULT: &str = "../../docs/llms/llm-comparison-summary.json";

pub const ALL_MODES: &[&str] = &["docs", "llms.md", "cursor_rules", "rustdoc_json"];

#[inline]
pub fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOCS_DIR_DEFAULT)
}

// Docs benchmark paths (CI)
#[inline]
pub fn docs_benchmark_details() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOCS_BENCHMARK_DETAILS_DEFAULT)
}
#[inline]
pub fn docs_benchmark_summary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOCS_BENCHMARK_SUMMARY_DEFAULT)
}
#[inline]
pub fn docs_benchmark_comment() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOCS_BENCHMARK_COMMENT_DEFAULT)
}

// LLM comparison paths (manual)
#[inline]
pub fn llm_comparison_details() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(LLM_COMPARISON_DETAILS_DEFAULT)
}
#[inline]
pub fn llm_comparison_summary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(LLM_COMPARISON_SUMMARY_DEFAULT)
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
