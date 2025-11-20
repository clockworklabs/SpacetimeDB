use std::path::PathBuf;

pub const DOCS_DIR_DEFAULT: &str = "../../docs";
pub const RESULTS_PATH_DETAILS_DEFAULT: &str = "../../docs/llms/llm-benchmark-details.json";
pub const RESULTS_PATH_SUMMARY_DEFAULT: &str = "../../docs/llms/llm-benchmark-summary.json";
pub const RESULTS_PATH_RUN_DEFAULT: &str = "../../docs/llms/llm-benchmark-run.json";
pub const RUSTDOC_CRATE_ROOT_DEFAULT: &str = "../bindings";

pub const ALL_MODES: &[&str] = &["docs", "llms.md", "cursor_rules", "rustdoc_json"];

#[inline]
pub fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DOCS_DIR_DEFAULT)
}
#[inline]
pub fn results_path_details() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RESULTS_PATH_DETAILS_DEFAULT)
}
#[inline]
pub fn results_path_summary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RESULTS_PATH_SUMMARY_DEFAULT)
}
#[inline]
pub fn results_path_run() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RESULTS_PATH_RUN_DEFAULT)
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
