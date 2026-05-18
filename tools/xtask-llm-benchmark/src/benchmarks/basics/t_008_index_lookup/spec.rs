use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_reducer_data_parity_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::Value;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let insert_reducer = ident("InsertUser", crate::eval::Casing::Snake);
        let lookup_reducer = ident("LookupUserName", crate::eval::Casing::Snake);
        let result_table = table_name("result", lang);

        // Seed a user row via reducer on both DBs (auto-inc assigns id=1)
        v.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            &insert_reducer,
            vec![Value::from("Alice"), Value::from(30), Value::from(true)],
            "seed_user_via_reducer",
        ));

        // After calling the lookup reducer, the projection should be present in results
        let select_result = sb.select_by_id(&result_table, &["id", "name"], "id", 1);

        v.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: lookup_reducer.into(),
                args: vec![Value::from(1)],
                select_query: select_result.clone(),
                id_str: "index_lookup_projection_parity",
                collapse_ws: true,
                timeout: Duration::from_secs(10),
            },
        ));

        v
    })
}
