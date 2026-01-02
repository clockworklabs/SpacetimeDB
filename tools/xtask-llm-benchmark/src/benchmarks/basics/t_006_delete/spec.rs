use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_sql_count_scorer,
    make_sql_exec_both_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, ReducerSqlCountConfig, SqlBuilder};
use serde_json::Value;
use std::time;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let seed = sb.insert_values("users", &["id","name","age","active"], &["1","'Alice'","30","true"]);
        let count = sb.count_by_id("users", "id", 1);
        let reducer_name = ident("DeleteUser", casing);

        v.push(make_sql_exec_both_scorer(
            file!(),
            route_tag,
            &seed,
            "seed_users_row",
            time::Duration::from_secs(10),
        ));

        v.push(make_reducer_sql_count_scorer(ReducerSqlCountConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer_name.into(),
            args: vec![Value::from(1)],
            sql_count_query: count.clone(),
            expected_count: 0,
            id_str: "delete_user_count_zero",
            timeout: time::Duration::from_secs(10),
        }));

        v
    })
}
