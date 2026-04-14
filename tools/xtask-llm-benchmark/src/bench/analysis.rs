use crate::bench::types::RunOutcome;
use crate::eval::ScoreDetails;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::Segment;
use crate::llm::{LlmProvider, ModelRoute};
use anyhow::Result;
use spacetimedb_data_structures::map::HashMap;

/// Run AI analysis on a batch of outcomes, returning the analysis markdown.
/// Only analyzes failures (passed_tests < total_tests with llm_output present).
/// Returns `None` if there are no failures to analyze.
pub async fn run_analysis(
    outcomes: &[RunOutcome],
    lang: &str,
    mode: &str,
    llm: &dyn LlmProvider,
) -> Result<Option<String>> {
    let failures: Vec<&RunOutcome> = outcomes
        .iter()
        .filter(|o| o.passed_tests < o.total_tests && o.llm_output.is_some())
        .collect();

    if failures.is_empty() {
        return Ok(None);
    }

    let prompt = build_analysis_prompt(lang, mode, &failures);

    // Use a fast/cheap model for analysis
    let route = ModelRoute::new(
        "gpt-4.1-mini",
        crate::llm::types::Vendor::OpenAi,
        "gpt-4.1-mini",
        Some("openai/gpt-4.1-mini"),
    );

    let built = BuiltPrompt {
        system: Some(
            "You are an expert at analyzing SpacetimeDB benchmark failures. \
             Analyze the test failures and provide actionable insights."
                .to_string(),
        ),
        static_prefix: None,
        segments: vec![Segment::new("user", prompt)],
        search_enabled: false,
    };

    let response = llm.generate(&route, &built).await?;
    Ok(Some(response.text))
}

fn build_analysis_prompt(lang: &str, mode: &str, failures: &[&RunOutcome]) -> String {
    let lang_display = match lang {
        "rust" => "Rust",
        "csharp" => "C#",
        "typescript" => "TypeScript",
        _ => lang,
    };

    let mut prompt = format!(
        "Analyze the following SpacetimeDB benchmark test failures for {} / {} ({} failures).\n\n\
         IMPORTANT: For each failure you analyze, include the actual code examples inline.\n\
         Show what the LLM generated vs what was expected, highlighting specific differences.\n\
         Focus on SPECIFIC, ACTIONABLE documentation changes.\n\n",
        lang_display,
        mode,
        failures.len()
    );

    // Group by failure type
    let table_naming: Vec<_> = failures.iter().filter(|f| categorize(f) == "table_naming").collect();
    let compile: Vec<_> = failures.iter().filter(|f| categorize(f) == "compile").collect();
    let timeout: Vec<_> = failures.iter().filter(|f| categorize(f) == "timeout").collect();
    let other: Vec<_> = failures.iter().filter(|f| categorize(f) == "other").collect();

    if !table_naming.is_empty() {
        prompt.push_str(&format!("## Table Naming Issues ({} failures)\n\n", table_naming.len()));
        for f in table_naming.iter().take(3) {
            write_failure_detail(&mut prompt, f);
        }
        if table_naming.len() > 3 {
            prompt.push_str(&format!(
                "**Additional similar failures**: {}\n\n",
                table_naming.iter().skip(3).map(|f| f.task.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
    }

    if !compile.is_empty() {
        prompt.push_str(&format!("## Compile/Publish Errors ({} failures)\n\n", compile.len()));
        for f in compile.iter().take(3) {
            write_failure_detail(&mut prompt, f);
        }
        if compile.len() > 3 {
            prompt.push_str(&format!(
                "**Additional compile failures**: {}\n\n",
                compile.iter().skip(3).map(|f| f.task.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
    }

    if !timeout.is_empty() {
        prompt.push_str(&format!("## Timeout Issues ({} failures)\n\n", timeout.len()));
        for f in &timeout {
            prompt.push_str(&format!("- {}\n", f.task));
        }
        prompt.push('\n');
    }

    if !other.is_empty() {
        prompt.push_str(&format!("## Other Failures ({} failures)\n\n", other.len()));
        for f in other.iter().take(5) {
            write_failure_detail(&mut prompt, f);
        }
        if other.len() > 5 {
            prompt.push_str(&format!(
                "**Additional failures**: {}\n\n",
                other.iter().skip(5).map(|f| f.task.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
    }

    prompt.push_str(
        "\n---\n\n## Instructions:\n\n\
         For EACH failure or group of similar failures:\n\
         1. **The generated code**: The actual LLM-generated code\n\
         2. **The error**: The error message or failure reason\n\
         3. **Explain the difference**: What specific API/syntax was wrong?\n\
         4. **Root cause**: What's missing or unclear in the documentation?\n\
         5. **Recommendation**: Specific fix\n\n\
         Group similar failures together.\n",
    );

    prompt
}

fn write_failure_detail(prompt: &mut String, f: &RunOutcome) {
    let reasons = f
        .scorer_details
        .as_ref()
        .map(extract_failure_reasons)
        .unwrap_or_default();

    prompt.push_str(&format!("### {} - {}/{} tests passed\n", f.task, f.passed_tests, f.total_tests));
    prompt.push_str(&format!("**Failure**: {}\n\n", reasons.join(", ")));

    if let Some(llm_out) = &f.llm_output {
        let truncated = truncate_str(llm_out, 1500);
        prompt.push_str(&format!("**LLM Output**:\n```\n{}\n```\n\n", truncated));
    }
}

fn extract_failure_reasons(details: &HashMap<String, ScoreDetails>) -> Vec<String> {
    details
        .iter()
        .filter_map(|(scorer_name, score)| {
            score.failure_reason().map(|reason| format!("{}: {}", scorer_name, reason))
        })
        .collect()
}

fn categorize(f: &RunOutcome) -> &'static str {
    let reasons = f
        .scorer_details
        .as_ref()
        .map(extract_failure_reasons)
        .unwrap_or_default();
    let reasons_str = reasons.join(" ");
    if reasons_str.contains("tables differ") {
        "table_naming"
    } else if reasons_str.contains("timed out") {
        "timeout"
    } else if reasons_str.contains("publish failed") || reasons_str.contains("compile") {
        "compile"
    } else {
        "other"
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}
