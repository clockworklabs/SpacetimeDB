use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_sql_count_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let case = casing_for_lang(lang);
        let sb   = SqlBuilder::new(case);

        let seed = ident("Seed", case);

        let user_id = ident("user_id", sb.case);
        let day     = ident("day", sb.case);

        v.push(make_reducer_sql_count_scorer(
            file!(), route_tag, &seed, vec![],
            "SELECT COUNT(*) AS n FROM logs",
            3, "mcindex_seed_count", Duration::from_secs(10),
        ));

        v.push(make_reducer_sql_count_scorer(
            file!(), route_tag, &seed, vec![],
            &format!("SELECT COUNT(*) AS n FROM logs WHERE {u}=7 AND {d}=1", u=user_id, d=day),
            1, "mcindex_lookup_u7_d1", Duration::from_secs(10),
        ));

        v.push(make_reducer_sql_count_scorer(
            file!(), route_tag, &seed, vec![],
            &format!("SELECT COUNT(*) AS n FROM logs WHERE {u}=7 AND {d}=2", u=user_id, d=day),
            1, "mcindex_lookup_u7_d2", Duration::from_secs(10),
        ));

        v
    })
}
