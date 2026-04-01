use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::eval::lang::Lang;
use crate::llm::segmentation::Segment;

#[derive(Clone, Debug)]
pub struct PromptBuilder {
    pub lang: String,
    pub task_id: String,
    pub instructions: String,
}

pub struct BuiltPrompt {
    pub system: Option<String>,
    pub static_prefix: Option<String>,
    pub segments: Vec<Segment<'static>>,
    /// When true, the provider should enable web search (OpenRouter :online).
    pub search_enabled: bool,
}

impl PromptBuilder {
    pub fn build_segmented(&self, mode: &str, context: &str) -> BuiltPrompt {
        let version = "1.6";
        let search_enabled = mode == "search";

        // SYSTEM: hygiene-only for Knowledge; hygiene + stricter output rules for Conformance.
        let system = Some(format!(
            "You are a precise code generator. Output only {lang} code as raw text.\n\
Rules:\n\
- No markdown fences/backticks/quotes. No commentary or explanations.\n\
- Single source file only.\n\
- Identifiers from the TASK are case-sensitive; copy them verbatim.\n\
- The final TASK segment is authoritative; DOCS are reference-only.",
            lang = self.lang
        ));

        let static_prefix = Some(if search_enabled {
            "<<<DOCS START>>>\nYou MUST search the web for SpacetimeDB documentation and examples before writing any code. Do not write code until you have searched.\n<<<DOCS END>>>\n".to_string()
        } else if context.trim().is_empty() {
            "<<<DOCS START>>>\nUse your knowledge of the latest SpacetimeDB syntax and conventions.\n<<<DOCS END>>>\n"
                .to_string()
        } else {
            let preamble = match mode {
                "cursor_rules" => format!(
                    "The following are SpacetimeDB coding rules for {}. \
                     Read ALL rules carefully before writing any code. \
                     They contain ❌ WRONG examples showing common mistakes to AVOID \
                     and ✅ CORRECT examples showing the right patterns to follow. \
                     Focus on the ✅ patterns. Write SERVER-SIDE module code only.",
                    self.lang
                ),
                "guidelines" => format!(
                    "The following are SpacetimeDB {} guidelines. \
                     All examples shown are correct patterns to follow.",
                    self.lang
                ),
                _ => "Reference documentation:".to_string(),
            };
            format!("<<<DOCS START>>>\n{preamble}\n\n{context}\n<<<DOCS END>>>\n",)
        });

        // TASK: identical in both modes; API details must come from DOCS in Knowledge mode.
        // In Conformance mode we keep it the same—to measure formatting discipline separately in scoring.
        let dynamic = format!(
            "<<<TASK START>>>\n\
Task ({task_id}):\n{instructions}\n\n\
HARD CONSTRAINTS:\n\
- Use valid SpacetimeDB {version} syntax for {lang}.\n\
- Output RAW SOURCE ONLY (no markdown/fences/backticks/quotes).\n\
- Single file. No extra modules, tests, mains, or commentary.\n\
- Use EXACT identifiers from the TASK; do not rename or re-case anything.\n\
<<<TASK END>>>",
            task_id = self.task_id,
            instructions = self.instructions,
            version = version,
            lang = self.lang,
        );

        BuiltPrompt {
            system,
            static_prefix,
            segments: vec![Segment::new("user", dynamic).keep().min_chars(0).weight(8.0)],
            search_enabled,
        }
    }
}
pub fn make_prompt_from_task(spec_file: &str, task_id: &str, lang: Lang) -> Result<PromptBuilder> {
    let xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // crates/xtask-llm-benchmark
    let workspace_root = xtask_dir
        .parent()
        .and_then(|p| p.parent()) // SpacetimeDB
        .context("could not determine workspace root")?;

    let spec_path = workspace_root.join(spec_file);
    let spec_path = spec_path
        .canonicalize()
        .with_context(|| format!("canonicalize spec_file {}", spec_path.display()))?;

    let task_root = spec_path.parent().context("spec file has no parent (task dir)")?;

    let tasks_file = find_tasks_file(task_root, lang)
        .with_context(|| format!("missing tasks file for {} in {}", lang.as_str(), task_root.display()))?;

    let instructions =
        std::fs::read_to_string(&tasks_file).with_context(|| format!("read {}", tasks_file.display()))?;

    Ok(PromptBuilder {
        lang: lang.display_name().to_string(),
        task_id: task_id.to_string(),
        instructions,
    })
}

fn find_tasks_file(task_root: &Path, lang: Lang) -> Option<PathBuf> {
    let dir = task_root.join("tasks");
    match lang {
        Lang::CSharp => {
            let p = dir.join("csharp.txt");
            p.exists().then_some(p)
        }
        Lang::Rust => {
            let p = dir.join("rust.txt");
            p.exists().then_some(p)
        }
        Lang::TypeScript => {
            let p = dir.join("typescript.txt");
            p.exists().then_some(p)
        }
    }
}
