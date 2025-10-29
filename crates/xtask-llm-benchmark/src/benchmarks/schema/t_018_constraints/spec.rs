use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_data_parity_scorer,
    make_sql_count_only_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let reducer = ident("Seed", casing);

        let select = sb.select_by_id("accounts", &["id","email","name"], "id", 1);
        v.push(make_reducer_data_parity_scorer(
            file!(),
            route_tag,
            reducer,
            vec![],
            &select,
            "constraints_row_parity_after_seed",
            true,
            Duration::from_secs(10),
        ));

        let count = sb.count_by_id("accounts", "id", 2);
        v.push(make_sql_count_only_scorer(
            file!(),
            route_tag,
            &count,
            1,
            "constraints_seed_two_rows",
            Duration::from_secs(10),
        ));

        v
    })
}
