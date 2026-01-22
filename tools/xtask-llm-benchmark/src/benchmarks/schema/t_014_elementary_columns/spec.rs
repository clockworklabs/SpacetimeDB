use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use std::time::Duration;


pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);
        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let reducer = ident("Seed", casing);

        let select = sb.select_by_id(
            "primitive",
            &["id","count","total","price","ratio","active","name"],
            "id",
            1
        );

        v.push(make_reducer_data_parity_scorer(host_url, ReducerDataParityConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer.into(),
            args: vec![], // no args
            select_query: select.clone(),
            id_str: "elementary_columns_row_parity",
            collapse_ws: true,
            timeout: Duration::from_secs(10),
        }));

        let count = sb.count_by_id("primitive", "id", 1);
        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            &count,
            1,
            "elementary_columns_row_count",
            Duration::from_secs(10),
        ));

        v
    })
}