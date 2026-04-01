use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_sql_count_only_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);

        let case = casing_for_lang(lang);
        let sb = SqlBuilder::new(case);

        let seed = ident("Seed", crate::eval::Casing::Snake);
        let log_table = table_name("log", lang);

        let user_id = ident("user_id", sb.case);
        let day = ident("day", sb.case);

        // Seed once via reducer on both DBs
        v.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            &seed,
            vec![],
            "mcindex_seed",
        ));

        // Then just query — don't call seed again
        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {log_table}"),
            3,
            "mcindex_seed_count",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!(
                "SELECT COUNT(*) AS n FROM {log_table} WHERE {u}=7 AND {d}=1",
                u = user_id,
                d = day
            ),
            1,
            "mcindex_lookup_u7_d1",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!(
                "SELECT COUNT(*) AS n FROM {log_table} WHERE {u}=7 AND {d}=2",
                u = user_id,
                d = day
            ),
            1,
            "mcindex_lookup_u7_d2",
            Duration::from_secs(10),
        ));

        v
    })
}
