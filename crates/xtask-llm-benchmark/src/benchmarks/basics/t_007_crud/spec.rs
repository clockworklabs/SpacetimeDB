use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let reducer = ident("Crud", casing);

        let select_id1 = sb.select_by_id("users", &["id","name","age","active"], "id", 1);
        let count_id2  = sb.count_by_id("users", "id", 2);
        let count_all  = "SELECT COUNT(*) AS n FROM users";

        v.push(make_reducer_data_parity_scorer(ReducerDataParityConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer.into(),
            args: vec![],
            select_query: select_id1.clone(),
            id_str: "crud_row_id1_parity",
            collapse_ws: true,
            timeout: Duration::from_secs(10),
        }));
        v.push(make_sql_count_only_scorer(
            file!(), route_tag, &count_id2, 0, "crud_row_id2_deleted", Duration::from_secs(10),
        ));
        v.push(make_sql_count_only_scorer(
            file!(), route_tag, count_all, 1, "crud_total_count_one", Duration::from_secs(10),
        ));

        v
    })
}