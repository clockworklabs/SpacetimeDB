use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_data_parity_scorer,
    make_sql_exec_both_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::Value;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let reducer_name = ident("LookupUserName", casing);
        let user_table = table_name("user", lang);
        let result_table = table_name("result", lang);

        // Seed a user row in both DBs so the lookup has something to find
        let seed_users = sb.insert_values(
            &user_table,
            &["id","name","age","active"],
            &["1","'Alice'","30","true"],
        );

        v.push(make_sql_exec_both_scorer(
            host_url,
            file!(),
            route_tag,
            &seed_users,
            "seed_user_row",
            Duration::from_secs(10),
        ));

        // After calling the reducer, the projection should be present in results
        let select_result = sb.select_by_id(
            &result_table,
            &["id","name"],
            "id",
            1,
        );

        v.push(make_reducer_data_parity_scorer(host_url, ReducerDataParityConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer_name.into(),
            args: vec![Value::from(1)],
            select_query: select_result.clone(),
            id_str: "index_lookup_projection_parity",
            collapse_ws: true,
            timeout: Duration::from_secs(10),
        }));

        v
    })
}
