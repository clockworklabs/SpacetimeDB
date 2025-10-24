use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_data_parity_scorer,
    make_sql_exec_both_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, SqlBuilder};
use serde_json::Value;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let reducer_name = ident("LookupUserName", casing);

        // Seed a user row in both DBs so the lookup has something to find
        let seed_users = sb.insert_values(
            "users",
            &["id","name","age","active"],
            &["1","'Alice'","30","true"],
        );

        v.push(make_sql_exec_both_scorer(
            file!(),
            route_tag,
            &seed_users,
            "seed_user_row",
            Duration::from_secs(10),
        ));

        // After calling the reducer, the projection should be present in results
        let select_result = sb.select_by_id(
            "results",
            &["id","name"],
            "id",
            1,
        );

        v.push(make_reducer_data_parity_scorer(
            file!(),
            route_tag,
            reducer_name,
            vec![Value::from(1)],
            &select_result,
            "index_lookup_projection_parity",
            true,
            Duration::from_secs(10),
        ));

        v
    })
}
