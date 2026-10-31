//! Append-only local evidence. A failed or interrupted batch never gets a complete marker.
use super::types::{BenchRunContext, RunOneError, RunOutcome, TaskPaths};
use crate::llm::clients::oa_compat::ProviderRefusal;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) struct BatchRecord {
    path: PathBuf,
    file: File,
    // Latest state determines coverage. Earlier attempts remain in the journal.
    tasks: BTreeMap<String, TaskState>,
}

enum TaskState {
    Pending,
    Scored { passed: u32, total: u32 },
    Refused,
    ProviderError,
    InfrastructureError,
}

impl BatchRecord {
    pub(super) fn new(cfg: &BenchRunContext<'_>, tasks: &[TaskPaths]) -> Result<Self> {
        let root = std::env::var_os("LLM_BENCHMARK_REPORT_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runs"))
            .join("batches");
        let record = Self::create(
            &root,
            json!({
                "hash": cfg.hash, "lang": cfg.lang.as_str(), "mode": cfg.mode,
                "model": cfg.route.model_id(), "api_model": cfg.route.api_model,
                "vendor": cfg.route.vendor.slug(), "dry_run": cfg.dry_run,
            }),
            tasks
                .iter()
                .map(|task| task.root.to_string_lossy().into_owned())
                .collect(),
        )?;
        eprintln!("[runner] local batch record: {}", record.path.display());
        Ok(record)
    }

    fn create(root: &Path, identity: Value, tasks: Vec<String>) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let path = root.join(format!(
            "{}-{}.jsonl",
            Utc::now().timestamp_nanos_opt().context("timestamp out of range")?,
            std::process::id()
        ));
        let file = OpenOptions::new().write(true).create_new(true).open(&path)?;
        let mut record = Self {
            path,
            file,
            tasks: tasks.iter().map(|task| (task.clone(), TaskState::Pending)).collect(),
        };
        record.append(
            json!({"event": "start", "schema_version": 1, "identity": identity, "tasks": tasks}),
            false,
        )?;
        Ok(record)
    }

    pub(super) fn record(&mut self, task: &str, result: &Result<RunOutcome, RunOneError>) -> Result<()> {
        let (state, attempt) = match result {
            Ok(outcome) => (
                TaskState::Scored {
                    passed: outcome.passed_tests,
                    total: outcome.total_tests,
                },
                json!({"status": "scored", "outcome": outcome}),
            ),
            Err(RunOneError::Other(error)) => match error.downcast_ref::<ProviderRefusal>() {
                Some(refusal) => (
                    TaskState::Refused,
                    json!({"status": "refused", "refusal": refusal, "error": format!("{error:#}")}),
                ),
                None => (
                    TaskState::ProviderError,
                    json!({"status": "provider_or_setup_error", "error": format!("{error:#}"),
                        "provider_response": error.downcast_ref::<crate::llm::clients::oa_compat::ProviderResponseError>(),
                        "http_response": error.downcast_ref::<crate::llm::clients::http::HttpResponseError>()}),
                ),
            },
            Err(RunOneError::Infrastructure {
                msg,
                llm_output,
                provider_error,
                completed_outcome,
            }) => (
                TaskState::InfrastructureError,
                json!({"status": "infrastructure_error", "error": msg, "llm_output": llm_output, "outcome": completed_outcome,
                    "refusal": provider_error.as_ref().and_then(|error| error.downcast_ref::<ProviderRefusal>()),
                    "provider_response": provider_error.as_ref().and_then(|error| error.downcast_ref::<crate::llm::clients::oa_compat::ProviderResponseError>()),
                    "http_response": provider_error.as_ref().and_then(|error| error.downcast_ref::<crate::llm::clients::http::HttpResponseError>())}),
            ),
            Err(RunOneError::WithOutput { .. }) => bail!("normalize generated-code failures before recording"),
        };
        *self.tasks.get_mut(task).context("task not in planned batch")? = state;
        self.append(json!({"event": "attempt", "task": task, "attempt": attempt}), false)
    }

    pub(super) fn complete(&mut self) -> Result<()> {
        if !self
            .tasks
            .values()
            .all(|state| matches!(state, TaskState::Scored { .. }))
        {
            bail!("incomplete batch record; refusing to upload");
        }
        self.append(json!({"event": "complete"}), true)
    }

    fn append(&mut self, mut event: Value, complete: bool) -> Result<()> {
        let (mut scored, mut passed, mut refused, mut provider, mut infrastructure, mut pending) = (0, 0, 0, 0, 0, 0);
        let (mut passed_checks, mut scored_checks) = (0u64, 0u64);
        for state in self.tasks.values() {
            match state {
                TaskState::Pending => pending += 1,
                TaskState::Refused => refused += 1,
                TaskState::ProviderError => provider += 1,
                TaskState::InfrastructureError => infrastructure += 1,
                TaskState::Scored { passed: p, total } => {
                    scored += 1;
                    passed += usize::from(*total > 0 && p == total);
                    passed_checks += u64::from(*p);
                    scored_checks += u64::from(*total);
                }
            }
        }
        event["at"] = json!(Utc::now());
        event["complete"] = json!(complete);
        event["summary"] = json!({
            "planned_tasks": self.tasks.len(), "scored_tasks": scored, "passed_tasks": passed,
            "refused_tasks": refused, "provider_or_setup_errors": provider,
            "infrastructure_errors": infrastructure, "pending_tasks": pending,
            "passed_checks": passed_checks, "scored_checks": scored_checks,
        });
        // Serialize first. An interrupted write can affect only the last line, not earlier evidence.
        let mut bytes = serde_json::to_vec(&event)?;
        bytes.push(b'\n');
        self.file
            .write_all(&bytes)
            .with_context(|| format!("write {}", self.path.display()))?;
        self.file
            .sync_data()
            .with_context(|| format!("sync {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::clients::oa_compat::OACompatResp;

    fn outcome(passed: u32) -> RunOutcome {
        serde_json::from_value(json!({
            "hash": "test", "task": "a", "lang": "rust", "golden_published": true,
            "model_name": "test", "total_tests": 3, "passed_tests": passed,
            "llm_output": "generated code", "scorer_details": {}, "vendor": "test"
        }))
        .unwrap()
    }

    #[test]
    fn incomplete_records_keep_scores_refusals_and_retry_history() {
        let root = std::env::temp_dir().join(format!(
            "benchmark-record-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut record = BatchRecord::create(&root, json!({"model": "test"}), vec!["a".into(), "b".into()]).unwrap();
        record.record("a", &Ok(outcome(3))).unwrap();
        let response: OACompatResp = serde_json::from_value(json!({
            "id": "gen-test", "provider": "test-provider", "choices": [{
                "finish_reason": "content_filter", "native_finish_reason": "refusal",
                "message": {"content": null, "refusal": "full refusal reason"}
            }]
        }))
        .unwrap();
        record
            .record("b", &Err(RunOneError::Other(response.first_text().unwrap_err())))
            .unwrap();
        assert!(record.complete().is_err());
        let rows = read_rows(&record.path);
        let last = rows.last().unwrap();
        assert_eq!(last["summary"]["scored_tasks"], 1);
        assert_eq!(last["summary"]["passed_tasks"], 1);
        assert_eq!(last["summary"]["refused_tasks"], 1);
        assert_eq!(last["summary"]["passed_checks"], 3);
        assert_eq!(last["summary"]["scored_checks"], 3);
        assert_eq!(last["complete"], false);
        assert_eq!(last["attempt"]["refusal"]["refusal"], "full refusal reason");
        assert_eq!(rows[1]["attempt"]["outcome"]["llm_output"], "generated code");
        // A generated-code failure is a scored result, unlike a provider refusal.
        record.record("b", &Ok(outcome(0))).unwrap();
        record.complete().unwrap();
        let rows = read_rows(&record.path);
        let last = rows.last().unwrap();
        assert_eq!(last["complete"], true);
        assert_eq!(last["summary"]["scored_tasks"], 2);
        assert_eq!(last["summary"]["passed_tasks"], 1);
        assert_eq!(last["summary"]["refused_tasks"], 0);
        assert_eq!(last["summary"]["scored_checks"], 6);
        assert_eq!(rows[2]["attempt"]["refusal"]["generation_id"], "gen-test");
        drop(record);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn infrastructure_failure_keeps_prior_results_and_pending_tasks() {
        let root = std::env::temp_dir().join(format!(
            "benchmark-infra-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut record = BatchRecord::create(&root, json!({}), vec!["a".into(), "b".into(), "c".into()]).unwrap();
        record.record("a", &Ok(outcome(3))).unwrap();
        record
            .record(
                "b",
                &Err(RunOneError::Infrastructure {
                    msg: "server unavailable".into(),
                    llm_output: Some("unscored code".into()),
                    provider_error: None,
                    completed_outcome: None,
                }),
            )
            .unwrap();
        assert!(record.complete().is_err());
        let rows = read_rows(&record.path);
        let last = rows.last().unwrap();
        assert_eq!(last["summary"]["scored_tasks"], 1);
        assert_eq!(last["summary"]["infrastructure_errors"], 1);
        assert_eq!(last["summary"]["pending_tasks"], 1);
        assert_eq!(last["attempt"]["llm_output"], "unscored code");
        let refusal = serde_json::from_value::<OACompatResp>(json!({
            "id": "gen-cleanup", "provider": "test-provider",
            "choices": [{"finish_reason": "content_filter", "message": {"content": null, "refusal": "private detail"}}]
        }))
        .unwrap()
        .first_text()
        .unwrap_err();
        record
            .record(
                "b",
                &Err(RunOneError::Infrastructure {
                    msg: "reference database cleanup failed".into(),
                    llm_output: None,
                    provider_error: Some(refusal),
                    completed_outcome: None,
                }),
            )
            .unwrap();
        assert!(record.complete().is_err());
        let rows = read_rows(&record.path);
        let last = rows.last().unwrap();
        assert_eq!(last["summary"]["infrastructure_errors"], 1);
        assert_eq!(last["summary"]["scored_tasks"], 1);
        assert_eq!(last["attempt"]["refusal"]["generation_id"], "gen-cleanup");
        assert_eq!(last["attempt"]["refusal"]["refusal"], "private detail");
        record
            .record(
                "b",
                &Err(RunOneError::Infrastructure {
                    msg: "reference database cleanup failed".into(),
                    llm_output: Some("generated code".into()),
                    provider_error: None,
                    completed_outcome: Some(Box::new(outcome(3))),
                }),
            )
            .unwrap();
        assert!(record.complete().is_err());
        let rows = read_rows(&record.path);
        let last = rows.last().unwrap();
        assert_eq!(last["attempt"]["outcome"]["passed_tests"], 3);
        assert_eq!(last["attempt"]["outcome"]["llm_output"], "generated code");
        drop(record);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_scored_tasks_have_no_score_denominator_and_write_errors_fail_closed() {
        let root = std::env::temp_dir().join(format!(
            "benchmark-write-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut record = BatchRecord::create(&root, json!({}), vec!["a".into()]).unwrap();
        record
            .record("a", &Err(RunOneError::Other(anyhow::anyhow!("402 Payment Required"))))
            .unwrap();
        record
            .record(
                "a",
                &Err(RunOneError::Other(
                    crate::llm::clients::http::HttpResponseError {
                        status: 403,
                        body: "full HTTP refusal details".into(),
                    }
                    .into(),
                )),
            )
            .unwrap();
        assert!(record.complete().is_err());
        let rows = read_rows(&record.path);
        assert_eq!(rows.last().unwrap()["summary"]["scored_checks"], 0);
        assert_eq!(rows.last().unwrap()["summary"]["scored_tasks"], 0);
        assert_eq!(rows.last().unwrap()["summary"]["provider_or_setup_errors"], 1);
        assert_eq!(
            rows.last().unwrap()["attempt"]["http_response"]["body"],
            "full HTTP refusal details"
        );
        let full_message = "refusal details ".repeat(100);
        let response: OACompatResp = serde_json::from_value(json!({
            "id": "gen-error", "choices": [{
                "finish_reason": "error", "error": {"code": 403, "message": full_message},
                "message": {"content": "unscored partial code", "refusal": "private refusal detail"}
            }]
        }))
        .unwrap();
        record
            .record("a", &Err(RunOneError::Other(response.first_text().unwrap_err())))
            .unwrap();
        let rows = read_rows(&record.path);
        assert_eq!(
            rows.last().unwrap()["attempt"]["provider_response"]["error"]["message"],
            full_message
        );
        assert_eq!(
            rows.last().unwrap()["attempt"]["provider_response"]["content"],
            "unscored partial code"
        );
        assert_eq!(
            rows.last().unwrap()["attempt"]["provider_response"]["refusal"],
            "private refusal detail"
        );
        let response: OACompatResp = serde_json::from_value(json!({
            "id": "gen-finish-error", "choices": [{"finish_reason": "error",
                "message": {"content": "partial code without an error object"}}]
        }))
        .unwrap();
        record
            .record("a", &Err(RunOneError::Other(response.first_text().unwrap_err())))
            .unwrap();
        let rows = read_rows(&record.path);
        assert_eq!(
            rows.last().unwrap()["attempt"]["provider_response"]["content"],
            "partial code without an error object"
        );
        // Replace the writer with a read-only handle to inject an actual I/O failure.
        record.file = File::open(&record.path).unwrap();
        assert!(record.record("a", &Ok(outcome(3))).is_err());
        assert!(record.complete().is_err());
        assert_eq!(read_rows(&record.path), rows, "prior evidence must remain intact");
        drop(record);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn read_rows(path: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}
