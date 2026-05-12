use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_reducer_sql_count_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerSqlCountConfig, SqlBuilder};
use serde_json::Value;
use std::time;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let user_table = table_name("user", lang);
        let count = sb.count_by_id(&user_table, "id", 1);
        let insert_reducer = ident("InsertUser", crate::eval::Casing::Snake);
        let delete_reducer = ident("DeleteUser", crate::eval::Casing::Snake);

        // Seed a user row via reducer on both DBs (auto-inc assigns id=1)
        v.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            &insert_reducer,
            vec![Value::from("Alice"), Value::from(30), Value::from(true)],
            "seed_user_via_reducer",
        ));

        v.push(make_reducer_sql_count_scorer(
            host_url,
            ReducerSqlCountConfig {
                src_file: file!(),
                route_tag,
                reducer: delete_reducer.into(),
                args: vec![Value::from(1)],
                sql_count_query: count.clone(),
                expected_count: 0,
                id_str: "delete_user_count_zero",
                timeout: time::Duration::from_secs(10),
            },
        ));

        v
    })
}
