use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_sql_count_only_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        // Schema parity ensures the scheduled columns exist with correct names/types.
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        // After publish (Init ran), exactly one scheduled row should exist.
        let sb = SqlBuilder::new(casing_for_lang(lang));
        let idcol = ident("scheduled_id", sb.case);
        let q = format!("SELECT COUNT(*) AS n FROM tick_timer WHERE {idcol}>=0");

        v.push(make_sql_count_only_scorer(
            file!(),
            route_tag,
            &q,
            1,
            "scheduled_seeded_one_row",
            Duration::from_secs(10),
        ));

        v
    })
}
