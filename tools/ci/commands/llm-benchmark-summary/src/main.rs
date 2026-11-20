use anyhow::{bail, Context, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const INFRASTRUCTURE: &str = "Infrastructure/provider problem";
const LOGO: &str = include_str!("logo.svg");
/// Discord rejects embed descriptions longer than this, counted in UTF-16 units.
const DESCRIPTION_LIMIT: usize = 4096;
const TITLE_LIMIT: usize = 200;
const CATEGORY_LIMIT: usize = 60;

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
    /// Details of the problems on our side, as .html, .md or .txt by extension; empty when there are none.
    #[arg(long)]
    attachment: PathBuf,
}

#[derive(Clone, Copy)]
enum Format {
    Html,
    Markdown,
    Text,
}

/// A problem on our side: a recommended action or an infrastructure failure.
struct Problem {
    category: String,
    /// How sure the analyst is, not how bad the problem is.
    confidence: String,
    source: String,
    title: String,
    details: Vec<String>,
}

struct Report {
    language: String,
    mode: String,
    passed_tasks: u64,
    total_tasks: u64,
    problems: Vec<Problem>,
}

type Counts = (u64, u64);

struct Summary<'a> {
    by_mode: BTreeMap<&'a str, (Counts, BTreeMap<&'a str, Counts>)>,
    /// Most common category first; within one, most confident first.
    categories: Vec<(&'a str, Vec<&'a Problem>)>,
}

fn field<'a>(text: &'a str, name: &str) -> &'a str {
    let prefix = format!("- {name}: ");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or("unknown")
}

fn mode_name(mode: &str) -> &str {
    match mode {
        "guidelines" => "Guidelines",
        "no_context" => "No context",
        other => other,
    }
}

fn language_name(language: &str) -> &str {
    match language {
        "csharp" => "C#",
        "rust" => "Rust",
        "typescript" => "TypeScript",
        other => other,
    }
}

fn confidence_rank(confidence: &str) -> u8 {
    match confidence.to_ascii_lowercase().as_str() {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

fn clip(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", text[..end].trim_end()),
        None => text.to_owned(),
    }
}

/// Splits `**[Category | Confidence] Title.** Details` into its parts.
fn parse_action(source: &str, item: &str) -> Problem {
    const SEPARATORS: [char; 5] = [' ', '—', '–', '-', ':'];
    let parts = item.strip_prefix("**[").and_then(|rest| {
        let (tag, rest) = rest.split_once(']')?;
        let (title, details) = rest.split_once("**")?;
        let (category, confidence) = tag.split_once('|').unwrap_or((tag, ""));
        Some((
            category.trim(),
            confidence.trim(),
            title.trim(),
            details.trim_start_matches(SEPARATORS).trim(),
        ))
    });
    let (category, confidence, mut title, mut details) = parts.unwrap_or(("Other", "", item.trim(), ""));
    if title.is_empty() {
        // The bold closed right after the tag, so the title is the first clause of the details.
        (title, details) = details
            .split_once(" — ")
            .or_else(|| details.split_once(": "))
            .map_or((details, ""), |(title, rest)| (title.trim(), rest.trim()));
    }
    Problem {
        category: clip(category, CATEGORY_LIMIT),
        confidence: confidence.to_owned(),
        source: source.to_owned(),
        title: clip(title.replace("**", "").trim_end_matches('.'), TITLE_LIMIT),
        details: (!details.is_empty())
            .then(|| details.replace("**", ""))
            .into_iter()
            .collect(),
    }
}

fn is_infrastructure(classification: &str) -> bool {
    classification
        .trim()
        .trim_matches('`')
        .trim_end_matches('.')
        .trim()
        .eq_ignore_ascii_case(INFRASTRUCTURE)
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
        mode: mode.to_owned(),
        passed_tasks,
        total_tasks,
        problems: Vec::new(),
    };
    let source = format!("{} / {mode} / {model}", language_name(language));
    let mut section = "";
    let mut title = "";
    // Indented lines right after an action belong to it.
    let mut in_action = false;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading;
            title = "";
            in_action = false;
        } else if section == "Recommended actions" {
            if let Some(item) = line.strip_prefix("- **[").map(|_| &line[2..]) {
                report.problems.push(parse_action(&source, item));
                in_action = true;
            } else if in_action && line.starts_with([' ', '\t']) && !line.trim().is_empty() {
                if let Some(problem) = report.problems.last_mut() {
                    problem.details.push(line.trim().replace("**", ""));
                }
            } else {
                in_action = false;
            }
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
            } else if line
                .strip_prefix("- **Classification:**")
                .is_some_and(is_infrastructure)
                && !title.is_empty()
            {
                report.problems.push(Problem {
                    category: INFRASTRUCTURE.to_owned(),
                    confidence: String::new(),
                    source: source.clone(),
                    title: clip(title, TITLE_LIMIT),
                    details: Vec::new(),
                });
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

/// Rounded to one decimal, so the shown rate and its color always agree.
fn percent((passed, total): Counts) -> f64 {
    if total == 0 {
        0.0
    } else {
        (passed as f64 * 1000.0 / total as f64).round() / 10.0
    }
}

/// `good`, `warn` or `bad`; also the CSS class for the HTML report.
fn state(rate: f64) -> &'static str {
    if rate >= 95.0 {
        "good"
    } else if rate >= 90.0 {
        "warn"
    } else {
        "bad"
    }
}

fn dot(rate: f64) -> &'static str {
    match state(rate) {
        "good" => "🟢",
        "warn" => "🟡",
        _ => "🔴",
    }
}

fn summarize(reports: &[Report]) -> Summary<'_> {
    let mut by_mode = BTreeMap::<&str, (Counts, BTreeMap<&str, Counts>)>::new();
    let mut by_category = BTreeMap::<&str, Vec<&Problem>>::new();
    for report in reports {
        let (totals, languages) = by_mode.entry(&report.mode).or_default();
        let counts = languages.entry(&report.language).or_default();
        for counts in [totals, counts] {
            counts.0 += report.passed_tasks;
            counts.1 += report.total_tasks;
        }
        for problem in &report.problems {
            by_category.entry(&problem.category).or_default().push(problem);
        }
    }
    let mut categories: Vec<_> = by_category.into_iter().collect();
    categories.sort_by_key(|(_, problems)| Reverse(problems.len()));
    for (_, problems) in &mut categories {
        problems.sort_by_key(|problem| confidence_rank(&problem.confidence));
    }
    Summary { by_mode, categories }
}

fn payload(summary: &Summary, run_url: &str, run_label: &str) -> Value {
    let total: u64 = summary.by_mode.values().map(|((_, total), _)| total).sum();
    let rates = if total == 0 {
        "No analysis reports were produced.".to_owned()
    } else {
        summary
            .by_mode
            .iter()
            .map(|(mode, (counts, languages))| {
                let languages: Vec<_> = languages
                    .iter()
                    .map(|(language, counts)| format!("{} {:.1}%", language_name(language), percent(*counts)))
                    .collect();
                let rate = percent(*counts);
                format!(
                    "{} **{} {rate:.1}%** · {}",
                    dot(rate),
                    mode_name(mode),
                    languages.join(" · ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut lines: Vec<_> = summary
        .categories
        .iter()
        .map(|(category, problems)| format!("- {category} ({}): {}", problems.len(), problems[0].title))
        .collect();
    let mut hidden = 0;
    let description = loop {
        let mut description = rates.clone();
        if !lines.is_empty() || hidden > 0 {
            description = format!("{description}\n\n{}", lines.join("\n"));
        }
        if hidden > 0 {
            description = format!("{description}\n- ...and {hidden} more");
        }
        if lines.is_empty() || description.encode_utf16().count() <= DESCRIPTION_LIMIT {
            break description;
        }
        lines.pop();
        hidden += 1;
    };
    json!({
        "username": "Spacetime LLM Benchmarks",
        "allowed_mentions": {"parse": []},
        "embeds": [{
            "title": format!("LLM Benchmark Analysis · {run_label}"),
            "url": run_url,
            "description": description
        }]
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escapes text and turns `code` spans into <code> elements.
fn inline_html(text: &str) -> String {
    text.split('`')
        .enumerate()
        .map(|(index, part)| {
            if index % 2 == 1 {
                format!("<code>{}</code>", escape(part))
            } else {
                escape(part)
            }
        })
        .collect()
}

/// Paragraphs, with runs of `- ` lines as a list.
fn details_html(lines: &[String]) -> String {
    let mut html = String::new();
    let mut in_list = false;
    for line in lines {
        match line.strip_prefix("- ") {
            Some(item) => {
                if !in_list {
                    html.push_str("<ul>");
                    in_list = true;
                }
                html.push_str(&format!("<li>{}</li>", inline_html(item)));
            }
            None => {
                if in_list {
                    html.push_str("</ul>");
                    in_list = false;
                }
                html.push_str(&format!("<p>{}</p>", inline_html(line)));
            }
        }
    }
    if in_list {
        html.push_str("</ul>");
    }
    html
}

const STYLE: &str = r#"
:root{--bg:#0b0e12;--panel:#0f191f;--raised:#121e24;--line:#162d38;--text:#e6e9f0;--soft:#b6c0cf;--muted:#76808f;--green:#4cf490;--yellow:#fbdc8e;--red:#ff4c4c;--purple:#a880ff;--blue:#02befa}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;color:var(--text);font:15px/1.55 Inter,system-ui,sans-serif;background:radial-gradient(900px 520px at 85% -12%,rgba(138,56,245,.22),transparent 65%),radial-gradient(760px 460px at -8% -6%,rgba(76,244,144,.12),transparent 60%),var(--bg)}
main{max-width:960px;margin:0 auto;padding:40px 20px 72px}
header{display:flex;align-items:center;gap:14px;margin-bottom:36px}
header svg{flex:none;width:32px;height:30px}
h1{margin:0;font-size:22px;font-weight:600;letter-spacing:-.01em}
.label{color:var(--muted);font:13px "Source Code Pro",ui-monospace,monospace}
header a{margin-left:auto;padding:7px 14px;border:1px solid rgba(76,244,144,.35);border-radius:8px;color:var(--green);font:500 13px "Source Code Pro",ui-monospace,monospace;text-decoration:none;white-space:nowrap}
header a:hover{background:rgba(76,244,144,.1)}
.modes{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:16px;margin-bottom:44px}
.mode{padding:20px 22px;background:var(--panel);border:1px solid var(--line);border-radius:14px}
.mode h2{margin:0 0 4px;color:var(--muted);font:500 12px "Source Code Pro",ui-monospace,monospace;letter-spacing:.08em;text-transform:uppercase}
.rate{margin-bottom:16px;font-size:44px;font-weight:700;letter-spacing:-.03em}
.good{--tone:var(--green)}.warn{--tone:var(--yellow)}.bad{--tone:var(--red)}
.rate.good,.rate.warn,.rate.bad{color:var(--tone)}
.bar{display:grid;grid-template-columns:92px 1fr 52px;align-items:center;gap:12px;margin-top:8px;font-size:13px}
.bar span:last-child{text-align:right;color:var(--soft);font-family:"Source Code Pro",ui-monospace,monospace}
.track{height:6px;overflow:hidden;background:var(--raised);border-radius:3px}
.fill{height:100%;background:var(--tone);border-radius:3px}
section{margin-top:32px}
section h2{display:flex;align-items:center;gap:10px;margin:0 0 12px;font-size:16px;font-weight:600}
.count{padding:1px 9px;color:var(--purple);background:rgba(168,128,255,.14);border-radius:999px;font:500 12px "Source Code Pro",ui-monospace,monospace}
details,.row{margin:8px 0;background:var(--panel);border:1px solid var(--line);border-radius:10px}
details[open]{border-color:#1d3a48}
summary,.row{display:flex;flex-wrap:wrap;align-items:baseline;gap:6px 10px;padding:12px 16px}
summary{cursor:pointer;list-style:none}
summary::-webkit-details-marker{display:none}
summary::before,.row::before{width:8px;color:var(--muted)}
summary::before{content:"›";transition:transform .15s}
.row::before{content:""}
details[open] summary::before{transform:rotate(90deg)}
.title{flex:1 1 320px;font-weight:500}
.conf{padding:1px 8px;border-radius:999px;font:500 11px "Source Code Pro",ui-monospace,monospace;color:var(--muted);background:var(--raised)}
.conf.high{color:var(--purple);background:rgba(168,128,255,.16)}
.conf.medium{color:#c9b3ff;background:rgba(168,128,255,.08)}
.source{color:var(--muted);font:12px "Source Code Pro",ui-monospace,monospace}
.body{padding:0 16px 14px 34px;color:var(--soft)}
.body p{margin:0 0 8px}
.body ul{margin:0 0 8px;padding-left:18px}
code{padding:1px 5px;color:var(--green);background:var(--raised);border:1px solid var(--line);border-radius:4px;font:.88em "Source Code Pro",ui-monospace,monospace}
@media (max-width:560px){header{flex-wrap:wrap}header a{margin-left:0}.rate{font-size:36px}}
"#;

fn html(summary: &Summary, run_url: &str, run_label: &str) -> String {
    let modes: String = summary
        .by_mode
        .iter()
        .map(|(mode, (counts, languages))| {
            let rate = percent(*counts);
            let bars: String = languages
                .iter()
                .map(|(language, counts)| {
                    let rate = percent(*counts);
                    format!(
                        r#"<div class="bar {}"><span>{}</span><div class="track"><div class="fill" style="width:{rate:.1}%"></div></div><span>{rate:.1}%</span></div>"#,
                        state(rate),
                        escape(language_name(language))
                    )
                })
                .collect();
            format!(
                r#"<div class="mode"><h2>{}</h2><div class="rate {}">{rate:.1}%</div>{bars}</div>"#,
                escape(mode_name(mode)),
                state(rate)
            )
        })
        .collect();
    let sections: String = summary
        .categories
        .iter()
        .map(|(category, problems)| {
            let items: String = problems
                .iter()
                .map(|problem| {
                    let confidence = if problem.confidence.is_empty() {
                        String::new()
                    } else {
                        format!(
                            r#"<span class="conf {}">{} confidence</span>"#,
                            escape(&problem.confidence.to_ascii_lowercase()),
                            escape(&problem.confidence)
                        )
                    };
                    let line = format!(
                        r#"{confidence}<span class="title">{}</span><span class="source">{}</span>"#,
                        inline_html(&problem.title),
                        escape(&problem.source)
                    );
                    if problem.details.is_empty() {
                        format!(r#"<div class="row">{line}</div>"#)
                    } else {
                        format!(
                            r#"<details><summary>{line}</summary><div class="body">{}</div></details>"#,
                            details_html(&problem.details)
                        )
                    }
                })
                .collect();
            format!(
                r#"<section><h2>{}<span class="count">{}</span></h2>{items}</section>"#,
                escape(category),
                problems.len()
            )
        })
        .collect();
    let link = if run_url.is_empty() {
        String::new()
    } else {
        format!(r#"<a href="{}">View run</a>"#, escape(run_url))
    };
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>LLM Benchmark Analysis · {label}</title>
<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Source+Code+Pro:wght@400;500&display=swap">
<style>{STYLE}</style></head>
<body><main><header>{LOGO}<div><h1>LLM Benchmark Analysis</h1><div class="label">{label}</div></div>{link}</header>
<div class="modes">{modes}</div>{sections}</main></body></html>
"#,
        label = escape(run_label)
    )
}

fn attachment(summary: &Summary, format: Format, run_url: &str, run_label: &str) -> String {
    if summary.categories.is_empty() {
        return String::new();
    }
    let problems = || summary.categories.iter().flat_map(|(_, problems)| problems);
    let join = |items: Vec<String>| format!("{}\n", items.join("\n\n"));
    match format {
        Format::Html => html(summary, run_url, run_label),
        Format::Markdown => join(
            problems()
                .map(|problem| {
                    let tag = if problem.confidence.is_empty() {
                        problem.category.clone()
                    } else {
                        format!("{} | {}", problem.category, problem.confidence)
                    };
                    let mut text = format!("- {}: **[{tag}] {}.**", problem.source, problem.title);
                    for (index, line) in problem.details.iter().enumerate() {
                        text.push_str(&if index == 0 {
                            format!(" {line}")
                        } else {
                            format!("\n  {line}")
                        });
                    }
                    text
                })
                .collect(),
        ),
        // Plain text, because Discord shows a .txt attachment as a short preview that expands.
        Format::Text => join(
            problems()
                .map(|problem| {
                    let label = if problem.confidence.is_empty() {
                        format!("{}, {}", problem.category, problem.source)
                    } else {
                        format!(
                            "[{} confidence] {}, {}",
                            problem.confidence, problem.category, problem.source
                        )
                    };
                    [label, format!("{}.", problem.title)]
                        .into_iter()
                        .chain(problem.details.iter().cloned())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect(),
        ),
    }
}

fn format_of(path: &Path) -> Result<Format> {
    Ok(match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => Format::Html,
        Some("md") => Format::Markdown,
        Some("txt") => Format::Text,
        _ => bail!("attachment {} must end in .html, .md or .txt", path.display()),
    })
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let format = format_of(&args.attachment)?;
    let reports = load_reports(&args.reports_dir)?;
    let summary = summarize(&reports);
    fs::write(
        &args.output,
        serde_json::to_vec(&payload(&summary, &args.run_url, &args.run_label))?,
    )
    .with_context(|| format!("writing Discord payload {}", args.output.display()))?;
    fs::write(
        &args.attachment,
        attachment(&summary, format, &args.run_url, &args.run_label),
    )
    .with_context(|| format!("writing Discord attachment {}", args.attachment.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(language: &str, tasks: &str, action: &str, classification: &str) -> Report {
        report_in("guidelines", language, tasks, action, classification)
    }

    fn report_in(mode: &str, language: &str, tasks: &str, action: &str, classification: &str) -> Report {
        parse_report(&format!(
            "# LLM Benchmark Analysis\n\n- Language: {language}\n- Mode: {mode}\n- Model: test-model\n- Tasks: {tasks}\n- Scorers: 100/100 (100.0%)\n\n## Recommended actions\n\n{action}\n\n## Failure patterns\n\n### Incorrect sum-type syntax (1 task)\n\n- **Classification:** {classification}\n"
        ))
    }

    fn only_problem(action: &str) -> Problem {
        report("csharp", "1/1", action, "").problems.remove(0)
    }

    #[test]
    fn summarizes_rates_and_leaves_out_model_limitations() {
        let reports = [
            report("csharp", "36/37 (97.3%)", "", ""),
            report("rust", "34/37 (91.9%)", "", "Model limitation"),
            report_in("no_context", "rust", "20/37 (54.1%)", "", "Model limitation"),
        ];
        let summary = summarize(&reports);
        let payload = payload(&summary, "https://example.com/run", "Weekly run");
        let embed = &payload["embeds"][0];
        assert_eq!(
            embed["description"],
            "🟡 **Guidelines 94.6%** · C# 97.3% · Rust 91.9%\n🔴 **No context 54.1%** · Rust 54.1%"
        );
        assert_eq!(embed["title"], "LLM Benchmark Analysis · Weekly run");
        assert_eq!(embed["url"], "https://example.com/run");
        assert_eq!(payload["allowed_mentions"], json!({"parse": []}));
        for format in [Format::Html, Format::Markdown, Format::Text] {
            assert_eq!(attachment(&summary, format, "", ""), "");
        }
    }

    #[test]
    fn lists_one_line_per_category_and_attaches_the_details() {
        let reports = [
            report(
                "csharp",
                "37/37",
                "- **[Eval problem | Medium] Keep publish errors.** Store all `stderr`.\n  - Keep every line.\n  Evidence: t_002.\n- **[API/ergonomics problem | Low] Rename Index.** It collides.",
                "",
            ),
            report(
                "rust",
                "37/37",
                "- **[Eval problem | High] Capture build output** — Evidence: t_001.",
                INFRASTRUCTURE,
            ),
        ];
        let summary = summarize(&reports);
        assert_eq!(
            payload(&summary, "", "")["embeds"][0]["description"],
            "🟢 **Guidelines 100.0%** · C# 100.0% · Rust 100.0%\n\n- Eval problem (2): Capture build output\n- API/ergonomics problem (1): Rename Index\n- Infrastructure/provider problem (1): Incorrect sum-type syntax"
        );
        assert_eq!(
            attachment(&summary, Format::Text, "", ""),
            "[High confidence] Eval problem, Rust / guidelines / test-model\nCapture build output.\nEvidence: t_001.\n\n[Medium confidence] Eval problem, C# / guidelines / test-model\nKeep publish errors.\nStore all `stderr`.\n- Keep every line.\nEvidence: t_002.\n\n[Low confidence] API/ergonomics problem, C# / guidelines / test-model\nRename Index.\nIt collides.\n\nInfrastructure/provider problem, Rust / guidelines / test-model\nIncorrect sum-type syntax.\n"
        );
        assert_eq!(
            attachment(&summary, Format::Markdown, "", ""),
            "- Rust / guidelines / test-model: **[Eval problem | High] Capture build output.** Evidence: t_001.\n\n- C# / guidelines / test-model: **[Eval problem | Medium] Keep publish errors.** Store all `stderr`.\n  - Keep every line.\n  Evidence: t_002.\n\n- C# / guidelines / test-model: **[API/ergonomics problem | Low] Rename Index.** It collides.\n\n- Rust / guidelines / test-model: **[Infrastructure/provider problem] Incorrect sum-type syntax.**\n"
        );
        let html = attachment(&summary, Format::Html, "", "");
        assert!(html.contains(
            "<p>Store all <code>stderr</code>.</p><ul><li>Keep every line.</li></ul><p>Evidence: t_002.</p>"
        ));
        assert!(html.contains(r#"<span class="conf high">High confidence</span>"#));
        // An infrastructure problem has no details, so it does not expand.
        assert!(html.contains(r#"<div class="row"><span class="title">Incorrect sum-type syntax</span>"#));
    }

    #[test]
    fn parses_loosely_formatted_items() {
        let problem = only_problem("- **[Eval problem | Medium] Capture diagnostics**: Each error is cut.");
        assert_eq!(
            (problem.title.as_str(), problem.details[0].as_str()),
            ("Capture diagnostics", "Each error is cut.")
        );
        let problem = only_problem("- **[Eval problem | High]** Bold closes early — details here");
        assert_eq!(
            (problem.title.as_str(), problem.details[0].as_str()),
            ("Bold closes early", "details here")
        );
        let problem = only_problem(&format!("- **[Eval problem | High] {}", "x".repeat(4200)));
        assert_eq!(problem.category, "Other");
        assert_eq!(problem.title.chars().count(), TITLE_LIMIT + 1);
        for classification in ["`Infrastructure/provider problem`", "Infrastructure/provider problem."] {
            assert_eq!(
                report("rust", "1/1", "", classification).problems[0].category,
                INFRASTRUCTURE
            );
        }
    }

    #[test]
    fn description_stays_within_discord_limit() {
        let actions: String = (0..60)
            .map(|index| {
                format!(
                    "- **[Category {index} {} | High] {}.** Text.\n",
                    "c".repeat(80),
                    "t".repeat(300)
                )
            })
            .collect();
        let reports = [report("csharp", "1/1", &actions, "")];
        let description = payload(&summarize(&reports), "", "")["embeds"][0]["description"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(description.encode_utf16().count() <= DESCRIPTION_LIMIT);
        assert!(description.ends_with(" more"));
    }

    #[test]
    fn html_escapes_report_text() {
        let reports = [report(
            "csharp",
            "1/2",
            "- **[Eval problem | High] Fix <script>.** Use `a<b` & more.",
            "",
        )];
        let html = attachment(
            &summarize(&reports),
            Format::Html,
            "https://example.com/?a=1&b=2",
            "Manual run",
        );
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Fix &lt;script&gt;"));
        assert!(html.contains("Use <code>a&lt;b</code> &amp; more."));
        assert!(html.contains(r#"href="https://example.com/?a=1&amp;b=2""#));
        assert!(html.contains(r#"<div class="rate bad">50.0%</div>"#));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn dots_match_the_rounded_rate_and_formats_follow_the_extension() {
        assert_eq!([dot(95.0), dot(94.9), dot(90.0), dot(89.9)], ["🟢", "🟡", "🟡", "🔴"]);
        // 2188/2304 is 94.97%, shown as 95.0%.
        let reports = [report("csharp", "2188/2304", "", "")];
        assert_eq!(
            payload(&summarize(&reports), "", "")["embeds"][0]["description"],
            "🟢 **Guidelines 95.0%** · C# 95.0%"
        );
        assert!(matches!(format_of(Path::new("a.html")), Ok(Format::Html)));
        assert!(format_of(Path::new("a.pdf")).is_err());
        let reports: [Report; 0] = [];
        assert_eq!(
            payload(&summarize(&reports), "", "")["embeds"][0]["description"],
            "No analysis reports were produced."
        );
    }
}
