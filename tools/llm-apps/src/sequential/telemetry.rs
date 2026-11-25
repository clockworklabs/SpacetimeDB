//! Turns Claude Code's OpenTelemetry log into a run's `COST_REPORT.md` and
//! `cost-summary.json`.
//!
//! Every run on a machine shares one collector, so records are attributed to a run by
//! the `session.id` and `run.id` resource attributes `generate`, `upgrade` and `fix` set,
//! and then by the run's time window.

use crate::format::{grouped, json_number, to_fixed};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// One API call's usage.
struct ApiCall {
    model: String,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_creation: f64,
    cost_usd: f64,
    duration_ms: f64,
}

/// The totals `report` aggregates, written as cost-summary.json.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostSummary {
    #[serde(skip_serializing_if = "Value::is_null")]
    backend: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    level: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    variant: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    rules: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    run_index: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    session_id: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    started_at: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    ended_at: Value,
    total_input_tokens: Value,
    total_output_tokens: Value,
    total_tokens: Value,
    cache_read_tokens: Value,
    cache_creation_tokens: Value,
    total_cost_usd: Value,
    api_calls: usize,
    total_duration_sec: Value,
}

/// OTLP attributes flattened to name → value. A present key with no usable value is
/// `None`, which counts as missing.
type Attrs = HashMap<String, Option<Value>>;

/// Flattens an OTLP attribute list (`[{key, value: {stringValue|intValue|...}}]`), or
/// copies a plain object as-is.
fn flatten(list: &Value, out: &mut Attrs) {
    match list {
        Value::Array(items) => {
            for kv in items {
                let (Some(key), Some(value)) = (kv["key"].as_str(), kv.get("value")) else {
                    continue;
                };
                if key.is_empty() {
                    continue;
                }
                let first_truthy = ["stringValue", "intValue", "doubleValue", "boolValue"]
                    .iter()
                    .filter_map(|k| value.get(*k))
                    .find(|v| truthy(v))
                    .cloned();
                out.insert(key.to_owned(), first_truthy);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                out.insert(k.clone(), Some(v.clone()));
            }
        }
        _ => {}
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// Log-record attributes, plus `_eventType` from the record body (for example
/// `claude_code.api_request`).
fn record_attributes(record: &Value) -> Attrs {
    let mut attrs = Attrs::new();
    if let Some(a) = record.get("attributes") {
        flatten(a, &mut attrs);
    }
    for rl in record["resourceLogs"].as_array().into_iter().flatten() {
        for sl in rl["scopeLogs"].as_array().into_iter().flatten() {
            for lr in sl["logRecords"].as_array().into_iter().flatten() {
                if let Some(event) = lr["body"]["stringValue"].as_str().filter(|s| !s.is_empty()) {
                    attrs.insert("_eventType".into(), Some(Value::from(event)));
                }
                if let Some(a) = lr.get("attributes") {
                    flatten(a, &mut attrs);
                }
                if let Some(values) = lr["body"]["kvlistValue"].get("values") {
                    flatten(values, &mut attrs);
                }
            }
        }
    }
    attrs
}

/// Resource attributes, which carry `session.id` and `run.id`.
fn resource_attributes(record: &Value) -> Attrs {
    let mut attrs = Attrs::new();
    for rl in record["resourceLogs"].as_array().into_iter().flatten() {
        if let Some(a) = rl["resource"].get("attributes") {
            flatten(a, &mut attrs);
        }
    }
    attrs
}

fn get<'a>(attrs: &'a Attrs, key: &str) -> Option<&'a Value> {
    attrs.get(key).and_then(Option::as_ref)
}

fn text(v: Option<&Value>) -> Option<String> {
    v.map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
}

/// A numeric attribute, with missing, zero and unparseable values all counting as 0.
fn number(attrs: &Attrs, key: &str) -> f64 {
    match get(attrs, key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

/// Milliseconds since the epoch for the timestamp formats runs and Claude Code write.
fn parse_time(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z"))
        .ok()
        .map(|t| t.timestamp_millis())
}

/// Parses the run's telemetry and writes its reports. Returns the cost summary.
pub fn run(
    run_dir: &Path,
    logs_file: Option<&Path>,
    end_override: Option<&str>,
    extract_raw: bool,
) -> Result<CostSummary> {
    let logs_file = logs_file.map_or_else(|| run_dir.join("../logs.jsonl"), Path::to_path_buf);
    if !logs_file.is_file() {
        bail!(
            "Telemetry file not found: {}\nMake sure the OTel Collector is running and Claude Code has CLAUDE_CODE_ENABLE_TELEMETRY=1",
            logs_file.display()
        );
    }
    let metadata: Value = fs::read_to_string(run_dir.join("metadata.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({"level": "?", "backend": "?", "timestamp": "?"}));
    let field = |k: &str| metadata.get(k).filter(|v| truthy(v)).cloned();
    let field_str = |k: &str| field(k).and_then(|v| v.as_str().map(str::to_owned));

    // With both backends running in parallel, time windows overlap, so the session ID is
    // the only reliable way to attribute a record.
    let session_id = field_str("sessionId");
    let run_id = field_str("runId");
    match &session_id {
        Some(id) => println!("Session-ID filtering enabled: session.id={id}"),
        None => {
            println!("WARNING: No sessionId in metadata — falling back to time-range-only filtering.");
            println!("         Results may include records from other concurrent runs.");
        }
    }

    let start = field_str("startedAtUtc").or_else(|| field_str("startedAt"));
    let end = end_override
        .map(str::to_owned)
        .or_else(|| field_str("endedAtUtc"))
        .or_else(|| field_str("endedAt"));
    let start_ms = start.as_deref().map_or(Some(0), parse_time);
    let end_ms = end.as_deref().map_or(Some(Utc::now().timestamp_millis()), parse_time);
    if end.is_none() {
        println!("WARNING: No end time found in metadata — using current time as upper bound.");
        println!("         The run may have crashed or the metadata update failed.");
    }
    println!(
        "Filtering telemetry: {} → {}",
        start.as_deref().unwrap_or("(start)"),
        end.as_deref().unwrap_or("(now)")
    );

    let content = fs::read_to_string(&logs_file).with_context(|| format!("failed to read {}", logs_file.display()))?;
    let lines: Vec<&str> = content.trim().split('\n').filter(|l| !l.is_empty()).collect();
    let mut calls = Vec::new();
    let mut raw = Vec::new();
    let (mut out_of_range, mut non_api, mut wrong_session) = (0, 0, 0);

    for line in &lines {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let attrs = record_attributes(&record);
        let resource = resource_attributes(&record);

        if let Some(session_id) = &session_id {
            let rec_session = text(get(&resource, "session.id"));
            let rec_run = text(get(&resource, "run.id"));
            // Untagged records predate run tags and fall through to the time filter.
            let tagged = rec_session.is_some() || rec_run.is_some();
            let ours = rec_session.as_ref() == Some(session_id) || (rec_run.is_some() && rec_run == run_id);
            if tagged && !ours {
                wrong_session += 1;
                continue;
            }
        }

        let event_time = text(get(&attrs, "event.timestamp").or_else(|| get(&attrs, "timestamp")));
        if let Some(t) = event_time.as_deref().and_then(parse_time)
            && (start_ms.is_some_and(|s| t < s) || end_ms.is_some_and(|e| t > e))
        {
            out_of_range += 1;
            continue;
        }

        if extract_raw {
            raw.push(*line);
        }

        if let Some(event) = text(get(&attrs, "_eventType"))
            && event != "claude_code.api_request"
        {
            non_api += 1;
            continue;
        }
        if get(&attrs, "input_tokens").is_some() {
            calls.push(ApiCall {
                model: text(get(&attrs, "model")).unwrap_or_else(|| "unknown".into()),
                input: number(&attrs, "input_tokens"),
                output: number(&attrs, "output_tokens"),
                cache_read: number(&attrs, "cache_read_tokens"),
                cache_creation: number(&attrs, "cache_creation_tokens"),
                cost_usd: number(&attrs, "cost_usd"),
                duration_ms: number(&attrs, "duration_ms"),
            });
        }
    }

    let sum = |f: fn(&ApiCall) -> f64| calls.iter().map(f).sum::<f64>();
    let (input, output) = (sum(|c| c.input), sum(|c| c.output));
    let (cache_read, cache_creation) = (sum(|c| c.cache_read), sum(|c| c.cache_creation));
    let cost = sum(|c| c.cost_usd);
    let duration_sec = sum(|c| c.duration_ms) / 1000.0;
    let total = input + output;

    let show = |v: Option<Value>| v.map_or_else(|| "undefined".to_owned(), |v| text(Some(&v)).unwrap_or_default());
    let mut report = format!(
        "# Cost Report

**App:** chat-app
**Backend:** {}
**Level:** {}
**Date:** {}
**Started:** {}

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | {} |
| Total output tokens     | {} |
| Total tokens            | {} |
| Cache read tokens       | {} |
| Cache creation tokens   | {} |
| Total cost (USD)        | ${} |
| Total API time          | {}s |
| API calls               | {} |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
",
        show(metadata.get("backend").cloned()),
        show(metadata.get("level").cloned()),
        Utc::now().format("%Y-%m-%d"),
        show(field("startedAt").or_else(|| field("timestamp"))),
        grouped(input),
        grouped(output),
        grouped(total),
        grouped(cache_read),
        grouped(cache_creation),
        to_fixed(cost, 4),
        to_fixed(duration_sec, 1),
        calls.len(),
    );
    let rows: Vec<String> = calls
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "| {} | {} | {} | {} | {} | ${} | {}s |",
                i + 1,
                c.model,
                grouped(c.input),
                grouped(c.output),
                grouped(c.cache_read),
                to_fixed(c.cost_usd, 4),
                to_fixed(c.duration_ms / 1000.0, 1)
            )
        })
        .collect();
    report.push_str(&rows.join("\n"));
    report.push_str(
        "

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
",
    );
    let report_path = run_dir.join("COST_REPORT.md");
    fs::write(&report_path, report)?;

    println!(
        "Parsed {} API calls from {} telemetry records.",
        calls.len(),
        lines.len()
    );
    println!("  Skipped: {out_of_range} out of time range, {non_api} non-API events, {wrong_session} wrong session");
    println!(
        "Total tokens: {} ({} in / {} out)",
        grouped(total),
        grouped(input),
        grouped(output)
    );
    println!("Total cost: ${}", to_fixed(cost, 4));
    println!("Report saved to: {}", report_path.display());

    if extract_raw && !raw.is_empty() {
        let raw_path = run_dir.join("raw-telemetry.jsonl");
        fs::write(&raw_path, raw.join("\n") + "\n")?;
        println!("Raw telemetry: {} records saved to {}", raw.len(), raw_path.display());
    }

    let meta = |k: &str| metadata.get(k).cloned().unwrap_or(Value::Null);
    let summary = CostSummary {
        backend: meta("backend"),
        level: meta("level"),
        variant: meta("variant"),
        rules: meta("rules"),
        run_index: meta("runIndex"),
        session_id: meta("sessionId"),
        started_at: field("startedAtUtc")
            .or_else(|| field("startedAt"))
            .unwrap_or(Value::Null),
        ended_at: field("endedAtUtc").or_else(|| field("endedAt")).unwrap_or(Value::Null),
        total_input_tokens: json_number(input),
        total_output_tokens: json_number(output),
        total_tokens: json_number(total),
        cache_read_tokens: json_number(cache_read),
        cache_creation_tokens: json_number(cache_creation),
        total_cost_usd: json_number(cost),
        api_calls: calls.len(),
        total_duration_sec: json_number(duration_sec),
    };
    let summary_path = run_dir.join("cost-summary.json");
    fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    println!("Cost summary JSON: {}", summary_path.display());
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(session: &str, event: &str, time: &str, input: &str, cost: f64) -> String {
        json!({"resourceLogs": [{
            "resource": {"attributes": [{"key": "session.id", "value": {"stringValue": session}}]},
            "scopeLogs": [{"logRecords": [{
                "body": {"stringValue": event},
                "attributes": [
                    {"key": "event.timestamp", "value": {"stringValue": time}},
                    {"key": "input_tokens", "value": {"intValue": input}},
                    {"key": "output_tokens", "value": {"intValue": "100"}},
                    {"key": "cost_usd", "value": {"doubleValue": cost}},
                    {"key": "duration_ms", "value": {"intValue": "1500"}},
                    {"key": "model", "value": {"stringValue": "claude-sonnet-4-6"}},
                ]
            }]}]
        }]})
        .to_string()
    }

    #[test]
    fn counts_only_this_sessions_api_calls_in_its_window() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("run");
        fs::create_dir_all(&run).unwrap();
        fs::write(
            run.join("metadata.json"),
            json!({
                "level": 1, "backend": "spacetime", "variant": "sequential-upgrade",
                "sessionId": "ours", "runId": "spacetime-level1-x",
                "startedAt": "2026-01-01T05:00:00-0500", "startedAtUtc": "2026-01-01T10:00:00Z",
                "endedAtUtc": "2026-01-01T11:00:00Z",
            })
            .to_string(),
        )
        .unwrap();
        let logs = [
            record(
                "ours",
                "claude_code.api_request",
                "2026-01-01T10:30:00.000Z",
                "1200",
                0.5,
            ),
            record(
                "ours",
                "claude_code.api_request",
                "2026-01-01T10:45:00.000Z",
                "800",
                0.25,
            ),
            record(
                "theirs",
                "claude_code.api_request",
                "2026-01-01T10:30:00.000Z",
                "9999",
                9.0,
            ),
            record(
                "ours",
                "claude_code.api_request",
                "2026-01-01T12:00:00.000Z",
                "9999",
                9.0,
            ),
            record(
                "ours",
                "claude_code.tool_result",
                "2026-01-01T10:31:00.000Z",
                "9999",
                9.0,
            ),
            "not json".to_owned(),
        ];
        fs::write(dir.path().join("logs.jsonl"), logs.join("\n")).unwrap();

        super::run(&run, None, None, true).unwrap();
        let summary: Value = serde_json::from_str(&fs::read_to_string(run.join("cost-summary.json")).unwrap()).unwrap();
        assert_eq!(summary["apiCalls"], 2);
        assert_eq!(summary["totalInputTokens"], 2000);
        assert_eq!(summary["totalTokens"], 2200);
        assert_eq!(summary["totalCostUsd"], 0.75);
        assert_eq!(summary["totalDurationSec"], 3);
        assert_eq!(summary["startedAt"], "2026-01-01T10:00:00Z");
        assert!(summary.get("rules").is_none(), "absent metadata fields are left out");

        let report = fs::read_to_string(run.join("COST_REPORT.md")).unwrap();
        assert!(report.contains("| Total input tokens      | 2,000 |"));
        assert!(report.contains("| Total cost (USD)        | $0.7500 |"));
        assert!(report.contains("| 1 | claude-sonnet-4-6 | 1,200 | 100 | 0 | $0.5000 | 1.5s |"));
        // The raw extract keeps every record of this session in the window, API or not.
        assert_eq!(
            fs::read_to_string(run.join("raw-telemetry.jsonl"))
                .unwrap()
                .lines()
                .count(),
            3
        );
    }

    #[test]
    fn otlp_values_use_the_first_truthy_field() {
        let mut attrs = Attrs::new();
        flatten(
            &json!([
                {"key": "a", "value": {"stringValue": "", "intValue": "7"}},
                {"key": "b", "value": {"boolValue": false}},
            ]),
            &mut attrs,
        );
        assert_eq!(number(&attrs, "a"), 7.0);
        assert!(attrs.contains_key("b") && get(&attrs, "b").is_none());
    }
}
