use crate::bench::types::RunOutcome;
use crate::eval::ScoreDetails;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::Segment;
use crate::llm::{LlmProvider, ModelRoute};
use anyhow::Result;
use spacetimedb_data_structures::map::HashMap;
use std::path::Path;

const MAX_CONTEXT_CHARS: usize = 20_000;

pub async fn run_analysis(
    outcomes: &[RunOutcome],
    lang: &str,
    mode: &str,
    model_name: &str,
    context: &str,
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

    let prompt = build_prompt(lang, mode, model_name, context, bench_root, &failures);

    let route = ModelRoute::new(
        "gpt-5.4-mini",
        crate::llm::types::Vendor::OpenAi,
        "gpt-5.4-mini",
        Some("openai/gpt-5.4-mini"),
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
You turn LLM benchmark failures for SpacetimeDB into evidence-based, structured markdown. \
Each failure includes the model's generated code, the scorer error, and the golden (correct) answer when available. \
Write in third person for a public benchmark page. Do not address the reader. \
Recommend repository changes only when the supplied evidence supports them.";

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
    context: &str,
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

    let context_is_complete = context.chars().count() <= MAX_CONTEXT_CHARS;
    if has_context(mode) {
        prompt.push_str("### Context supplied to the model\n\n");
        prompt.push_str(&format!("```\n{}\n```\n", truncate(context, MAX_CONTEXT_CHARS)));
        if !context_is_complete {
            prompt.push_str("The context excerpt was truncated. Treat context-gap conclusions as low confidence.\n");
        }
        prompt.push('\n');
    }

    for f in failures.iter().take(15) {
        prompt.push_str(&format!("### {} ({}/{})\n", f.task, f.passed_tests, f.total_tests));

        let reasons = f.scorer_details.as_ref().map(extract_reasons).unwrap_or_default();
        if !reasons.is_empty() {
            prompt.push_str(&format!("Error: {}\n", reasons.join("; ")));
        }

        if let Some(ref out) = f.llm_output {
            prompt.push_str(&format!("Generated:\n```{}\n{}\n```\n", lang, truncate(out, 1500)));
        }

        if let Some(golden) = read_golden(bench_root, &f.task, lang) {
            prompt.push_str(&format!("Expected:\n```{}\n{}\n```\n", lang, truncate(&golden, 1500)));
        }

        prompt.push('\n');
    }

    if failures.len() > 15 {
        prompt.push_str(&format!("({} more failures not shown)\n\n", failures.len() - 15));
    }

    prompt.push_str(&analysis_instructions_with_context(mode, context_is_complete));
    prompt
}

pub fn analysis_instructions(mode: &str) -> String {
    analysis_instructions_with_context(mode, false)
}

fn analysis_instructions_with_context(mode: &str, context_is_complete: bool) -> String {
    let context_gap_line = if has_context(mode) {
        let name = context_name(mode);
        if context_is_complete {
            format!("- **{name} gap:** What is missing or unclear in the {name}, or `None`\n")
        } else {
            format!("- **{name} gap:** `Needs manual review`; the complete {name} was not supplied\n")
        }
    } else {
        String::new()
    };
    let context_rule = if has_context(mode) && !context_is_complete {
        "- Because the complete context was not supplied, do not classify a group as `Skill problem` or `Documentation problem`.\n"
    } else {
        ""
    };

    format!(
        "\
---

Begin with this section:

## Recommended actions

List at most five distinct, repository-owned actions supported by the evidence. Use:

- **[Classification | Confidence] Short title** — Plain-language action. Evidence: task IDs.

If the failures are isolated model mistakes, provider failures, or otherwise do not justify a repository change, write:
`No repository changes recommended.`

Then group failures by root cause using this exact structure:

## Failure patterns

### [Pattern Name] (N tasks)

- **Classification:** One of `Eval problem`, `Skill problem`, `Documentation problem`, `API/ergonomics problem`, `Model limitation`, `Infrastructure/provider problem`, or `No action`
- **Confidence:** `High`, `Medium`, or `Low`
- **What the model wrote:** Relevant incorrect lines from the generated code
- **What was expected:** Relevant lines from the golden answer
- **What the error says:** The scorer error that identifies the problem
- **Why this happened:** The likely root cause
- **Suggested action:** A plain-language repository change, or `None`
- **Suggested area:** The likely repository area, or `None`
- **Affected tasks:** Task IDs
{context_gap_line}
Rules:
- Group tasks that fail for the same reason. Do not repeat the same analysis per task.
- Show only the relevant lines, not entire files.
- Skip provider errors (timeouts, 429s) with a brief note.
- Do not recommend changing an eval, skill, documentation, or API merely because one model made an isolated mistake.
- Do not invent evidence, repository paths, or implementation details that were not supplied.
- Prefer `Model limitation`, `Infrastructure/provider problem`, or `No action` when the evidence does not support a repository change.
- Classify a context gap as `Skill problem` or `Documentation problem` only when the supplied context directly supports that conclusion.
- `Eval problem`, `Skill problem`, `Documentation problem`, and `API/ergonomics problem` require a non-`None` suggested action and a matching entry under `Recommended actions`.
- When `Suggested action` is `None`, use `Model limitation`, `Infrastructure/provider problem`, or `No action`.
- Write `No repository changes recommended.` only when no failure group recommends a repository change.
{context_rule}\
"
    )
}

pub fn build_report(
    outcomes: &[RunOutcome],
    lang: &str,
    mode: &str,
    model_name: &str,
    analysis: Option<&str>,
) -> String {
    let passed_tasks = outcomes
        .iter()
        .filter(|outcome| outcome.total_tests > 0 && outcome.passed_tests == outcome.total_tests)
        .count();
    let total_tasks = outcomes.len();
    let passed_scorers: u32 = outcomes.iter().map(|outcome| outcome.passed_tests).sum();
    let total_scorers: u32 = outcomes.iter().map(|outcome| outcome.total_tests).sum();
    let failures_without_output: Vec<&str> = outcomes
        .iter()
        .filter(|outcome| outcome.passed_tests < outcome.total_tests && outcome.llm_output.is_none())
        .map(|outcome| outcome.task.as_str())
        .collect();
    let unavailable_output_section = if failures_without_output.is_empty() {
        None
    } else {
        let task_count = failures_without_output.len();
        let task_label = if task_count == 1 { "task" } else { "tasks" };
        let tasks = failures_without_output
            .iter()
            .map(|task| format!("`{task}`"))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "\
### Model output unavailable ({task_count} {task_label})

- **Classification:** Infrastructure/provider problem
- **Tasks:** {tasks}
- **What happened:** The model request failed before producing output, so source-level failure analysis was not possible.
- **Suggested action:** Retry the affected tasks and inspect the benchmark logs if the failure persists."
        ))
    };

    let mut report = format!(
        "\
# LLM Benchmark Analysis

- Language: {lang}
- Mode: {mode}
- Model: {model_name}
- Tasks: {passed_tasks}/{total_tasks} ({task_percent:.1}%)
- Scorers: {passed_scorers}/{total_scorers} ({scorer_percent:.1}%)

",
        task_percent = percent(passed_tasks as u32, total_tasks as u32),
        scorer_percent = percent(passed_scorers, total_scorers),
    );

    match analysis {
        Some(analysis) => {
            report.push_str(analysis.trim());
            if let Some(section) = unavailable_output_section {
                report.push_str("\n\n");
                report.push_str(&section);
            }
        }
        None if unavailable_output_section.is_some() => {
            report.push_str(&format!(
                "\
## Recommended actions

No repository changes recommended.

## Failure patterns

{}",
                unavailable_output_section.unwrap()
            ));
        }
        None => report.push_str(
            "\
## Recommended actions

No repository changes recommended.

## Failure patterns

No failures detected.",
        ),
    }
    report.push('\n');
    report
}

fn percent(passed: u32, total: u32) -> f64 {
    if total == 0 {
        0.0
    } else {
        f64::from(passed) * 100.0 / f64::from(total)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(task: &str, passed_tests: u32, total_tests: u32) -> RunOutcome {
        RunOutcome {
            hash: String::new(),
            task: task.to_string(),
            lang: "typescript".to_string(),
            golden_published: true,
            model_name: "test-model".to_string(),
            total_tests,
            passed_tests,
            llm_output: None,
            category: None,
            route_api_model: None,
            golden_db: None,
            llm_db: None,
            work_dir_golden: None,
            work_dir_llm: None,
            scorer_details: None,
            vendor: String::new(),
            input_tokens: None,
            output_tokens: None,
            generation_duration_ms: None,
            started_at: None,
            finished_at: None,
        }
    }

    #[test]
    fn instructions_request_actionable_evidence_based_output() {
        let instructions = analysis_instructions("guidelines");

        assert!(instructions.contains("## Recommended actions"));
        assert!(instructions.contains("Eval problem"));
        assert!(instructions.contains("Skill problem"));
        assert!(instructions.contains("Model limitation"));
        assert!(instructions.contains("Do not invent evidence"));
        assert!(instructions.contains("AI guidelines gap"));
        assert!(instructions.contains("Needs manual review"));
        assert!(instructions.contains("do not classify a group as `Skill problem` or `Documentation problem`"));
    }

    #[test]
    fn no_context_does_not_request_a_context_gap() {
        let instructions = analysis_instructions("no_context");

        assert!(!instructions.contains("context gap:"));
        assert!(!instructions.contains("guidelines gap:"));
    }

    #[test]
    fn report_includes_task_and_scorer_pass_rates() {
        let outcomes = vec![outcome("t_001", 3, 3), outcome("t_002", 1, 2), outcome("t_003", 0, 0)];
        let report = build_report(
            &outcomes,
            "typescript",
            "guidelines",
            "test-model",
            Some("## Recommended actions\n\n- Fix the skill."),
        );

        assert!(report.contains("- Tasks: 1/3 (33.3%)"));
        assert!(report.contains("- Scorers: 4/5 (80.0%)"));
        assert!(report.contains("- Fix the skill."));
    }

    #[test]
    fn live_prompt_supplies_context_for_gap_analysis() {
        let failed = outcome("t_001", 0, 1);
        let prompt = build_prompt(
            "typescript",
            "guidelines",
            "test-model",
            "Use transactions for procedure writes.",
            Path::new("missing-benchmark-root"),
            &[&failed],
        );

        assert!(prompt.contains("### Context supplied to the model"));
        assert!(prompt.contains("Use transactions for procedure writes."));
        assert!(prompt.contains("What is missing or unclear in the AI guidelines"));
        assert!(!prompt.contains("complete AI guidelines was not supplied"));
    }

    #[test]
    fn passing_report_recommends_no_changes() {
        let report = build_report(
            &[outcome("t_001", 3, 3)],
            "typescript",
            "guidelines",
            "test-model",
            None,
        );

        assert!(report.contains("No repository changes recommended."));
        assert!(report.contains("No failures detected."));
    }

    #[test]
    fn failed_request_without_output_is_reported_as_infrastructure() {
        let report = build_report(
            &[outcome("t_001", 0, 1)],
            "typescript",
            "guidelines",
            "test-model",
            None,
        );

        assert!(report.contains("Model output unavailable (1 task)"));
        assert!(report.contains("Infrastructure/provider problem"));
        assert!(report.contains("`t_001`"));
        assert!(!report.contains("No failures detected."));
    }

    #[test]
    fn failed_request_without_output_is_added_to_model_analysis() {
        let mut generated_failure = outcome("t_001", 0, 1);
        generated_failure.llm_output = Some("generated source".to_string());
        let report = build_report(
            &[generated_failure, outcome("t_002", 0, 1)],
            "typescript",
            "guidelines",
            "test-model",
            Some("## Recommended actions\n\nNo repository changes recommended.\n\n## Failure patterns\n\n### Invalid API"),
        );

        assert!(report.contains("### Invalid API"));
        assert!(report.contains("Model output unavailable (1 task)"));
        assert!(report.contains("`t_002`"));
    }
}
