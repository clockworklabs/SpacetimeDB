use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, SqlBuilder};
use std::time::Duration;


pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);
        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let reducer = ident("Seed", casing);

        let select = sb.select_by_id(
            "primitives",
            &["id","count","total","price","ratio","active","name"],
            "id",
            1
        );

        v.push(make_reducer_data_parity_scorer(
            file!(),
            route_tag,
            reducer,
            vec![], // no args
            &select,
            "elementary_columns_row_parity",
            true,
            Duration::from_secs(10),
        ));

        let count = sb.count_by_id("primitives", "id", 1);
        v.push(make_sql_count_only_scorer(
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