use crate::bench::types::RunOutcome;
use crate::eval::ScoreDetails;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::Segment;
use crate::llm::{LlmProvider, ModelRoute};
use anyhow::Result;
use spacetimedb_data_structures::map::HashMap;
use std::path::Path;

pub async fn run_analysis(
    outcomes: &[RunOutcome],
    lang: &str,
    mode: &str,
    model_name: &str,
    bench_root: &Path,
    llm: &dyn LlmProvider,
) -> Result<Option<String>> {
    let failures: Vec<&RunOutcome> = outcomes
        .iter()
        .filter(|o| o.passed_tests < o.total_tests && o.llm_output.is_some())
        .collect();

    if failures.is_empty() {
        return Ok(None);
    }

    let prompt = build_prompt(lang, mode, model_name, bench_root, &failures);

    let route = ModelRoute::new(
        "gpt-4.1-mini",
        crate::llm::types::Vendor::OpenAi,
        "gpt-4.1-mini",
        Some("openai/gpt-4.1-mini"),
    );

    let built = BuiltPrompt {
        system: Some(system_prompt()),
        static_prefix: None,
        segments: vec![Segment::new("user", prompt)],
        search_enabled: false,
    };

    let response = llm.generate(&route, &built).await?;
    Ok(Some(response.text))
}

pub fn system_prompt() -> String {
    String::from(SYSTEM_PROMPT)
}

pub const SYSTEM_PROMPT: &str = "\
You summarize LLM benchmark failures for SpacetimeDB into structured markdown. \
Each failure includes the model's generated code, the scorer error, and the golden (correct) answer when available. \
Write in third person for a public benchmark page. Do not address the reader.";

fn context_description(mode: &str) -> &'static str {
    match mode {
        "guidelines" => "the SpacetimeDB AI guidelines (concise cheat-sheets for code generation)",
        "docs" => "SpacetimeDB markdown documentation",
        "rustdoc_json" => "SpacetimeDB rustdoc JSON (auto-generated API reference)",
        "llms.md" => "the SpacetimeDB llms.md file",
        "no_context" | "none" | "no_guidelines" => "no documentation (testing base model knowledge only)",
        "search" => "web search results (no local docs)",
        _ => "unspecified context",
    }
}

fn has_context(mode: &str) -> bool {
    !matches!(mode, "no_context" | "none" | "no_guidelines" | "search")
}

fn context_name(mode: &str) -> &'static str {
    match mode {
        "guidelines" => "AI guidelines",
        "docs" => "documentation",
        "rustdoc_json" => "rustdoc",
        "llms.md" => "llms.md",
        _ => "context",
    }
}

/// Read the golden answer for a task from disk.
/// Scans `bench_root/<category>/<task_id>/answers/{rust.rs,csharp.cs,typescript.ts}`.
fn read_golden(bench_root: &Path, task_id: &str, lang: &str) -> Option<String> {
    let answer_file = match lang {
        "rust" => "rust.rs",
        "csharp" => "csharp.cs",
        "typescript" => "typescript.ts",
        _ => return None,
    };

    // Scan categories to find the task directory
    let Ok(cats) = std::fs::read_dir(bench_root) else {
        return None;
    };
    for cat in cats.filter_map(|e| e.ok()) {
        let task_dir = cat.path().join(task_id);
        let path = task_dir.join("answers").join(answer_file);
        if path.is_file() {
            return std::fs::read_to_string(&path).ok();
        }
    }
    None
}

pub fn build_prompt(
    lang: &str,
    mode: &str,
    model_name: &str,
    bench_root: &Path,
    failures: &[&RunOutcome],
) -> String {
    let lang_display = match lang {
        "rust" => "Rust",
        "csharp" => "C#",
        "typescript" => "TypeScript",
        _ => lang,
    };

    let mut prompt = format!(
        "{model_name} was given {ctx} and asked to generate {lang_display} SpacetimeDB modules. \
         It failed {count} tasks.\n\n",
        ctx = context_description(mode),
        count = failures.len(),
    );

    for f in failures.iter().take(15) {
        prompt.push_str(&format!(
            "### {} ({}/{})\n",
            f.task, f.passed_tests, f.total_tests
        ));

        let reasons = f
            .scorer_details
            .as_ref()
            .map(extract_reasons)
            .unwrap_or_default();
        if !reasons.is_empty() {
            prompt.push_str(&format!("Error: {}\n", reasons.join("; ")));
        }

        if let Some(ref out) = f.llm_output {
            prompt.push_str(&format!(
                "Generated:\n```{}\n{}\n```\n",
                lang,
                truncate(out, 1500)
            ));
        }

        if let Some(golden) = read_golden(bench_root, &f.task, lang) {
            prompt.push_str(&format!(
                "Expected:\n```{}\n{}\n```\n",
                lang,
                truncate(&golden, 1500)
            ));
        }

        prompt.push('\n');
    }

    if failures.len() > 15 {
        prompt.push_str(&format!(
            "({} more failures not shown)\n\n",
            failures.len() - 15
        ));
    }

    prompt.push_str(&analysis_instructions(mode));
    prompt
}

pub fn analysis_instructions(mode: &str) -> String {
    let fix_line = if has_context(mode) {
        let name = context_name(mode);
        format!("5. **{name} gap:** What's missing or unclear in the {name} that led to this mistake\n")
    } else {
        String::new()
    };

    format!(
        "\
---

Group failures by root cause pattern. Use this exact structure for each group:

### [Pattern Name] (N tasks)

1. **What the model wrote:** Show the relevant incorrect lines from the generated code
2. **What was expected:** Show the relevant lines from the golden answer
3. **What the error says:** Quote the scorer error that identifies the problem
4. **Why this happened:** Why the model likely made this mistake (e.g. confused with another framework, hallucinated API, singular vs plural naming)
5. **Affected tasks:** list of task IDs
{fix_line}
Rules:
- Group tasks that fail for the same reason. Do not repeat the same analysis per task.
- Show only the relevant lines, not entire files.
- Skip provider errors (timeouts, 429s) with a brief note.
"
    )
}

fn extract_reasons(details: &HashMap<String, ScoreDetails>) -> Vec<String> {
    details
        .iter()
        .filter_map(|(name, score)| {
            score
                .failure_reason()
                .map(|r| format!("{}: {}", name, truncate(&r, 150)))
        })
        .collect()
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}
