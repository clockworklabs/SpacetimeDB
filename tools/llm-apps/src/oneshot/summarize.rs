//! Aggregates graded one-shot apps into `docs/llms/oneshot-summary.md` (for people) and
//! `docs/llms/oneshot-grades.json` (for the website).

use crate::format::{js_number, to_fixed};
use crate::paths;
use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Backend {
    Spacetime,
    Postgres,
}

impl Backend {
    fn key(self) -> &'static str {
        match self {
            Backend::Spacetime => "spacetime",
            Backend::Postgres => "postgres",
        }
    }
}

#[derive(Serialize)]
struct FeatureScore {
    feature: u32,
    name: String,
    #[serde(serialize_with = "js")]
    score: f64,
    #[serde(serialize_with = "js")]
    max: f64,
}

/// Serializes a number the way `JSON.stringify` writes it.
fn js<S: serde::Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::format::json_number(*x).serialize(s)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GradeResult {
    app: String,
    language: String,
    llm: String,
    backend: Backend,
    run_id: String,
    date: String,
    prompt_level: Option<u32>,
    #[serde(serialize_with = "js")]
    total_score: f64,
    #[serde(serialize_with = "js")]
    max_score: f64,
    #[serde(serialize_with = "js")]
    percentage: f64,
    compiles: bool,
    runs: bool,
    loc_backend: Option<u64>,
    loc_frontend: Option<u64>,
    num_files: Option<u64>,
    feature_scores: Vec<FeatureScore>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    runs: usize,
    #[serde(serialize_with = "js")]
    avg_percent: f64,
    #[serde(serialize_with = "js")]
    best: f64,
    #[serde(serialize_with = "js")]
    worst: f64,
    #[serde(serialize_with = "js")]
    total_score: f64,
    #[serde(serialize_with = "js")]
    max_score: f64,
}

impl Stats {
    fn of<'a>(results: impl IntoIterator<Item = &'a GradeResult>) -> Option<Stats> {
        let results: Vec<_> = results.into_iter().collect();
        if results.is_empty() {
            return None;
        }
        let percentages: Vec<f64> = results.iter().map(|r| r.percentage).collect();
        let round1 = |x: f64| (x * 10.0).round() / 10.0;
        Some(Stats {
            runs: results.len(),
            avg_percent: round1(percentages.iter().sum::<f64>() / percentages.len() as f64),
            best: round1(percentages.iter().copied().fold(f64::MIN, f64::max)),
            worst: round1(percentages.iter().copied().fold(f64::MAX, f64::min)),
            total_score: results.iter().map(|r| r.total_score).sum(),
            max_score: results.iter().map(|r| r.max_score).sum(),
        })
    }
}

/// Spacetime and Postgres stats for one slice of the results, in that order.
#[derive(Serialize)]
struct ByBackend {
    #[serde(skip_serializing_if = "Option::is_none")]
    spacetime: Option<Stats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    postgres: Option<Stats>,
}

impl ByBackend {
    fn of(results: &[&GradeResult]) -> ByBackend {
        let of = |b| Stats::of(results.iter().copied().filter(|r| r.backend == b));
        ByBackend {
            spacetime: of(Backend::Spacetime),
            postgres: of(Backend::Postgres),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    by_backend: ByBackend,
    by_llm: BTreeMap<String, ByBackend>,
    by_app: BTreeMap<String, ByBackend>,
}

impl Summary {
    fn of(results: &[GradeResult]) -> Summary {
        let all: Vec<&GradeResult> = results.iter().collect();
        let group = |key: fn(&GradeResult) -> &str| {
            let mut keys: Vec<&str> = results.iter().map(key).collect();
            keys.sort_unstable();
            keys.dedup();
            keys.into_iter()
                .map(|k| {
                    let slice: Vec<&GradeResult> = results.iter().filter(|r| key(r) == k).collect();
                    (k.to_owned(), ByBackend::of(&slice))
                })
                .collect()
        };
        Summary {
            by_backend: ByBackend::of(&all),
            by_llm: group(|r| &r.llm),
            by_app: group(|r| &r.app),
        }
    }
}

/// Every `GRADING_RESULTS.md` under `dir`, in path order.
fn find_grading_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if entry.file_type()?.is_dir() {
            out.extend(find_grading_files(&entry.path())?);
        } else if entry.file_name() == "GRADING_RESULTS.md" {
            out.push(entry.path());
        }
    }
    Ok(out)
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

static TOTAL: LazyLock<Regex> = LazyLock::new(|| re(r"\*\*Total Feature Score\*\*\s*\|\s*(\d+(?:\.\d+)?)\s*/\s*(\d+)"));
static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"\*\*Date:\*\*\s*(\d{4}-\d{2}-\d{2})"));
static LEVEL: LazyLock<Regex> = LazyLock::new(|| re(r"\*\*Prompt Level Used\*\*\s*\|\s*(\d+)"));
static COMPILES: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)- \[x\] Compiles"));
static RUNS: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)- \[x\] Runs"));
static LOC_BACKEND: LazyLock<Regex> = LazyLock::new(|| re(r"Lines of code \(backend\)\s*\|\s*~?(\d+)"));
static LOC_FRONTEND: LazyLock<Regex> = LazyLock::new(|| re(r"Lines of code \(frontend\)\s*\|\s*~?(\d+)"));
static NUM_FILES: LazyLock<Regex> = LazyLock::new(|| re(r"Number of files(?:\s+created)?\s*\|\s*(\d+)"));
static FEATURE: LazyLock<Regex> =
    LazyLock::new(|| re(r"## Feature (\d+):\s*([^(]+?)\s*\(Score:\s*(\d+(?:\.\d+)?)\s*/\s*(\d+)\)"));

/// Parses `apps/<app>/<lang>/<llm>/<backend>/<run>/GRADING_RESULTS.md`, warning and
/// returning `None` when the path or the file does not have the expected shape.
fn parse_grading_file(file: &Path, apps_dir: &Path, content: &str) -> Option<GradeResult> {
    let parts: Vec<String> = file
        .strip_prefix(apps_dir)
        .ok()?
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if parts.len() < 6 {
        eprintln!("Unexpected path structure: {}", file.display());
        return None;
    }
    let backend = match parts[3].as_str() {
        "spacetime" => Backend::Spacetime,
        "postgres" => Backend::Postgres,
        other => {
            eprintln!("Unknown backend \"{other}\" in: {}", file.display());
            return None;
        }
    };

    let Some(total) = TOTAL.captures(content) else {
        eprintln!("Could not parse total score from: {}", file.display());
        return None;
    };
    let total_score: f64 = total[1].parse().ok()?;
    let max_score: f64 = total[2].parse().ok()?;
    let int = |r: &Regex| r.captures(content).and_then(|c| c[1].parse().ok());

    Some(GradeResult {
        app: parts[0].clone(),
        language: parts[1].clone(),
        llm: parts[2].clone(),
        backend,
        run_id: parts[4].clone(),
        date: DATE.captures(content).map(|c| c[1].to_owned()).unwrap_or_default(),
        prompt_level: int(&LEVEL).map(|n: u64| n as u32),
        total_score,
        max_score,
        percentage: if max_score > 0.0 {
            total_score / max_score * 100.0
        } else {
            0.0
        },
        compiles: COMPILES.is_match(content),
        runs: RUNS.is_match(content),
        loc_backend: int(&LOC_BACKEND),
        loc_frontend: int(&LOC_FRONTEND),
        num_files: int(&NUM_FILES),
        feature_scores: FEATURE
            .captures_iter(content)
            .filter_map(|c| {
                Some(FeatureScore {
                    feature: c[1].parse().ok()?,
                    name: c[2].trim().to_owned(),
                    score: c[3].parse().ok()?,
                    max: c[4].parse().ok()?,
                })
            })
            .collect(),
    })
}

fn signed(x: f64, digits: usize) -> String {
    if x >= 0.0 {
        format!("+{}", to_fixed(x, digits))
    } else {
        to_fixed(x, digits)
    }
}

fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
}

fn summary_markdown(summary: &Summary, results: &[GradeResult], today: &str) -> String {
    let mut lines: Vec<String> = vec![
        "# LLM One-Shot Benchmark Summary".into(),
        String::new(),
        format!("**Generated:** {today}"),
        format!("**Total Runs:** {}", results.len()),
        String::new(),
        "## Overall Results by Backend".into(),
        String::new(),
        "| Backend | Runs | Avg Score | Best | Worst |".into(),
        "|---------|------|-----------|------|-------|".into(),
    ];
    let row = |label: &str, s: &Stats| {
        format!(
            "| {label} | {} | {}% | {}% | {}% |",
            s.runs,
            to_fixed(s.avg_percent, 1),
            to_fixed(s.best, 1),
            to_fixed(s.worst, 1)
        )
    };
    let by = &summary.by_backend;
    if let Some(s) = &by.spacetime {
        lines.push(row("SpacetimeDB", s));
    }
    if let Some(p) = &by.postgres {
        lines.push(row("PostgreSQL", p));
    }
    if let (Some(s), Some(p)) = (&by.spacetime, &by.postgres) {
        lines.push(String::new());
        lines.push(format!(
            "**SpacetimeDB advantage:** {} percentage points",
            signed(s.avg_percent - p.avg_percent, 1)
        ));
    }
    lines.push(String::new());

    lines.push("## Results by LLM".into());
    lines.push(String::new());
    lines.push("| LLM | STDB Runs | STDB Avg | PG Runs | PG Avg | Delta |".into());
    lines.push("|-----|-----------|----------|---------|--------|-------|".into());
    for (llm, stats) in &summary.by_llm {
        let runs = |s: &Option<Stats>| s.as_ref().map_or("-".into(), |s| s.runs.to_string());
        let avg = |s: &Option<Stats>| {
            s.as_ref()
                .map_or("-".into(), |s| format!("{}%", to_fixed(s.avg_percent, 1)))
        };
        let delta = match (&stats.spacetime, &stats.postgres) {
            (Some(s), Some(p)) => format!("{}%", signed(s.avg_percent - p.avg_percent, 1)),
            _ => "-".into(),
        };
        lines.push(format!(
            "| {llm} | {} | {} | {} | {} | {delta} |",
            runs(&stats.spacetime),
            avg(&stats.spacetime),
            runs(&stats.postgres),
            avg(&stats.postgres)
        ));
    }
    lines.push(String::new());

    // Per-feature averages; each feature keeps the name and max of its first appearance.
    let mut features: BTreeMap<u32, (String, f64, Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for r in results {
        for f in &r.feature_scores {
            let entry = features
                .entry(f.feature)
                .or_insert_with(|| (f.name.clone(), f.max, vec![], vec![]));
            match r.backend {
                Backend::Spacetime => entry.2.push(f.score),
                Backend::Postgres => entry.3.push(f.score),
            }
        }
    }
    if !features.is_empty() {
        lines.push("## Feature Scores (Average)".into());
        lines.push(String::new());
        lines.push("| Feature | Max | STDB Avg | PG Avg | Winner |".into());
        lines.push("|---------|-----|----------|--------|--------|".into());
        for (num, (name, max, stdb, pg)) in &features {
            let (stdb, pg) = (mean(stdb), mean(pg));
            let fmt = |m: Option<f64>| m.map_or("-".into(), |m| to_fixed(m, 2));
            let winner = match (stdb, pg) {
                (Some(s), Some(p)) if s > p => "STDB",
                (Some(s), Some(p)) if p > s => "PG",
                (Some(_), Some(_)) => "Tie",
                _ => "-",
            };
            lines.push(format!(
                "| {num}. {name} | {} | {} | {} | {winner} |",
                js_number(*max),
                fmt(stdb),
                fmt(pg)
            ));
        }
        lines.push(String::new());
    }

    lines.push("## All Runs".into());
    lines.push(String::new());
    lines.push("| LLM | Backend | Date | Score | % | Level |".into());
    lines.push("|-----|---------|------|-------|---|-------|".into());
    let mut sorted: Vec<&GradeResult> = results.iter().collect();
    sorted.sort_by(|a, b| {
        a.llm
            .cmp(&b.llm)
            .then_with(|| a.backend.key().cmp(b.backend.key()))
            .then_with(|| b.date.cmp(&a.date))
    });
    for r in sorted {
        lines.push(format!(
            "| {} | {} | {} | {}/{} | {}% | {} |",
            r.llm,
            if r.backend == Backend::Spacetime { "STDB" } else { "PG" },
            r.date,
            js_number(r.total_score),
            js_number(r.max_score),
            to_fixed(r.percentage, 1),
            r.prompt_level.map_or("-".into(), |l| l.to_string())
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

pub fn run(apps_dir: &Path, app: Option<&str>, llm: Option<&str>) -> Result<()> {
    let output_dir = paths::repo_root().join("docs/llms");
    println!("Scanning for grading files in: {}", apps_dir.display());
    println!("Output directory: {}", output_dir.display());

    let files = find_grading_files(apps_dir)?;
    println!("Found {} grading files", files.len());
    let mut results = Vec::new();
    for file in &files {
        let content = fs::read_to_string(file).with_context(|| format!("failed to read {}", file.display()))?;
        results.extend(parse_grading_file(file, apps_dir, &content));
    }
    println!("Successfully parsed {} grading files", results.len());

    if let Some(app) = app {
        results.retain(|r| r.app == app);
        println!("Filtered to app \"{app}\": {} results", results.len());
    }
    if let Some(llm) = llm {
        results.retain(|r| r.llm == llm);
        println!("Filtered to LLM \"{llm}\": {} results", results.len());
    }
    if results.is_empty() {
        println!("No results to summarize.");
        return Ok(());
    }

    fs::create_dir_all(&output_dir)?;
    let summary = Summary::of(&results);
    let now = chrono::Utc::now();

    let summary_path = output_dir.join("oneshot-summary.md");
    fs::write(
        &summary_path,
        summary_markdown(&summary, &results, &now.format("%Y-%m-%d").to_string()),
    )?;
    println!("Wrote: {}", summary_path.display());

    #[derive(Serialize)]
    struct Grades<'a> {
        generated: String,
        summary: &'a Summary,
        runs: &'a [GradeResult],
    }
    let grades = Grades {
        generated: now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        summary: &summary,
        runs: &results,
    };
    let json_path = output_dir.join("oneshot-grades.json");
    fs::write(&json_path, serde_json::to_string_pretty(&grades)?)?;
    println!("Wrote: {}", json_path.display());

    println!("\nDone!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRADING: &str = "\
# Grading Results
**Date:** 2026-01-05

| **Total Feature Score** | 34.5 / 36 |
| **Prompt Level Used** | 9 (threading) |
| Lines of code (backend) | ~650 |
| Lines of code (frontend) | 1200 |
| Number of files created | 14 |

- [x] Compiles
- [ ] Runs

## Feature 1: Basic Chat Features (Score: 3 / 3)
## Feature 2: Typing Indicators (Score: 1.5 / 3)
";

    #[test]
    fn parses_a_grading_file() {
        let apps = Path::new("/r/apps");
        let file = apps.join("chat-app/typescript/opus-4-5/spacetime/chat-app-1/GRADING_RESULTS.md");
        let r = parse_grading_file(&file, apps, GRADING).unwrap();
        assert_eq!(
            (r.app.as_str(), r.llm.as_str(), r.run_id.as_str()),
            ("chat-app", "opus-4-5", "chat-app-1")
        );
        assert_eq!((r.total_score, r.max_score), (34.5, 36.0));
        assert_eq!(r.date, "2026-01-05");
        assert_eq!(r.prompt_level, Some(9));
        assert!(r.compiles && !r.runs);
        assert_eq!(
            (r.loc_backend, r.loc_frontend, r.num_files),
            (Some(650), Some(1200), Some(14))
        );
        let features: Vec<_> = r
            .feature_scores
            .iter()
            .map(|f| (f.feature, f.name.as_str(), f.score))
            .collect();
        assert_eq!(
            features,
            vec![(1, "Basic Chat Features", 3.0), (2, "Typing Indicators", 1.5)]
        );
    }

    #[test]
    fn rejects_unknown_backends_and_missing_totals() {
        let apps = Path::new("/r/apps");
        let mongo = apps.join("chat-app/typescript/opus-4-5/mongodb/run/GRADING_RESULTS.md");
        assert!(parse_grading_file(&mongo, apps, GRADING).is_none());
        let ok = apps.join("chat-app/typescript/opus-4-5/postgres/run/GRADING_RESULTS.md");
        assert!(parse_grading_file(&ok, apps, "no score here").is_none());
    }

    #[test]
    fn negative_advantage_keeps_its_sign() {
        assert_eq!(signed(-3.04, 1), "-3.0");
        assert_eq!(signed(2.0, 1), "+2.0");
    }
}
