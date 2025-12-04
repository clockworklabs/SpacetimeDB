use crate::eval::defaults::{default_schema_parity_scorers, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, BenchmarkSpec, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let sb = SqlBuilder::new(casing_for_lang(lang));
        let id   = sb.cols(&["id"])[0].clone();
        let name = sb.cols(&["name"])[0].clone();
        let age  = sb.cols(&["age"])[0].clone();
        let act  = sb.cols(&["active"])[0].clone();

        let q_alice = format!("SELECT COUNT(*) AS n FROM users WHERE {id}=1 AND {name}='Alice' AND {age}=30 AND {act}=true");
        let q_bob   = format!("SELECT COUNT(*) AS n FROM users WHERE {id}=2 AND {name}='Bob'   AND {age}=22 AND {act}=false");
        let q_total = "SELECT COUNT(*) AS n FROM users";

        v.push(make_sql_count_only_scorer(file!(), route_tag, q_alice, 1, "init_seed_alice", Duration::from_secs(10)));
        v.push(make_sql_count_only_scorer(file!(), route_tag, q_bob,   1, "init_seed_bob",   Duration::from_secs(10)));
        v.push(make_sql_count_only_scorer(file!(), route_tag, q_total, 2, "init_total_two",  Duration::from_secs(10)));

        v
    })
}