//! `report`: a run's BENCHMARK_REPORT.md, from each backend's cost summaries, grades and
//! lines of code.

use super::subdirs;
use crate::format::{grouped, to_fixed};
use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// One session's cost-summary.json.
struct Session {
    level: i64,
    cost: f64,
    calls: f64,
    tokens: f64,
    duration_sec: f64,
    variant: Option<String>,
    rules: Option<String>,
}

/// A backend's totals. A level that took several sessions (an upgrade and its fixes)
/// counts one of them: the last in telemetry-directory name order, which is how the
/// published reports were computed. Since `spacetime-fix-level1-*` sorts before
/// `spacetime-level1-*`, that is usually the session that first built the level.
struct Totals {
    cost: f64,
    calls: f64,
    tokens: f64,
    duration_sec: f64,
    by_level: BTreeMap<i64, Session>,
}

impl Totals {
    fn of(mut sessions: Vec<Session>) -> Totals {
        // Stable, so sessions of one level stay in directory-name order.
        sessions.sort_by_key(|s| s.level);
        let by_level: BTreeMap<i64, Session> = sessions.into_iter().map(|s| (s.level, s)).collect();
        let sum = |f: fn(&Session) -> f64| by_level.values().map(f).sum();
        Totals {
            cost: sum(|s| s.cost),
            calls: sum(|s| s.calls),
            tokens: sum(|s| s.tokens),
            duration_sec: sum(|s| s.duration_sec),
            by_level,
        }
    }
}

fn read_sessions(run: &Path) -> Result<BTreeMap<String, Vec<Session>>> {
    let mut by_backend: BTreeMap<String, Vec<Session>> = BTreeMap::new();
    for backend_dir in subdirs(run) {
        for session_dir in subdirs(&backend_dir.join("telemetry")) {
            let path = session_dir.join("cost-summary.json");
            let Ok(text) = fs::read_to_string(&path) else { continue };
            let v: Value =
                serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
            let num = |k: &str| v[k].as_f64().unwrap_or(0.0);
            let string = |k: &str| v[k].as_str().map(str::to_owned);
            let session = Session {
                level: v["level"].as_i64().unwrap_or(0),
                cost: num("totalCostUsd"),
                calls: num("apiCalls"),
                tokens: num("totalTokens"),
                duration_sec: num("totalDurationSec"),
                variant: string("variant"),
                rules: string("rules"),
            };
            let backend = string("backend").unwrap_or_else(|| "unknown".into());
            by_backend.entry(backend).or_default().push(session);
        }
    }
    Ok(by_backend)
}

/// The backend's newest app (`<backend>/results/chat-app-*`, by name).
fn latest_app(run: &Path, backend: &str) -> Option<PathBuf> {
    subdirs(&run.join(backend).join("results")).into_iter().rfind(|d| {
        d.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("chat-app-"))
    })
}

static TOTAL_SLASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*TOTAL\*\*.*?\*\*(\d+)\s*/\s*(\d+)\*\*").unwrap());
static TOTAL_CELLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*TOTAL\*\*.*?\*\*(\d+)\*\*.*?\*\*(\d+)\*\*").unwrap());
static TOTAL_PROSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Total Feature Score.*?(\d+)\s*/\s*(\d+)").unwrap());

/// The feature score `(score, max)` from a GRADING_RESULTS.md.
fn grading_score(content: &str) -> Option<(u64, u64)> {
    let pair = |c: regex::Captures| Some((c[1].parse().ok()?, c[2].parse().ok()?));
    // Canonical: `| **TOTAL** | **X/Y** | |`.
    if let Some(c) = TOTAL_SLASH.captures(content) {
        return pair(c);
    }
    // Older files: `**TOTAL** | **MAX** | **SCORE**`.
    if let Some(c) = TOTAL_CELLS.captures(content) {
        return pair(c).map(|(max, score)| (score, max));
    }
    // Prose: `Total Feature Score: X / Y`.
    TOTAL_PROSE.captures(content).and_then(pair)
}

/// Lines of TypeScript and JavaScript under `dir`, leaving out dependencies, build output,
/// generated bindings and level snapshots.
fn count_lines(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    let mut total = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), "node_modules" | "dist" | ".vite" | "module_bindings") || name.starts_with("level-")
        {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            total += count_lines(&path);
        } else if [".ts", ".tsx", ".js", ".jsx"].iter().any(|ext| name.ends_with(ext)) {
            // Counts like `split('\n').length`, so a trailing newline adds a line.
            total += fs::read_to_string(&path).map_or(0, |t| t.split('\n').count() as u64);
        }
    }
    total
}

/// `(backend, frontend)` lines of code of the backend's newest app.
fn lines_of_code(run: &Path, backend: &str) -> Option<(u64, u64)> {
    let app = latest_app(run, backend)?;
    let express = app.join("server");
    let server = if express.is_dir() {
        count_lines(&express)
    } else {
        count_lines(&app.join("backend/spacetimedb/src"))
    };
    Some((server, count_lines(&app.join("client/src"))))
}

fn minutes(sec: f64) -> String {
    to_fixed(sec / 60.0, 1)
}

pub fn run(run: &Path) -> Result<()> {
    if !run.is_dir() {
        bail!("run directory not found: {}", run.display());
    }
    let sessions = read_sessions(run)?;
    if sessions.is_empty() {
        bail!(
            "no cost-summary.json files under {}/<backend>/telemetry/; run `cargo llm-apps sequential parse-telemetry` first",
            run.display()
        );
    }
    let first = sessions.values().flatten().next().expect("non-empty");
    let variant = first.variant.clone().unwrap_or_else(|| "unknown".into());
    let rules = first.rules.clone().unwrap_or_else(|| "unknown".into());
    let backends: Vec<String> = sessions.keys().cloned().collect();
    let totals: BTreeMap<String, Totals> = sessions.into_iter().map(|(b, s)| (b, Totals::of(s))).collect();
    let score = |b: &str| {
        latest_app(run, b)
            .and_then(|app| fs::read_to_string(app.join("GRADING_RESULTS.md")).ok())
            .and_then(|content| grading_score(&content))
    };

    let mut report = format!(
        "# LLM Cost-to-Done Benchmark Report\n\n**Generated:** {}\n**Variant:** {variant}\n**Rules:** {rules}\n\n---\n\n## Summary\n\n",
        chrono::Utc::now().format("%Y-%m-%d")
    );
    if let [b1, b2, ..] = backends.as_slice() {
        let (t1, t2) = (&totals[b1], &totals[b2]);
        let (cheaper, low, high) = if t1.cost < t2.cost {
            (b1, t1.cost, t2.cost)
        } else {
            (b2, t2.cost, t1.cost)
        };
        let delta = if high > 0.0 {
            format!("{cheaper} {}% cheaper", to_fixed((high - low) / high * 100.0, 0))
        } else {
            String::new()
        };
        report += &format!(
            "| | {b1} | {b2} | Delta |\n|--|-----|-----|-------|\n\
             | **Total LLM cost** | **${}** | **${}** | {delta} |\n\
             | **API calls** | {} | {} | |\n\
             | **Total tokens** | {} | {} | |\n\
             | **Duration** | {} min | {} min | |\n",
            to_fixed(t1.cost, 2),
            to_fixed(t2.cost, 2),
            t1.calls,
            t2.calls,
            grouped(t1.tokens),
            grouped(t2.tokens),
            minutes(t1.duration_sec),
            minutes(t2.duration_sec),
        );
        if let (Some((s1, m1)), Some((s2, m2))) = (score(b1), score(b2)) {
            report += &format!("| **Feature score** | {s1}/{m1} | {s2}/{m2} | |\n");
        }
        if let (Some((be1, fe1)), Some((be2, fe2))) = (lines_of_code(run, b1), lines_of_code(run, b2)) {
            report += &format!("| **Backend LOC** | {be1} | {be2} | |\n");
            report += &format!("| **Frontend LOC** | {fe1} | {fe2} | |\n");
            report += &format!("| **Total LOC** | {} | {} | |\n", be1 + fe1, be2 + fe2);
        }
    } else {
        let b = &backends[0];
        let t = &totals[b];
        report += &format!(
            "| Metric | Value |\n|--------|-------|\n\
             | **Backend** | {b} |\n\
             | **Total LLM cost** | ${} |\n\
             | **API calls** | {} |\n\
             | **Total tokens** | {} |\n\
             | **Duration** | {} min |\n",
            to_fixed(t.cost, 2),
            t.calls,
            grouped(t.tokens),
            minutes(t.duration_sec),
        );
        if let Some((s, m)) = score(b) {
            report += &format!("| **Feature score** | {s}/{m} |\n");
        }
    }

    report += "\n---\n\n## Per-Level Cost Breakdown\n\n";
    for (backend, t) in &totals {
        report += &format!(
            "### {backend}\n\n| Level | Cost | API Calls | Duration |\n|-------|------|-----------|----------|\n"
        );
        for (level, s) in &t.by_level {
            report += &format!(
                "| {level} | ${} | {} | {} min |\n",
                to_fixed(s.cost, 2),
                s.calls,
                minutes(s.duration_sec)
            );
        }
        report += &format!(
            "| **Total** | **${}** | **{}** | **{} min** |\n\n",
            to_fixed(t.cost, 2),
            t.calls,
            minutes(t.duration_sec)
        );
    }
    report += "---\n\n*Generated by `cargo llm-apps sequential report`*\n";

    let path = run.join("BENCHMARK_REPORT.md");
    fs::write(&path, report)?;
    println!("Report written to: {}", path.display());
    println!("Backends: {}", backends.join(", "));
    for (backend, t) in &totals {
        println!(
            "  {backend}: ${} ({} calls, {} min)",
            to_fixed(t.cost, 2),
            t.calls,
            minutes(t.duration_sec)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn grading_scores_in_every_known_format() {
        assert_eq!(grading_score("| **TOTAL** | **40/45** | |"), Some((40, 45)));
        assert_eq!(grading_score("| **TOTAL** | **45** | **38** |"), Some((38, 45)));
        assert_eq!(grading_score("Total Feature Score: 12 / 15"), Some((12, 15)));
        assert_eq!(grading_score("nothing"), None);
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn session(run: &Path, backend: &str, id: &str, level: i64, started: &str, cost: f64) {
        let summary = json!({
            "backend": backend, "level": level, "variant": "sequential-upgrade", "rules": "guided",
            "startedAt": started, "totalTokens": 1500, "totalCostUsd": cost, "apiCalls": 3,
            "totalDurationSec": 90,
        });
        write(
            &run.join(backend).join("telemetry").join(id).join("cost-summary.json"),
            &summary.to_string(),
        );
    }

    #[test]
    fn report_compares_two_backends_counting_one_session_per_level() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path();
        session(run, "spacetime", "spacetime-level1-a", 1, "2026-01-01T10:00:00Z", 1.0);
        // The fix sorts before the generate session by name, so the generate session counts.
        session(
            run,
            "spacetime",
            "spacetime-fix-level1-b",
            1,
            "2026-01-01T11:00:00Z",
            0.5,
        );
        session(
            run,
            "spacetime",
            "spacetime-upgrade-to-level2-c",
            2,
            "2026-01-01T12:00:00Z",
            2.0,
        );
        session(run, "postgres", "postgres-level1-a", 1, "2026-01-01T10:00:00Z", 5.0);
        let app = run.join("spacetime/results/chat-app-1");
        write(&app.join("GRADING_RESULTS.md"), "| **TOTAL** | **14/15** | |");
        write(&app.join("backend/spacetimedb/src/index.ts"), "a\nb\n");
        write(&app.join("client/src/App.tsx"), "a\n");
        write(&app.join("client/src/module_bindings/x.ts"), "a\nb\nc\n");
        let pg = run.join("postgres/results/chat-app-1");
        write(&pg.join("GRADING_RESULTS.md"), "| **TOTAL** | **12/15** | |");
        write(&pg.join("server/src/index.ts"), "a\nb\nc\nd\n");
        write(&pg.join("client/src/App.tsx"), "a\nb\n");

        super::run(run).unwrap();
        let report = fs::read_to_string(run.join("BENCHMARK_REPORT.md")).unwrap();
        assert!(report.contains("| | postgres | spacetime | Delta |"), "{report}");
        assert!(
            report.contains("| **Total LLM cost** | **$5.00** | **$3.00** | spacetime 40% cheaper |"),
            "{report}"
        );
        assert!(report.contains("| **Feature score** | 12/15 | 14/15 | |"), "{report}");
        assert!(report.contains("| **Backend LOC** | 5 | 3 | |"), "{report}");
        assert!(report.contains("| **Frontend LOC** | 3 | 2 | |"), "{report}");
        assert!(report.contains("| 1 | $1.00 | 3 | 1.5 min |"), "{report}");
        assert!(
            report.contains("| **Total** | **$3.00** | **6** | **3.0 min** |"),
            "{report}"
        );
    }

    #[test]
    fn report_needs_cost_summaries() {
        let dir = tempfile::tempdir().unwrap();
        assert!(super::run(dir.path()).is_err());
    }
}
