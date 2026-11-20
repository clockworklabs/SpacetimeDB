use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const GREEN: u32 = 0x57F287;
const YELLOW: u32 = 0xFEE75C;
const RED: u32 = 0xED4245;
const INFRASTRUCTURE: &str = "Infrastructure/provider problem";

#[derive(Parser)]
#[command(about = "Build a Discord embed and attachment from LLM benchmark analysis reports.")]
struct Cli {
    #[arg(long)]
    reports_dir: PathBuf,
    #[arg(long)]
    run_url: String,
    #[arg(long)]
    run_label: String,
    #[arg(long)]
    output: PathBuf,
    /// Full text of the problems on our side; left empty when there are none.
    #[arg(long)]
    attachment: PathBuf,
}

/// A problem on our side: a recommended action or an infrastructure failure.
struct Problem {
    category: String,
    severity: String,
    title: String,
    text: String,
}

struct Report {
    language: String,
    passed_tasks: u64,
    total_tasks: u64,
    problems: Vec<Problem>,
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

fn severity_rank(severity: &str) -> u8 {
    match severity.to_ascii_lowercase().as_str() {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

fn problem(category: &str, severity: &str, source: &str, title: &str, details: &str) -> Problem {
    let title = title.trim().trim_end_matches('.');
    let label = if severity.is_empty() {
        format!("{category}, {source}")
    } else {
        format!("[{severity}] {category}, {source}")
    };
    // Plain text, because Discord shows a .txt attachment as a short preview that expands.
    let mut text = format!("{label}\n{title}.");
    if !details.is_empty() {
        text = format!("{text}\n{}", details.replace("**", ""));
    }
    Problem {
        category: category.to_owned(),
        severity: severity.to_owned(),
        title: title.to_owned(),
        text,
    }
}

/// Splits `**[Category | Severity] Title.** Details` into its parts.
fn parse_action(source: &str, item: &str) -> Problem {
    let parts = item.strip_prefix("**[").and_then(|rest| {
        let (tag, rest) = rest.split_once(']')?;
        let (title, details) = rest.split_once("**")?;
        let (category, severity) = tag.split_once('|').unwrap_or((tag, ""));
        Some((
            category.trim(),
            severity.trim(),
            title,
            details.trim_start_matches([' ', '—', '–', '-']).trim(),
        ))
    });
    let (category, severity, title, details) = parts.unwrap_or(("Other", "", item, ""));
    problem(category, severity, source, title, details)
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
        problems: Vec::new(),
    };
    let language = language_name(language);
    let mut section = "";
    let mut title = "";
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading;
            title = "";
        } else if section == "Recommended actions" && line.starts_with("- **[") {
            report
                .problems
                .push(parse_action(&format!("{language} / {mode}"), &line[2..]));
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
            } else if line.strip_prefix("- **Classification:** ") == Some(INFRASTRUCTURE) && !title.is_empty() {
                report.problems.push(problem(
                    INFRASTRUCTURE,
                    "",
                    &format!("{language} / {mode} / {model}"),
                    title,
                    "",
                ));
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

fn percent(passed: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        passed as f64 * 100.0 / total as f64
    }
}

/// One line per category, most common first, with its most severe title.
fn problem_lines(problems: &[&Problem]) -> Vec<String> {
    let mut by_category = BTreeMap::<&str, Vec<&Problem>>::new();
    for problem in problems {
        by_category.entry(&problem.category).or_default().push(problem);
    }
    let mut categories: Vec<_> = by_category.into_iter().collect();
    categories.sort_by_key(|(_, items)| Reverse(items.len()));
    categories
        .into_iter()
        .filter_map(|(category, items)| {
            let top = items.iter().min_by_key(|problem| severity_rank(&problem.severity))?;
            Some(format!("- {category} ({}): {}", items.len(), top.title))
        })
        .collect()
}

fn attachment(problems: &[&Problem]) -> String {
    let mut problems = problems.to_vec();
    problems.sort_by_key(|problem| severity_rank(&problem.severity));
    let mut seen = BTreeSet::new();
    let texts: Vec<_> = problems
        .iter()
        .map(|problem| problem.text.as_str())
        .filter(|text| seen.insert(*text))
        .collect();
    if texts.is_empty() {
        String::new()
    } else {
        format!("{}\n", texts.join("\n\n"))
    }
}

fn build(reports: &[Report], run_url: &str, run_label: &str) -> (Value, String) {
    let (mut passed, mut total) = (0, 0);
    let mut by_language = BTreeMap::<&str, (u64, u64)>::new();
    for report in reports {
        passed += report.passed_tasks;
        total += report.total_tasks;
        let counts = by_language.entry(&report.language).or_default();
        counts.0 += report.passed_tasks;
        counts.1 += report.total_tasks;
    }
    let problems: Vec<_> = reports.iter().flat_map(|report| &report.problems).collect();
    let rate = percent(passed, total);
    let color = if total == 0 || rate < 90.0 || problems.iter().any(|problem| problem.category == INFRASTRUCTURE) {
        RED
    } else if !problems.is_empty() || rate < 95.0 {
        YELLOW
    } else {
        GREEN
    };
    let mut description = if total == 0 {
        "No analysis reports were produced.".to_owned()
    } else {
        let languages: Vec<_> = by_language
            .into_iter()
            .map(|(language, (passed, total))| format!("{} {:.1}%", language_name(language), percent(passed, total)))
            .collect();
        format!("**{rate:.1}%** passed ({passed}/{total}) · {}", languages.join(" · "))
    };
    let lines = problem_lines(&problems);
    if !lines.is_empty() {
        description = format!("{description}\n\n{}", lines.join("\n"));
    }
    let payload = json!({
        "username": "Spacetime LLM Benchmarks",
        "allowed_mentions": {"parse": []},
        "embeds": [{
            "title": format!("LLM Benchmark Analysis · {run_label}"),
            "url": run_url,
            "description": description,
            "color": color
        }]
    });
    (payload, attachment(&problems))
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let reports = load_reports(&args.reports_dir)?;
    let (payload, attachment) = build(&reports, &args.run_url, &args.run_label);
    fs::write(&args.output, serde_json::to_vec(&payload)?)
        .with_context(|| format!("writing Discord payload {}", args.output.display()))?;
    fs::write(&args.attachment, attachment)
        .with_context(|| format!("writing Discord attachment {}", args.attachment.display()))
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
    fn summarizes_rates_and_leaves_out_model_limitations() {
        let reports = [
            report("csharp", "36/37 (97.3%)", "", ""),
            report("rust", "34/37 (91.9%)", "", "Model limitation"),
        ];
        let (payload, attachment) = build(&reports, "https://example.com/run", "Weekly run");
        let embed = &payload["embeds"][0];
        assert_eq!(embed["description"], "**94.6%** passed (70/74) · C# 97.3% · Rust 91.9%");
        assert_eq!(embed["color"], YELLOW);
        assert_eq!(embed["title"], "LLM Benchmark Analysis · Weekly run");
        assert_eq!(embed["url"], "https://example.com/run");
        assert_eq!(attachment, "");
        assert_eq!(payload["allowed_mentions"], json!({"parse": []}));
    }

    #[test]
    fn lists_one_line_per_category_and_attaches_the_details() {
        let reports = [
            report(
                "csharp",
                "37/37",
                "- **[Eval problem | Medium] Keep publish errors.** Store all stderr.\n- **[API/ergonomics problem | Low] Rename Index.** It collides.",
                "",
            ),
            report(
                "rust",
                "37/37",
                "- **[Eval problem | High] Capture build output** — Evidence: t_001.",
                INFRASTRUCTURE,
            ),
        ];
        let (payload, attachment) = build(&reports, "", "");
        assert_eq!(
            payload["embeds"][0]["description"],
            "**100.0%** passed (74/74) · C# 100.0% · Rust 100.0%\n\n- Eval problem (2): Capture build output\n- API/ergonomics problem (1): Rename Index\n- Infrastructure/provider problem (1): Incorrect sum-type syntax"
        );
        assert_eq!(payload["embeds"][0]["color"], RED);
        assert_eq!(
            attachment,
            "[High] Eval problem, Rust / guidelines\nCapture build output.\nEvidence: t_001.\n\n[Medium] Eval problem, C# / guidelines\nKeep publish errors.\nStore all stderr.\n\n[Low] API/ergonomics problem, C# / guidelines\nRename Index.\nIt collides.\n\nInfrastructure/provider problem, Rust / guidelines / test-model\nIncorrect sum-type syntax.\n"
        );
    }

    #[test]
    fn colors_reflect_pass_rate_problems_and_infrastructure() {
        for (tasks, action, classification, color) in [
            ("37/37", "", "", GREEN),
            ("35/37", "", "Model limitation", YELLOW),
            ("30/37", "", "Model limitation", RED),
            (
                "37/37",
                "- **[Skill problem | High] Clarify transactions** — Update the skill.",
                "",
                YELLOW,
            ),
            ("37/37", "", INFRASTRUCTURE, RED),
        ] {
            let (payload, _) = build(&[report("csharp", tasks, action, classification)], "", "");
            assert_eq!(payload["embeds"][0]["color"], color);
        }
        let (payload, _) = build(&[], "", "");
        assert_eq!(payload["embeds"][0]["color"], RED);
        assert_eq!(
            payload["embeds"][0]["description"],
            "No analysis reports were produced."
        );
    }
}
