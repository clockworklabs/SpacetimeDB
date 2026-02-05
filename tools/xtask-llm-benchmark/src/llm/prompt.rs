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
}

impl PromptBuilder {
    pub fn build_segmented(&self, context: &str) -> BuiltPrompt {
        let version = "1.6";

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

        let static_prefix = Some(format!(
            "<<<DOCS START>>>Context:\n{context}\n<<<DOCS END>>>\n",
            context = context,
        ));

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
