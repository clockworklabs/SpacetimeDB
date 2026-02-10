use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_sql_count_scorer, make_sql_count_only_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerSqlCountConfig, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);
        let casing = casing_for_lang(lang);

        let sb = SqlBuilder::new(casing);
        let reducer_name = ident("Seed", casing);
        let membership_table = table_name("membership", lang);

        let user_id = ident("user_id", sb.case);
        let group_id = ident("group_id", sb.case);

        v.push(make_reducer_sql_count_scorer(host_url, ReducerSqlCountConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer_name.into(),
            args: vec![],
            sql_count_query: format!(
                "SELECT COUNT(*) AS n FROM {membership_table} WHERE {user_id}=1 AND {group_id}=10"
            ),
            expected_count: 1,
            id_str: "m2m_has_1_10",
            timeout: Duration::from_secs(10),
        }));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            &format!("SELECT COUNT(*) AS n FROM {membership_table} WHERE {user_id}=1 AND {group_id}=20"),
            1,
            "m2m_has_1_20",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            &format!("SELECT COUNT(*) AS n FROM {membership_table} WHERE {user_id}=2 AND {group_id}=20"),
            1,
            "m2m_has_2_20",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            &format!("SELECT COUNT(*) AS n FROM {membership_table}"),
            3,
            "memberships_three_rows",
            Duration::from_secs(10),
        ));

        v
    })
}
