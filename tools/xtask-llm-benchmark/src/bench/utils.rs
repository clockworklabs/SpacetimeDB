use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn sanitize_db_name(raw: &str) -> String {
    // lowercase and strip invalids to hyphens
    let s: String = raw
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // collapse multiple '-' and trim
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch == '-' {
            if !prev_dash {
                out.push('-');
            }
            prev_dash = true;
        } else {
            out.push(ch);
            prev_dash = false;
        }
    }
    while out.starts_with('-') {
        out.remove(0);
    }
    while out.ends_with('-') {
        out.pop();
    }

    // must start with [a-z0-9]; if empty, prefix
    if out.is_empty() || !out.chars().next().unwrap().is_ascii_alphanumeric() {
        out.insert_str(0, "db");
    }

    out
}

pub fn work_server_dir_scoped(category: &str, task: &str, lang: &str, phase: &str, route_tag: &str) -> PathBuf {
    let target = env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
    Path::new(&target)
        .join("llm-runs")
        .join(category)
        .join(task)
        .join(lang)
        .join("server")
        .join(route_tag)
        .join(phase)
}

pub fn max_chars() -> usize {
    env::var("LLM_OUTPUT_MAX_CHARS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000)
}

pub fn print_llm_output(model: &str, task: &str, s: &str) {
    let limit = max_chars();
    let mut end = s.len().min(limit);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let s = &s[..end];
    println!("\n===== {} :: {} =====\n{}\n===== end =====\n", model, task, s);
}

pub fn task_slug(p: &Path) -> String {
    p.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string()
}
pub fn category_slug(p: &Path) -> String {
    p.parent()
        .and_then(|x| x.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

pub fn debug_llm() -> bool {
    matches!(env::var("LLM_DEBUG").as_deref(), Ok("1" | "true" | "yes"))
}

pub fn debug_llm_verbose() -> bool {
    matches!(env::var("LLM_DEBUG_VERBOSE").as_deref(), Ok("1" | "true" | "yes"))
}

pub fn bench_concurrency() -> usize {
    env::var("LLM_BENCH_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8)
}

/// Concurrency for C# builds. Lower default than Rust due to dotnet/WASI SDK
/// instability under high parallelism (causes SIGSEGV and "Pipe is broken" errors).
pub fn bench_csharp_concurrency() -> usize {
    env::var("LLM_BENCH_CSHARP_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4)
}

pub fn bench_route_concurrency() -> usize {
    env::var("LLM_BENCH_ROUTE_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4)
}

pub fn fmt_dur(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 1.0 {
        format!("{} ms", d.as_millis())
    } else if secs < 60.0 {
        format!("{:.2} s", secs)
    } else {
        let m = (secs / 60.0).floor() as u64;
        let s = secs - (m as f64) * 60.0;
        format!("{}m {:.1}s", m, s)
    }
}
