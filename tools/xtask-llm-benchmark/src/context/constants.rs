use std::path::PathBuf;

pub const DOCS_DIR_DEFAULT: &str = "../../docs";
pub const SKILLS_DIR_DEFAULT: &str = "../../skills";
pub const RUSTDOC_CRATE_ROOT_DEFAULT: &str = "../../crates/bindings";

// Docs benchmark files (CI - single "best" model to test documentation quality)
pub const DOCS_BENCHMARK_DETAILS_DEFAULT: &str = "../../docs/llms/docs-benchmark-details.json";
pub const DOCS_BENCHMARK_SUMMARY_DEFAULT: &str = "../../docs/llms/docs-benchmark-summary.json";
pub const DOCS_BENCHMARK_COMMENT_DEFAULT: &str = "../../docs/llms/docs-benchmark-comment.md";

// LLM comparison files (manual runs - all models to compare LLM performance)
pub const LLM_COMPARISON_DETAILS_DEFAULT: &str = "../../docs/llms/llm-comparison-details.json";
pub const LLM_COMPARISON_SUMMARY_DEFAULT: &str = "../../docs/llms/llm-comparison-summary.json";

// ## Context modes
//
// `guidelines` and `cursor_rules` serve different purposes:
//
// - `guidelines` (docs/static/ai-guidelines/): Constructive "happy path" cheat sheets
//   optimized for one-shot code generation. Show correct patterns only, no anti-patterns.
//   Used by the benchmark to measure how well models generate SpacetimeDB code from scratch.
//
// - `cursor_rules` (docs/static/ai-rules/): IDE-oriented .mdc rules designed for Cursor,
//   Windsurf, and similar AI coding assistants. Include anti-hallucination guardrails,
//   common mistake tables, client-side patterns, and migration guidance. These work well
//   in an IDE context where the model has project context, can iterate, and is editing
//   existing code — but they are NOT optimized for single-shot benchmark generation.
//
// Do not conflate the two. They have different audiences and different design goals.
pub const ALL_MODES: &[&str] = &[
    "docs",
    "llms.md",
    "guidelines",   // constructive-only AI guidelines (docs/static/ai-guidelines/)
    "cursor_rules", // IDE-oriented cursor/IDE rules   (docs/static/ai-rules/)
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
