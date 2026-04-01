use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_reducer_data_parity_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::Value;
use std::time;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let user_table = table_name("user", lang);
        let select = sb.select_by_id(&user_table, &["id", "name", "age", "active"], "id", 1);
        let insert_reducer = ident("InsertUser", crate::eval::Casing::Snake);
        let update_reducer = ident("UpdateUser", crate::eval::Casing::Snake);

        // Seed a user row via reducer on both DBs (auto-inc assigns id=1)
        v.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            &insert_reducer,
            vec![Value::from("Alice"), Value::from(30), Value::from(true)],
            "seed_user_via_reducer",
        ));

        v.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: update_reducer.into(),
                args: vec![
                    Value::from(1),
                    Value::from("Alice2"),
                    Value::from(31),
                    Value::from(false),
                ],
                select_query: select.clone(),
                id_str: "data_parity_update_user",
                collapse_ws: true,
                timeout: time::Duration::from_secs(10),
            },
        ));

        v
    })
}
