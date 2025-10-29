use crate::bench::utils::{sanitize_db_name, server_name};
use crate::eval::derive_cat_task_from_file;
use crate::eval::scorers::{
    ReducerDataParityScorer, ReducerSqlCountScorer, SchemaParityScorer, Scorer, SqlCountOnlyScorer, SqlExecBothScorer,
};
use serde_json::Value;
use std::time::Duration;

pub fn default_schema_parity_scorers(src_file: &str, route_tag: &str) -> Vec<Box<dyn Scorer>> {
    let (cat, task) = derive_cat_task_from_file(src_file);

    let golden_db = sanitize_db_name(&format!("{}-{}-golden", cat, task));
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));

    let srv = server_name();

    vec![Box::new(SchemaParityScorer {
        server: srv,
        golden_db,
        llm_db,
        timeout: Duration::from_secs(10),
        id_str: "schema_parity",
    }) as Box<dyn Scorer>]
}

pub fn make_reducer_sql_count_scorer(
    src_file: &str,
    route_tag: &str,
    reducer: impl Into<String>,
    args: Vec<Value>,
    sql_count_query: impl Into<String>,
    expected_count: i64,
    id_str: &'static str,
    timeout: Duration,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    let server = server_name();

    Box::new(ReducerSqlCountScorer {
        server,
        db: llm_db,
        reducer: reducer.into(),
        args,
        sql: sql_count_query.into(),
        expected: expected_count,
        timeout,
        id_str,
    }) as Box<dyn Scorer>
}

pub fn make_sql_count_only_scorer(
    src_file: &str,
    route_tag: &str,
    sql: impl Into<String>,
    expected: i64,
    id_str: &'static str,
    timeout: Duration,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    Box::new(SqlCountOnlyScorer {
        server: server_name(),
        db: llm_db,
        sql: sql.into(),
        expected,
        timeout,
        id_str,
    })
}

pub fn make_reducer_data_parity_scorer(
    src_file: &str,
    route_tag: &str,
    reducer: impl Into<String>,
    args: Vec<Value>,
    select_query: impl Into<String>,
    id_str: &'static str,
    collapse_ws: bool,
    timeout: Duration,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let golden_db = sanitize_db_name(&format!("{}-{}-golden", cat, task));
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    let server = server_name();

    Box::new(ReducerDataParityScorer {
        server,
        golden_db,
        llm_db,
        reducer: reducer.into(),
        args,
        query: select_query.into(),
        collapse_ws,
        timeout,
        id_str,
    }) as Box<dyn Scorer>
}

pub fn make_sql_exec_both_scorer(
    src_file: &str,
    route_tag: &str,
    sql: &str,
    id_str: &'static str,
    timeout: Duration,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let golden_db = sanitize_db_name(&format!("{}-{}-golden", cat, task));
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    let server = server_name();

    Box::new(SqlExecBothScorer {
        server,
        golden_db,
        llm_db,
        sql: sql.to_string(),
        timeout,
        id_str,
    }) as Box<dyn Scorer>
}
