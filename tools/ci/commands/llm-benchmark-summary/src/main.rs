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
const MAX_ITEMS: usize = 5;
const MAX_FIELD_LENGTH: usize = 1000;
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

struct Action {
    category: String,
    severity: String,
    headline: String,
    line: String,
}

struct Report {
    language: String,
    passed_tasks: u64,
    total_tasks: u64,
    actions: Vec<Action>,
    infrastructure: Vec<String>,
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

/// Splits `**[Category | Severity] Title.** Details` into its parts.
fn parse_action(prefix: &str, item: &str) -> Action {
    let parts = item.strip_prefix("**[").and_then(|rest| {
        let (tag, rest) = rest.split_once(']')?;
        let (title, _) = rest.split_once("**")?;
        let (category, severity) = tag.split_once('|').unwrap_or((tag, ""));
        Some((category.trim(), severity.trim(), title.trim().trim_end_matches('.')))
    });
    let (category, severity, title) = parts.unwrap_or(("Other", "", item));
    Action {
        category: category.to_owned(),
        severity: severity.to_owned(),
        headline: if severity.is_empty() {
            format!("- {prefix}: {title}")
        } else {
            format!("- {prefix}: [{severity}] {title}")
        },
        line: format!("- {prefix}: {item}"),
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
        infrastructure: Vec::new(),
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
                .actions
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
                report
                    .infrastructure
                    .push(format!("- {language} / {mode} / {model}: {title}"));
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

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Keeps as many whole lines as Discord allows in a field, then says how many were left out.
fn field_value(lines: &[String], empty: &str, max_items: usize) -> String {
    let mut seen = BTreeSet::new();
    let unique: Vec<_> = lines.iter().filter(|line| seen.insert(line.as_str())).collect();
    if unique.is_empty() {
        return empty.to_owned();
    }
    let more = |hidden: usize| (hidden > 0).then(|| format!("- ...and {hidden} more"));
    for shown in (1..=unique.len().min(max_items)).rev() {
        let mut value = unique[..shown]
            .iter()
            .map(|line| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(more) = more(unique.len() - shown) {
            value = format!("{value}\n{more}");
        }
        // Discord counts UTF-16 units.
        if utf16_len(&value) <= MAX_FIELD_LENGTH {
            return value;
        }
    }
    // Even the first line is too long: cut it at a word.
    let tail = more(unique.len() - 1).map_or(String::new(), |more| format!("\n{more}"));
    let budget = MAX_FIELD_LENGTH.saturating_sub(utf16_len(&tail) + 1);
    let mut length = 0;
    let first: String = unique[0]
        .chars()
        .take_while(|ch| {
            length += ch.len_utf16();
            length <= budget
        })
        .collect();
    let first = first
        .rsplit_once(' ')
        .map_or(first.as_str(), |(head, _)| head)
        .trim_end();
    format!("{first}…{tail}")
}

fn action_lines(actions: &[&Action]) -> Vec<String> {
    let mut by_category = BTreeMap::<&str, Vec<&Action>>::new();
    for action in actions {
        by_category.entry(&action.category).or_default().push(action);
    }
    let mut categories: Vec<_> = by_category.into_iter().collect();
    categories.sort_by_key(|(_, items)| Reverse(items.len()));
    let mut lines = Vec::new();
    for (category, items) in &categories {
        let high = items
            .iter()
            .filter(|action| severity_rank(&action.severity) <= 1)
            .count();
        lines.push(if high > 0 {
            format!("- {category}: {} ({high} high)", items.len())
        } else {
            format!("- {category}: {}", items.len())
        });
    }
    // The most severe headline of each category, most common category first.
    for (_, items) in &categories {
        if let Some(action) = items.iter().min_by_key(|action| severity_rank(&action.severity)) {
            lines.push(action.headline.clone());
        }
    }
    lines
}

fn attachment(actions: &[&Action], infrastructure: &[&String]) -> String {
    let mut actions = actions.to_vec();
    actions.sort_by_key(|action| severity_rank(&action.severity));
    let sections = [
        (
            "Action items",
            actions.iter().map(|action| action.line.as_str()).collect::<Vec<_>>(),
        ),
        (
            "Infrastructure/provider problems",
            infrastructure.iter().map(|line| line.as_str()).collect(),
        ),
    ];
    let mut text = String::new();
    for (name, lines) in sections {
        let mut seen = BTreeSet::new();
        let lines: Vec<_> = lines.into_iter().filter(|line| seen.insert(*line)).collect();
        if !lines.is_empty() {
            text.push_str(&format!(
                "## {name}

{}

",
                lines.join(
                    "
"
                )
            ));
        }
    }
    text
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
    let actions: Vec<_> = reports.iter().flat_map(|report| &report.actions).collect();
    let infrastructure: Vec<_> = reports.iter().flat_map(|report| &report.infrastructure).collect();
    let percent = if total == 0 {
        0.0
    } else {
        passed as f64 * 100.0 / total as f64
    };
    let color = if total == 0 || percent < 90.0 || !infrastructure.is_empty() {
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
    let mut fields = vec![
        json!({"name": "By language", "value": field_value(&language_rates, "No analysis reports were produced.", usize::MAX), "inline": false}),
        json!({"name": "Action items", "value": field_value(&action_lines(&actions), "None", usize::MAX), "inline": false}),
    ];
    if !infrastructure.is_empty() {
        let lines: Vec<_> = infrastructure.iter().map(|line| (*line).clone()).collect();
        fields.push(json!({"name": "Infrastructure/provider problems", "value": field_value(&lines, "None", MAX_ITEMS), "inline": false}));
    }
    let payload = json!({
        "username": "SpacetimeDB LLM Benchmarks",
        "allowed_mentions": {"parse": []},
        "embeds": [{
            "title": "LLM Benchmark Analysis",
            "url": run_url,
            "description": format!("**{}** task runs passed", rate(passed, total)),
            "color": color,
            "fields": fields,
            "footer": {"text": run_label}
        }]
    });
    (payload, attachment(&actions, &infrastructure))
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
    fn aggregates_rates_and_leaves_out_model_limitations() {
        let reports = [
            report("csharp", "36/37 (97.3%)", "", ""),
            report("rust", "34/37 (91.9%)", "", "Model limitation"),
        ];
        let (payload, attachment) = build(&reports, "https://example.com/run", "Weekly run");
        let embed = &payload["embeds"][0];
        assert_eq!(embed["description"], "**70/74 (94.6%)** task runs passed");
        assert_eq!(embed["color"], YELLOW);
        assert_eq!(
            embed["fields"][0]["value"],
            "- **C#:** 36/37 (97.3%)\n- **Rust:** 34/37 (91.9%)"
        );
        assert_eq!(embed["fields"][1]["value"], "None");
        assert_eq!(embed["fields"].as_array().unwrap().len(), 2);
        assert_eq!(attachment, "");
        assert_eq!(payload["allowed_mentions"], json!({"parse": []}));
        assert_eq!(embed["url"], "https://example.com/run");
        assert_eq!(embed["footer"]["text"], "Weekly run");
    }

    #[test]
    fn groups_actions_by_category_and_attaches_the_details() {
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
        let fields = &payload["embeds"][0]["fields"];
        assert_eq!(
            fields[1]["value"],
            "- Eval problem: 2 (1 high)\n- API/ergonomics problem: 1\n- Rust / guidelines: [High] Capture build output\n- C# / guidelines: [Low] Rename Index"
        );
        assert_eq!(fields[2]["name"], "Infrastructure/provider problems");
        assert_eq!(
            fields[2]["value"],
            "- Rust / guidelines / test-model: Incorrect sum-type syntax"
        );
        assert_eq!(payload["embeds"][0]["color"], RED);
        assert_eq!(
            attachment,
            "## Action items\n\n- Rust / guidelines: **[Eval problem | High] Capture build output** — Evidence: t_001.\n- C# / guidelines: **[Eval problem | Medium] Keep publish errors.** Store all stderr.\n- C# / guidelines: **[API/ergonomics problem | Low] Rename Index.** It collides.\n\n## Infrastructure/provider problems\n\n- Rust / guidelines / test-model: Incorrect sum-type syntax\n\n"
        );
    }

    #[test]
    fn colors_reflect_pass_rate_actions_and_infrastructure() {
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
    }

    #[test]
    fn empty_and_long_fields_stay_valid() {
        let (payload, _) = build(&[], "", "");
        assert_eq!(payload["embeds"][0]["color"], RED);
        assert_eq!(
            payload["embeds"][0]["fields"][0]["value"],
            "No analysis reports were produced."
        );
        let lines: Vec<_> = (0..6).map(|i| format!("- {i} {}", "🦀".repeat(400))).collect();
        let value = field_value(&lines, "None", usize::MAX);
        assert!(utf16_len(&value) <= MAX_FIELD_LENGTH);
        assert!(value.ends_with("\n- ...and 5 more"));
        let value = field_value(&["word ".repeat(300)], "None", usize::MAX);
        assert!(utf16_len(&value) <= MAX_FIELD_LENGTH);
        assert!(value.ends_with("word…"));
        let mut lines: Vec<_> = (0..6).map(|i| format!("- {i}")).collect();
        lines.insert(1, "- 0".to_owned());
        assert_eq!(
            field_value(&lines, "None", 5),
            "- 0\n- 1\n- 2\n- 3\n- 4\n- ...and 1 more"
        );
    }
}
