use crate::bench::utils::debug_llm_verbose;
use crate::eval::utils::{run_with_timeout, sql_exec_with_timeout};
use crate::eval::{normalize, spacetime_command, ScoreDetails};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
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

#[derive(Debug, Default, PartialEq, Eq)]
struct SchemaSnapshot {
    tables: BTreeMap<String, BTreeMap<String, String>>,
    reducers: BTreeSet<String>,
    exports: BTreeSet<String>,
    row_level_security: BTreeSet<String>,
}

#[derive(Debug, Default)]
struct SchemaNames {
    tables: BTreeMap<String, String>,
    functions: BTreeMap<String, String>,
}

impl SchemaNames {
    fn from_schema(schema: &Value) -> Self {
        let Some(sections) = schema.get("sections") else {
            return Self::default();
        };
        let mut names = Self::default();

        if let Some(entries) = sections
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|section| section.get("ExplicitNames"))
            .and_then(|names| names.get("entries"))
            .and_then(Value::as_array)
        {
            for entry in entries {
                if let Some(mapping) = entry.get("Table") {
                    insert_name_mapping(&mut names.tables, mapping);
                } else if let Some(mapping) = entry.get("Function") {
                    insert_name_mapping(&mut names.functions, mapping);
                }
            }
        }
        names
    }

    fn table(&self, name: String) -> String {
        self.canonical(name, &self.tables)
    }

    fn function(&self, name: String) -> String {
        self.canonical(name, &self.functions)
    }

    fn canonical(&self, name: String, explicit: &BTreeMap<String, String>) -> String {
        explicit.get(&name).cloned().unwrap_or(name)
    }
}

fn insert_name_mapping(names: &mut BTreeMap<String, String>, mapping: &Value) {
    let source = schema_name(mapping.get("source_name"));
    let canonical = schema_name(mapping.get("canonical_name"));
    if !source.is_empty() && !canonical.is_empty() {
        names.insert(source, canonical);
    }
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

        let schema_a = extract_schema(&golden);
        let schema_b = extract_schema(&llm);

        let tables_diff = diff_maps(&schema_a.tables, &schema_b.tables);
        let reducers_diff = diff_sets(&schema_a.reducers, &schema_b.reducers);
        let exports_diff = diff_sets(&schema_a.exports, &schema_b.exports);
        let rls_diff = diff_sets(&schema_a.row_level_security, &schema_b.row_level_security);
        let pass = tables_diff.is_null() && reducers_diff.is_null() && exports_diff.is_null() && rls_diff.is_null();

        ScoreDetails {
            pass,
            partial: if pass { 1.0 } else { 0.0 },
            notes: json!({
                "server": self.server,
                "golden_db": self.golden_db,
                "llm_db": self.llm_db,
                "tables_equal": tables_diff.is_null(),
                "reducers_equal": reducers_diff.is_null(),
                "exports_equal": exports_diff.is_null(),
                "row_level_security_equal": rls_diff.is_null(),
                "tables_diff": tables_diff,
                "reducers_diff": reducers_diff,
                "exports_diff": exports_diff,
                "row_level_security_diff": rls_diff,
            }),
        }
    }
}

/* helpers */

fn describe_db(server: &str, db: &str, timeout: Duration) -> io::Result<Value> {
    let mut cmd = spacetime_command();
    cmd.arg("describe")
        .arg("--json")
        .arg("--schema-version")
        .arg("10")
        .arg("-s")
        .arg(server)
        .arg("-y")
        .arg(db);
    let (code, out, err) = run_with_timeout(cmd, Path::new("."), timeout)?;
    if code != 0 {
        return Err(io::Error::other(format!(
            "describe failed: {}",
            String::from_utf8_lossy(&err)
        )));
    }
    let v: Value = serde_json::from_slice(&out).map_err(|e| io::Error::other(format!("parse json: {}", e)))?;
    Ok(v)
}

fn extract_schema(v: &Value) -> SchemaSnapshot {
    let mut schema = SchemaSnapshot::default();
    let names = SchemaNames::from_schema(v);
    let typespace = v.get("typespace").or_else(|| section(v, "Typespace"));
    let types = typespace
        .and_then(|value| value.get("types"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    if let Some(ts) = schema_array(v, "tables", "Tables") {
        for t in ts {
            let name = names.table(schema_name(t.get("source_name").or_else(|| t.get("name"))));
            let mut cols = BTreeMap::new();

            // Older CLI descriptions put columns directly on the table. Keep
            // accepting that shape while also reading the current typespace
            // representation.
            let legacy_columns = t.get("columns").and_then(Value::as_array);
            let current_columns = t
                .get("product_type_ref")
                .and_then(Value::as_u64)
                .and_then(|idx| types.get(idx as usize))
                .and_then(|ty| ty.pointer("/Product/elements"))
                .and_then(Value::as_array);

            if let Some(cs) = legacy_columns.or(current_columns) {
                for c in cs {
                    let cname = schema_name(c.get("name"));
                    let cty = c
                        .get("type")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            c.get("algebraic_type")
                                .map(|ty| canonical_type(ty, types, &mut BTreeSet::new()).to_string())
                        })
                        .unwrap_or_default();
                    cols.insert(cname, cty);
                }
            }

            let column_names: Vec<String> = current_columns
                .or(legacy_columns)
                .into_iter()
                .flatten()
                .map(|column| schema_name(column.get("name")))
                .collect();

            insert_schema_property(
                &mut cols,
                "primary_key",
                column_list(t.get("primary_key"), &column_names),
            );
            insert_schema_property(&mut cols, "indexes", normalize_indexes(t.get("indexes"), &column_names));
            insert_schema_property(
                &mut cols,
                "constraints",
                normalize_constraints(t.get("constraints"), &column_names),
            );
            insert_schema_property(
                &mut cols,
                "sequences",
                normalize_sequences(t.get("sequences"), &column_names),
            );
            insert_schema_property(&mut cols, "schedule", canonical_value(t.get("schedule")));
            insert_schema_property(&mut cols, "table_type", canonical_value(t.get("table_type")));
            insert_schema_property(&mut cols, "table_access", canonical_value(t.get("table_access")));
            insert_schema_property(&mut cols, "default_values", canonical_value(t.get("default_values")));
            insert_schema_property(&mut cols, "is_event", canonical_value(t.get("is_event")));
            schema.tables.insert(name, cols);
        }
    }

    if let Some(rs) = schema_array(v, "reducers", "Reducers") {
        for r in rs {
            let name = names.function(schema_name(r.get("source_name").or_else(|| r.get("name"))));
            let mut sig = format!("{}({})", name, parameter_types(r, types).join(","));
            append_field(&mut sig, "visibility", r.get("visibility"));
            append_type(&mut sig, "ok", r.get("ok_return_type"), types);
            append_type(&mut sig, "err", r.get("err_return_type"), types);
            append_field(&mut sig, "lifecycle", r.get("lifecycle"));
            schema.reducers.insert(sig);
        }
    }

    for export in v.get("misc_exports").and_then(Value::as_array).into_iter().flatten() {
        if let Some(procedure) = export.get("Procedure") {
            schema
                .exports
                .insert(function_export("procedure", procedure, types, &names));
        } else if let Some(view) = export.get("View") {
            schema.exports.insert(view_export(view, types, &names));
        } else if let Some(default) = export.get("ColumnDefaultValue") {
            schema
                .exports
                .insert(format!("column_default:{}", canonical_value(Some(default))));
        }
    }

    for procedure in schema_array(v, "procedures", "Procedures").into_iter().flatten() {
        schema
            .exports
            .insert(function_export("procedure", procedure, types, &names));
    }
    for view in schema_array(v, "views", "Views").into_iter().flatten() {
        schema.exports.insert(view_export(view, types, &names));
    }
    insert_canonical_exports(&mut schema.exports, v, "schedules", "Schedules", "schedule");
    insert_canonical_exports(
        &mut schema.exports,
        v,
        "life_cycle_reducers",
        "LifeCycleReducers",
        "lifecycle",
    );
    insert_http_routes(&mut schema.exports, v);
    insert_canonical_exports(
        &mut schema.exports,
        v,
        "view_primary_keys",
        "ViewPrimaryKeys",
        "view_primary_key",
    );

    for rule in schema_array(v, "row_level_security", "RowLevelSecurity")
        .into_iter()
        .flatten()
    {
        schema.row_level_security.insert(canonical_value(Some(rule)));
    }

    schema
}

fn section<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.get("sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|item| item.get(name))
}

fn schema_array<'a>(v: &'a Value, direct_name: &str, section_name: &str) -> Option<&'a Vec<Value>> {
    v.get(direct_name)
        .or_else(|| section(v, section_name))
        .and_then(Value::as_array)
}

fn parameter_types(function: &Value, types: &[Value]) -> Vec<String> {
    function
        .pointer("/params/elements")
        .and_then(Value::as_array)
        .or_else(|| function.get("args").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .map(|param| {
            param
                .get("algebraic_type")
                .map(|ty| canonical_type(ty, types, &mut BTreeSet::new()).to_string())
                .or_else(|| param.get("type").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_default()
        })
        .collect()
}

fn append_field(signature: &mut String, label: &str, value: Option<&Value>) {
    if let Some(value) = value {
        signature.push_str(&format!("|{label}={}", canonical_value(Some(value))));
    }
}

fn append_type(signature: &mut String, label: &str, value: Option<&Value>, types: &[Value]) {
    if let Some(value) = value {
        signature.push_str(&format!(
            "|{label}={}",
            canonical_type(value, types, &mut BTreeSet::new())
        ));
    }
}

fn function_export(kind: &str, function: &Value, types: &[Value], names: &SchemaNames) -> String {
    let name = names.function(schema_name(
        function.get("source_name").or_else(|| function.get("name")),
    ));
    let mut signature = format!("{kind}:{name}({})", parameter_types(function, types).join(","));
    append_type(&mut signature, "return", function.get("return_type"), types);
    append_field(&mut signature, "visibility", function.get("visibility"));
    signature
}

fn view_export(view: &Value, types: &[Value], names: &SchemaNames) -> String {
    let mut signature = function_export("view", view, types, names);
    append_field(&mut signature, "public", view.get("is_public"));
    append_field(&mut signature, "anonymous", view.get("is_anonymous"));
    signature
}

fn insert_canonical_exports(
    exports: &mut BTreeSet<String>,
    schema: &Value,
    direct_name: &str,
    section_name: &str,
    kind: &str,
) {
    for export in schema_array(schema, direct_name, section_name).into_iter().flatten() {
        exports.insert(format!("{kind}:{}", canonical_value(Some(export))));
    }
}

fn insert_http_routes(exports: &mut BTreeSet<String>, schema: &Value) {
    for route in schema_array(schema, "http_routes", "HttpRoutes").into_iter().flatten() {
        let mut route = route.clone();
        if let Some(route) = route.as_object_mut() {
            route.remove("handler_function");
        }
        exports.insert(format!("http_route:{}", canonical_value(Some(&route))));
    }
}

fn schema_name(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .or_else(|| value.and_then(|value| value.get("some")).and_then(Value::as_str))
        .unwrap_or("")
        .to_owned()
}

fn canonical_type(value: &Value, types: &[Value], visiting: &mut BTreeSet<usize>) -> Value {
    if let Some(idx) = value.get("Ref").and_then(Value::as_u64).map(|idx| idx as usize) {
        if !visiting.insert(idx) {
            return json!({ "recursive_ref": idx });
        }
        let resolved = types
            .get(idx)
            .map(|value| canonical_type(value, types, visiting))
            .unwrap_or_else(|| json!({ "missing_ref": idx }));
        visiting.remove(&idx);
        return resolved;
    }

    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| canonical_type(value, types, visiting))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_type(value, types, visiting)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn column_name(value: &Value, columns: &[String]) -> String {
    value
        .as_u64()
        .and_then(|idx| columns.get(idx as usize))
        .cloned()
        .unwrap_or_else(|| value.to_string())
}

fn column_list(value: Option<&Value>, columns: &[String]) -> String {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|value| column_name(value, columns))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

fn normalize_indexes(value: Option<&Value>, columns: &[String]) -> String {
    let mut indexes = BTreeSet::new();
    for index in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(algorithm) = index.get("algorithm").and_then(Value::as_object) else {
            continue;
        };
        for (kind, indexed_columns) in algorithm {
            let normalized_columns = match indexed_columns {
                Value::Array(values) => values
                    .iter()
                    .map(|value| column_name(value, columns))
                    .collect::<Vec<_>>()
                    .join(","),
                value => column_name(value, columns),
            };
            indexes.insert(format!("{kind}({normalized_columns})"));
        }
    }
    indexes.into_iter().collect::<Vec<_>>().join(";")
}

fn normalize_constraints(value: Option<&Value>, columns: &[String]) -> String {
    let mut constraints = BTreeSet::new();
    for constraint in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(data) = constraint.get("data").and_then(Value::as_object) else {
            continue;
        };
        for (kind, detail) in data {
            let normalized_columns = column_list(detail.get("columns"), columns);
            constraints.insert(format!("{kind}({normalized_columns})"));
        }
    }
    constraints.into_iter().collect::<Vec<_>>().join(";")
}

fn normalize_sequences(value: Option<&Value>, columns: &[String]) -> String {
    let mut sequences = BTreeSet::new();
    for sequence in value.and_then(Value::as_array).into_iter().flatten() {
        let column = sequence
            .get("column")
            .map(|value| column_name(value, columns))
            .unwrap_or_default();
        let increment = canonical_value(sequence.get("increment"));
        let start = canonical_value(sequence.get("start"));
        let min = canonical_value(sequence.get("min_value"));
        let max = canonical_value(sequence.get("max_value"));
        sequences.insert(format!(
            "{column}:increment={increment}:start={start}:min={min}:max={max}"
        ));
    }
    sequences.into_iter().collect::<Vec<_>>().join(";")
}

fn canonical_value(value: Option<&Value>) -> String {
    value.map(Value::to_string).unwrap_or_default()
}

fn insert_schema_property(columns: &mut BTreeMap<String, String>, name: &str, value: String) {
    if !value.is_empty() {
        columns.insert(format!("@{name}"), value);
    }
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
    call_reducer_json_out_with_timeout(db, reducer, args, host, Duration::from_secs(30))
}

fn call_reducer_json_out_with_timeout(
    db: &str,
    reducer: &str,
    args: &[Value],
    host: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    let mut cmd = spacetime_command();
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

    let (code, stdout, stderr) = run_with_timeout(cmd, Path::new("."), timeout)
        .map_err(|e| format!("spacetime call failed or timed out: {e}"))?;
    if debug_llm_verbose() {
        eprintln!(
            "[dbg] spacetime call exit={} stdout:\n{}\n-- stderr:\n{}\n",
            code,
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }
    if code != 0 {
        return Err(format!("spacetime call failed:\n{}", String::from_utf8_lossy(&stderr)));
    }
    Ok(String::from_utf8_lossy(&stdout).to_string())
}

pub fn sql_raw(db: &str, query: &str, host: Option<&str>) -> Result<String, String> {
    sql_raw_with_timeout(db, query, host, Duration::from_secs(30))
}

fn sql_raw_with_timeout(db: &str, query: &str, host: Option<&str>, timeout: Duration) -> Result<String, String> {
    let mut cmd = spacetime_command();
    cmd.arg("sql").arg(db).arg(query);
    if let Some(h) = host {
        cmd.arg("--server").arg(h);
    }

    if debug_llm_verbose() {
        eprintln!("[dbg] spacetime sql: {:?}", cmd);
    }

    let (code, stdout, stderr) = run_with_timeout(cmd, Path::new("."), timeout)
        .map_err(|e| format!("spacetime sql failed or timed out: {e}"))?;
    if debug_llm_verbose() {
        eprintln!(
            "[dbg] spacetime sql exit={} stdout:\n{}\n-- stderr:\n{}\n",
            code,
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }
    if code != 0 {
        return Err(format!("spacetime sql failed:\n{}", String::from_utf8_lossy(&stderr)));
    }
    Ok(String::from_utf8_lossy(&stdout).to_string())
}

pub fn sql_count(db: &str, query: &str, host: Option<&str>) -> Result<i64, String> {
    sql_count_with_timeout(db, query, host, Duration::from_secs(30))
}

fn sql_count_with_timeout(db: &str, query: &str, host: Option<&str>, timeout: Duration) -> Result<i64, String> {
    let mut cmd = spacetime_command();
    cmd.arg("sql").arg(db).arg(query);
    if let Some(h) = host {
        cmd.arg("--server").arg(h);
    }

    if debug_llm_verbose() {
        eprintln!("[dbg] spacetime sql-count: {:?}", cmd);
    }

    let (code, stdout, stderr) = run_with_timeout(cmd, Path::new("."), timeout)
        .map_err(|e| format!("spacetime sql failed or timed out: {e}"))?;
    if debug_llm_verbose() {
        eprintln!(
            "[dbg] spacetime sql-count exit={} stdout:\n{}\n-- stderr:\n{}\n",
            code,
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }
    if code != 0 {
        return Err(format!("spacetime sql failed:\n{}", String::from_utf8_lossy(&stderr)));
    }
    let s = String::from_utf8_lossy(&stdout);
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
        let call_res =
            call_reducer_json_out_with_timeout(&self.db, &self.reducer, &self.args, Some(&self.server), self.timeout);
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
        match sql_raw_with_timeout(&self.db, &self.sql, Some(&self.server), self.timeout) {
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
        let call =
            call_reducer_json_out_with_timeout(&self.db, &self.reducer, &self.args, Some(&self.server), self.timeout);
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
        match sql_count_with_timeout(&self.db, &self.sql, Some(&self.server), self.timeout) {
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

        if let Err(e) = call_reducer_json_out_with_timeout(
            &self.golden_db,
            &self.reducer,
            &self.args,
            Some(&self.server),
            self.timeout,
        ) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({"phase":"call_reducer_golden","error":e}),
            };
        }
        if let Err(e) = call_reducer_json_out_with_timeout(
            &self.llm_db,
            &self.reducer,
            &self.args,
            Some(&self.server),
            self.timeout,
        ) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({"phase":"call_reducer_llm","error":e}),
            };
        }

        if debug_llm_verbose() {
            eprintln!("[dbg] query for parity: {}", self.query);
        }
        let g = match sql_raw_with_timeout(&self.golden_db, &self.query, Some(&self.server), self.timeout) {
            Ok(s) => s,
            Err(e) => {
                return ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({"phase":"sql_golden","error":e}),
                }
            }
        };
        let l = match sql_raw_with_timeout(&self.llm_db, &self.query, Some(&self.server), self.timeout) {
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

pub struct SqlOutputExcludesScorer {
    pub server: String,
    pub db: String,
    pub sql: String,
    pub excluded: Vec<String>,
    pub id_str: &'static str,
}

pub struct EventuallySqlCountScorer {
    pub server: String,
    pub db: String,
    pub sql: String,
    pub expected: i64,
    pub timeout: Duration,
    pub id_str: &'static str,
}

impl Scorer for EventuallySqlCountScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        let started = Instant::now();
        loop {
            let remaining = self.timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({ "sql": self.sql, "expected": self.expected, "last": { "error": "timeout" } }),
                };
            }
            let last = match sql_count_with_timeout(&self.db, &self.sql, Some(&self.server), remaining) {
                Ok(actual) if actual == self.expected => {
                    return ScoreDetails {
                        pass: true,
                        partial: 1.0,
                        notes: json!({ "sql": self.sql, "expected": self.expected, "actual": actual }),
                    };
                }
                Ok(actual) => json!({ "actual": actual }),
                Err(error) => json!({ "error": error }),
            };
            if started.elapsed() >= self.timeout {
                return ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({ "sql": self.sql, "expected": self.expected, "last": last }),
                };
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Scorer for SqlCountOnlyScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }
    fn score(&self, _llm_output: &str) -> ScoreDetails {
        match sql_count_with_timeout(&self.db, &self.sql, Some(&self.server), self.timeout) {
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

impl Scorer for SqlOutputExcludesScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        match sql_raw(&self.db, &self.sql, Some(&self.server)) {
            Ok(output) => {
                let found = self
                    .excluded
                    .iter()
                    .filter(|value| output.contains(value.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                let pass = found.is_empty();
                ScoreDetails {
                    pass,
                    partial: if pass { 1.0 } else { 0.0 },
                    notes: json!({ "sql": self.sql, "excluded": self.excluded, "found": found, "actual": output }),
                }
            }
            Err(error) => ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase": "sql", "error": error }),
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
        if let Err(e) = sql_exec_with_timeout(&self.golden_db, &self.sql, Some(&self.server), self.timeout) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase":"sql_golden", "error": e, "sql": self.sql }),
            };
        }
        if let Err(e) = sql_exec_with_timeout(&self.llm_db, &self.sql, Some(&self.server), self.timeout) {
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

pub struct ReducerCallBothScorer {
    pub server: String,
    pub golden_db: String,
    pub llm_db: String,
    pub reducer: String,
    pub args: Vec<Value>,
    pub attempts: usize,
    pub id_str: &'static str,
}

pub struct CallOutputParityScorer {
    pub server: String,
    pub golden_db: String,
    pub llm_db: String,
    pub function: String,
    pub args: Vec<Value>,
    pub collapse_ws: bool,
    pub attempts: usize,
    pub id_str: &'static str,
}

fn call_with_retries(
    db: &str,
    function: &str,
    args: &[Value],
    server: &str,
    attempts: usize,
) -> Result<String, String> {
    let attempts = attempts.max(1);
    let mut last_error = None;
    for attempt in 0..attempts {
        match call_reducer_json_out(db, function, args, Some(server)) {
            Ok(output) => return Ok(output),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < attempts {
            thread::sleep(Duration::from_millis(250));
        }
    }
    Err(last_error.expect("at least one call attempt must run"))
}

pub struct HttpRouteCase {
    pub method: String,
    pub path: String,
    pub body: Option<String>,
}

pub struct HttpRouteParityScorer {
    pub server: String,
    pub golden_db: String,
    pub llm_db: String,
    pub cases: Vec<HttpRouteCase>,
    pub compare_content_type: bool,
    pub timeout: Duration,
    pub id_str: &'static str,
}

fn call_http_route(
    server: &str,
    db: &str,
    case: &HttpRouteCase,
    timeout: Duration,
) -> Result<(u16, String, String), String> {
    let server = server.trim_end_matches('/').to_string();
    let db = db.to_string();
    let method = case.method.clone();
    let path = case.path.clone();
    let body = case.body.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        runtime.block_on(async move {
            let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|error| error.to_string())?;
            let url = format!("{server}/v1/database/{db}/route{path}");
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|error| error.to_string())?;
            let mut request = client.request(method, url);
            if let Some(body) = body {
                request = request.header("content-type", "text/plain").body(body);
            }
            let response = request.send().await.map_err(|error| error.to_string())?;
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_string();
            let body = response.text().await.map_err(|error| error.to_string())?;
            Ok((status, content_type, body))
        })
    })
    .join()
    .map_err(|_| "HTTP route worker panicked".to_string())?
}

fn http_route_results_equal(
    golden_results: &[(u16, String, String)],
    llm_results: &[(u16, String, String)],
    compare_content_type: bool,
) -> bool {
    golden_results.len() == llm_results.len()
        && golden_results
            .iter()
            .zip(llm_results)
            .all(|(golden, llm)| golden.0 == llm.0 && golden.2 == llm.2 && (!compare_content_type || golden.1 == llm.1))
}

impl Scorer for HttpRouteParityScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        let mut golden_results = Vec::new();
        let mut llm_results = Vec::new();
        for case in &self.cases {
            match call_http_route(&self.server, &self.golden_db, case, self.timeout) {
                Ok(result) => golden_results.push(result),
                Err(error) => {
                    return ScoreDetails {
                        pass: false,
                        partial: 0.0,
                        notes: json!({ "phase": "http_golden", "error": error }),
                    }
                }
            }
            match call_http_route(&self.server, &self.llm_db, case, self.timeout) {
                Ok(result) => llm_results.push(result),
                Err(error) => {
                    return ScoreDetails {
                        pass: false,
                        partial: 0.0,
                        notes: json!({ "phase": "http_llm", "error": error }),
                    }
                }
            }
        }
        let pass = http_route_results_equal(&golden_results, &llm_results, self.compare_content_type);
        ScoreDetails {
            pass,
            partial: if pass { 1.0 } else { 0.0 },
            notes: json!({
                "golden": golden_results,
                "llm": llm_results,
                "compared_content_type": self.compare_content_type,
            }),
        }
    }
}

impl Scorer for CallOutputParityScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        let golden = match call_with_retries(&self.golden_db, &self.function, &self.args, &self.server, self.attempts) {
            Ok(output) => output,
            Err(error) => {
                return ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({ "phase": "call_golden", "function": self.function, "error": error }),
                }
            }
        };
        let llm = match call_with_retries(&self.llm_db, &self.function, &self.args, &self.server, self.attempts) {
            Ok(output) => output,
            Err(error) => {
                return ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({ "phase": "call_llm", "function": self.function, "error": error }),
                }
            }
        };
        let golden = normalize(&golden, self.collapse_ws);
        let llm = normalize(&llm, self.collapse_ws);
        let pass = golden == llm;
        ScoreDetails {
            pass,
            partial: if pass { 1.0 } else { 0.0 },
            notes: json!({ "function": self.function, "golden": golden, "llm": llm }),
        }
    }
}

impl Scorer for ReducerCallBothScorer {
    fn id(&self) -> &'static str {
        self.id_str
    }

    fn score(&self, _llm_output: &str) -> ScoreDetails {
        if debug_llm_verbose() {
            eprintln!(
                "[dbg] ReducerCallBothScorer: reducer={} args={:?} golden_db={} llm_db={} server={}",
                self.reducer, self.args, self.golden_db, self.llm_db, self.server
            );
        }
        if let Err(e) = call_with_retries(&self.golden_db, &self.reducer, &self.args, &self.server, self.attempts) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase":"call_reducer_golden", "error": e, "reducer": self.reducer }),
            };
        }
        if let Err(e) = call_with_retries(&self.llm_db, &self.reducer, &self.args, &self.server, self.attempts) {
            return ScoreDetails {
                pass: false,
                partial: 0.0,
                notes: json!({ "phase":"call_reducer_llm", "error": e, "reducer": self.reducer }),
            };
        }
        if debug_llm_verbose() {
            eprintln!("[dbg] ReducerCallBothScorer: success");
        }
        ScoreDetails {
            pass: true,
            partial: 1.0,
            notes: json!({ "reducer": self.reducer, "args": self.args }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::db::raw_def::v10::{MethodOrAny, RawModuleDefV10Builder};
    use spacetimedb_lib::db::raw_def::v9::Lifecycle;
    use spacetimedb_lib::sats::serde::SerdeWrapper;
    use spacetimedb_lib::sats::{AlgebraicType, ProductType};

    fn current_schema(include_owner_index: bool) -> Value {
        let mut indexes = vec![json!({ "algorithm": { "BTree": [0] } })];
        if include_owner_index {
            indexes.push(json!({ "algorithm": { "BTree": [1] } }));
        }
        json!({
            "typespace": {
                "types": [{
                    "Product": {
                        "elements": [
                            { "name": { "some": "id" }, "algebraic_type": { "U64": [] } },
                            { "name": { "some": "owner_id" }, "algebraic_type": { "U64": [] } }
                        ]
                    }
                }]
            },
            "tables": [{
                "name": "child_item",
                "product_type_ref": 0,
                "primary_key": [0],
                "indexes": indexes,
                "constraints": [{ "data": { "Unique": { "columns": [0] } } }],
                "sequences": [{ "column": 0, "increment": 1 }],
                "schedule": { "none": [] },
                "table_type": { "User": [] },
                "table_access": { "Public": [] }
            }],
            "reducers": []
        })
    }

    #[test]
    fn current_schema_extracts_columns_and_table_properties() {
        let schema = extract_schema(&current_schema(true));
        let child_item = &schema.tables["child_item"];

        assert_eq!(child_item["id"], r#"{"U64":[]}"#);
        assert_eq!(child_item["owner_id"], r#"{"U64":[]}"#);
        assert_eq!(child_item["@primary_key"], "id");
        assert_eq!(child_item["@indexes"], "BTree(id);BTree(owner_id)");
        assert_eq!(child_item["@constraints"], "Unique(id)");
        assert_eq!(child_item["@sequences"], "id:increment=1:start=:min=:max=");
        assert_eq!(child_item["@table_access"], r#"{"Public":[]}"#);
        assert!(schema.reducers.is_empty());
        assert!(schema.row_level_security.is_empty());
    }

    #[test]
    fn missing_index_produces_a_schema_diff() {
        let golden = extract_schema(&current_schema(true));
        let candidate = extract_schema(&current_schema(false));

        assert!(!diff_maps(&golden.tables, &candidate.tables).is_null());
    }

    #[test]
    fn v10_schema_compares_explicit_canonical_table_names() {
        let schema = |source_name: &str| {
            json!({
                "sections": [
                    { "Typespace": { "types": [{ "Product": { "elements": [] } }] } },
                    { "Tables": [{
                        "source_name": source_name,
                        "product_type_ref": 0,
                        "primary_key": [],
                        "indexes": [],
                        "constraints": [],
                        "sequences": [],
                        "table_type": { "User": [] },
                        "table_access": { "Public": [] },
                        "default_values": [],
                        "is_event": false
                    }] },
                    { "ExplicitNames": { "entries": [{
                        "Table": {
                            "source_name": source_name,
                            "canonical_name": "child_item"
                        }
                    }] } }
                ]
            })
        };

        assert_eq!(
            extract_schema(&schema("childItem")),
            extract_schema(&schema("child_item"))
        );
    }

    #[test]
    fn row_level_security_produces_a_schema_diff() {
        let mut golden = current_schema(true);
        golden["row_level_security"] = json!([{ "sql": "SELECT * FROM users WHERE identity = :sender" }]);
        let candidate = current_schema(true);
        let golden = extract_schema(&golden);
        let candidate = extract_schema(&candidate);

        assert!(!diff_sets(&golden.row_level_security, &candidate.row_level_security).is_null());
    }

    #[test]
    fn current_schema_extracts_reducer_parameter_types() {
        let mut schema = current_schema(true);
        schema["reducers"] = json!([{
            "name": "set_owner",
            "params": {
                "elements": [
                    { "name": { "some": "id" }, "algebraic_type": { "U64": [] } },
                    { "name": { "some": "owner" }, "algebraic_type": { "String": [] } }
                ]
            }
        }]);

        let reducers = extract_schema(&schema).reducers;

        assert_eq!(
            reducers,
            BTreeSet::from([r#"set_owner({"U64":[]},{"String":[]})"#.to_owned()])
        );
    }

    #[test]
    fn reducer_parameter_type_produces_a_schema_diff() {
        let mut golden = current_schema(true);
        golden["reducers"] = json!([{
            "name": "set_owner",
            "params": { "elements": [
                { "name": { "some": "owner" }, "algebraic_type": { "String": [] } }
            ] }
        }]);
        let mut candidate = current_schema(true);
        candidate["reducers"] = json!([{
            "name": "set_owner",
            "params": { "elements": [
                { "name": { "some": "owner" }, "algebraic_type": { "U64": [] } }
            ] }
        }]);
        let golden_reducers = extract_schema(&golden).reducers;
        let candidate_reducers = extract_schema(&candidate).reducers;

        assert!(!diff_sets(&golden_reducers, &candidate_reducers).is_null());
    }

    #[test]
    fn reducer_parameter_names_do_not_produce_a_schema_diff() {
        let mut golden = current_schema(true);
        golden["reducers"] = json!([{
            "name": "send_reminder",
            "params": { "elements": [
                { "name": { "some": "timer" }, "algebraic_type": { "U64": [] } }
            ] }
        }]);
        let mut candidate = current_schema(true);
        candidate["reducers"] = json!([{
            "name": "send_reminder",
            "params": { "elements": [
                { "name": { "some": "reminder" }, "algebraic_type": { "U64": [] } }
            ] }
        }]);
        let golden_reducers = extract_schema(&golden).reducers;
        let candidate_reducers = extract_schema(&candidate).reducers;

        assert!(diff_sets(&golden_reducers, &candidate_reducers).is_null());
    }

    #[test]
    fn legacy_reducer_args_remain_supported() {
        let mut schema = current_schema(true);
        schema["reducers"] = json!([{
            "name": "set_owner",
            "args": [{ "name": "owner", "type": "String" }]
        }]);

        let reducers = extract_schema(&schema).reducers;

        assert_eq!(reducers, BTreeSet::from(["set_owner(String)".to_owned()]));
    }

    #[test]
    fn http_route_parity_ignores_unspecified_content_type() {
        let golden = vec![(201, String::new(), "created".to_string())];
        let candidate = vec![(201, "text/plain".to_string(), "created".to_string())];

        assert!(http_route_results_equal(&golden, &candidate, false));
        assert!(!http_route_results_equal(&golden, &candidate, true));
    }

    #[test]
    fn http_route_schema_ignores_handler_names() {
        let schema = |handler: &str| {
            json!({
                "sections": [
                    { "HttpHandlers": [{ "source_name": handler }] },
                    { "HttpRoutes": [{
                        "handler_function": handler,
                        "method": { "Method": { "Get": [] } },
                        "path": "/items"
                    }] }
                ]
            })
        };

        assert_eq!(
            extract_schema(&schema("listItems")),
            extract_schema(&schema("get_items"))
        );
    }

    #[test]
    fn v10_schema_extracts_lifecycle_procedures_views_and_http_exports() {
        let schema = json!({
            "sections": [
                { "Typespace": { "types": [] } },
                { "Tables": [] },
                { "Reducers": [{
                    "source_name": "connected",
                    "params": { "elements": [] },
                    "visibility": { "Private": [] },
                    "ok_return_type": { "Unit": [] },
                    "err_return_type": { "String": [] }
                }] },
                { "Procedures": [{
                    "source_name": "fetch",
                    "params": { "elements": [] },
                    "return_type": { "String": [] },
                    "visibility": { "ClientCallable": [] }
                }] },
                { "Views": [{
                    "source_name": "profile",
                    "params": { "elements": [] },
                    "return_type": { "String": [] },
                    "is_public": true,
                    "is_anonymous": false
                }] },
                { "LifeCycleReducers": [{
                    "lifecycle_spec": { "OnConnect": [] },
                    "function_name": "connected"
                }] },
                { "HttpHandlers": [{ "source_name": "webhook" }] },
                { "HttpRoutes": [{
                    "handler_function": "webhook",
                    "method": { "Any": [] },
                    "path": "/hook"
                }] },
                { "ViewPrimaryKeys": [{ "view_source_name": "profile", "columns": ["id"] }] }
            ]
        });

        let extracted = extract_schema(&schema);

        assert_eq!(extracted.reducers.len(), 1);
        assert!(extracted
            .exports
            .iter()
            .any(|value| value.starts_with("procedure:fetch")));
        assert!(extracted.exports.iter().any(|value| value.starts_with("view:profile")));
        assert!(extracted.exports.iter().any(|value| value.starts_with("lifecycle:")));
        assert!(extracted.exports.iter().any(|value| value.starts_with("http_route:")));
        assert!(extracted
            .exports
            .iter()
            .any(|value| value.starts_with("view_primary_key:")));
    }

    #[test]
    fn extracts_exports_from_actual_v10_serialization() {
        let mut module = RawModuleDefV10Builder::new();
        module.add_lifecycle_reducer(Lifecycle::OnConnect, "connected", ProductType::unit());
        module.add_procedure("fetch", ProductType::unit(), AlgebraicType::unit());
        module.add_view("profile", 0, true, false, ProductType::unit(), AlgebraicType::unit());
        module.add_view_primary_key("profile", ["id"]);
        module.add_http_handler("webhook");
        module.add_http_route("webhook", MethodOrAny::Any, "/hook");
        let module = module.finish();
        let json = serde_json::to_value(SerdeWrapper::from_ref(&module)).unwrap();

        let extracted = extract_schema(&json);

        assert_eq!(extracted.reducers.len(), 1, "serialized schema was {json}");
        assert_eq!(extracted.exports.len(), 5, "serialized schema was {json}");
        assert!(
            extracted
                .exports
                .iter()
                .any(|value| value.starts_with("procedure:fetch")),
            "serialized schema was {json}"
        );
        assert!(
            extracted.exports.iter().any(|value| value.starts_with("view:profile")),
            "serialized schema was {json}"
        );

        let mut missing_assignments = json.clone();
        missing_assignments["sections"]
            .as_array_mut()
            .unwrap()
            .retain(|section| section.get("LifeCycleReducers").is_none() && section.get("ViewPrimaryKeys").is_none());
        let missing_assignments = extract_schema(&missing_assignments);
        assert!(!diff_sets(&extracted.exports, &missing_assignments.exports).is_null());
    }
}
