use crate::bench::utils::{golden_db_name, sanitize_db_name};
use crate::eval::scorers::{
    CallOutputParityScorer, EventuallySqlCountScorer, HttpRouteCase, HttpRouteParityScorer, ReducerCallBothScorer,
    ReducerDataParityScorer, ReducerSqlCountScorer, SchemaParityScorer, Scorer, SqlCountOnlyScorer,
    SqlDistinctRowsScorer, SqlExecBothScorer, SqlOutputExcludesScorer,
};
use crate::eval::{derive_cat_task_from_file, ReducerDataParityConfig, ReducerSqlCountConfig};
use std::time::Duration;

pub fn default_schema_parity_scorers(host_url: &str, src_file: &str, route_tag: &str) -> Vec<Box<dyn Scorer>> {
    let (cat, task) = derive_cat_task_from_file(src_file);

    let golden_db = golden_db_name(&cat, &task, route_tag);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));

    vec![Box::new(SchemaParityScorer {
        server: host_url.to_string(),
        golden_db,
        llm_db,
        timeout: Duration::from_secs(10),
        id_str: "schema_parity",
    }) as Box<dyn Scorer>]
}

pub fn make_reducer_sql_count_scorer(host_url: &str, cfg: ReducerSqlCountConfig<'_>) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(cfg.src_file);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, cfg.route_tag));

    Box::new(ReducerSqlCountScorer {
        server: host_url.to_string(),
        db: llm_db,
        reducer: cfg.reducer,
        args: cfg.args,
        sql: cfg.sql_count_query,
        expected: cfg.expected_count,
        timeout: cfg.timeout,
        id_str: cfg.id_str,
    }) as Box<dyn Scorer>
}

pub fn make_sql_count_only_scorer(
    host_url: &str,
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
        server: host_url.to_string(),
        db: llm_db,
        sql: sql.into(),
        expected,
        timeout,
        id_str,
    })
}

pub fn make_sql_distinct_rows_scorer(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    sql: impl Into<String>,
    expected: usize,
    id_str: &'static str,
    timeout: Duration,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    Box::new(SqlDistinctRowsScorer {
        server: host_url.to_string(),
        db: llm_db,
        sql: sql.into(),
        expected,
        timeout,
        id_str,
    })
}

pub fn make_eventually_sql_count_scorer(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    sql: impl Into<String>,
    expected: i64,
    id_str: &'static str,
    timeout: Duration,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    Box::new(EventuallySqlCountScorer {
        server: host_url.to_string(),
        db: llm_db,
        sql: sql.into(),
        expected,
        timeout,
        id_str,
    })
}

pub fn make_reducer_data_parity_scorer(host_url: &str, cfg: ReducerDataParityConfig<'_>) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(cfg.src_file);
    let golden_db = golden_db_name(&cat, &task, cfg.route_tag);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, cfg.route_tag));

    Box::new(ReducerDataParityScorer {
        server: host_url.to_string(),
        golden_db,
        llm_db,
        reducer: cfg.reducer,
        args: cfg.args,
        query: cfg.select_query,
        collapse_ws: cfg.collapse_ws,
        timeout: cfg.timeout,
        id_str: cfg.id_str,
    }) as Box<dyn Scorer>
}

pub fn make_sql_exec_both_scorer(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    sql: &str,
    id_str: &'static str,
    timeout: Duration,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let golden_db = golden_db_name(&cat, &task, route_tag);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));

    Box::new(SqlExecBothScorer {
        server: host_url.to_string(),
        golden_db,
        llm_db,
        sql: sql.to_string(),
        timeout,
        id_str,
    }) as Box<dyn Scorer>
}

pub fn make_reducer_call_both_scorer(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    reducer: &str,
    args: Vec<serde_json::Value>,
    id_str: &'static str,
) -> Box<dyn Scorer> {
    make_reducer_call_both_scorer_with_attempts(host_url, src_file, route_tag, reducer, args, id_str, 1)
}

pub fn make_sql_output_excludes_scorer(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    sql: impl Into<String>,
    excluded: Vec<String>,
    id_str: &'static str,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    Box::new(SqlOutputExcludesScorer {
        server: host_url.to_string(),
        db: llm_db,
        sql: sql.into(),
        excluded,
        id_str,
    })
}

pub fn make_reducer_call_both_scorer_with_attempts(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    reducer: &str,
    args: Vec<serde_json::Value>,
    id_str: &'static str,
    attempts: usize,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let golden_db = golden_db_name(&cat, &task, route_tag);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));

    Box::new(ReducerCallBothScorer {
        server: host_url.to_string(),
        golden_db,
        llm_db,
        reducer: reducer.to_string(),
        args,
        attempts,
        id_str,
    }) as Box<dyn Scorer>
}

pub fn make_call_output_parity_scorer(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    function: &str,
    args: Vec<serde_json::Value>,
    id_str: &'static str,
) -> Box<dyn Scorer> {
    make_call_output_parity_scorer_with_attempts(host_url, src_file, route_tag, function, args, id_str, 1)
}

pub fn make_call_output_parity_scorer_with_attempts(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    function: &str,
    args: Vec<serde_json::Value>,
    id_str: &'static str,
    attempts: usize,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let golden_db = golden_db_name(&cat, &task, route_tag);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    Box::new(CallOutputParityScorer {
        server: host_url.to_string(),
        golden_db,
        llm_db,
        function: function.to_string(),
        args,
        collapse_ws: true,
        attempts,
        id_str,
    })
}

pub fn make_http_route_parity_scorer(
    host_url: &str,
    src_file: &str,
    route_tag: &str,
    cases: Vec<(&str, &str, Option<&str>)>,
    compare_content_type: bool,
    id_str: &'static str,
) -> Box<dyn Scorer> {
    let (cat, task) = derive_cat_task_from_file(src_file);
    let golden_db = golden_db_name(&cat, &task, route_tag);
    let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", cat, task, route_tag));
    Box::new(HttpRouteParityScorer {
        server: host_url.to_string(),
        golden_db,
        llm_db,
        id_str,
        compare_content_type,
        timeout: Duration::from_secs(10),
        cases: cases
            .into_iter()
            .map(|(method, path, body)| HttpRouteCase {
                method: method.to_string(),
                path: path.to_string(),
                body: body.map(str::to_string),
            })
            .collect(),
    })
}
