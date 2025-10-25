use crate::bench::utils::debug_llm_verbose;
use crate::eval::{normalize, sql_exec, ScoreDetails};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{io, thread};

pub trait Scorer {
    fn id(&self) -> &'static str;
    fn score(&self, llm_output: &str) -> ScoreDetails;
}

pub struct SchemaParityScorer {
    pub server: String,
    pub golden_db: String,
    pub llm_db: String,
    pub timeout: Duration,
    pub id_str: &'static str,
}

impl Scorer for SchemaParityScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        let golden = match describe_db(&self.server, &self.golden_db, self.timeout) {
            Ok(v) => v,
            Err(e) => return err_details("describe_golden", e),
        };
        let llm = match describe_db(&self.server, &self.llm_db, self.timeout) {
            Ok(v) => v,
            Err(e) => return err_details("describe_llm", e),
        };

        if debug_llm_verbose() {
            if let Ok(s) = serde_json::to_string_pretty(&golden) {
                println!("\n=== schema (golden: {}) ===\n{}\n", self.golden_db, s);
            }
            if let Ok(s) = serde_json::to_string_pretty(&llm) {
                println!("=== schema (llm: {}) ===\n{}\n", self.llm_db, s);
            }
        }

        let (tables_a, reducers_a) = extract_schema(&golden);
        let (tables_b, reducers_b) = extract_schema(&llm);

        let tables_diff = diff_maps(&tables_a, &tables_b);
        let reducers_diff = diff_sets(&reducers_a, &reducers_b);
        let pass = tables_diff.is_null() && reducers_diff.is_null();

        ScoreDetails {
            pass,
            partial: if pass { 1.0 } else { 0.0 },
            notes: json!({
                "server": self.server,
                "golden_db": self.golden_db,
                "llm_db": self.llm_db,
                "tables_equal": tables_diff.is_null(),
                "reducers_equal": reducers_diff.is_null(),
                "tables_diff": tables_diff,
                "reducers_diff": reducers_diff,
            }),
        }
    }
}

/* helpers */

fn run_with_timeout(mut cmd: Command, cwd: &Path, timeout: Duration) -> io::Result<(i32, Vec<u8>, Vec<u8>)> {
    let mut child = cmd
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let out = child.wait_with_output()?;
            let code = status.code().unwrap_or(-1);
            return Ok((code, out.stdout, out.stderr));
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(io::Error::new(io::ErrorKind::TimedOut, "process timeout"));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn describe_db(server: &str, db: &str, timeout: Duration) -> io::Result<Value> {
    let mut cmd = Command::new("spacetime");
    cmd.arg("describe")
        .arg("--json")
        .arg("-s")
        .arg(server)
        .arg("-y")
        .arg(db);
    let (code, out, err) = run_with_timeout(cmd, Path::new("."), timeout)?;
    if code != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("describe failed: {}", String::from_utf8_lossy(&err)),
        ));
    }
    let v: Value =
        serde_json::from_slice(&out).map_err(|e| io::Error::new(io::ErrorKind::Other, format!("parse json: {}", e)))?;
    Ok(v)
}

fn extract_schema(v: &Value) -> (BTreeMap<String, BTreeMap<String, String>>, BTreeSet<String>) {
    let mut tables: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut reducers: BTreeSet<String> = BTreeSet::new();

    if let Some(ts) = v.get("tables").and_then(|x| x.as_array()) {
        for t in ts {
            let name = t.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let mut cols = BTreeMap::new();
            if let Some(cs) = t.get("columns").and_then(|x| x.as_array()) {
                for c in cs {
                    let cname = c.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let cty = c.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    cols.insert(cname, cty);
                }
            }
            tables.insert(name, cols);
        }
    }

    if let Some(rs) = v.get("reducers").and_then(|x| x.as_array()) {
        for r in rs {
            let name = r.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let sig = if let Some(args) = r.get("args").and_then(|x| x.as_array()) {
                let tys: Vec<String> = args
                    .iter()
                    .map(|a| a.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string())
                    .collect();
                format!("{}({})", name, tys.join(","))
            } else {
                format!("{}()", name)
            };
            reducers.insert(sig);
        }
    }

    (tables, reducers)
}

fn diff_maps(a: &BTreeMap<String, BTreeMap<String, String>>, b: &BTreeMap<String, BTreeMap<String, String>>) -> Value {
    let mut only_a = BTreeMap::new();
    let mut only_b = BTreeMap::new();
    let mut changed = BTreeMap::new();

    for (k, va) in a {
        match b.get(k) {
            None => {
                only_a.insert(k.clone(), va.clone());
            }
            Some(vb) if vb != va => {
                changed.insert(k.clone(), json!({ "golden": va, "llm": vb }));
            }
            _ => {}
        }
    }
    for (k, vb) in b {
        if !a.contains_key(k) {
            only_b.insert(k.clone(), vb.clone());
        }
    }

    if only_a.is_empty() && only_b.is_empty() && changed.is_empty() {
        Value::Null
    } else {
        json!({ "only_golden": only_a, "only_llm": only_b, "changed": changed })
    }
}

fn diff_sets(a: &BTreeSet<String>, b: &BTreeSet<String>) -> Value {
    let only_a: BTreeSet<_> = a.difference(b).cloned().collect();
    let only_b: BTreeSet<_> = b.difference(a).cloned().collect();
    if only_a.is_empty() && only_b.is_empty() {
        Value::Null
    } else {
        json!({ "only_golden": only_a, "only_llm": only_b })
    }
}

fn err_details(phase: &str, e: io::Error) -> ScoreDetails {
    ScoreDetails {
        pass: false,
        partial: 0.0,
        notes: json!({ "phase": phase, "error": e.to_string() }),
    }
}

/* reducer/sql helpers */

pub fn call_reducer_json_out(db: &str, reducer: &str, args: &[Value], host: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new("spacetime");
    cmd.arg("call").arg(db).arg(reducer);

    for v in args {
        let lit = serde_json::to_string(v).map_err(|e| format!("json encode arg failed: {e}"))?;
        cmd.arg(lit);
    }
    if let Some(h) = host {
        cmd.arg("--server").arg(h);
    }

    if debug_llm_verbose() {
        eprintln!("[dbg] spacetime call: {:?}", cmd);
    }

    let out = cmd
        .output()
        .map_err(|e| format!("failed to spawn spacetime call: {e}"))?;
    if debug_llm_verbose() {
        eprintln!(
            "[dbg] spacetime call exit={} stdout:\n{}\n-- stderr:\n{}\n",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    if !out.status.success() {
        return Err(format!(
            "spacetime call failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn sql_raw(db: &str, query: &str, host: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new("spacetime");
    cmd.arg("sql").arg(db).arg(query);
    if let Some(h) = host {
        cmd.arg("--server").arg(h);
    }

    if debug_llm_verbose() {
        eprintln!("[dbg] spacetime sql: {:?}", cmd);
    }

    let out = cmd
        .output()
        .map_err(|e| format!("failed to spawn spacetime sql: {e}"))?;
    if debug_llm_verbose() {
        eprintln!(
            "[dbg] spacetime sql exit={} stdout:\n{}\n-- stderr:\n{}\n",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    if !out.status.success() {
        return Err(format!(
            "spacetime sql failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn sql_count(db: &str, query: &str, host: Option<&str>) -> Result<i64, String> {
    let mut cmd = Command::new("spacetime");
    cmd.arg("sql").arg(db).arg(query);
    if let Some(h) = host {
        cmd.arg("--server").arg(h);
    }

    if debug_llm_verbose() {
        eprintln!("[dbg] spacetime sql-count: {:?}", cmd);
    }

    let out = cmd
        .output()
        .map_err(|e| format!("failed to spawn spacetime sql: {e}"))?;
    if debug_llm_verbose() {
        eprintln!(
            "[dbg] spacetime sql-count exit={} stdout:\n{}\n-- stderr:\n{}\n",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    if !out.status.success() {
        return Err(format!(
            "spacetime sql failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    for tok in s.split_whitespace() {
        if let Ok(n) = tok.parse::<i64>() {
            if debug_llm_verbose() {
                eprintln!("[dbg] parsed count: {n}");
            }
            return Ok(n);
        }
    }
    Err(format!("could not parse count from output:\n{s}"))
}

/* generalized equality scorer */

pub struct ReducerSqlEqualsScorer {
    pub server: String,
    pub db: String,
    pub reducer: String,
    pub args: Vec<Value>,
    pub sql: String,
    pub expected: String,
    pub collapse_ws: bool,
    pub timeout: Duration,
    pub id_str: &'static str,
}

impl Scorer for ReducerSqlEqualsScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        if debug_llm_verbose() {
            eprintln!(
                "[dbg] ReducerSqlEqualsScorer: calling reducer={} args={:?} db={} server={}",
                self.reducer, self.args, self.db, self.server
            );
        }
        let call_res = call_reducer_json_out(&self.db, &self.reducer, &self.args, Some(&self.server));
        if let Err(e) = call_res {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase": "call_reducer", "error": e }),
            };
        }

        if debug_llm_verbose() {
            eprintln!("[dbg] ReducerSqlEqualsScorer: running sql: {}", self.sql);
        }
        match sql_raw(&self.db, &self.sql, Some(&self.server)) {
            Ok(out) => {
                let actual = normalize(&out, self.collapse_ws);
                let expected = normalize(&self.expected, self.collapse_ws);
                let pass = actual == expected;
                if debug_llm_verbose() {
                    eprintln!(
                        "[dbg] expected:\n{}\n[dbg] actual:\n{}\n[dbg] pass={}",
                        expected, actual, pass
                    );
                }
                ScoreDetails {
                    pass,
                    partial: if pass { 1.0 } else { 0.0 },
                    notes: json!({
                        "server": self.server,
                        "db": self.db,
                        "reducer": self.reducer,
                        "args": self.args,
                        "sql": self.sql,
                        "expected": expected,
                        "actual": actual,
                    }),
                }
            }
            Err(e) => ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase": "sql", "error": e }),
            },
        }
    }
}

pub struct ReducerSqlCountScorer {
    pub server: String,
    pub db: String,
    pub reducer: String,
    pub args: Vec<serde_json::Value>,
    pub sql: String,
    pub expected: i64,
    pub timeout: std::time::Duration,
    pub id_str: &'static str,
}

impl Scorer for ReducerSqlCountScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        if debug_llm_verbose() {
            eprintln!(
                "[dbg] ReducerSqlCountScorer: call reducer={} args={:?} db={} server={}",
                self.reducer, self.args, self.db, self.server
            );
        }
        let call = call_reducer_json_out(&self.db, &self.reducer, &self.args, Some(&self.server));
        if let Err(e) = call {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({"phase":"call_reducer","error":e}),
            };
        }

        if debug_llm_verbose() {
            eprintln!("[dbg] ReducerSqlCountScorer: running sql: {}", self.sql);
        }
        match sql_count(&self.db, &self.sql, Some(&self.server)) {
            Ok(n) => {
                let pass = n == self.expected;
                if debug_llm_verbose() {
                    eprintln!("[dbg] count expected={} actual={} pass={}", self.expected, n, pass);
                }
                ScoreDetails {
                    pass,
                    partial: if pass { 1.0 } else { 0.0 },
                    notes: json!({ "expected": self.expected, "actual": n, "sql": self.sql }),
                }
            }
            Err(e) => ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({"phase":"sql","error":e}),
            },
        }
    }
}

pub struct ReducerDataParityScorer {
    pub server: String,
    pub golden_db: String,
    pub llm_db: String,
    pub reducer: String,
    pub args: Vec<Value>,
    pub query: String,
    pub collapse_ws: bool,
    pub timeout: Duration,
    pub id_str: &'static str,
}

impl Scorer for ReducerDataParityScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        if debug_llm_verbose() {
            eprintln!(
                "[dbg] ReducerDataParityScorer: reducer={} args={:?} golden_db={} llm_db={} server={}",
                self.reducer, self.args, self.golden_db, self.llm_db, self.server
            );
        }

        if let Err(e) = call_reducer_json_out(&self.golden_db, &self.reducer, &self.args, Some(&self.server)) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({"phase":"call_reducer_golden","error":e}),
            };
        }
        if let Err(e) = call_reducer_json_out(&self.llm_db, &self.reducer, &self.args, Some(&self.server)) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({"phase":"call_reducer_llm","error":e}),
            };
        }

        if debug_llm_verbose() {
            eprintln!("[dbg] query for parity: {}", self.query);
        }
        let g = match sql_raw(&self.golden_db, &self.query, Some(&self.server)) {
            Ok(s) => s,
            Err(e) => {
                return ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({"phase":"sql_golden","error":e}),
                }
            }
        };
        let l = match sql_raw(&self.llm_db, &self.query, Some(&self.server)) {
            Ok(s) => s,
            Err(e) => {
                return ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({"phase":"sql_llm","error":e}),
                }
            }
        };

        let g_n = normalize(&g, self.collapse_ws);
        let l_n = normalize(&l, self.collapse_ws);
        let pass = g_n == l_n;

        if debug_llm_verbose() {
            eprintln!(
                "[dbg] golden out:\n{}\n[dbg] llm out:\n{}\n[dbg] pass={}",
                g_n, l_n, pass
            );
        }

        ScoreDetails {
            pass,
            partial: if pass { 1.0 } else { 0.0 },
            notes: json!({
                "server": self.server,
                "golden_db": self.golden_db,
                "llm_db": self.llm_db,
                "reducer": self.reducer,
                "args": self.args,
                "query": self.query,
                "golden_out": g_n,
                "llm_out": l_n
            }),
        }
    }
}

pub struct SqlCountOnlyScorer {
    pub server: String,
    pub db: String,
    pub sql: String,
    pub expected: i64,
    pub timeout: Duration,
    pub id_str: &'static str,
}

impl Scorer for SqlCountOnlyScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }
    fn score(&self, _llm_output: &str) -> ScoreDetails {
        match sql_count(&self.db, &self.sql, Some(&self.server)) {
            Ok(n) => {
                let pass = n == self.expected;
                ScoreDetails {
                    pass,
                    partial: if pass { 1.0 } else { 0.0 },
                    notes: json!({ "sql": self.sql, "expected": self.expected, "actual": n }),
                }
            }
            Err(e) => ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase":"sql","error": e }),
            },
        }
    }
}

pub struct SqlExecBothScorer {
    pub server: String,
    pub golden_db: String,
    pub llm_db: String,
    pub sql: String,
    pub timeout: Duration,
    pub id_str: &'static str,
}

impl Scorer for SqlExecBothScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        if debug_llm_verbose() {
            eprintln!(
                "[dbg] SqlExecBothScorer: sql on both dbs: {}\n  golden_db={} llm_db={} server={}",
                self.sql, self.golden_db, self.llm_db, self.server
            );
        }
        if let Err(e) = sql_exec(&self.golden_db, &self.sql, Some(&self.server)) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase":"sql_golden", "error": e, "sql": self.sql }),
            };
        }
        if let Err(e) = sql_exec(&self.llm_db, &self.sql, Some(&self.server)) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase":"sql_llm", "error": e, "sql": self.sql }),
            };
        }
        if debug_llm_verbose() {
            eprintln!("[dbg] SqlExecBothScorer: success");
        }
        ScoreDetails {
            pass: true,
            partial: 1.0,
            notes: json!({ "sql": self.sql }),
        }
    }
}
