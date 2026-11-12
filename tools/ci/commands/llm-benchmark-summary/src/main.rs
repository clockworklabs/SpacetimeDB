use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const GREEN: u32 = 0x57F287;
const YELLOW: u32 = 0xFEE75C;
const RED: u32 = 0xED4245;
const MAX_ITEMS: usize = 5;
const MAX_FIELD_LENGTH: usize = 1000;

#[derive(Parser)]
#[command(about = "Build a Discord embed from LLM benchmark analysis reports.")]
struct Cli {
    #[arg(long)]
    reports_dir: PathBuf,
    #[arg(long)]
    run_url: String,
    #[arg(long)]
    run_label: String,
    #[arg(long)]
    output: PathBuf,
}

struct Report {
    language: String,
    passed_tasks: u64,
    total_tasks: u64,
    actions: Vec<String>,
    other_findings: Vec<String>,
}

fn field<'a>(text: &'a str, name: &str) -> &'a str {
    let prefix = format!("- {name}: ");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or("unknown")
}

fn language_name(language: &str) -> &str {
    match language {
        "csharp" => "C#",
        "rust" => "Rust",
        "typescript" => "TypeScript",
        other => other,
    }
}

fn parse_report(text: &str) -> Report {
    let language = field(text, "Language");
    let mode = field(text, "Mode");
    let model = field(text, "Model");
    let counts = field(text, "Tasks").split_whitespace().next().unwrap_or_default();
    let (passed_tasks, total_tasks) = counts
        .split_once('/')
        .and_then(|(passed, total)| Some((passed.parse().ok()?, total.parse().ok()?)))
        .unwrap_or((0, 0));
    let mut report = Report {
        language: language.to_owned(),
        passed_tasks,
        total_tasks,
        actions: Vec::new(),
        other_findings: Vec::new(),
    };
    let language = language_name(language);
    let mut section = "";
    let mut title = "";
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading;
            title = "";
        } else if section == "Recommended actions" && line.starts_with("- **[") {
            report.actions.push(format!("- {language} / {mode}: {}", &line[2..]));
        } else if section == "Failure patterns" {
            if let Some(heading) = line.strip_prefix("### ") {
                title = heading
                    .rsplit_once(" (")
                    .filter(|(_, suffix)| {
                        suffix
                            .strip_suffix(" tasks)")
                            .or_else(|| suffix.strip_suffix(" task)"))
                            .is_some_and(|count| count.parse::<u64>().is_ok())
                    })
                    .map_or(heading, |(title, _)| title);
            } else if let Some(classification) = line.strip_prefix("- **Classification:** ")
                && !title.is_empty()
                && matches!(
                    classification,
                    "Model limitation" | "Infrastructure/provider problem" | "No action"
                )
            {
                report
                    .other_findings
                    .push(format!("- {language} / {mode} / {model}: {title} - {classification}"));
            }
        }
    }
    report
}

fn load_reports(directory: &Path) -> Result<Vec<Report>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("reading reports in {}", directory.display())),
    };
    let mut entries = entries.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());
    let mut reports = Vec::new();
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            reports.extend(load_reports(&path)?);
        } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "md") {
            let text = fs::read_to_string(&path).with_context(|| format!("reading report {}", path.display()))?;
            reports.push(parse_report(&text));
        }
    }
    Ok(reports)
}

fn rate(passed: u64, total: u64) -> String {
    let percent = if total == 0 {
        0.0
    } else {
        passed as f64 * 100.0 / total as f64
    };
    format!("{passed}/{total} ({percent:.1}%)")
}

fn field_value(lines: &[String], empty: &str, overflow_label: &str) -> String {
    let mut seen = BTreeSet::new();
    let unique: Vec<_> = lines.iter().filter(|line| seen.insert(line.as_str())).collect();
    let mut value = if unique.is_empty() {
        empty.to_owned()
    } else {
        unique
            .iter()
            .take(MAX_ITEMS)
            .map(|line| line.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    if unique.len() > MAX_ITEMS {
        value.push_str(&format!(
            "\n- ...and {} more {overflow_label}(s)",
            unique.len() - MAX_ITEMS
        ));
    }
    // Discord counts UTF-16 units; keep Unicode text intact when limiting a field.
    if value.encode_utf16().count() > MAX_FIELD_LENGTH {
        let suffix = "\n... View the full analysis.";
        let mut length = 0;
        value = value
            .chars()
            .take_while(|ch| {
                length += ch.len_utf16();
                length <= MAX_FIELD_LENGTH - suffix.len()
            })
            .collect();
        value.truncate(value.trim_end().len());
        value.push_str(suffix);
    }
    value
}

fn build_payload(reports: &[Report], run_url: &str, run_label: &str) -> Value {
    let (mut passed, mut total) = (0, 0);
    let mut by_language = BTreeMap::<&str, (u64, u64)>::new();
    let mut actions = Vec::new();
    let mut other_findings = Vec::new();
    for report in reports {
        passed += report.passed_tasks;
        total += report.total_tasks;
        let counts = by_language.entry(&report.language).or_default();
        counts.0 += report.passed_tasks;
        counts.1 += report.total_tasks;
        actions.extend(report.actions.iter().cloned());
        other_findings.extend(report.other_findings.iter().cloned());
    }
    let percent = if total == 0 {
        0.0
    } else {
        passed as f64 * 100.0 / total as f64
    };
    let color = if total == 0
        || percent < 90.0
        || other_findings
            .iter()
            .any(|item| item.contains("Infrastructure/provider problem"))
    {
        RED
    } else if !actions.is_empty() || percent < 95.0 {
        YELLOW
    } else {
        GREEN
    };
    let language_rates: Vec<_> = by_language
        .into_iter()
        .map(|(language, (passed, total))| format!("- **{}:** {}", language_name(language), rate(passed, total)))
        .collect();
    json!({
        "username": "SpacetimeDB LLM Benchmarks",
        "allowed_mentions": {"parse": []},
        "embeds": [{
            "title": "LLM Benchmark Analysis",
            "url": run_url,
            "description": format!("**{}** task runs passed", rate(passed, total)),
            "color": color,
            "fields": [
                {"name": "By language", "value": field_value(&language_rates, "No analysis reports were produced.", "language"), "inline": false},
                {"name": "Action items", "value": field_value(&actions, "None", "action"), "inline": false},
                {"name": "Other failures", "value": field_value(&other_findings, "None", "finding"), "inline": false}
            ],
            "footer": {"text": run_label}
        }]
    })
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let reports = load_reports(&args.reports_dir)?;
    let payload = build_payload(&reports, &args.run_url, &args.run_label);
    fs::write(&args.output, serde_json::to_vec(&payload)?)
        .with_context(|| format!("writing Discord payload {}", args.output.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(language: &str, tasks: &str, action: &str, classification: &str) -> Report {
        parse_report(&format!(
            "# LLM Benchmark Analysis\n\n- Language: {language}\n- Mode: guidelines\n- Model: test-model\n- Tasks: {tasks}\n- Scorers: 100/100 (100.0%)\n\n## Recommended actions\n\n{action}\n\n## Failure patterns\n\n### Incorrect sum-type syntax (1 task)\n\n- **Classification:** {classification}\n"
        ))
    }

    #[test]
    fn aggregates_rates_and_formats_findings() {
        let reports = [
            report("csharp", "36/37 (97.3%)", "", ""),
            report("rust", "34/37 (91.9%)", "", "Model limitation"),
        ];
        let payload = build_payload(&reports, "https://example.com/run", "Weekly run");
        let embed = &payload["embeds"][0];
        assert_eq!(embed["description"], "**70/74 (94.6%)** task runs passed");
        assert_eq!(embed["color"], YELLOW);
        assert_eq!(
            embed["fields"][0]["value"],
            "- **C#:** 36/37 (97.3%)\n- **Rust:** 34/37 (91.9%)"
        );
        assert_eq!(
            embed["fields"][2]["value"],
            "- Rust / guidelines / test-model: Incorrect sum-type syntax - Model limitation"
        );
        assert_eq!(payload["allowed_mentions"], json!({"parse": []}));
        assert_eq!(embed["url"], "https://example.com/run");
        assert_eq!(embed["footer"]["text"], "Weekly run");
    }

    #[test]
    fn colors_reflect_pass_rate_actions_and_infrastructure() {
        for (tasks, action, classification, color) in [
            ("37/37", "", "", GREEN),
            ("35/37", "", "Model limitation", YELLOW),
            ("30/37", "", "Model limitation", RED),
            (
                "37/37",
                "- **[Skill problem | High] Clarify transactions** — Update the skill. Evidence: t_075.",
                "Skill problem",
                YELLOW,
            ),
            ("37/37", "", "Infrastructure/provider problem", RED),
        ] {
            let payload = build_payload(&[report("csharp", tasks, action, classification)], "", "");
            assert_eq!(payload["embeds"][0]["color"], color);
            if !action.is_empty() {
                assert_eq!(
                    payload["embeds"][0]["fields"][1]["value"],
                    format!("- C# / guidelines: {}", &action[2..])
                );
                assert_eq!(payload["embeds"][0]["fields"][2]["value"], "None");
            }
        }
    }

    #[test]
    fn empty_and_long_fields_stay_valid() {
        let payload = build_payload(&[], "", "");
        assert_eq!(payload["embeds"][0]["color"], RED);
        assert_eq!(
            payload["embeds"][0]["fields"][0]["value"],
            "No analysis reports were produced."
        );
        let lines: Vec<_> = (0..6).map(|i| format!("- {i} {}", "🦀".repeat(400))).collect();
        let value = field_value(&lines, "None", "finding");
        assert!(value.encode_utf16().count() <= MAX_FIELD_LENGTH);
        assert!(value.ends_with("... View the full analysis."));
        let mut lines: Vec<_> = (0..6).map(|i| format!("- {i}")).collect();
        lines.insert(1, "- 0".to_owned());
        assert_eq!(
            field_value(&lines, "None", "finding"),
            "- 0\n- 1\n- 2\n- 3\n- 4\n- ...and 1 more finding(s)"
        );
    }
}
