use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_sql_count_scorer, make_sql_count_only_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, ReducerSqlCountConfig, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);
        let casing = casing_for_lang(lang);

        let sb = SqlBuilder::new(casing);
        let reducer_name = ident("Seed", casing);

        let user_id = ident("user_id", sb.case);
        let group_id = ident("group_id", sb.case);

        v.push(make_reducer_sql_count_scorer(ReducerSqlCountConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer_name.into(),
            args: vec![],
            sql_count_query: format!(
                "SELECT COUNT(*) AS n FROM memberships WHERE {user_id}=1 AND {group_id}=10"
            ),
            expected_count: 1,
            id_str: "m2m_has_1_10",
            timeout: Duration::from_secs(10),
        }));

        v.push(make_sql_count_only_scorer(
            file!(),
            route_tag,
            &format!("SELECT COUNT(*) AS n FROM memberships WHERE {user_id}=1 AND {group_id}=20"),
            1,
            "m2m_has_1_20",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            file!(),
            route_tag,
            &format!("SELECT COUNT(*) AS n FROM memberships WHERE {user_id}=2 AND {group_id}=20"),
            1,
            "m2m_has_2_20",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            file!(),
            route_tag,
            "SELECT COUNT(*) AS n FROM memberships",
            3,
            "memberships_three_rows",
            Duration::from_secs(10),
        ));

        v
    })
}
